use krad_domain::{ChatEvent, ToolOutput, ToolUseId};
use std::time::{Duration, Instant};

use super::Worker;
use crate::acp::ids::TerminalId;
use crate::acp::terminals::TerminalOutputResponse;
use crate::control::LiveEvent;

pub(super) const TERMINAL_REFRESH: Duration = Duration::from_millis(250);
const SHOWN_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(super) struct Watched {
    tool: ToolUseId,
    terminal: TerminalId,
    shown: Option<ToolOutput>,
}

fn shown_output(snapshot: TerminalOutputResponse) -> ToolOutput {
    let mut text = snapshot.output;
    let cut = text.len() > SHOWN_OUTPUT_BYTES;
    if cut {
        let mut start = text.len() - SHOWN_OUTPUT_BYTES;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        text.drain(..start);
    }
    let exit = snapshot.exit_status.unwrap_or_default();
    ToolOutput {
        text,
        truncated: snapshot.truncated || cut,
        exit_code: exit.exit_code,
        signal: exit.signal,
    }
}

impl<S: FnMut(LiveEvent)> Worker<S> {
    pub(super) fn watch_new_terminals(&mut self) {
        let terminals = self.translator.take_terminals();
        self.watched
            .extend(terminals.into_iter().map(|(tool, terminal)| Watched {
                tool,
                terminal,
                shown: None,
            }));
    }

    pub(super) fn refresh_terminals(&mut self) {
        self.refreshed = Instant::now();
        let mut watched = std::mem::take(&mut self.watched);
        watched.retain_mut(|watched| {
            let Some(snapshot) = self.client_side.terminals.snapshot(&watched.terminal) else {
                return false;
            };
            let finished = snapshot.exit_status.is_some();
            let output = shown_output(snapshot);
            if watched.shown.as_ref() != Some(&output) {
                watched.shown = Some(output.clone());
                (self.sink)(LiveEvent::Chat(ChatEvent::ToolOutput {
                    tool: watched.tool.clone(),
                    output,
                }));
            }
            !finished
        });
        watched.append(&mut self.watched);
        self.watched = watched;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::terminals::TerminalExitStatus;

    #[test]
    fn long_output_keeps_its_tail_on_a_char_boundary() {
        let snapshot = TerminalOutputResponse {
            output: format!("é{}", "a".repeat(SHOWN_OUTPUT_BYTES - 1)),
            truncated: false,
            exit_status: Some(TerminalExitStatus {
                exit_code: Some(1),
                signal: None,
            }),
        };
        let output = shown_output(snapshot);
        assert!(output.truncated);
        assert_eq!(output.text.len(), SHOWN_OUTPUT_BYTES - 1);
        assert_eq!(output.exit_code, Some(1));
    }
}
