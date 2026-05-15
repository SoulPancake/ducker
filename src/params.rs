use std::sync::atomic::{AtomicU32, Ordering};

pub const THRESHOLD_MIN: f32 = -60.0;
pub const THRESHOLD_MAX: f32 = 0.0;
pub const RATIO_MIN: f32 = 1.0;
pub const RATIO_MAX: f32 = 20.0;
pub const ATTACK_MIN: f32 = 0.1;
pub const ATTACK_MAX: f32 = 200.0;
pub const RELEASE_MIN: f32 = 10.0;
pub const RELEASE_MAX: f32 = 2000.0;
pub const KNEE_MIN: f32 = 0.0;
pub const KNEE_MAX: f32 = 12.0;
pub const MAKEUP_MIN: f32 = -12.0;
pub const MAKEUP_MAX: f32 = 12.0;
pub const HPF_MIN: f32 = 20.0;
pub const HPF_MAX: f32 = 500.0;
pub const MIX_MIN: f32 = 0.0;
pub const MIX_MAX: f32 = 100.0;

#[derive(Debug)]
pub struct Params {
    threshold_db: AtomicU32,
    ratio: AtomicU32,
    attack_ms: AtomicU32,
    release_ms: AtomicU32,
    knee_db: AtomicU32,
    makeup_db: AtomicU32,
    sc_highpass_on: AtomicU32,
    sc_hpf_freq_hz: AtomicU32,
    dry_wet_percent: AtomicU32,
    quack_intensity: AtomicU32,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamsSnapshot {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub knee_db: f32,
    pub makeup_db: f32,
    pub sc_highpass_on: bool,
    pub sc_hpf_freq_hz: f32,
    pub dry_wet_percent: f32,
    pub quack_intensity: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            threshold_db: AtomicU32::new((-30.0f32).to_bits()),
            ratio: AtomicU32::new((10.0f32).to_bits()),
            attack_ms: AtomicU32::new((1.0f32).to_bits()),
            release_ms: AtomicU32::new((120.0f32).to_bits()),
            knee_db: AtomicU32::new((1.0f32).to_bits()),
            makeup_db: AtomicU32::new((0.0f32).to_bits()),
            sc_highpass_on: AtomicU32::new(0),
            sc_hpf_freq_hz: AtomicU32::new((80.0f32).to_bits()),
            dry_wet_percent: AtomicU32::new((100.0f32).to_bits()),
            quack_intensity: AtomicU32::new((1.0f32).to_bits()),
        }
    }
}

impl Params {
    #[inline]
    fn set_f32(field: &AtomicU32, v: f32) {
        field.store(v.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    fn get_f32(field: &AtomicU32) -> f32 {
        f32::from_bits(field.load(Ordering::Relaxed))
    }

    pub fn set_threshold(&self, v: f32) {
        Self::set_f32(&self.threshold_db, v.clamp(THRESHOLD_MIN, THRESHOLD_MAX));
    }

    pub fn get_threshold(&self) -> f32 {
        Self::get_f32(&self.threshold_db)
    }

    pub fn set_ratio(&self, v: f32) {
        Self::set_f32(&self.ratio, v.clamp(RATIO_MIN, RATIO_MAX));
    }

    pub fn get_ratio(&self) -> f32 {
        Self::get_f32(&self.ratio)
    }

    pub fn set_attack_ms(&self, v: f32) {
        Self::set_f32(&self.attack_ms, v.clamp(ATTACK_MIN, ATTACK_MAX));
    }

    pub fn get_attack_ms(&self) -> f32 {
        Self::get_f32(&self.attack_ms)
    }

    pub fn set_release_ms(&self, v: f32) {
        Self::set_f32(&self.release_ms, v.clamp(RELEASE_MIN, RELEASE_MAX));
    }

    pub fn get_release_ms(&self) -> f32 {
        Self::get_f32(&self.release_ms)
    }

    pub fn set_knee_db(&self, v: f32) {
        Self::set_f32(&self.knee_db, v.clamp(KNEE_MIN, KNEE_MAX));
    }

    pub fn get_knee_db(&self) -> f32 {
        Self::get_f32(&self.knee_db)
    }

    pub fn set_makeup_db(&self, v: f32) {
        Self::set_f32(&self.makeup_db, v.clamp(MAKEUP_MIN, MAKEUP_MAX));
    }

    pub fn get_makeup_db(&self) -> f32 {
        Self::get_f32(&self.makeup_db)
    }

    pub fn set_sc_highpass_on(&self, v: bool) {
        self.sc_highpass_on.store(u32::from(v), Ordering::Relaxed);
    }

    pub fn get_sc_highpass_on(&self) -> bool {
        self.sc_highpass_on.load(Ordering::Relaxed) == 1
    }

    pub fn set_sc_hpf_freq_hz(&self, v: f32) {
        Self::set_f32(&self.sc_hpf_freq_hz, v.clamp(HPF_MIN, HPF_MAX));
    }

    pub fn get_sc_hpf_freq_hz(&self) -> f32 {
        Self::get_f32(&self.sc_hpf_freq_hz)
    }

    pub fn set_dry_wet_percent(&self, v: f32) {
        Self::set_f32(&self.dry_wet_percent, v.clamp(MIX_MIN, MIX_MAX));
    }

    pub fn get_dry_wet_percent(&self) -> f32 {
        Self::get_f32(&self.dry_wet_percent)
    }

    pub fn set_quack_intensity(&self, v: f32) {
        Self::set_f32(&self.quack_intensity, v.clamp(0.1, 10.0));
    }

    pub fn get_quack_intensity(&self) -> f32 {
        Self::get_f32(&self.quack_intensity)
    }

    pub fn snapshot(&self) -> ParamsSnapshot {
        ParamsSnapshot {
            threshold_db: self.get_threshold(),
            ratio: self.get_ratio(),
            attack_ms: self.get_attack_ms(),
            release_ms: self.get_release_ms(),
            knee_db: self.get_knee_db(),
            makeup_db: self.get_makeup_db(),
            sc_highpass_on: self.get_sc_highpass_on(),
            sc_hpf_freq_hz: self.get_sc_hpf_freq_hz(),
            dry_wet_percent: self.get_dry_wet_percent(),
            quack_intensity: self.get_quack_intensity(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_f32_in_atomics() {
        let p = Params::default();
        p.set_threshold(-13.5);
        p.set_ratio(8.0);
        p.set_sc_highpass_on(false);
        assert_eq!(p.get_threshold(), -13.5);
        assert_eq!(p.get_ratio(), 8.0);
        assert!(!p.get_sc_highpass_on());
    }
}
