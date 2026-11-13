use futures::channel::oneshot;
use futures::FutureExt as _;
use gpui_kit::App;
use krad_client::ClientError;
use krad_protocol::{Answer, Command, Reply, RequestId};
use std::collections::HashMap;
use std::future::Future;

use super::RuntimeLink;

#[derive(Default)]
pub(in crate::environment_shell) struct Waiters(HashMap<RequestId, oneshot::Sender<Answer>>);

impl Waiters {
    fn wait(&mut self, request: RequestId) -> oneshot::Receiver<Answer> {
        self.0.retain(|_, waiter| !waiter.is_canceled());
        let (waiter, answer) = oneshot::channel();
        self.0.insert(request, waiter);
        answer
    }

    pub(in crate::environment_shell) fn answered(&mut self, request: RequestId, answer: Answer) {
        let Some(waiter) = self.0.remove(&request) else {
            return;
        };
        if waiter.send(answer).is_err() {
            tracing::trace!(?request, "nobody waits for this answer any more");
        }
    }

    pub(in crate::environment_shell) fn forget_all(&mut self) {
        self.0.clear();
    }
}

impl RuntimeLink {
    pub(in crate::environment_shell) fn ask(
        &self,
        command: Command,
        cx: &mut App,
    ) -> impl Future<Output = Result<Answer, ClientError>> + 'static {
        self.ask_as(self.client.next_request(), command, cx)
    }

    pub(in crate::environment_shell) fn ask_as(
        &self,
        request: RequestId,
        command: Command,
        cx: &mut App,
    ) -> impl Future<Output = Result<Answer, ClientError>> + 'static {
        let answer = self.view.update(cx, |model, _| model.waiters.wait(request));
        let reply = self.client.send_as(request, command);
        async move {
            match reply.await? {
                Reply::Accepted => match answer.await.map_err(|_| ClientError::Stopped)? {
                    Answer::Failed { code, message } => Err(ClientError::Failed { code, message }),
                    answer => Ok(answer),
                },
                refused @ Reply::Refused { .. } => Err(ClientError::from(refused)),
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::super::ViewModel;
    use super::*;
    use futures::executor::block_on;
    use krad_protocol::{Event, EventKind, Feed, FoundMentions, Seq};

    fn answered(seq: u64, request: RequestId, answer: Answer) -> Feed {
        Feed::Event(Event {
            seq: Seq(seq),
            kind: EventKind::Answered {
                request,
                answer: Box::new(answer),
            },
        })
    }

    #[test]
    fn an_answered_event_reaches_its_own_waiter_once_and_a_lost_feed_lets_the_rest_go() {
        let mut model = ViewModel::default();
        let first = model.waiters.wait(RequestId(1));
        let second = model.waiters.wait(RequestId(2));
        let found = Answer::Mentions(FoundMentions::default());
        model.feed(answered(1, RequestId(1), found.clone()));
        model.feed(answered(2, RequestId(1), Answer::Unknown));
        assert_eq!(block_on(first).ok(), Some(found));
        assert!(model.take_news().heard.is_empty());
        model.waiters.forget_all();
        assert!(block_on(second).is_err());
    }
}
