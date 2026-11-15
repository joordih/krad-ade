use futures::StreamExt as _;
use krad_domain::{GridSize, ProcessRun};
use krad_protocol::{AgentRef, Command, Reply, TerminalCommand, WriteBy};
use krad_runtime::{TerminalAction, TerminalFrame, TerminalStatus, TerminalSurface, TerminalView};
use krad_session::{
    AnsiPalette, CellPixels, MouseAction, VtEngine, WheelInput, DEFAULT_COLS, DEFAULT_ROWS,
};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread;

use crate::client::{Answer, RuntimeClient, TerminalBytesFeed};

const MIRROR_THREAD: &str = "krad-terminal-mirror";
const FEED_THREAD: &str = "krad-terminal-mirror-feed";
const INBOX_CAPACITY: usize = 1024;

#[derive(Debug, thiserror::Error)]
#[error("a mirror thread did not start")]
pub struct MirrorError(#[from] std::io::Error);

enum Mirrored {
    Bytes(Vec<u8>),
    Act(TerminalAction),
    Resize(GridSize),
    Recolor(AnsiPalette),
    Run(ProcessRun),
    Wake,
}

pub struct TerminalMirror {
    inbox: SyncSender<Mirrored>,
    view: TerminalView,
}

struct Watched {
    terminal: AgentRef,
    client: Arc<dyn RuntimeClient>,
    surface: TerminalSurface,
    palette: AnsiPalette,
}

impl Watched {
    fn mirror(self) -> Option<Mirror> {
        let engine = VtEngine::new(usize::from(DEFAULT_COLS), usize::from(DEFAULT_ROWS))
            .inspect_err(|error| tracing::warn!(%error, "no emulator for a mirrored terminal"))
            .ok()?;
        Some(Mirror {
            terminal: self.terminal,
            client: self.client,
            engine,
            surface: self.surface,
            palette: self.palette,
            status: TerminalStatus::default(),
            grid: None,
            dirty: true,
        })
    }
}

struct Mirror {
    terminal: AgentRef,
    client: Arc<dyn RuntimeClient>,
    engine: VtEngine,
    surface: TerminalSurface,
    palette: AnsiPalette,
    status: TerminalStatus,
    grid: Option<GridSize>,
    dirty: bool,
}

fn answer<T>(reply: futures::channel::oneshot::Sender<T>, value: T) {
    if reply.send(value).is_err() {
        tracing::trace!("a client left before the mirrored terminal answered");
    }
}

impl Mirror {
    fn write(&mut self, bytes: Vec<u8>, by: WriteBy) {
        if by == WriteBy::User {
            self.engine.scroll_to_bottom();
            self.dirty = true;
        }
        self.client.post(Command::Terminal(TerminalCommand::Write {
            terminal: self.terminal.clone(),
            bytes,
            by,
        }));
    }

    fn hear(&mut self, bytes: &[u8]) {
        self.engine.push(bytes);
        drop(self.engine.take_replies());
        self.dirty = true;
    }

    fn resize(&mut self, size: GridSize) {
        if self.grid.replace(size) == Some(size) {
            return;
        }
        let cell = CellPixels {
            width: u32::from(size.cell_width_px),
            height: u32::from(size.cell_height_px),
        };
        self.engine
            .resize(usize::from(size.cols), usize::from(size.rows), cell);
        self.dirty = true;
        self.client.post(Command::Terminal(TerminalCommand::Resize {
            terminal: self.terminal.clone(),
            size,
        }));
    }

    fn act(&mut self, action: TerminalAction) {
        let engine = &mut self.engine;
        match action {
            TerminalAction::Key { key, text, held } => {
                if let Some(bytes) = engine.encode_key(&key, text.as_deref(), held) {
                    self.write(bytes, WriteBy::User);
                }
                return;
            }
            TerminalAction::Wheel { lines, at } => match engine.wheel(lines, at) {
                WheelInput::History(lines) => engine.scroll_lines(lines),
                WheelInput::Program(bytes) => return self.write(bytes, WriteBy::App),
            },
            TerminalAction::Press { at, report: true } => {
                if let Some(bytes) = engine.click_report(MouseAction::Press, at) {
                    engine.clear_selection();
                    self.dirty = true;
                    return self.write(bytes, WriteBy::App);
                }
            }
            TerminalAction::Press { at, report: false } => engine.start_selection(at),
            TerminalAction::Mouse { action, at } => {
                if let Some(bytes) = engine.click_report(action, at) {
                    return self.write(bytes, WriteBy::App);
                }
            }
            TerminalAction::ExtendSelection(at) => engine.extend_selection(at),
            TerminalAction::ClearSelection => engine.clear_selection(),
            TerminalAction::Search {
                query,
                direction,
                restart,
                found,
            } => {
                if restart {
                    engine.clear_selection();
                }
                let hit = !query.is_empty() && engine.search(&query, direction);
                answer(found, hit);
            }
            TerminalAction::CopySelection(text) => answer(text, engine.selection_text()),
            TerminalAction::RecentLines { lines, tail } => {
                answer(tail, Some(engine.recent_lines(lines)));
            }
            TerminalAction::Hooked => self.status.hooked = true,
        }
        self.dirty = true;
    }

    fn take(&mut self, mirrored: Mirrored) {
        match mirrored {
            Mirrored::Bytes(bytes) => self.hear(&bytes),
            Mirrored::Act(action) => self.act(action),
            Mirrored::Resize(size) => self.resize(size),
            Mirrored::Recolor(palette) => {
                self.palette = palette;
                self.dirty = true;
            }
            Mirrored::Run(run) => self.status.run = run,
            Mirrored::Wake => {}
        }
    }

    fn publish(&mut self) {
        let status = TerminalStatus {
            bracketed_paste: self.engine.bracketed_paste(),
            mouse_tracking: self.engine.mouse_tracking(),
            ..self.status.clone()
        };
        if status != self.status {
            self.status = status;
        }
        self.surface.set_status(self.status.clone());
        if !self.dirty {
            return;
        }
        if !self.surface.is_wanted() {
            self.surface.mark_stale();
            return;
        }
        self.surface.publish(TerminalFrame {
            screen: self.engine.screen(&self.palette),
            images: HashMap::new(),
            cell: Some(self.engine.cell_pixels()),
        });
        self.dirty = false;
    }

    fn run(mut self, inbox: &Receiver<Mirrored>) {
        while let Ok(first) = inbox.recv() {
            self.take(first);
            loop {
                match inbox.try_recv() {
                    Ok(next) => self.take(next),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return self.publish(),
                }
            }
            self.publish();
        }
    }
}

fn forward(feed: TerminalBytesFeed, inbox: &SyncSender<Mirrored>) {
    futures::executor::block_on(feed.for_each(|bytes| {
        if inbox.send(Mirrored::Bytes(bytes)).is_err() {
            tracing::trace!("a mirrored terminal closed while bytes were arriving");
        }
        futures::future::ready(())
    }));
}

impl TerminalMirror {
    /// # Errors
    ///
    /// Fails when its threads cannot start.
    pub fn watch(
        client: Arc<dyn RuntimeClient>,
        terminal: AgentRef,
        palette: AnsiPalette,
    ) -> Result<(Self, Answer<Reply>), MirrorError> {
        let (inbox, mirrored) = mpsc::sync_channel(INBOX_CAPACITY);
        let waker = inbox.clone();
        let (surface, view) = TerminalSurface::new(move || {
            if waker.try_send(Mirrored::Wake).is_err() {
                tracing::trace!("a mirrored terminal is busy; it renders on its next batch");
            }
        });
        let (watched, feed) = client.watch_terminal(terminal.clone());
        let watched_terminal = Watched {
            terminal,
            client,
            surface,
            palette,
        };
        thread::Builder::new()
            .name(MIRROR_THREAD.to_owned())
            .spawn(move || {
                if let Some(mirror) = watched_terminal.mirror() {
                    mirror.run(&mirrored);
                }
            })?;
        let bytes = inbox.clone();
        thread::Builder::new()
            .name(FEED_THREAD.to_owned())
            .spawn(move || forward(feed, &bytes))?;
        Ok((Self { inbox, view }, watched))
    }

    #[must_use]
    pub fn view(&self) -> TerminalView {
        self.view.clone()
    }

    fn tell(&self, mirrored: Mirrored) {
        if self.inbox.send(mirrored).is_err() {
            tracing::debug!("a mirrored terminal had already stopped");
        }
    }

    pub fn act(&self, action: TerminalAction) {
        self.tell(Mirrored::Act(action));
    }

    pub fn resize(&self, size: GridSize) {
        self.tell(Mirrored::Resize(size));
    }

    pub fn recolor(&self, palette: AnsiPalette) {
        self.tell(Mirrored::Recolor(palette));
    }

    pub fn hear_run(&self, run: ProcessRun) {
        self.tell(Mirrored::Run(run));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc as feed;
    use futures::FutureExt as _;
    use krad_domain::{EnvironmentName, TerminalId};
    use krad_protocol::{Feed, Hello, RequestId, Scope, Seq, Snapshot, Welcome};
    use krad_session::{KeyModifiers, XTERM_PALETTE};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use crate::client::{ClientError, Feeds};

    const PATIENCE: Duration = Duration::from_secs(5);

    struct Recording {
        posted: Mutex<Vec<Command>>,
        bytes: Mutex<Option<feed::UnboundedReceiver<Vec<u8>>>>,
    }

    impl RuntimeClient for Recording {
        fn hello(&self, _: Hello) -> Answer<Welcome> {
            futures::future::ready(Err(ClientError::Stopped)).boxed()
        }

        fn next_request(&self) -> RequestId {
            RequestId(1)
        }

        fn send_as(&self, _: RequestId, command: Command) -> Answer<Reply> {
            self.posted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(command);
            futures::future::ready(Ok(Reply::Accepted)).boxed()
        }

        fn subscribe(&self, _: Option<Seq>) -> Feeds {
            futures::stream::empty::<Feed>().boxed()
        }

        fn snapshot(&self, _: Scope) -> Answer<Snapshot> {
            futures::future::ready(Err(ClientError::Stopped)).boxed()
        }

        fn watch_terminal(&self, _: AgentRef) -> (Answer<Reply>, TerminalBytesFeed) {
            let bytes = self
                .bytes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let feed = match bytes {
                Some(bytes) => bytes.boxed(),
                None => futures::stream::empty().boxed(),
            };
            (futures::future::ready(Ok(Reply::Accepted)).boxed(), feed)
        }
    }

    fn terminal() -> AgentRef {
        AgentRef {
            environment: EnvironmentName::new("this-computer".to_owned()),
            terminal: TerminalId(3),
        }
    }

    fn text(frame: &TerminalFrame) -> Vec<String> {
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

    fn eventually(mut done: impl FnMut() -> bool) -> bool {
        let until = Instant::now() + PATIENCE;
        while Instant::now() < until {
            if done() {
                return true;
            }
            std::thread::yield_now();
        }
        false
    }

    #[test]
    fn watched_bytes_paint_the_view_and_keys_go_back_as_writes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (sender, receiver) = feed::unbounded();
        let client = Arc::new(Recording {
            posted: Mutex::new(Vec::new()),
            bytes: Mutex::new(Some(receiver)),
        });
        let (mirror, _) = TerminalMirror::watch(client.clone(), terminal(), XTERM_PALETTE)?;
        let view = mirror.view();
        sender.unbounded_send(b"$ ls\r\na.txt".to_vec())?;
        assert!(eventually(|| text(&view.paint_frame())
            .first()
            .is_some_and(|line| line == "$ ls")));
        assert_eq!(text(&view.frame())[1], "a.txt");
        mirror.act(TerminalAction::Key {
            key: "a".to_owned(),
            text: Some("a".to_owned()),
            held: KeyModifiers::default(),
        });
        let wrote_a = |command: &Command| matches!(command, Command::Terminal(TerminalCommand::Write { bytes, by: WriteBy::User, .. }) if bytes == b"a");
        assert!(eventually(|| client
            .posted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(wrote_a)));
        Ok(())
    }

    #[test]
    fn a_new_grid_resizes_the_remote_terminal_once() -> Result<(), Box<dyn std::error::Error>> {
        let client = Arc::new(Recording {
            posted: Mutex::new(Vec::new()),
            bytes: Mutex::new(None),
        });
        let (mirror, _) = TerminalMirror::watch(client.clone(), terminal(), XTERM_PALETTE)?;
        let size = GridSize {
            cols: 40,
            rows: 10,
            cell_width_px: 8,
            cell_height_px: 18,
        };
        mirror.resize(size);
        mirror.resize(size);
        let resizes = || {
            client
                .posted
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .filter(|command| {
                    matches!(command, Command::Terminal(TerminalCommand::Resize { .. }))
                })
                .count()
        };
        assert!(eventually(|| resizes() == 1));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(resizes(), 1);
        Ok(())
    }
}
