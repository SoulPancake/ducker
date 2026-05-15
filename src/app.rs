use crate::{
    audio::{
        list_input_devices, list_output_devices, spawn_audio_engine, AudioCommand,
        AudioEngineHandle, AudioSettings, AudioStatus,
    },
    meter::{MeterData, PeakHold},
    params::{
        Params, ATTACK_MAX, ATTACK_MIN, HPF_MAX, HPF_MIN, KNEE_MAX, KNEE_MIN, MAKEUP_MAX,
        MAKEUP_MIN, MIX_MAX, MIX_MIN, RATIO_MAX, RATIO_MIN, RELEASE_MAX, RELEASE_MIN,
        THRESHOLD_MAX, THRESHOLD_MIN,
    },
};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::{f32::consts::TAU, sync::Arc};

const APP_KEY: &str = "ducker-app-state";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    knee_db: f32,
    makeup_db: f32,
    sc_highpass_on: bool,
    sc_hpf_freq_hz: f32,
    dry_wet_percent: f32,
    input_device_name: Option<String>,
    output_device_name: Option<String>,
    main_channel: usize,
    sidechain_channel: usize,
    sample_rate_hz: u32,
    buffer_size: u32,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 5.0,
            release_ms: 150.0,
            knee_db: 3.0,
            makeup_db: 0.0,
            sc_highpass_on: true,
            sc_hpf_freq_hz: 80.0,
            dry_wet_percent: 100.0,
            input_device_name: None,
            output_device_name: None,
            main_channel: 0,
            sidechain_channel: 1,
            sample_rate_hz: 48_000,
            buffer_size: 256,
        }
    }
}

pub struct DuckerApp {
    params: Arc<Params>,
    state: PersistedState,
    engine: AudioEngineHandle,
    status: AudioStatus,
    meter_data: MeterData,
    input_peak_hold: PeakHold,
    side_peak_hold: PeakHold,
    gr_peak_hold: PeakHold,
    output_peak_hold: PeakHold,
}

