use krad_domain::CloudRunnerId;
use krad_protocol::{read_frame, write_frame, Frame, Message};
use krad_runner::RuntimeServer;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufReader, BufWriter};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::watch::{pass_on, Watcher};
use super::Notice;
use crate::stopping::Stopping;

const RUNNER_THREAD: &str = "krad-web-push-runner";
const RETRY_AFTER: Duration = Duration::from_mins(1);

#[derive(Default)]
struct Lines {
    open: HashSet<CloudRunnerId>,
    tried: HashMap<CloudRunnerId, Instant>,
}

impl Lines {
    fn start(&mut self, runner: CloudRunnerId, now: Instant) -> bool {
        let recent = self
            .tried
            .get(&runner)
            .is_some_and(|tried| now.duration_since(*tried) < RETRY_AFTER);
        if recent || self.open.contains(&runner) {
            return false;
        }
        self.tried.insert(runner, now);
        self.open.insert(runner)
    }
}

#[derive(Clone)]
pub(super) struct Reached {
    lines: Arc<Mutex<Lines>>,
    stopping: Stopping,
}

fn send(output: &mut impl io::Write, message: Message) -> io::Result<()> {
    write_frame(output, &Frame::Message(Box::new(message)))
        .map_err(|error| io::Error::other(error.to_string()))?;
    output.flush()
}

fn follow(
    server: &RuntimeServer,
    runner: CloudRunnerId,
    sender: &SyncSender<Notice>,
    stopping: &Stopping,
) -> io::Result<()> {
    let line = futures::executor::block_on(server.handle.connect_runner(runner))
        .map_err(|_| io::Error::other("the runtime stopped"))?
        .map_err(|error| io::Error::other(error.to_string()))?;
    let stop = line.stop;
    let mut output = BufWriter::new(line.output);
    let mut input = BufReader::new(line.input);
    let followed = (|| {
        send(&mut output, Message::Hello(line.hello))?;
        let mut watcher = Watcher::runner(runner);
        while !stopping.stopped() {
            let frame =
                read_frame(&mut input).map_err(|error| io::Error::other(error.to_string()))?;
            match frame {
                Some(Frame::Message(message)) => match *message {
                    Message::Welcome(_) => send(&mut output, Message::Subscribe { since: None })?,
                    Message::Feed(feed) => {
                        if !pass_on(sender, watcher.take(feed)) {
                            return Ok(());
                        }
                    }
                    _ => {}
                },
                Some(_) => {}
                None => return Ok(()),
            }
        }
        Ok(())
    })();
    stop.stop();
    followed
}

impl Reached {
    pub(super) fn new(stopping: Stopping) -> Self {
        Self {
            lines: Arc::default(),
            stopping,
        }
    }

    fn lines(&self) -> MutexGuard<'_, Lines> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn reach(
        &self,
        server: &Arc<RuntimeServer>,
        runner: CloudRunnerId,
        sender: &SyncSender<Notice>,
    ) {
        let started = self.lines().start(runner, Instant::now());
        if !started {
            return;
        }
        let (reached, server, sender) = (self.clone(), Arc::clone(server), sender.clone());
        let spawned = thread::Builder::new()
            .name(RUNNER_THREAD.to_owned())
            .spawn(move || {
                if let Err(error) = follow(&server, runner, &sender, &reached.stopping) {
                    tracing::debug!(%error, "push notifications stopped following a cloud runner");
                }
                reached.lines().open.remove(&runner);
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "push notifications could not follow a cloud runner");
            self.lines().open.remove(&runner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_runner_is_followed_once_and_retried_only_after_a_while() {
        let mut lines = Lines::default();
        let runner = CloudRunnerId(1);
        let start = Instant::now();
        assert!(lines.start(runner, start));
        assert!(!lines.start(runner, start));
        lines.open.remove(&runner);
        assert!(!lines.start(runner, start + Duration::from_secs(5)));
        assert!(lines.start(runner, start + RETRY_AFTER));
    }
}
