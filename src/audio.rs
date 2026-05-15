use crate::{dsp::DuckerDsp, meter::MeterData, params::Params};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct SharedStats {
    pub cpu_load_bits: u32,
    pub latency_ms_bits: u32,
    pub sample_rate_hz: u32,
    pub buffer_size: u32,
}

#[derive(Debug)]
pub struct AudioStats {
    pub cpu_load_bits: AtomicU32,
    pub latency_ms_bits: AtomicU32,
    pub sample_rate_hz: AtomicU32,
    pub buffer_size: AtomicU32,
}

impl Default for AudioStats {
    fn default() -> Self {
        Self {
            cpu_load_bits: AtomicU32::new(0.0f32.to_bits()),
            latency_ms_bits: AtomicU32::new(0.0f32.to_bits()),
            sample_rate_hz: AtomicU32::new(0),
            buffer_size: AtomicU32::new(0),
        }
    }
}

impl AudioStats {
    pub fn snapshot(&self) -> SharedStats {
        SharedStats {
            cpu_load_bits: self.cpu_load_bits.load(Ordering::Relaxed),
            latency_ms_bits: self.latency_ms_bits.load(Ordering::Relaxed),
            sample_rate_hz: self.sample_rate_hz.load(Ordering::Relaxed),
            buffer_size: self.buffer_size.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AudioSettings {
    pub input_device_name: Option<String>,
    pub output_device_name: Option<String>,
    pub main_channel: usize,
    pub sidechain_channel: usize,
    pub sample_rate_hz: Option<u32>,
    pub buffer_size: Option<u32>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device_name: None,
            output_device_name: None,
            main_channel: 0,
            sidechain_channel: 1,
            sample_rate_hz: Some(48_000),
            buffer_size: Some(256),
        }
    }
}

#[derive(Clone, Debug)]
pub enum AudioCommand {
    Restart(AudioSettings),
    Stop,
}

#[derive(Clone, Debug)]
pub enum AudioStatus {
    Running,
    Stopped,
    Error(String),
}

pub struct AudioEngineHandle {
    pub command_tx: Sender<AudioCommand>,
    pub meter_rx: Receiver<MeterData>,
    pub status_rx: Receiver<AudioStatus>,
    pub stats: Arc<AudioStats>,
}

struct RunningStreams {
    _input_stream: cpal::Stream,
    _output_stream: cpal::Stream,
}

pub fn list_input_devices() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(devices) = cpal::default_host().input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

pub fn list_output_devices() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(devices) = cpal::default_host().output_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

pub fn spawn_audio_engine(params: Arc<Params>, initial: AudioSettings) -> AudioEngineHandle {
    let (command_tx, command_rx) = bounded::<AudioCommand>(8);
    let (meter_tx, meter_rx) = bounded::<MeterData>(8);
    let (status_tx, status_rx) = bounded::<AudioStatus>(16);
    let stats = Arc::new(AudioStats::default());

    let thread_params = Arc::clone(&params);
    let thread_stats = Arc::clone(&stats);
    let thread_status = status_tx.clone();

    thread::spawn(move || {
        let _ = thread_status.try_send(AudioStatus::Stopped);
        let mut current = initial;
        let mut streams: Option<RunningStreams> = None;

        loop {
            if streams.is_none() {
                match build_streams(
                    Arc::clone(&thread_params),
                    current.clone(),
                    meter_tx.clone(),
                    status_tx.clone(),
                    Arc::clone(&thread_stats),
                ) {
                    Ok(s) => {
                        streams = Some(s);
                        let _ = status_tx.try_send(AudioStatus::Running);
                    }
                    Err(err) => {
                        let _ = status_tx.try_send(AudioStatus::Error(err));
                    }
                }
            }

            match command_rx.recv() {
                Ok(AudioCommand::Restart(new_settings)) => {
                    streams = None;
                    current = new_settings;
                }
                Ok(AudioCommand::Stop) | Err(_) => {
                    streams = None;
                    let _ = status_tx.try_send(AudioStatus::Stopped);
                    break;
                }
            }
        }
    });

    AudioEngineHandle {
        command_tx,
        meter_rx,
        status_rx,
        stats,
    }
}

fn find_input_device(host: &cpal::Host, desired: Option<&str>) -> Result<cpal::Device, String> {
    if let Some(name) = desired {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if let Ok(device_name) = d.name() {
                    if device_name == name {
                        return Ok(d);
                    }
                }
            }
        }
    }

    host.default_input_device()
        .ok_or_else(|| "No input device available".to_string())
}