impl DuckerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|s| eframe::get_value::<PersistedState>(s, APP_KEY))
            .unwrap_or_default();

        let params = Arc::new(Params::default());
        params.set_threshold(state.threshold_db);
        params.set_ratio(state.ratio);
        params.set_attack_ms(state.attack_ms);
        params.set_release_ms(state.release_ms);
        params.set_knee_db(state.knee_db);
        params.set_makeup_db(state.makeup_db);
        params.set_sc_highpass_on(state.sc_highpass_on);
        params.set_sc_hpf_freq_hz(state.sc_hpf_freq_hz);
        params.set_dry_wet_percent(state.dry_wet_percent);

        let engine = spawn_audio_engine(
            Arc::clone(&params),
            AudioSettings {
                input_device_name: state.input_device_name.clone(),
                output_device_name: state.output_device_name.clone(),
                main_channel: state.main_channel,
                sidechain_channel: state.sidechain_channel,
                sample_rate_hz: Some(state.sample_rate_hz),
                buffer_size: Some(state.buffer_size),
            },
        );

        Self {
            params,
            state,
            engine,
            status: AudioStatus::Stopped,
            meter_data: MeterData::default(),
            input_peak_hold: PeakHold::new(),
            side_peak_hold: PeakHold::new(),
            gr_peak_hold: PeakHold::new(),
            output_peak_hold: PeakHold::new(),
        }
    }

    fn restart_audio(&self) {
        let _ = self
            .engine
            .command_tx
            .try_send(AudioCommand::Restart(AudioSettings {
                input_device_name: self.state.input_device_name.clone(),
                output_device_name: self.state.output_device_name.clone(),
                main_channel: self.state.main_channel,
                sidechain_channel: self.state.sidechain_channel,
                sample_rate_hz: Some(self.state.sample_rate_hz),
                buffer_size: Some(self.state.buffer_size),
            }));
    }

    fn knob(
        ui: &mut egui::Ui,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        label: &str,
        suffix: &str,
    ) -> bool {
        let desired = Vec2::new(84.0, 112.0);
        let (rect, mut response) = ui.allocate_exact_size(desired, Sense::drag());

        if response.dragged() {
            let span = *range.end() - *range.start();
            let delta = -response.drag_delta().y * span / 300.0;
            let next = (*value + delta).clamp(*range.start(), *range.end());
            if (next - *value).abs() > f32::EPSILON {
                *value = next;
                response.mark_changed();
            }
        }

        let painter = ui.painter_at(rect);
        let center = Pos2::new(rect.center().x, rect.top() + 44.0);
        let radius = 28.0;
        let accent = Color32::from_rgb(0x00, 0xAA, 0xFF);
        painter.circle_filled(center, radius, Color32::from_rgb(0x24, 0x24, 0x24));
        painter.circle_stroke(center, radius, Stroke::new(1.5, Color32::from_gray(90)));

        let t = (*value - *range.start()) / (*range.end() - *range.start());
        let start_angle = -0.75 * TAU / 2.0;
        let end_angle = 0.75 * TAU / 2.0;
        let angle = start_angle + (end_angle - start_angle) * t;

        let mut points = Vec::new();
        let steps = 32;
        for i in 0..=steps {
            let a = start_angle + (angle - start_angle) * (i as f32 / steps as f32);
            points.push(Pos2::new(
                center.x + a.cos() * (radius - 4.0),
                center.y + a.sin() * (radius - 4.0),
            ));
        }
        painter.add(egui::Shape::line(points, Stroke::new(3.0, accent)));

        let dot = Pos2::new(
            center.x + angle.cos() * (radius - 8.0),
            center.y + angle.sin() * (radius - 8.0),
        );
        painter.circle_filled(dot, 3.5, Color32::WHITE);

        painter.text(
            Pos2::new(rect.center().x, rect.top() + 4.0),
            egui::Align2::CENTER_TOP,
            format!("{value:.1}{suffix}"),
            egui::FontId::proportional(13.0),
            Color32::LIGHT_GRAY,
        );
        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 8.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(12.0),
            Color32::from_gray(180),
        );

        response.changed()
    }

    fn meter_bar(
        ui: &mut egui::Ui,
        label: &str,
        db: f32,
        peak_db: f32,
        inverted: bool,
        accent: Color32,
    ) {
        let size = Vec2::new(48.0, 180.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_rgb(0x11, 0x11, 0x11));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(80)),
            StrokeKind::Outside,
        );

        let bottom = rect.bottom() - 20.0;
        let top = rect.top() + 8.0;
        let height = bottom - top;

        let norm = if inverted {
            (db.abs() / 30.0).clamp(0.0, 1.0)
        } else {
            ((db + 60.0) / 60.0).clamp(0.0, 1.0)
        };

        let fill_top = bottom - (height * norm);
        let fill_rect = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, fill_top),
            Pos2::new(rect.right() - 8.0, bottom),
        );
        painter.rect_filled(fill_rect, 2.0, accent);

        let hold_norm = if inverted {
            (peak_db.abs() / 30.0).clamp(0.0, 1.0)
        } else {
            ((peak_db + 60.0) / 60.0).clamp(0.0, 1.0)
        };
        let hold_y = bottom - (height * hold_norm);
        painter.line_segment(
            [
                Pos2::new(rect.left() + 8.0, hold_y),
                Pos2::new(rect.right() - 8.0, hold_y),
            ],
            Stroke::new(2.0, Color32::WHITE),
        );

        painter.text(
            Pos2::new(rect.center().x, rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(11.0),
            Color32::from_gray(180),
        );
    }
}

