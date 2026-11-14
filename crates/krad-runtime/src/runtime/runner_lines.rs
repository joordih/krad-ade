use futures::channel::oneshot;
use krad_domain::CloudRunnerId;
use krad_protocol::Hello;

use super::Runtime;
use crate::runners::{open_runner_line, RunnerLine, RunnerLineError};

const BUILD: &str = env!("CARGO_PKG_VERSION");

type LineReply = oneshot::Sender<Result<RunnerLine, RunnerLineError>>;

fn hand(reply: LineReply, line: Result<RunnerLine, RunnerLineError>) {
    if reply.send(line).is_err() {
        tracing::debug!("a runner connection was opened for a caller that left");
    }
}

impl Runtime {
    #[tracing::instrument(skip_all, fields(%runner))]
    pub(super) fn open_runner_line(&mut self, runner: CloudRunnerId, reply: LineReply) {
        let Some(ready) = self.runners.directory.get(runner) else {
            return hand(reply, Err(RunnerLineError::NotReady));
        };
        let hello = Hello::paired(BUILD, ready.pairing);
        let (link, cli) = (ready.link, ready.cli);
        let spawned = self.jobs.run_reporting(move |_| {
            hand(reply, open_runner_line(&link, &cli, hello));
        });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start a runner connection");
        }
    }
}
