use crate::protocol::ErrorCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
    Ok,
    Usage,
    Bus,
    TimedOut,
    NotFound,
    Refused,
    UnsupportedApi,
    Failed,
}

impl Exit {
    pub(crate) const ALL: [Self; 8] = [
        Self::Ok,
        Self::Usage,
        Self::Bus,
        Self::TimedOut,
        Self::NotFound,
        Self::Refused,
        Self::UnsupportedApi,
        Self::Failed,
    ];

    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::Ok => 0,
            Self::Usage => 1,
            Self::Bus => 2,
            Self::TimedOut => 3,
            Self::NotFound => 4,
            Self::Refused => 5,
            Self::UnsupportedApi => 6,
            Self::Failed => 7,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Ok => "done",
            Self::Usage => "wrong arguments",
            Self::Bus => "the app cannot be reached",
            Self::TimedOut => "timed out",
            Self::NotFound => "no such agent, session or file",
            Self::Refused => "refused for this caller",
            Self::UnsupportedApi => "the app speaks another API version",
            Self::Failed => "the app could not do it",
        }
    }
}

impl From<ErrorCode> for Exit {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::BadRequest => Self::Usage,
            ErrorCode::Unavailable | ErrorCode::SlowSubscriber => Self::Bus,
            ErrorCode::NotFound => Self::NotFound,
            ErrorCode::Unauthorized
            | ErrorCode::Forbidden
            | ErrorCode::Unsupported
            | ErrorCode::NotOwned
            | ErrorCode::Busy
            | ErrorCode::OutOfReach => Self::Refused,
            ErrorCode::UnsupportedApiVersion => Self::UnsupportedApi,
            ErrorCode::Failed | ErrorCode::WorktreeNotClean => Self::Failed,
        }
    }
}

pub(crate) fn exit_codes_help() -> String {
    Exit::ALL
        .iter()
        .map(|exit| format!("  {}  {}", exit.code(), exit.label()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable_and_distinct() {
        let codes: Vec<i32> = Exit::ALL.iter().map(|exit| exit.code()).collect();
        assert_eq!(codes, (0..8).collect::<Vec<_>>());
        assert_eq!(Exit::from(ErrorCode::NotFound), Exit::NotFound);
        assert_eq!(Exit::from(ErrorCode::Unauthorized), Exit::Refused);
        assert_eq!(
            Exit::from(ErrorCode::UnsupportedApiVersion),
            Exit::UnsupportedApi
        );
        assert_eq!(Exit::from(ErrorCode::Unavailable), Exit::Bus);
        assert_eq!(Exit::from(ErrorCode::SlowSubscriber), Exit::Bus);
        assert_eq!(Exit::from(ErrorCode::NotOwned), Exit::Refused);
        assert_eq!(Exit::from(ErrorCode::Busy), Exit::Refused);
        assert_eq!(Exit::from(ErrorCode::OutOfReach), Exit::Refused);
        assert_eq!(Exit::from(ErrorCode::WorktreeNotClean), Exit::Failed);
        assert!(exit_codes_help().contains("  4  no such agent, session or file"));
    }
}
