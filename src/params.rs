use std::sync::atomic::{AtomicU32, Ordering};

// Envelope filter parameters
pub const SENSITIVITY_MIN: f32 = 0.0;
pub const SENSITIVITY_MAX: f32 = 1.0;
pub const FREQ_MIN_MIN: f32 = 80.0;
pub const FREQ_MIN_MAX: f32 = 800.0;
pub const FREQ_MAX_MIN: f32 = 500.0;
pub const FREQ_MAX_MAX: f32 = 5000.0;
pub const RESONANCE_MIN: f32 = 0.5;
pub const RESONANCE_MAX: f32 = 8.0;
pub const ATTACK_MIN: f32 = 1.0;
pub const ATTACK_MAX: f32 = 200.0;
pub const RELEASE_MIN: f32 = 10.0;
pub const RELEASE_MAX: f32 = 500.0;
pub const COMP_THRESHOLD_MIN: f32 = -40.0;
pub const COMP_THRESHOLD_MAX: f32 = 0.0;
pub const COMP_RATIO_MIN: f32 = 1.0;
pub const COMP_RATIO_MAX: f32 = 20.0;
pub const MAKEUP_MIN: f32 = -12.0;
pub const MAKEUP_MAX: f32 = 12.0;
pub const MIX_MIN: f32 = 0.0;
pub const MIX_MAX: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FilterMode {
    LowPass = 0,
    BandPass = 1,
    HighPass = 2,
}

impl FilterMode {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::LowPass,
            1 => Self::BandPass,
            _ => Self::HighPass,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::BandPass => "BP",
            Self::HighPass => "HP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Direction {
    Up = 0,
    Down = 1,
}

impl Direction {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Up,
            _ => Self::Down,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
        }
    }
}

