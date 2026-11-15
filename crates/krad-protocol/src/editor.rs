use krad_domain::{BufferVersion, EnvironmentName, TextEdit};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ids::RequestId;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(transparent)]
pub struct BufferId(pub u64);

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum EditorCommand {
    Open {
        environment: EnvironmentName,
        path: PathBuf,
    },
    Edit {
        buffer: BufferId,
        based_on: BufferVersion,
        edit: TextEdit,
    },
    Save {
        buffer: BufferId,
    },
    Close {
        buffer: BufferId,
    },
    #[cfg_attr(feature = "ts", ts(skip))]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenedBuffer {
    pub buffer: BufferId,
    pub environment: EnvironmentName,
    pub path: PathBuf,
    pub version: BufferVersion,
    pub saved: BufferVersion,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BufferEdited {
    pub buffer: BufferId,
    pub version: BufferVersion,
    pub edit: TextEdit,
    pub author: RequestId,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BufferNotOpened {
    pub request: RequestId,
    pub environment: EnvironmentName,
    pub path: PathBuf,
    pub message: String,
}
