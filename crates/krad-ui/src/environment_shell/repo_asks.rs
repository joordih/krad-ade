use gpui_kit::App;
use krad_client::ClientError;
use krad_domain::{EnvironmentName, ProjectTask};
use krad_protocol::{Answer, Command, ErrorCode, HostCommand, RepoAsk, RepoCommand};
use std::future::Future;

use super::EnvironmentShell;

const UNEXPECTED_ANSWER: &str = "the runtime answered something else";

pub(super) type Pick<T> = fn(Answer) -> Option<T>;

pub(super) fn picked<T>(
    answer: Result<Answer, ClientError>,
    pick: Pick<T>,
) -> Result<T, ClientError> {
    pick(answer?).ok_or_else(|| ClientError::Failed {
        code: ErrorCode::Unknown,
        message: UNEXPECTED_ANSWER.to_owned(),
    })
}

impl EnvironmentShell {
    pub(super) fn ask_repo<T: 'static>(
        &self,
        environment: &EnvironmentName,
        ask: RepoAsk,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        let command = Command::Repo(RepoCommand {
            environment: environment.clone(),
            ask,
        });
        let asked = self.runtime.ask(command, cx);
        async move { picked(asked.await, pick) }
    }

    pub(super) fn ask_active_repo<T: 'static>(
        &self,
        ask: RepoAsk,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        self.ask_repo(&self.active, ask, pick, cx)
    }

    pub(super) fn ask_host_query<T: 'static>(
        &self,
        ask: HostCommand,
        pick: Pick<T>,
        cx: &mut App,
    ) -> impl Future<Output = Result<T, ClientError>> + 'static {
        let asked = self.runtime.ask(Command::Host(ask), cx);
        async move { picked(asked.await, pick) }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a pick takes the answer it reads"
)]
pub(super) fn done(answer: Answer) -> Option<()> {
    matches!(answer, Answer::Done).then_some(())
}

pub(super) fn tasks_of(answer: Answer) -> Option<Vec<ProjectTask>> {
    match answer {
        Answer::Tasks { tasks } => Some(tasks),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_of_another_kind_is_an_error_and_a_failure_keeps_its_code() {
        assert_eq!(picked(Ok(Answer::Done), done), Ok(()));
        assert!(matches!(
            picked(
                Ok(Answer::Diff {
                    text: String::new()
                }),
                done
            ),
            Err(ClientError::Failed {
                code: ErrorCode::Unknown,
                ..
            })
        ));
        let failed = ClientError::Failed {
            code: ErrorCode::Unavailable,
            message: "gone".to_owned(),
        };
        assert_eq!(picked(Err(failed.clone()), done), Err(failed));
    }
}
