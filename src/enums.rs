#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, FromPrimitive, ToPrimitive)]
pub enum LidarMode {
    LidarModeNormal = 1,
    LidarModePowerSaving = 2,
    LidarModeStandby = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, FromPrimitive, ToPrimitive)]
pub enum LidarState {
    LidarStateInit = 0,
    LidarStateNormal = 1,
    LidarStatePowerSaving = 2,
    LidarStateStandBy = 3,
    LidarStateError = 4,
    LidarStateUnknown = 5,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LidarStateMask {
    Init = 1,
    Normal = 2,
    PowerSaving = 4,
    StandBy = 8,
    Error = 16,
    Unknown = 32,
    Any = 0x1F,
}
