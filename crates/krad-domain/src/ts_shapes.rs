use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ts_rs::TS)]
pub struct SerdeSystemTime {
    pub secs_since_epoch: u64,
    pub nanos_since_epoch: u32,
}

#[derive(Serialize, Deserialize, ts_rs::TS)]
pub struct SerdeDuration {
    pub secs: u64,
    pub nanos: u32,
}
