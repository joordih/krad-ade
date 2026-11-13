use krad_domain::ToastKind;
use krad_protocol::{ErrorCode, Problem};

use super::EnvironmentShell;

const fn toast_kind(code: ErrorCode) -> ToastKind {
    match code {
        ErrorCode::Busy | ErrorCode::AlreadyAnswered => ToastKind::Warning,
        ErrorCode::Unsupported
        | ErrorCode::NotFound
        | ErrorCode::Invalid
        | ErrorCode::Unavailable
        | ErrorCode::Forbidden
        | ErrorCode::Failed
        | ErrorCode::Unknown => ToastKind::Error,
    }
}

impl EnvironmentShell {
    pub(super) fn problem_heard(&mut self, problem: Problem) {
        tracing::debug!(scope = ?problem.scope, code = ?problem.code, "the runtime reported a problem");
        self.push_toast(toast_kind(problem.code), problem.message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_the_user_can_wait_out_is_a_warning_and_the_rest_are_errors() {
        assert_eq!(toast_kind(ErrorCode::Busy), ToastKind::Warning);
        assert_eq!(toast_kind(ErrorCode::Unavailable), ToastKind::Error);
    }
}
