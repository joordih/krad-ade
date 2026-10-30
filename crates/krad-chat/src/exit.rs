use krad_domain::ChatEvent;

pub(crate) const STDERR_TAIL_CHARS: usize = 600;
const EXIT_PREFIX: &str = "the harness exited with";
const NO_STATUS: &str = "an unknown status";

pub(crate) fn closing_event(
    closed: bool,
    cancelled: bool,
    exit: Option<i32>,
    success: bool,
    stderr: &str,
) -> Option<ChatEvent> {
    if closed {
        return None;
    }
    if cancelled || success {
        return Some(ChatEvent::Done);
    }
    let tail = stderr.trim();
    let tail = &tail[tail
        .char_indices()
        .rev()
        .nth(STDERR_TAIL_CHARS)
        .map_or(0, |(at, _)| at)..];
    Some(ChatEvent::Error(if tail.is_empty() {
        exit.map_or_else(
            || format!("{EXIT_PREFIX} {NO_STATUS}"),
            |code| format!("{EXIT_PREFIX} code {code}"),
        )
    } else {
        tail.to_owned()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_closes_once() {
        assert_eq!(closing_event(true, false, Some(1), false, "boom"), None);
        assert_eq!(
            closing_event(false, false, Some(0), true, ""),
            Some(ChatEvent::Done)
        );
        assert_eq!(
            closing_event(false, true, None, false, "killed"),
            Some(ChatEvent::Done),
            "a cancelled turn just ends"
        );
    }

    #[test]
    fn failures_explain_themselves_with_stderr_or_the_exit_code() {
        assert_eq!(
            closing_event(false, false, Some(2), false, "  unknown flag --verbose\n"),
            Some(ChatEvent::Error("unknown flag --verbose".to_owned()))
        );
        assert_eq!(
            closing_event(false, false, Some(127), false, ""),
            Some(ChatEvent::Error(format!("{EXIT_PREFIX} code 127")))
        );
        let noisy = "x".repeat(STDERR_TAIL_CHARS * 2);
        let Some(ChatEvent::Error(text)) = closing_event(false, false, Some(1), false, &noisy)
        else {
            unreachable!("a failed turn reports an error");
        };
        assert!(text.chars().count() <= STDERR_TAIL_CHARS + 1);
    }
}
