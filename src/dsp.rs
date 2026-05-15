use crate::params::ParamsSnapshot;

#[derive(Debug)]
pub struct DuckerDsp {
    sample_rate: f32,
    envelope: f32,
    hp_x1: f32,
    hp_y1: f32,
}

impl DuckerDsp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            envelope: 0.0,
            hp_x1: 0.0,
            hp_y1: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
    }

    fn coeff_for_ms(&self, ms: f32) -> f32 {
        (-1.0 / ((ms.max(0.001) * 0.001) * self.sample_rate)).exp()
    }

    fn highpass(&mut self, sample: f32, cutoff_hz: f32) -> f32 {
        let coeff = (-2.0 * std::f32::consts::PI * cutoff_hz.max(1.0) / self.sample_rate).exp();
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

        let gain = 10.0f32.powf(gr_db / 20.0);
        let makeup = 10.0f32.powf(params.makeup_db / 20.0);
        let wet = main_sample * gain * makeup;
        let dry_wet = (params.dry_wet_percent / 100.0).clamp(0.0, 1.0);
        let out = (main_sample * (1.0 - dry_wet)) + (wet * dry_wet);

        (out, gr_db)
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
