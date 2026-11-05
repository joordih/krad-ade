#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusCode {
    Ok,
    Eof,
    NoSuchFile,
    PermissionDenied,
    Failure,
    BadMessage,
    OpUnsupported,
    Other(u32),
}

const STATUS_OK: u32 = 0;
const STATUS_EOF: u32 = 1;
const STATUS_NO_SUCH_FILE: u32 = 2;
const STATUS_PERMISSION_DENIED: u32 = 3;
const STATUS_FAILURE: u32 = 4;
const STATUS_BAD_MESSAGE: u32 = 5;
const STATUS_OP_UNSUPPORTED: u32 = 8;

impl From<u32> for StatusCode {
    fn from(code: u32) -> Self {
        match code {
            STATUS_OK => Self::Ok,
            STATUS_EOF => Self::Eof,
            STATUS_NO_SUCH_FILE => Self::NoSuchFile,
            STATUS_PERMISSION_DENIED => Self::PermissionDenied,
            STATUS_FAILURE => Self::Failure,
            STATUS_BAD_MESSAGE => Self::BadMessage,
            STATUS_OP_UNSUPPORTED => Self::OpUnsupported,
            other => Self::Other(other),
        }
    }
}

impl From<StatusCode> for u32 {
    fn from(code: StatusCode) -> Self {
        match code {
            StatusCode::Ok => STATUS_OK,
            StatusCode::Eof => STATUS_EOF,
            StatusCode::NoSuchFile => STATUS_NO_SUCH_FILE,
            StatusCode::PermissionDenied => STATUS_PERMISSION_DENIED,
            StatusCode::Failure => STATUS_FAILURE,
            StatusCode::BadMessage => STATUS_BAD_MESSAGE,
            StatusCode::OpUnsupported => STATUS_OP_UNSUPPORTED,
            StatusCode::Other(other) => other,
        }
    }
}
