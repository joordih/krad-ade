use gpui_kit::App;
use krad_client::ClientError;
use krad_domain::{EnvironmentName, FileStat};
use krad_protocol::{Answer, Command, FileAsk, FileCommand, RequestId};
use std::future::Future;

use super::repo_asks::{picked, Pick};
use super::EnvironmentShell;

impl EnvironmentShell {
    pub(super) fn ask_file<T: 'static>(
        &self,
        environment: &EnvironmentName,
        ask: FileAsk,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        self.ask_file_as(
            self.runtime.client.next_request(),
            environment,
            ask,
            pick,
            cx,
        )
    }

    pub(super) fn ask_file_as<T: 'static>(
        &self,
        request: RequestId,
        environment: &EnvironmentName,
        ask: FileAsk,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        let command = Command::File(FileCommand {
            environment: environment.clone(),
            ask,
        });
        let asked = self.runtime.ask_as(request, command, cx);
        async move { picked(asked.await, pick) }
    }

    pub(super) fn ask_active_file<T: 'static>(
        &self,
        ask: FileAsk,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        self.ask_file(&self.active, ask, pick, cx)
    }
}

pub(super) struct Statted(pub(super) Option<FileStat>);

#[expect(
    clippy::needless_pass_by_value,
    reason = "a pick takes the answer it reads"
)]
pub(super) fn stat_of(answer: Answer) -> Option<Statted> {
    match answer {
        Answer::Stat { stat } | Answer::Written { stat } => Some(Statted(stat)),
        _ => None,
    }
}

pub(super) fn placed(answer: Answer) -> Option<std::path::PathBuf> {
    match answer {
        Answer::Placed { path } => Some(path),
        _ => None,
    }
}

pub(super) fn vault_path_of(answer: Answer) -> Option<String> {
    match answer {
        Answer::NoteAdded { name } => Some(name),
        _ => None,
    }
}
