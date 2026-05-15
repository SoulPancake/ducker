use crate::params::ParamsSnapshot;

const TWO_PI: f32 = 2.0 * std::f32::consts::PI;
const MIN_HPF_CUTOFF_HZ: f32 = 1.0;

#[derive(Debug)]
pub struct QuackGenerator {
    sample_rate: f32,
    phase: f32,
    quack_time: f32,
    quack_duration: f32,
    quack_gain: f32,
    is_quacking: bool,
}

impl QuackGenerator {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            phase: 0.0,
            quack_time: 0.0,
            quack_duration: 0.15,
            quack_gain: 0.35,
            is_quacking: false,
        }
    }

    pub fn trigger(&mut self) {
        if !self.is_quacking {
            self.is_quacking = true;
            self.quack_time = 0.0;
            self.phase = 0.0;
        }
    }

    pub fn process_sample(&mut self) -> f32 {
        if !self.is_quacking {
            return 0.0;
        }

        let dt = 1.0 / self.sample_rate;
        self.quack_time += dt;

        if self.quack_time >= self.quack_duration {
            self.is_quacking = false;
            return 0.0;
        }

        // Smooth attack/decay to avoid clicky transients.
        let env_t = self.quack_time / self.quack_duration;
        let attack = ((env_t * 10.0).min(1.0)).powf(0.6);
        let decay = (1.0 - env_t).powf(2.0);
        let envelope = attack * decay;

        // Pitch sweeps down (350 Hz -> 150 Hz)
        let freq_start = 350.0;
        let freq_end = 150.0;
        let freq = freq_start + (freq_end - freq_start) * env_t;

        let phase_increment = TWO_PI * freq * dt;
        self.phase += phase_increment;
        if self.phase > TWO_PI {
            self.phase -= TWO_PI;
        }

        self.phase.sin() * envelope * self.quack_gain
    }

    pub fn set_intensity(&mut self, intensity: f32) {
        let i = intensity.clamp(0.1, 10.0);
        self.quack_duration = (0.08 + (i * 0.02)).clamp(0.08, 0.28);
        self.quack_gain = (0.20 + (i * 0.09)).clamp(0.20, 0.95);
    }
}

#[derive(Debug)]
pub struct DuckerDsp {
    sample_rate: f32,
    envelope: f32,
    hp_x1: f32,
    hp_y1: f32,
    quack_gen: QuackGenerator,
    last_gr_db: f32,
    quack_armed: bool,
}

impl DuckerDsp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            hp_x1: 0.0,
            hp_y1: 0.0,
            quack_gen: QuackGenerator::new(sample_rate),
            last_gr_db: 0.0,
            quack_armed: true,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.quack_gen.sample_rate = sample_rate.max(1.0);
    }

    fn coeff_for_ms(&self, ms: f32) -> f32 {
        (-1.0 / ((ms.max(0.001) * 0.001) * self.sample_rate)).exp()
    }

    fn highpass(&mut self, sample: f32, cutoff_hz: f32) -> f32 {
        let coeff = (-TWO_PI * cutoff_hz.max(MIN_HPF_CUTOFF_HZ) / self.sample_rate).exp();
        let y = coeff * (self.hp_y1 + sample - self.hp_x1);
        self.hp_x1 = sample;
        self.hp_y1 = y;
        y
    }

    fn gain_reduction_db(env_db: f32, threshold_db: f32, ratio: f32, knee_db: f32) -> f32 {
        let ratio = ratio.max(1.0);
        let over = env_db - threshold_db;

        if knee_db <= 0.0 {
            if over <= 0.0 {
                0.0
            } else {
                (threshold_db + over / ratio) - env_db
            }
        } else {
            let half = knee_db * 0.5;
            if over <= -half {
                0.0
            } else if over >= half {
                (threshold_db + over / ratio) - env_db
            } else {
                let x = (over + half) / knee_db;
                let hard = (threshold_db + over / ratio) - env_db;
                hard * x * x
            }
        }
    }

    pub fn process_sample(
        &mut self,
        main_sample: f32,
        sidechain_sample: f32,
        params: &ParamsSnapshot,
    ) -> (f32, f32) {
        let sc = if params.sc_highpass_on {
            self.highpass(sidechain_sample, params.sc_hpf_freq_hz)
        } else {
            sidechain_sample
        };

        let rect = sc.abs();
        let attack_coeff = self.coeff_for_ms(params.attack_ms);
        let release_coeff = self.coeff_for_ms(params.release_ms);

        if rect > self.envelope {
            self.envelope = attack_coeff * self.envelope + (1.0 - attack_coeff) * rect;
        } else {
            self.envelope = release_coeff * self.envelope + (1.0 - release_coeff) * rect;
        }

        let env_db = 20.0 * self.envelope.max(1e-6).log10();
        let gr_db =
            Self::gain_reduction_db(env_db, params.threshold_db, params.ratio, params.knee_db)
                .min(0.0);

        // Set quack intensity from params
        self.quack_gen.set_intensity(params.quack_intensity);

        // Hysteresis trigger: avoids rapid retriggers that sound like "bullets".
        if self.quack_armed && gr_db < -1.5 {
            self.quack_gen.trigger();
            self.quack_armed = false;
        } else if gr_db > -0.2 {
            self.quack_armed = true;
        }
        self.last_gr_db = gr_db;

        let gain = 10.0f32.powf(gr_db / 20.0);
        let makeup = 10.0f32.powf(params.makeup_db / 20.0);
        let wet = main_sample * gain * makeup;
        let dry_wet = (params.dry_wet_percent / 100.0).clamp(0.0, 1.0);
        let out = (main_sample * (1.0 - dry_wet)) + (wet * dry_wet);

        let quack_sample = self.quack_gen.process_sample();

        // Soft clip to prevent harsh spikes when quack + guitar sum exceeds 0 dBFS.
        let final_out = (out + quack_sample).tanh();

        (final_out, gr_db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> ParamsSnapshot {
        ParamsSnapshot {
            threshold_db: -30.0,
            ratio: 10.0,
            attack_ms: 1.0,
            release_ms: 100.0,
            knee_db: 0.0,
            makeup_db: 0.0,
            sc_highpass_on: false,
            sc_hpf_freq_hz: 80.0,
            dry_wet_percent: 100.0,
            quack_intensity: 1.0,
        }
    }

    #[test]
    fn ducks_when_sidechain_is_hot() {
        let mut dsp = DuckerDsp::new(48_000.0);
        let p = snapshot();
        let mut out = 0.0;
        for _ in 0..5000 {
            out = dsp.process_sample(1.0, 1.0, &p).0;
        }
        assert!(out < 0.8);
    }
}
