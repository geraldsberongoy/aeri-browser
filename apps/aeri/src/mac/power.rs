//! Whether the Mac is saving power or running hot, for "Save power in Low
//! Power Mode" (Search `Motion.light`: Low Power Mode is on, or the thermal
//! state is Serious or worse).

use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};

pub fn saving_power() -> bool {
    let info = NSProcessInfo::processInfo();
    info.isLowPowerModeEnabled() || info.thermalState().0 >= NSProcessInfoThermalState::Serious.0
}
