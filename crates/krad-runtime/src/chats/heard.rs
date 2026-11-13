use krad_chat::{ChatEvent, LiveEvent, ResumeRefused, Rewound};
use krad_domain::{ChatEntry, ModelOffer};
use krad_protocol::{AgentRef, ChatAsk, ChatAskId, EventKind, UsageNews};

use super::chat::{Batch, Chat};
use super::{checkpoints, ensure_live, hand_over, ChatEffect, ChatWork, Chats};

enum Outcome {
    Applied,
    Asked,
    Retry(Batch, ChatEvent),
}

fn split_refusal(event: LiveEvent) -> (LiveEvent, Option<ResumeRefused>) {
    match event {
        LiveEvent::Refused(refused) => (
            LiveEvent::Chat(ChatEvent::Error(refused.to_string())),
            Some(refused),
        ),
        event => (event, None),
    }
}

impl Chats {
    pub(crate) fn hear(&mut self, work: &mut ChatWork<'_>) {
        let heard: Vec<_> = self.heard.try_iter().collect();
        let mut touched: Vec<AgentRef> = Vec::new();
        for heard in heard {
            let Some(chat) = self.open.get_mut(&heard.chat) else {
                continue;
            };
            if chat.generation != heard.generation {
                continue;
            }
            let ask = ChatAskId(self.next_ask);
            match hear_one(chat, heard.event, ask, work) {
                Outcome::Applied => {}
                Outcome::Asked => self.next_ask += 1,
                Outcome::Retry(batch, failure) => {
                    self.next_generation += 1;
                    retry(
                        chat,
                        (&self.queue, self.next_generation),
                        batch,
                        failure,
                        work,
                    );
                }
            }
            if !touched.contains(&heard.chat) {
                touched.push(heard.chat);
            }
        }
        for id in touched {
            self.flush(&id, work);
        }
    }
}

fn retry(
    chat: &mut Chat,
    (queue, generation): (&super::queue::ChatQueue, u64),
    batch: Batch,
    failure: ChatEvent,
    work: &mut ChatWork<'_>,
) {
    if ensure_live(chat, queue, generation, work.gateway) {
        hand_over(chat, batch);
        return;
    }
    chat.closes_turn(true);
    chat.cancel_asks(work.outbox);
    chat.fold(&ChatEntry::Event(failure), work.outbox);
}

fn hear_one(chat: &mut Chat, event: LiveEvent, ask: ChatAskId, work: &mut ChatWork<'_>) -> Outcome {
    let (event, refused) = split_refusal(event);
    match event {
        LiveEvent::Ready(catalog) => keep_models(chat, catalog.models, work),
        LiveEvent::Rewound(outcome) => rewound(chat, outcome, work.outbox),
        LiveEvent::Notice(notice) => chat.notices.push(notice),
        LiveEvent::Ask(held) => {
            let kind = held.kind.clone();
            work.outbox.push(EventKind::ChatAsk(ChatAsk {
                chat: chat.id.clone(),
                ask,
                kind: kind.clone(),
                can_allow_always: held.can_allow_always(),
            }));
            work.effects.push(ChatEffect::Raise {
                chat: chat.id.clone(),
                kind: kind.attention(),
            });
            chat.asks.insert(ask, *held);
            return Outcome::Asked;
        }
        LiveEvent::Spent(report) => work.outbox.push(Chats::spent(chat, report)),
        LiveEvent::Usage(usage) => usage_heard(chat, UsageNews::Read { usage }, work),
        LiveEvent::Limit(alert) => usage_heard(chat, UsageNews::Alert { alert }, work),
        LiveEvent::Acp(_) | LiveEvent::Refused(_) => {}
        LiveEvent::Chat(event) => return hear_chat(chat, event, refused, work.outbox),
    }
    Outcome::Applied
}

fn hear_chat(
    chat: &mut Chat,
    event: ChatEvent,
    refused: Option<ResumeRefused>,
    outbox: &mut Vec<EventKind>,
) -> Outcome {
    match event {
        ChatEvent::Session(resume) => chat.session_reported(resume),
        ChatEvent::Model(model) => chat.model = Some(model),
        ChatEvent::PermissionMode(mode) => chat.choices.permission_mode = Some(mode),
        ChatEvent::ContextTokens(tokens) => chat.context.tokens = Some(tokens),
        ChatEvent::ContextWindow(window) => chat.context.window = Some(window),
        ChatEvent::TurnStarted => {
            if chat.opens_turn() {
                chat.fold(&ChatEntry::Event(ChatEvent::TurnStarted), outbox);
            }
        }
        event @ (ChatEvent::Done | ChatEvent::Error(_)) => {
            if let Some(batch) = refused.and_then(|refused| chat.retries(&refused)) {
                return Outcome::Retry(batch, event);
            }
            let ended = chat.live().is_none();
            if chat.closes_turn(ended) {
                chat.cancel_asks(outbox);
                chat.fold(&ChatEntry::Event(event), outbox);
            } else if let ChatEvent::Error(error) = event {
                chat.problem = Some(error);
            }
        }
        ChatEvent::Prompted(point) => {
            checkpoints::prompted(chat, &point, outbox);
            chat.fold(&ChatEntry::Event(ChatEvent::Prompted(point)), outbox);
        }
        event => chat.fold(&ChatEntry::Event(event), outbox),
    }
    Outcome::Applied
}

fn rewound(
    chat: &mut Chat,
    outcome: Result<Rewound, krad_chat::RewindError>,
    outbox: &mut Vec<EventKind>,
) {
    let Some(rewinding) = chat.rewinding.take() else {
        return;
    };
    chat.end_work();
    match outcome {
        Err(error) => {
            chat.problem = Some(error.to_string());
            return;
        }
        Ok(Rewound::Restart(session)) => {
            chat.session = session;
            chat.live = None;
        }
        Ok(Rewound::InPlace) => {}
    }
    chat.cut(rewinding.index, outbox);
    chat.problem = None;
    chat.outbox.push_front(rewinding.prompt);
}

fn keep_models(chat: &Chat, models: Vec<ModelOffer>, work: &mut ChatWork<'_>) {
    work.effects.push(ChatEffect::Models {
        host: chat.place.host.clone(),
        harness: chat.harness,
        models,
    });
}

fn usage_heard(chat: &Chat, news: UsageNews, work: &mut ChatWork<'_>) {
    work.effects.push(ChatEffect::Usage {
        host: chat.place.host.clone(),
        harness: chat.harness,
        news,
    });
}
