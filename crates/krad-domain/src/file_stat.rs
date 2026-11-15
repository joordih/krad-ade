use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::EntryKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FileStat {
    pub kind: EntryKind,
    pub len: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<crate::ts_shapes::SerdeSystemTime>"))]
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum UploadExisting {
    #[default]
    Replace,
    Skip,
}