fn find_output_device(host: &cpal::Host, desired: Option<&str>) -> Result<cpal::Device, String> {
    if let Some(name) = desired {
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                if let Ok(device_name) = d.name() {
                    if device_name == name {
                        return Ok(d);
                    }
                }
            }
        }
    }

    host.default_output_device()
        .ok_or_else(|| "No output device available".to_string())
}

fn choose_config(
    input: &cpal::Device,
    output: &cpal::Device,
    requested_sr: Option<u32>,
    requested_buf: Option<u32>,
) -> Result<(cpal::StreamConfig, cpal::StreamConfig), String> {
    let requested = requested_sr.filter(|sr| *sr != 48_000 && *sr != 44_100);
    let preferred_sample_rates = requested.into_iter().chain([48_000_u32, 44_100_u32]);

    let input_cfgs = input
        .supported_input_configs()
        .map_err(|e| format!("Failed to query input configs: {e}"))?;

    let output_cfgs = output
        .supported_output_configs()
        .map_err(|e| format!("Failed to query output configs: {e}"))?;

    let mut chosen_in = None;
    for cfg in input_cfgs {
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            continue;
        }
        let min = cfg.min_sample_rate();
        let max = cfg.max_sample_rate();
        for candidate_rate in preferred_sample_rates.clone() {
            if candidate_rate >= min && candidate_rate <= max {
                chosen_in = Some(cfg.with_sample_rate(candidate_rate).config());
                break;
            }
        }
        if chosen_in.is_some() {
            break;
        }
    }

    let mut chosen_out = None;
    for cfg in output_cfgs {
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            continue;
        }
        let min = cfg.min_sample_rate();
        let max = cfg.max_sample_rate();
        for candidate_rate in preferred_sample_rates.clone() {
            if candidate_rate >= min && candidate_rate <= max {
                chosen_out = Some(cfg.with_sample_rate(candidate_rate).config());
                break;
            }
        }
        if chosen_out.is_some() {
            break;
        }
    }

    let mut input_cfg = chosen_in.ok_or_else(|| "No f32 input config found".to_string())?;
    let mut output_cfg = chosen_out.ok_or_else(|| "No f32 output config found".to_string())?;

    let chosen_sample_rate = input_cfg.sample_rate.min(output_cfg.sample_rate);
    input_cfg.sample_rate = chosen_sample_rate;
    output_cfg.sample_rate = chosen_sample_rate;

    let buf = requested_buf.unwrap_or(256);
    input_cfg.buffer_size = cpal::BufferSize::Fixed(buf);
    output_cfg.buffer_size = cpal::BufferSize::Fixed(buf);

    Ok((input_cfg, output_cfg))
}

#[cfg(target_os = "macos")]
fn request_pro_audio_mode() {}

#[cfg(not(target_os = "macos"))]
fn request_pro_audio_mode() {}

