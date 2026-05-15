use std::time::{Duration, Instant};

const PEAK_FALL_AMOUNT_DB: f32 = 30.0;
const PEAK_FALL_TIME_SECONDS: f32 = 0.3;

#[derive(Clone, Copy, Debug, Default)]
pub struct MeterData {
    pub input_peak_db: f32,
    pub sidechain_peak_db: f32,
    pub gain_reduction_db: f32,
    pub output_peak_db: f32,
}

#[derive(Debug)]
pub struct PeakHold {
    held_db: f32,
    hold_until: Instant,
    last_update: Instant,
}

impl PeakHold {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            held_db: -60.0,
            hold_until: now,
            last_update: now,
        }
    }

    pub fn update(&mut self, db: f32) {
        let now = Instant::now();
        let v = db.clamp(-60.0, 3.0);
        if v >= self.held_db {
            self.held_db = v;
            self.hold_until = now + Duration::from_secs(1);
        } else if now > self.hold_until {
            let elapsed = (now - self.last_update).as_secs_f32();
            let fall_per_second = PEAK_FALL_AMOUNT_DB / PEAK_FALL_TIME_SECONDS;
            self.held_db = (self.held_db - (fall_per_second * elapsed)).max(v);
        }
        self.last_update = now;
    }

    pub fn value(&self) -> f32 {
        self.held_db
    }
}

impl Default for PeakHold {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_hold_tracks_max() {
        let mut hold = PeakHold::new();
        hold.update(-18.0);
        hold.update(-6.0);
        assert_eq!(hold.value(), -6.0);
    }
}
