use krad_domain::{AgentPreset, ModelId};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::command::{chat_command, Orchestration};
use crate::error::ChatError;
use crate::parse::parse_line;
use crate::process::{harness_command, Runner};
use krad_domain::ChatEvent;

pub(crate) const STDERR_TAIL_CHARS: usize = 600;
const EXIT_PREFIX: &str = "the harness exited with";
const NO_STATUS: &str = "an unknown status";

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub preset: AgentPreset,
    pub runner: Runner,
    pub model: Option<ModelId>,
    pub cwd: PathBuf,
    pub extra_dirs: Vec<PathBuf>,
    pub orchestration: Option<Orchestration>,
    pub prompt: String,
}

pub struct TurnHandle {
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
}

impl TurnHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                child.kill().unwrap_or_default();
            }
        }
    }
}

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

/// # Errors
pub fn spawn_turn(
    request: ChatRequest,
    mut sink: impl FnMut(ChatEvent) + Send + 'static,
) -> Result<TurnHandle, ChatError> {
    let orchestration = request.orchestration.as_ref();
    let command = chat_command(
        request.preset,
        request.model.as_ref(),
        &request.extra_dirs,
        orchestration.map(|orchestration| orchestration.cli.as_str()),
    )
    .ok_or(ChatError::Unsupported(request.preset))?;
    let mut env = command.env;
    env.extend(orchestration.map_or_default(|orchestration| orchestration.env.0.clone()));
    let mut child = harness_command(&request.runner, &request.cwd, &command.args, &env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let prompt = request.prompt;
    thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            stdin.write_all(prompt.as_bytes()).unwrap_or_default();
        }
    });
    let stderr_text = Arc::new(Mutex::new(String::new()));
    let stderr_reader = stderr.map(|mut stderr| {
        let text = Arc::clone(&stderr_text);
        thread::spawn(move || {
            let mut buffer = String::new();
            stderr.read_to_string(&mut buffer).unwrap_or_default();
            if let Ok(mut text) = text.lock() {
                *text = buffer;
            }
        })
    });
    let child = Arc::new(Mutex::new(Some(child)));
    let cancelled = Arc::new(AtomicBool::new(false));
    let handle = TurnHandle {
        child: Arc::clone(&child),
        cancelled: Arc::clone(&cancelled),
    };
    let preset = request.preset;
    thread::spawn(move || {
        let mut closed = false;
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if cancelled.load(Ordering::SeqCst) {
                    continue;
                }
                for event in parse_line(preset, &line) {
                    if closed {
                        break;
                    }
                    closed = matches!(event, ChatEvent::Done | ChatEvent::Error(_));
                    sink(event);
                }
            }
        }
        if let Some(reader) = stderr_reader {
            reader.join().unwrap_or_default();
        }
        let taken = child.lock().ok().and_then(|mut child| child.take());
        let status = taken.and_then(|mut child| child.wait().ok());
        let stderr = stderr_text.lock().map_or_default(|text| text.clone());
        if let Some(event) = closing_event(
            closed,
            cancelled.load(Ordering::SeqCst),
            status.and_then(|status| status.code()),
            status.is_some_and(|status| status.success()),
            &stderr,
        ) {
            sink(event);
        }
    });
    Ok(handle)
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