fn build_streams(
    params: Arc<Params>,
    settings: AudioSettings,
    meter_tx: Sender<MeterData>,
    status_tx: Sender<AudioStatus>,
    stats: Arc<AudioStats>,
) -> Result<RunningStreams, String> {
    request_pro_audio_mode();

    let host = cpal::default_host();
    let input = find_input_device(&host, settings.input_device_name.as_deref())?;
    let output = find_output_device(&host, settings.output_device_name.as_deref())?;

    let (input_cfg, output_cfg) = choose_config(
        &input,
        &output,
        settings.sample_rate_hz,
        settings.buffer_size,
    )?;

    stats
        .sample_rate_hz
        .store(output_cfg.sample_rate, Ordering::Relaxed);
    let buffer_for_stats = match output_cfg.buffer_size {
        cpal::BufferSize::Fixed(v) => v,
        cpal::BufferSize::Default => 0,
    };
    stats.buffer_size.store(buffer_for_stats, Ordering::Relaxed);
    let latency_ms = if output_cfg.sample_rate > 0 {
        (buffer_for_stats as f32 / output_cfg.sample_rate as f32) * 1000.0
    } else {
        0.0
    };
    stats
        .latency_ms_bits
        .store(latency_ms.to_bits(), Ordering::Relaxed);

    let (input_pair_tx, input_pair_rx) = bounded::<(f32, f32)>(4096);

    let input_channels = input_cfg.channels as usize;
    let output_channels = output_cfg.channels as usize;
    let main_channel = settings.main_channel;
    let side_channel = settings.sidechain_channel;

    let input_err_status = status_tx.clone();
    let output_err_status = status_tx.clone();

    let input_stream = input
        .build_input_stream(
            &input_cfg,
            move |data: &[f32], _| {
                for frame in data.chunks(input_channels.max(1)) {
                    let main = frame.get(main_channel).copied().unwrap_or(0.0);
                    let side = frame.get(side_channel).copied().unwrap_or(0.0);
                    let _ = input_pair_tx.try_send((main, side));
                }
            },
            move |err| {
                let _ = input_err_status
                    .try_send(AudioStatus::Error(format!("Input stream error: {err}")));
            },
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {e}"))?;

    let mut dsp = DuckerDsp::new(output_cfg.sample_rate as f32);

    let meter_interval_samples = ((output_cfg.sample_rate as f32) * 0.03) as usize;
    let meter_interval_samples = meter_interval_samples.max(1);

    let output_stats = Arc::clone(&stats);

    let output_stream = output
        .build_output_stream(
            &output_cfg,
            move |data: &mut [f32], _| {
                let start = Instant::now();
                let params_snapshot = params.snapshot();

                let mut input_peak = 0.0f32;
                let mut side_peak = 0.0f32;
                let mut output_peak = 0.0f32;
                let mut gr_db = 0.0f32;
                let mut sample_counter = 0usize;

                for frame in data.chunks_mut(output_channels.max(1)) {
                    let (main, side) = input_pair_rx.try_recv().unwrap_or((0.0, 0.0));
                    input_peak = input_peak.max(main.abs());
                    side_peak = side_peak.max(side.abs());

                    let (ducked, this_gr) = dsp.process_sample(main, side, &params_snapshot);
                    gr_db = this_gr;

                    if !frame.is_empty() {
                        frame[0] = ducked;
                    }
                    if frame.len() > 1 {
                        frame[1] = ducked;
                    }
                    if frame.len() > 2 {
                        for sample in frame.iter_mut().skip(2) {
                            *sample = 0.0;
                        }
                    }

                    output_peak = output_peak.max(ducked.abs());

                    sample_counter = sample_counter.saturating_add(1);
                    if sample_counter >= meter_interval_samples {
                        sample_counter = 0;

                        let input_db = 20.0 * input_peak.max(1e-6).log10();
                        let side_db = 20.0 * side_peak.max(1e-6).log10();
                        let output_db = 20.0 * output_peak.max(1e-6).log10();

                        let _ = meter_tx.try_send(MeterData {
                            input_peak_db: input_db.max(-120.0),
                            sidechain_peak_db: side_db.max(-120.0),
                            gain_reduction_db: gr_db.min(0.0),
                            output_peak_db: output_db.max(-120.0),
                        });
                    }
                }

                let elapsed = start.elapsed().as_secs_f32();
                let frame_count = (data.len() / output_channels.max(1)) as f32;
                let frame_dur = if output_cfg.sample_rate > 0 {
                    frame_count / output_cfg.sample_rate as f32
                } else {
                    0.0
                };
                let cpu = if frame_dur > 0.0 {
                    (elapsed / frame_dur) * 100.0
                } else {
                    0.0
                };
                output_stats
                    .cpu_load_bits
                    .store(cpu.to_bits(), Ordering::Relaxed);
            },
            move |err| {
                let _ = output_err_status
                    .try_send(AudioStatus::Error(format!("Output stream error: {err}")));
            },
            None,
        )
        .map_err(|e| format!("Failed to build output stream: {e}"))?;

    input_stream
        .play()
        .map_err(|e| format!("Failed to start input stream: {e}"))?;
    output_stream
        .play()
        .map_err(|e| format!("Failed to start output stream: {e}"))?;

    Ok(RunningStreams {
        _input_stream: input_stream,
        _output_stream: output_stream,
    })
}
