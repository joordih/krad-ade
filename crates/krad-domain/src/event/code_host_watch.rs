use crate::event::AppEvent;
use crate::review::{CiStatus, MergeRequestIid, Pipeline, PipelineId};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineWatch {
    pub id: PipelineId,
    pub status: CiStatus,
}

impl From<&Pipeline> for PipelineWatch {
    fn from(pipeline: &Pipeline) -> Self {
        Self {
            id: pipeline.id,
            status: pipeline.status,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineSeen {
    Read(Option<PipelineWatch>),
    Unread,
}

impl PipelineSeen {
    const fn known(self) -> Option<PipelineWatch> {
        match self {
            Self::Read(pipeline) => pipeline,
            Self::Unread => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeHostSnapshot {
    open: BTreeMap<MergeRequestIid, PipelineSeen>,
}

impl CodeHostSnapshot {
    pub fn insert(&mut self, merge_request: MergeRequestIid, pipelines: &[Pipeline]) {
        let newest = pipelines
            .iter()
            .max_by_key(|pipeline| pipeline.id)
            .map(PipelineWatch::from);
        self.open.insert(merge_request, PipelineSeen::Read(newest));
    }

    pub fn insert_unread(&mut self, merge_request: MergeRequestIid) {
        self.open.insert(merge_request, PipelineSeen::Unread);
    }

    #[must_use]
    pub fn carried_over(mut self, before: Option<&Self>) -> Self {
        let Some(before) = before else {
            return self;
        };
        for (merge_request, seen) in &mut self.open {
            if *seen != PipelineSeen::Unread {
                continue;
            }
            if let Some(kept) = before.open.get(merge_request) {
                *seen = *kept;
            }
        }
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

fn settled_event(merge_request: MergeRequestIid, pipeline: PipelineWatch) -> Option<AppEvent> {
    match pipeline.status {
        CiStatus::Failed => Some(AppEvent::CiFailed {
            merge_request,
            pipeline: pipeline.id,
        }),
        CiStatus::Success => Some(AppEvent::CiPassed {
            merge_request,
            pipeline: pipeline.id,
        }),
        _ => None,
    }
}

#[must_use]
pub fn code_host_events(
    before: Option<&CodeHostSnapshot>,
    now: &CodeHostSnapshot,
) -> Vec<AppEvent> {
    let Some(before) = before else {
        return Vec::new();
    };
    let mut events = Vec::new();
    for (&merge_request, &seen) in &now.open {
        let known = before.open.get(&merge_request);
        if known.is_none() {
            events.push(AppEvent::MergeRequestOpened { merge_request });
        }
        let Some(pipeline) = seen.known() else {
            continue;
        };
        if known.copied().and_then(PipelineSeen::known) == Some(pipeline) {
            continue;
        }
        events.extend(settled_event(merge_request, pipeline));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::CommitSha;

    const FIRST: MergeRequestIid = MergeRequestIid(10);
    const SECOND: MergeRequestIid = MergeRequestIid(11);

    fn pipeline(id: u64, status: CiStatus) -> Pipeline {
        Pipeline {
            id: PipelineId(id),
            sha: CommitSha("0123456789abcdef".to_owned()),
            git_ref: "feature".to_owned(),
            status,
            web_url: "https://code.example.test/p".to_owned(),
            created_at: None,
            duration: None,
        }
    }

    fn snapshot(open: &[(MergeRequestIid, Vec<Pipeline>)]) -> CodeHostSnapshot {
        let mut snapshot = CodeHostSnapshot::default();
        for (merge_request, pipelines) in open {
            snapshot.insert(*merge_request, pipelines);
        }
        snapshot
    }

    #[test]
    fn the_first_look_only_sets_a_baseline() {
        let now = snapshot(&[(FIRST, vec![pipeline(1, CiStatus::Failed)])]);
        assert_eq!(code_host_events(None, &now), Vec::new());
    }

    #[test]
    fn a_new_merge_request_is_announced_with_its_settled_pipeline() {
        let before = snapshot(&[(FIRST, Vec::new())]);
        let now = snapshot(&[
            (FIRST, Vec::new()),
            (SECOND, vec![pipeline(5, CiStatus::Success)]),
        ]);
        assert_eq!(
            code_host_events(Some(&before), &now),
            vec![
                AppEvent::MergeRequestOpened {
                    merge_request: SECOND
                },
                AppEvent::CiPassed {
                    merge_request: SECOND,
                    pipeline: PipelineId(5),
                },
            ]
        );
    }

    #[test]
    fn a_pipeline_is_announced_once_when_it_settles() {
        let running = snapshot(&[(FIRST, vec![pipeline(3, CiStatus::Running)])]);
        let failed = snapshot(&[(FIRST, vec![pipeline(3, CiStatus::Failed)])]);
        let expected = vec![AppEvent::CiFailed {
            merge_request: FIRST,
            pipeline: PipelineId(3),
        }];
        assert_eq!(code_host_events(Some(&running), &failed), expected);
        assert_eq!(code_host_events(Some(&failed), &failed), Vec::new());
        assert_eq!(code_host_events(Some(&failed), &running), Vec::new());
    }

    #[test]
    fn only_the_newest_pipeline_counts() {
        let before = snapshot(&[(FIRST, vec![pipeline(3, CiStatus::Failed)])]);
        let now = snapshot(&[(
            FIRST,
            vec![
                pipeline(3, CiStatus::Failed),
                pipeline(4, CiStatus::Success),
            ],
        )]);
        assert_eq!(
            code_host_events(Some(&before), &now),
            vec![AppEvent::CiPassed {
                merge_request: FIRST,
                pipeline: PipelineId(4),
            }]
        );
    }

    #[test]
    fn a_failed_pipeline_read_keeps_the_last_known_pipeline() {
        let failed = snapshot(&[(FIRST, vec![pipeline(3, CiStatus::Failed)])]);
        let mut unread = CodeHostSnapshot::default();
        unread.insert_unread(FIRST);
        assert_eq!(code_host_events(Some(&failed), &unread), Vec::new());
        let kept = unread.carried_over(Some(&failed));
        assert_eq!(kept, failed);
        assert_eq!(code_host_events(Some(&kept), &failed), Vec::new());
        let mut first_look = CodeHostSnapshot::default();
        first_look.insert_unread(SECOND);
        let fresh = first_look.clone().carried_over(Some(&failed));
        assert_eq!(fresh, first_look);
        assert_eq!(
            code_host_events(Some(&failed), &fresh),
            vec![AppEvent::MergeRequestOpened {
                merge_request: SECOND
            }]
        );
    }
}
