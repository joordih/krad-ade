use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::EntryKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStat {
    pub kind: EntryKind,
    pub len: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadExisting {
    #[default]
    Replace,
    Skip,
}
