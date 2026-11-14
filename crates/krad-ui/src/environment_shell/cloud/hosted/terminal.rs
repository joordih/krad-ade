use krad_domain::{CloudRunnerId, EnvironmentData, GridSize, NodeKind, TerminalId};
use krad_protocol::AgentRef;
use krad_runtime::TerminalFrame;
use krad_session::{
    AnsiPalette, CellPixels, CellPoint, KeyModifiers, VtEngine, WheelInput, DEFAULT_COLS,
    DEFAULT_ROWS,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct HostedTerminalKey {
    pub(in crate::environment_shell) runner: CloudRunnerId,
    pub(in crate::environment_shell) terminal: AgentRef,
}

pub(in crate::environment_shell) struct RemoteTerminal {
    pub(in crate::environment_shell) key: HostedTerminalKey,
    engine: VtEngine,
    frame: Arc<TerminalFrame>,
    grid: Option<GridSize>,
    pub(in crate::environment_shell) ended: bool,
}

impl RemoteTerminal {
    pub(in crate::environment_shell) fn new(key: HostedTerminalKey) -> Option<Self> {
        let engine = VtEngine::new(usize::from(DEFAULT_COLS), usize::from(DEFAULT_ROWS))
            .inspect_err(|error| tracing::warn!(%error, "no emulator for a remote terminal"))
            .ok()?;
        Some(Self {
            key,
            engine,
            frame: Arc::new(TerminalFrame::default()),
            grid: None,
            ended: false,
        })
    }

    pub(in crate::environment_shell) fn hear(&mut self, bytes: &[u8], palette: &AnsiPalette) {
        self.engine.push(bytes);
        drop(self.engine.take_replies());
        self.frame = Arc::new(TerminalFrame {
            screen: self.engine.screen(palette),
            images: HashMap::new(),
            cell: None,
        });
    }

    pub(in crate::environment_shell) fn frame(&self) -> Arc<TerminalFrame> {
        Arc::clone(&self.frame)
    }

    pub(in crate::environment_shell) fn key_bytes(
        &mut self,
        key: &str,
        text: Option<&str>,
        held: KeyModifiers,
    ) -> Option<Vec<u8>> {
        self.engine.encode_key(key, text, held)
    }

    pub(in crate::environment_shell) fn wheel(
        &mut self,
        (lines, at): (i32, CellPoint),
        palette: &AnsiPalette,
    ) -> Option<Vec<u8>> {
        match self.engine.wheel(lines, at) {
            WheelInput::History(lines) => {
                self.engine.scroll_lines(lines);
                self.hear(&[], palette);
                None
            }
            WheelInput::Program(bytes) => Some(bytes),
        }
    }

    pub(in crate::environment_shell) fn regrid(
        &mut self,
        size: GridSize,
        palette: &AnsiPalette,
    ) -> bool {
        if self.grid.replace(size) == Some(size) {
            return false;
        }
        let cell = CellPixels {
            width: u32::from(size.cell_width_px),
            height: u32::from(size.cell_height_px),
        };
        self.engine
            .resize(usize::from(size.cols), usize::from(size.rows), cell);
        self.hear(&[], palette);
        true
    }
}

pub(in crate::environment_shell) fn session_terminals(
    data: &EnvironmentData,
    beside: TerminalId,
) -> Vec<TerminalId> {
    let Some(session) = data.session_of(beside) else {
        return Vec::new();
    };
    data.nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Terminal && node.harness.is_none())
        .filter(|node| data.session_of(node.id) == Some(session))
        .map(|node| node.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::{
        AgentPreset, AgentState, ChatBookmarks, ChatCheckpoints, ChatChoice, ChatUsage,
        EnvironmentName, GroupId, NodeEdit, NodePlace, SessionSpec, TerminalNode, TextScale,
    };
    use krad_session::XTERM_PALETTE;
    use std::path::PathBuf;

    fn screen_text(frame: &TerminalFrame) -> Vec<String> {
        frame
            .screen
            .rows
            .iter()
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    fn node(id: TerminalId, group: GroupId, harness: Option<AgentPreset>) -> TerminalNode {
        TerminalNode {
            id,
            title: "node".to_owned(),
            state: AgentState::Idle,
            harness,
            resume: None,
            group: Some(group),
            text_scale: TextScale::default(),
            kind: if harness.is_some() {
                NodeKind::Chat
            } else {
                NodeKind::Terminal
            },
            file: None,
            extension: None,
            bookmarks: ChatBookmarks::default(),
            usage: ChatUsage::default(),
            checkpoints: ChatCheckpoints::default(),
            choice: ChatChoice::default(),
            cloud: None,
        }
    }

    fn open(
        data: &mut EnvironmentData,
        group: GroupId,
        harness: Option<AgentPreset>,
    ) -> TerminalId {
        let id = TerminalId(data.next_id);
        let opened = NodeEdit::Open {
            node: Box::new(node(id, group, harness)),
            place: NodePlace::Tab { anchor: None },
        };
        data.edit(&opened.into());
        id
    }

    #[test]
    fn a_hosted_chat_offers_the_shell_terminals_of_its_own_session_only() {
        let mut data = EnvironmentData::empty(EnvironmentName::new("runner".to_owned()));
        let group = data.add_group(PathBuf::from("/runner/repos/api"), None);
        data.enter_sessions();
        data.new_session(Some(group), SessionSpec::default());
        let chat = open(&mut data, group, Some(AgentPreset::ClaudeCode));
        let shell = open(&mut data, group, None);
        data.new_session(Some(group), SessionSpec::default());
        let elsewhere = open(&mut data, group, None);
        let offered = session_terminals(&data, chat);
        assert_eq!(offered, [shell]);
        assert!(!offered.contains(&elsewhere));
    }

    fn remote() -> RemoteTerminal {
        let key = HostedTerminalKey {
            runner: CloudRunnerId(1),
            terminal: AgentRef {
                environment: EnvironmentName::new("runner".to_owned()),
                terminal: TerminalId(4),
            },
        };
        RemoteTerminal::new(key).unwrap_or_else(|| unreachable!("the emulator starts"))
    }

    #[test]
    fn the_watched_bytes_become_the_screen_the_desktop_paints() {
        let mut terminal = remote();
        terminal.hear(b"$ ls\r\n\x1b[1ma.txt\x1b[0m", &XTERM_PALETTE);
        terminal.hear(b"  b.txt\r\n$ ", &XTERM_PALETTE);
        let text = screen_text(&terminal.frame());
        assert_eq!(text[0], "$ ls");
        assert_eq!(text[1], "a.txt  b.txt");
        assert_eq!(text[2], "$");
    }

    #[test]
    fn a_replay_that_starts_with_a_full_redraw_paints_only_the_redraw() {
        let mut terminal = remote();
        terminal.hear(b"old line\r\n", &XTERM_PALETTE);
        terminal.hear(b"\x1b[H\x1b[2Jfresh", &XTERM_PALETTE);
        let text = screen_text(&terminal.frame());
        assert_eq!(text[0], "fresh");
        assert!(text.iter().all(|line| !line.contains("old")));
    }

    #[test]
    fn a_key_is_encoded_for_the_runner_and_a_new_grid_is_reported_once() {
        let mut terminal = remote();
        let held = KeyModifiers::default();
        assert_eq!(
            terminal.key_bytes("a", Some("a"), held),
            Some(b"a".to_vec())
        );
        let size = GridSize {
            cols: 40,
            rows: 10,
            cell_width_px: 8,
            cell_height_px: 18,
        };
        assert!(terminal.regrid(size, &XTERM_PALETTE));
        assert!(!terminal.regrid(size, &XTERM_PALETTE));
    }
}