impl eframe::App for DuckerApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, APP_KEY, &self.state);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint();

        while let Ok(status) = self.engine.status_rx.try_recv() {
            self.status = status;
        }

        while let Ok(meter) = self.engine.meter_rx.try_recv() {
            self.meter_data = meter;
        }

        self.input_peak_hold.update(self.meter_data.input_peak_db);
        self.side_peak_hold
            .update(self.meter_data.sidechain_peak_db);
        self.gr_peak_hold
            .update(self.meter_data.gain_reduction_db.abs());
        self.output_peak_hold.update(self.meter_data.output_peak_db);

        egui::TopBottomPanel::top("title").show(&ctx, |ui| {
            ui.set_height(86.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("DUCKER")
                        .monospace()
                        .size(30.0)
                        .strong(),
                );
                ui.add_space(8.0);

                let status_color = match &self.status {
                    AudioStatus::Running => Color32::LIGHT_GREEN,
                    AudioStatus::Stopped => Color32::RED,
                    AudioStatus::Error(_) => Color32::YELLOW,
                };
                let status_text = match &self.status {
                    AudioStatus::Running => "RUNNING",
                    AudioStatus::Stopped => "STOPPED",
                    AudioStatus::Error(_) => "ERROR",
                };
                ui.colored_label(status_color, status_text);
            });

            ui.horizontal_wrapped(|ui| {
                let input_devices = list_input_devices();
                let output_devices = list_output_devices();

                let selected_input = self
                    .state
                    .input_device_name
                    .clone()
                    .unwrap_or_else(|| "Default Input".to_string());
                egui::ComboBox::from_label("Input Device")
                    .selected_text(selected_input)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.state.input_device_name.is_none(),
                                "Default Input",
                            )
                            .clicked()
                        {
                            self.state.input_device_name = None;
                            self.restart_audio();
                        }
                        for name in &input_devices {
                            if ui
                                .selectable_label(
                                    self.state.input_device_name.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.state.input_device_name = Some(name.clone());
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("SC Ch")
                    .selected_text(format!("{}", self.state.sidechain_channel + 1))
                    .show_ui(ui, |ui| {
                        for ch in 0..16usize {
                            if ui
                                .selectable_label(
                                    self.state.sidechain_channel == ch,
                                    format!("{}", ch + 1),
                                )
                                .clicked()
                            {
                                self.state.sidechain_channel = ch;
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Main Ch")
                    .selected_text(format!("{}", self.state.main_channel + 1))
                    .show_ui(ui, |ui| {
                        for ch in 0..16usize {
                            if ui
                                .selectable_label(
                                    self.state.main_channel == ch,
                                    format!("{}", ch + 1),
                                )
                                .clicked()
                            {
                                self.state.main_channel = ch;
                                self.restart_audio();
                            }
                        }
                    });

                let selected_output = self
                    .state
                    .output_device_name
                    .clone()
                    .unwrap_or_else(|| "Default Output".to_string());
                egui::ComboBox::from_label("Output Device")
                    .selected_text(selected_output)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                self.state.output_device_name.is_none(),
                                "Default Output",
                            )
                            .clicked()
                        {
                            self.state.output_device_name = None;
                            self.restart_audio();
                        }
                        for name in &output_devices {
                            if ui
                                .selectable_label(
                                    self.state.output_device_name.as_deref() == Some(name),
                                    name,
                                )
                                .clicked()
                            {
                                self.state.output_device_name = Some(name.clone());
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Sample Rate")
                    .selected_text(format!("{}", self.state.sample_rate_hz))
                    .show_ui(ui, |ui| {
                        for sr in [44_100u32, 48_000u32, 96_000u32] {
                            if ui
                                .selectable_label(self.state.sample_rate_hz == sr, format!("{sr}"))
                                .clicked()
                            {
                                self.state.sample_rate_hz = sr;
                                self.restart_audio();
                            }
                        }
                    });

                egui::ComboBox::from_label("Buffer")
                    .selected_text(format!("{}", self.state.buffer_size))
                    .show_ui(ui, |ui| {
                        for bs in [128u32, 256u32, 512u32] {
                            if ui
                                .selectable_label(self.state.buffer_size == bs, format!("{bs}"))
                                .clicked()
                            {
                                self.state.buffer_size = bs;
                                self.restart_audio();
                            }
                        }
                    });
            });

            if let AudioStatus::Error(msg) = &self.status {
                ui.colored_label(Color32::YELLOW, msg);
            }
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(Color32::from_rgb(0x1a, 0x1a, 0x1a)))
            .show(&ctx, |ui| {
                ui.add_space(6.0);

                ui.horizontal_wrapped(|ui| {
                    let mut threshold = self.params.get_threshold();
                    if Self::knob(
                        ui,
                        &mut threshold,
                        THRESHOLD_MIN..=THRESHOLD_MAX,
                        "THRESHOLD",
                        " dB",
                    ) {
                        self.params.set_threshold(threshold);
                        self.state.threshold_db = threshold;
                    }

                    let mut ratio = self.params.get_ratio();
                    if Self::knob(ui, &mut ratio, RATIO_MIN..=RATIO_MAX, "RATIO", "") {
                        self.params.set_ratio(ratio);
                        self.state.ratio = ratio;
                    }

                    let mut attack = self.params.get_attack_ms();
                    if Self::knob(ui, &mut attack, ATTACK_MIN..=ATTACK_MAX, "ATTACK", " ms") {
                        self.params.set_attack_ms(attack);
                        self.state.attack_ms = attack;
                    }

                    let mut release = self.params.get_release_ms();
                    if Self::knob(
                        ui,
                        &mut release,
                        RELEASE_MIN..=RELEASE_MAX,
                        "RELEASE",
                        " ms",
                    ) {
                        self.params.set_release_ms(release);
                        self.state.release_ms = release;
                    }

                    let mut knee = self.params.get_knee_db();
                    if Self::knob(ui, &mut knee, KNEE_MIN..=KNEE_MAX, "KNEE", " dB") {
                        self.params.set_knee_db(knee);
                        self.state.knee_db = knee;
                    }

                    let mut makeup = self.params.get_makeup_db();
                    if Self::knob(ui, &mut makeup, MAKEUP_MIN..=MAKEUP_MAX, "MAKEUP", " dB") {
                        self.params.set_makeup_db(makeup);
                        self.state.makeup_db = makeup;
                    }

                    let mut mix = self.params.get_dry_wet_percent();
                    if Self::knob(ui, &mut mix, MIX_MIN..=MIX_MAX, "DRY/WET", " %") {
                        self.params.set_dry_wet_percent(mix);
                        self.state.dry_wet_percent = mix;
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    let mut hp_on = self.params.get_sc_highpass_on();
                    if ui
                        .button(if hp_on {
                            "SC HIGHPASS ON"
                        } else {
                            "SC HIGHPASS OFF"
                        })
                        .clicked()
                    {
                        hp_on = !hp_on;
                        self.params.set_sc_highpass_on(hp_on);
                        self.state.sc_highpass_on = hp_on;
                    }

                    let mut hpf = self.params.get_sc_hpf_freq_hz();
                    ui.add_enabled_ui(hp_on, |ui| {
                        if ui
                            .add(
                                egui::Slider::new(&mut hpf, HPF_MIN..=HPF_MAX)
                                    .text("SC HPF FREQ (Hz)"),
                            )
                            .changed()
                        {
                            self.params.set_sc_hpf_freq_hz(hpf);
                            self.state.sc_hpf_freq_hz = hpf;
                        }
                    });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("dBFS").color(Color32::from_gray(170)));
                        for lbl in ["0", "-6", "-12", "-18", "-24", "-36", "-60"] {
                            ui.label(egui::RichText::new(lbl).color(Color32::from_gray(140)));
                        }
                    });

                    Self::meter_bar(
                        ui,
                        "INPUT",
                        self.meter_data.input_peak_db,
                        self.input_peak_hold.value(),
                        false,
                        Color32::GREEN,
                    );
                    Self::meter_bar(
                        ui,
                        "SIDECHAIN",
                        self.meter_data.sidechain_peak_db,
                        self.side_peak_hold.value(),
                        false,
                        Color32::from_rgb(180, 220, 80),
                    );
                    Self::meter_bar(
                        ui,
                        "GR",
                        self.meter_data.gain_reduction_db,
                        self.gr_peak_hold.value(),
                        true,
                        Color32::from_rgb(0x00, 0xAA, 0xFF),
                    );
                    Self::meter_bar(
                        ui,
                        "OUTPUT",
                        self.meter_data.output_peak_db,
                        self.output_peak_hold.value(),
                        false,
                        Color32::from_rgb(230, 120, 80),
                    );
                });
            });

        egui::TopBottomPanel::bottom("footer").show(&ctx, |ui| {
            let stats = self.engine.stats.snapshot();
            let cpu = f32::from_bits(stats.cpu_load_bits);
            let latency = f32::from_bits(stats.latency_ms_bits);
            ui.horizontal(|ui| {
                ui.label(format!("CPU: {:>4.1}%", cpu));
                ui.separator();
                ui.label(format!("Latency: {:>4.2} ms", latency));
                ui.separator();
                ui.label(format!("SR: {} Hz", stats.sample_rate_hz));
                ui.separator();
                ui.label(format!("Buffer: {}", stats.buffer_size));
                ui.separator();
                ui.label(egui::RichText::new("Made with Rust + egui + cpal").color(Color32::GRAY));
            });
        });
    }
}

impl Drop for DuckerApp {
    fn drop(&mut self) {
        let _ = self.engine.command_tx.try_send(AudioCommand::Stop);
    }
}
