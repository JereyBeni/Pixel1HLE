//! Hardcoded permanent 100 % + charging

pub struct BatteryMock {
    force_full: bool,
}

impl BatteryMock {
    pub fn new(force_full_and_charging: bool) -> Self {
        Self {
            force_full: force_full_and_charging,
        }
    }

    pub fn get_level(&self) -> i32 {
        if self.force_full {
            100
        } else {
            87
        }
    }

    pub fn is_charging(&self) -> bool {
        self.force_full
    }

    pub fn get_status(&self) -> i32 {
        if self.force_full {
            2
        } else {
            3
        }
    }
}