#[derive(Debug)]
pub struct Params {
    sensitivity: AtomicU32,
    freq_min_hz: AtomicU32,
    freq_max_hz: AtomicU32,
    resonance: AtomicU32,
    attack_ms: AtomicU32,
    release_ms: AtomicU32,
    filter_mode: AtomicU32,
    direction: AtomicU32,
    pre_comp_on: AtomicU32,
    comp_threshold_db: AtomicU32,
    comp_ratio: AtomicU32,
    makeup_db: AtomicU32,
    dry_wet_percent: AtomicU32,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamsSnapshot {
    pub sensitivity: f32,
    pub freq_min_hz: f32,
    pub freq_max_hz: f32,
    pub resonance: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub filter_mode: FilterMode,
    pub direction: Direction,
    pub pre_comp_on: bool,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub makeup_db: f32,
    pub dry_wet_percent: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            sensitivity: AtomicU32::new((0.65f32).to_bits()),
            freq_min_hz: AtomicU32::new((250.0f32).to_bits()),
            freq_max_hz: AtomicU32::new((2500.0f32).to_bits()),
            resonance: AtomicU32::new((3.5f32).to_bits()),
            attack_ms: AtomicU32::new((6.0f32).to_bits()),
            release_ms: AtomicU32::new((100.0f32).to_bits()),
            filter_mode: AtomicU32::new(FilterMode::BandPass as u32),
            direction: AtomicU32::new(Direction::Up as u32),
            pre_comp_on: AtomicU32::new(1),
            comp_threshold_db: AtomicU32::new((-16.0f32).to_bits()),
            comp_ratio: AtomicU32::new((8.0f32).to_bits()),
            makeup_db: AtomicU32::new((0.0f32).to_bits()),
            dry_wet_percent: AtomicU32::new((100.0f32).to_bits()),
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

    pub fn set_sensitivity(&self, v: f32) {
        Self::set_f32(&self.sensitivity, v.clamp(SENSITIVITY_MIN, SENSITIVITY_MAX));
    }
    pub fn get_sensitivity(&self) -> f32 {
        Self::get_f32(&self.sensitivity)
    }

    pub fn set_freq_min(&self, v: f32) {
        Self::set_f32(&self.freq_min_hz, v.clamp(FREQ_MIN_MIN, FREQ_MIN_MAX));
    }
    pub fn get_freq_min(&self) -> f32 {
        Self::get_f32(&self.freq_min_hz)
    }

    pub fn set_freq_max(&self, v: f32) {
        Self::set_f32(&self.freq_max_hz, v.clamp(FREQ_MAX_MIN, FREQ_MAX_MAX));
    }
    pub fn get_freq_max(&self) -> f32 {
        Self::get_f32(&self.freq_max_hz)
    }

    pub fn set_resonance(&self, v: f32) {
        Self::set_f32(&self.resonance, v.clamp(RESONANCE_MIN, RESONANCE_MAX));
    }
    pub fn get_resonance(&self) -> f32 {
        Self::get_f32(&self.resonance)
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

    pub fn set_filter_mode(&self, v: FilterMode) {
        self.filter_mode.store(v as u32, Ordering::Relaxed);
    }
    pub fn get_filter_mode(&self) -> FilterMode {
        FilterMode::from_u32(self.filter_mode.load(Ordering::Relaxed))
    }

    pub fn set_direction(&self, v: Direction) {
        self.direction.store(v as u32, Ordering::Relaxed);
    }
    pub fn get_direction(&self) -> Direction {
        Direction::from_u32(self.direction.load(Ordering::Relaxed))
    }

    pub fn set_pre_comp_on(&self, v: bool) {
        self.pre_comp_on.store(u32::from(v), Ordering::Relaxed);
    }
    pub fn get_pre_comp_on(&self) -> bool {
        self.pre_comp_on.load(Ordering::Relaxed) == 1
    }

    pub fn set_comp_threshold(&self, v: f32) {
        Self::set_f32(
            &self.comp_threshold_db,
            v.clamp(COMP_THRESHOLD_MIN, COMP_THRESHOLD_MAX),
        );
    }
    pub fn get_comp_threshold(&self) -> f32 {
        Self::get_f32(&self.comp_threshold_db)
    }

    pub fn set_comp_ratio(&self, v: f32) {
        Self::set_f32(&self.comp_ratio, v.clamp(COMP_RATIO_MIN, COMP_RATIO_MAX));
    }
    pub fn get_comp_ratio(&self) -> f32 {
        Self::get_f32(&self.comp_ratio)
    }

    pub fn set_makeup_db(&self, v: f32) {
        Self::set_f32(&self.makeup_db, v.clamp(MAKEUP_MIN, MAKEUP_MAX));
    }
    pub fn get_makeup_db(&self) -> f32 {
        Self::get_f32(&self.makeup_db)
    }

    pub fn set_dry_wet_percent(&self, v: f32) {
        Self::set_f32(&self.dry_wet_percent, v.clamp(MIX_MIN, MIX_MAX));
    }
    pub fn get_dry_wet_percent(&self) -> f32 {
        Self::get_f32(&self.dry_wet_percent)
    }

    pub fn snapshot(&self) -> ParamsSnapshot {
        ParamsSnapshot {
            sensitivity: self.get_sensitivity(),
            freq_min_hz: self.get_freq_min(),
            freq_max_hz: self.get_freq_max(),
            resonance: self.get_resonance(),
            attack_ms: self.get_attack_ms(),
            release_ms: self.get_release_ms(),
            filter_mode: self.get_filter_mode(),
            direction: self.get_direction(),
            pre_comp_on: self.get_pre_comp_on(),
            comp_threshold_db: self.get_comp_threshold(),
            comp_ratio: self.get_comp_ratio(),
            makeup_db: self.get_makeup_db(),
            dry_wet_percent: self.get_dry_wet_percent(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_f32_in_atomics() {
        let p = Params::default();
        p.set_sensitivity(0.5);
        p.set_resonance(4.0);
        p.set_pre_comp_on(false);
        assert_eq!(p.get_sensitivity(), 0.5);
        assert_eq!(p.get_resonance(), 4.0);
        assert!(!p.get_pre_comp_on());
    }

    #[test]
    fn filter_mode_round_trips() {
        let p = Params::default();
        p.set_filter_mode(FilterMode::HighPass);
        assert_eq!(p.get_filter_mode(), FilterMode::HighPass);
        p.set_filter_mode(FilterMode::LowPass);
        assert_eq!(p.get_filter_mode(), FilterMode::LowPass);
    }

    #[test]
    fn direction_round_trips() {
        let p = Params::default();
        p.set_direction(Direction::Down);
        assert_eq!(p.get_direction(), Direction::Down);
        p.set_direction(Direction::Up);
        assert_eq!(p.get_direction(), Direction::Up);
    }
}
