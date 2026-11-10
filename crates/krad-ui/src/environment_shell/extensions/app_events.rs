use krad_domain::{AppEvent, EventKind, EventRecord, ReviewVerdict};
use krad_extensions::api::protocol::{
    AppEvent as WireEvent, AppEventKind, AppEventNotification, AppEventParams,
    ReviewVerdict as WireVerdict,
};
use std::collections::{BTreeSet, HashMap};

use super::super::EnvironmentShell;
use super::SlotKey;

pub(super) type Subscriptions = HashMap<SlotKey, BTreeSet<AppEventKind>>;

const fn wire_kind(kind: EventKind) -> Option<AppEventKind> {
    Some(match kind {
        EventKind::AgentStarted => AppEventKind::AgentStarted,
        EventKind::AgentWaiting => AppEventKind::AgentWaiting,
        EventKind::AgentFinished => AppEventKind::AgentFinished,
        EventKind::WorktreeCreated => AppEventKind::WorktreeCreated,
        EventKind::WorktreeRemoved => AppEventKind::WorktreeRemoved,
        EventKind::SessionOpened => AppEventKind::SessionOpened,
        EventKind::SessionClosed => AppEventKind::SessionClosed,
        EventKind::GitChanged => AppEventKind::GitChanged,
        EventKind::ReviewSent => AppEventKind::ReviewSent,
        EventKind::CiFailed => AppEventKind::CiFailed,
        EventKind::CiPassed => AppEventKind::CiPassed,
        EventKind::MergeRequestOpened => AppEventKind::MergeRequestOpened,
        EventKind::VariantsStarted
        | EventKind::VariantsFinished
        | EventKind::VariantWinnerChosen
        | EventKind::Ping => return None,
    })
}

const fn wire_verdict(verdict: ReviewVerdict) -> WireVerdict {
    match verdict {
        ReviewVerdict::Commented => WireVerdict::Commented,
        ReviewVerdict::Replied => WireVerdict::Replied,
        ReviewVerdict::Approved => WireVerdict::Approved,
    }
}

fn wire_event(event: &AppEvent) -> Option<WireEvent> {
    Some(match event {
        AppEvent::AgentStarted { terminal, harness } => WireEvent::AgentStarted {
            terminal: terminal.as_u64().to_string(),
            harness: harness.map(|harness| harness.to_string()),
        },
        AppEvent::AgentWaiting { terminal } => WireEvent::AgentWaiting {
            terminal: terminal.as_u64().to_string(),
        },
        AppEvent::AgentFinished { terminal } => WireEvent::AgentFinished {
            terminal: terminal.as_u64().to_string(),
        },
        AppEvent::WorktreeCreated { from } => WireEvent::WorktreeCreated {
            from: from.as_u64().to_string(),
        },
        AppEvent::WorktreeRemoved => WireEvent::WorktreeRemoved,
        AppEvent::SessionOpened { session } => WireEvent::SessionOpened {
            session: session.as_u64().to_string(),
        },
        AppEvent::SessionClosed { session } => WireEvent::SessionClosed {
            session: session.as_u64().to_string(),
        },
        AppEvent::GitChanged => WireEvent::GitChanged,
        AppEvent::ReviewSent {
            merge_request,
            verdict,
        } => WireEvent::ReviewSent {
            merge_request: merge_request.0,
            verdict: wire_verdict(*verdict),
        },
        AppEvent::CiFailed {
            merge_request,
            pipeline,
        } => WireEvent::CiFailed {
            merge_request: merge_request.0,
            pipeline: pipeline.0,
        },
        AppEvent::CiPassed {
            merge_request,
            pipeline,
        } => WireEvent::CiPassed {
            merge_request: merge_request.0,
            pipeline: pipeline.0,
        },
        AppEvent::MergeRequestOpened { merge_request } => WireEvent::MergeRequestOpened {
            merge_request: merge_request.0,
        },
        AppEvent::VariantsStarted { .. }
        | AppEvent::VariantsFinished { .. }
        | AppEvent::VariantWinnerChosen { .. }
        | AppEvent::Ping => return None,
    })
}

fn wire_params(record: &EventRecord) -> Option<AppEventParams> {
    Some(AppEventParams {
        version: record.api.0,
        environment: record.environment.as_str().to_owned(),
        group: record.group.map(|group| group.as_u64().to_string()),
        event: wire_event(&record.event)?,
    })
}

impl EnvironmentShell {
    pub(super) fn subscribe_app_events(&mut self, key: &SlotKey, kinds: BTreeSet<AppEventKind>) {
        if kinds.is_empty() {
            self.extensions.subscriptions.remove(key);
        } else {
            self.extensions.subscriptions.insert(key.clone(), kinds);
        }
    }

    pub(in crate::environment_shell) fn wants_code_host_events(&self) -> bool {
        self.extensions.subscriptions.values().any(|kinds| {
            kinds.iter().any(|kind| {
                matches!(
                    kind,
                    AppEventKind::CiFailed
                        | AppEventKind::CiPassed
                        | AppEventKind::MergeRequestOpened
                )
            })
        })
    }

    pub(in crate::environment_shell) fn announce_app_event(&mut self, record: &EventRecord) {
        let Some(kind) = wire_kind(record.event.kind()) else {
            return;
        };
        let Some(params) = wire_params(record) else {
            return;
        };
        let mut gone = Vec::new();
        for (key, kinds) in &self.extensions.subscriptions {
            let concerned = key
                .environment
                .as_ref()
                .is_none_or(|own| *own == record.environment);
            if !concerned || !kinds.contains(&kind) {
                continue;
            }
            let Some(connection) = self.extensions.connection(key) else {
                gone.push(key.clone());
                continue;
            };
            if let Err(error) = connection.notify::<AppEventNotification>(&params) {
                tracing::debug!(%error, "app event not delivered");
            }
        }
        for key in gone {
            self.extensions.subscriptions.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{MergeRequestIid, PipelineId};
    use krad_domain::{AgentPreset, EnvironmentName, GroupId, TerminalId};
    use std::time::SystemTime;

    #[test]
    fn every_selectable_kind_but_variants_reaches_extensions_and_a_ping_does_not() {
        let app_only = [
            EventKind::VariantsStarted,
            EventKind::VariantsFinished,
            EventKind::VariantWinnerChosen,
        ];
        for kind in EventKind::SELECTABLE {
            assert_eq!(
                wire_kind(kind).is_some(),
                !app_only.contains(&kind),
                "{kind:?}"
            );
        }
        assert_eq!(wire_kind(EventKind::Ping), None);
    }

    #[test]
    fn a_record_reaches_the_wire_with_its_ids() {
        let events = [
            AppEvent::AgentStarted {
                terminal: TerminalId(4),
                harness: Some(AgentPreset::Codex),
            },
            AppEvent::CiPassed {
                merge_request: MergeRequestIid(8),
                pipeline: PipelineId(90),
            },
            AppEvent::WorktreeRemoved,
        ];
        for event in events {
            let record = EventRecord::new(
                SystemTime::UNIX_EPOCH,
                EnvironmentName::new("main".to_owned()),
                Some(GroupId(3)),
                event,
            );
            let params = wire_params(&record).unwrap_or_else(|| unreachable!("a wire event"));
            assert_eq!(params.group.as_deref(), Some("3"));
            if let WireEvent::AgentStarted { harness, .. } = &params.event {
                assert_eq!(harness.as_deref(), Some("Codex"));
            }
            assert_eq!(Some(params.event.kind()), wire_kind(record.event.kind()));
        }
    }
}
