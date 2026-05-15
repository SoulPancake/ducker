use crate::params::{Direction, FilterMode, ParamsSnapshot};
use std::f32::consts::PI;

/// Envelope Filter (Auto-Wah) DSP processor.
///
/// Signal chain: [Pre-Compressor] → [Envelope Follower] → [State Variable Filter] → Output
#[derive(Debug)]
pub struct EnvelopeFilterDsp {
    sample_rate: f32,
    // Envelope follower state
    envelope: f32,
    // Chamberlin SVF state
    svf_low: f32,
    svf_band: f32,
    // Pre-compressor state
    comp_envelope: f32,
    // Last computed cutoff for metering
    last_cutoff_hz: f32,
}

impl EnvelopeFilterDsp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(1.0),
            envelope: 0.0,
            svf_low: 0.0,
            svf_band: 0.0,
            comp_envelope: 0.0,
            last_cutoff_hz: 200.0,
        }
    }

    #[allow(dead_code)]
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    #[inline]
    fn coeff_for_ms(&self, ms: f32) -> f32 {
        (-1.0 / ((ms.max(0.001) * 0.001) * self.sample_rate)).exp()
    }

    /// Simple feed-forward compressor. Returns compressed sample.
    #[inline]
    fn compress(&mut self, sample: f32, threshold_db: f32, ratio: f32) -> f32 {
        let abs_sample = sample.abs();
        // Track RMS-ish envelope for compressor
        let comp_attack_coeff = self.coeff_for_ms(1.0);
        let comp_release_coeff = self.coeff_for_ms(80.0);

        if abs_sample > self.comp_envelope {
            self.comp_envelope =
                comp_attack_coeff * self.comp_envelope + (1.0 - comp_attack_coeff) * abs_sample;
        } else {
            self.comp_envelope =
                comp_release_coeff * self.comp_envelope + (1.0 - comp_release_coeff) * abs_sample;
        }

        let env_db = 20.0 * self.comp_envelope.max(1e-6).log10();
        let over = env_db - threshold_db;
        if over <= 0.0 {
            return sample;
        }
        let ratio = ratio.max(1.0);
        let gr_db = over - over / ratio;
        let gain = 10.0f32.powf(-gr_db / 20.0);
        sample * gain
    }

    /// Process a single mono input sample. Returns (output_sample, envelope_value, cutoff_hz).
    pub fn process_sample(
        &mut self,
        input: f32,
        params: &ParamsSnapshot,
    ) -> (f32, f32, f32) {
        // --- Pre-Compressor (optional) ---
        let signal = if params.pre_comp_on {
            let compressed = self.compress(input, params.comp_threshold_db, params.comp_ratio);
            let makeup = 10.0f32.powf(params.makeup_db / 20.0);
            compressed * makeup
        } else {
            input
        };

        // --- Envelope Follower ---
        let rect = signal.abs();
        let attack_coeff = self.coeff_for_ms(params.attack_ms);
        let release_coeff = self.coeff_for_ms(params.release_ms);

        if rect > self.envelope {
            self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * rect;
        } else {
            self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * rect;
        }

        // --- Envelope → Cutoff Mapping ---
        let freq_min = params.freq_min_hz;
        let freq_max = params.freq_max_hz;
        let sensitivity = params.sensitivity;
        let scaled = (self.envelope * sensitivity).clamp(0.0, 1.0);

        let cutoff_hz = match params.direction {
            Direction::Up => freq_min * (freq_max / freq_min).powf(scaled),
            Direction::Down => freq_max * (freq_min / freq_max).powf(scaled),
        };
        self.last_cutoff_hz = cutoff_hz;

        // --- Chamberlin State Variable Filter ---
        let f = 2.0 * (PI * cutoff_hz / self.sample_rate).sin();
        let q = 1.0 / params.resonance;

        self.svf_low = self.svf_low + f * self.svf_band;
        let high = signal - self.svf_low - q * self.svf_band;
        self.svf_band = f * high + self.svf_band;

        let filtered = match params.filter_mode {
            FilterMode::LowPass => self.svf_low,
            FilterMode::BandPass => self.svf_band,
            FilterMode::HighPass => high,
        };

        // --- Dry/Wet Mix ---
        let dry_wet = (params.dry_wet_percent / 100.0).clamp(0.0, 1.0);
        let out = signal * (1.0 - dry_wet) + filtered * dry_wet;

        // Soft clip to prevent harsh spikes
        let final_out = out.tanh();

        (final_out, self.envelope, cutoff_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Direction, FilterMode};

    fn snapshot() -> ParamsSnapshot {
        ParamsSnapshot {
            sensitivity: 0.65,
            freq_min_hz: 250.0,
            freq_max_hz: 2500.0,
            resonance: 3.5,
            attack_ms: 6.0,
            release_ms: 100.0,
            filter_mode: FilterMode::BandPass,
            direction: Direction::Up,
            pre_comp_on: true,
            comp_threshold_db: -16.0,
            comp_ratio: 8.0,
            makeup_db: 0.0,
            dry_wet_percent: 100.0,
        }
    }

    #[test]
    fn filter_produces_output_on_signal() {
        let mut dsp = EnvelopeFilterDsp::new(48_000.0);
        let p = snapshot();
        let mut max_out = 0.0f32;
        for _ in 0..5000 {
            let (out, _, _) = dsp.process_sample(0.5, &p);
            max_out = max_out.max(out.abs());
        }
        assert!(max_out > 0.01, "Filter should produce audible output");
    }

    #[test]
    fn envelope_tracks_input() {
        let mut dsp = EnvelopeFilterDsp::new(48_000.0);
        let p = snapshot();
        // Feed silence
        for _ in 0..1000 {
            dsp.process_sample(0.0, &p);
        }
        let (_, env_silent, _) = dsp.process_sample(0.0, &p);
        // Feed loud signal
        for _ in 0..1000 {
            dsp.process_sample(0.8, &p);
        }
        let (_, env_loud, _) = dsp.process_sample(0.8, &p);
        assert!(env_loud > env_silent, "Envelope should increase with louder input");
    }

    #[test]
    fn cutoff_moves_with_direction() {
        let mut dsp_up = EnvelopeFilterDsp::new(48_000.0);
        let mut dsp_down = EnvelopeFilterDsp::new(48_000.0);
        let mut p_up = snapshot();
        p_up.direction = Direction::Up;
        p_up.pre_comp_on = false;
        p_up.sensitivity = 1.0;
        let mut p_down = snapshot();
        p_down.direction = Direction::Down;
        p_down.pre_comp_on = false;
        p_down.sensitivity = 1.0;

        // Feed identical loud signal to build envelope
        for _ in 0..4000 {
            dsp_up.process_sample(0.9, &p_up);
            dsp_down.process_sample(0.9, &p_down);
        }
        let (_, _, cutoff_up) = dsp_up.process_sample(0.9, &p_up);
        let (_, _, cutoff_down) = dsp_down.process_sample(0.9, &p_down);
        // UP: cutoff near freq_max; DOWN: cutoff near freq_min
        assert!(
            cutoff_up > cutoff_down,
            "UP cutoff ({cutoff_up}) should be higher than DOWN cutoff ({cutoff_down})"
        );
    }
}
