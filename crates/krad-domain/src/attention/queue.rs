use crate::agent::AgentState;
use crate::ids::{EnvironmentName, TerminalId};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowFocus {
    #[default]
    Foreground,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sight {
    Watched,
    Unwatched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttentionKind {
    Permission,
    Ask,
    Error,
    Finished,
}

impl AttentionKind {
    pub const ALL: [Self; 4] = [Self::Finished, Self::Ask, Self::Permission, Self::Error];

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Permission => "Permissions",
            Self::Ask => "Asks",
            Self::Error => "Errors",
            Self::Finished => "Finished",
        }
    }

    #[must_use]
    pub const fn waits_for_an_answer(self) -> bool {
        matches!(self, Self::Permission | Self::Ask)
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Permission => 0,
            Self::Ask => 1,
            Self::Error => 2,
            Self::Finished => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transition {
    NeedsInput,
    Finished,
    Settled,
    Unchanged,
}

impl Transition {
    const fn of(previous: AgentState, next: AgentState) -> Self {
        match (previous, next) {
            (AgentState::WaitingForInput, AgentState::WaitingForInput)
            | (AgentState::Done, AgentState::Done)
            | (AgentState::Idle, AgentState::Idle)
            | (AgentState::Working, AgentState::Working) => Self::Unchanged,
            (_, AgentState::WaitingForInput) => Self::NeedsInput,
            (_, AgentState::Done) | (AgentState::Working, AgentState::Idle) => Self::Finished,
            _ => Self::Settled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AttentionTarget {
    pub environment: EnvironmentName,
    pub terminal: TerminalId,
}

impl AttentionTarget {
    #[must_use]
    pub const fn new(environment: EnvironmentName, terminal: TerminalId) -> Self {
        Self {
            environment,
            terminal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateChange {
    pub target: AttentionTarget,
    pub previous: AgentState,
    pub next: AgentState,
    pub waiting_for: AttentionKind,
    pub sight: Sight,
    pub at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionEntry {
    pub target: AttentionTarget,
    pub kind: AttentionKind,
    pub since: Instant,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttentionQueue {
    entries: Vec<AttentionEntry>,
}

impl AttentionQueue {
    pub fn observe(&mut self, change: StateChange) -> Option<AttentionKind> {
        let kind = match Transition::of(change.previous, change.next) {
            Transition::Unchanged => return None,
            Transition::Settled => {
                self.forget(&change.target);
                return None;
            }
            Transition::NeedsInput => change.waiting_for,
            Transition::Finished => AttentionKind::Finished,
        };
        self.raise(change.target, kind, change.sight, change.at)
    }

    pub fn raise(
        &mut self,
        target: AttentionTarget,
        kind: AttentionKind,
        sight: Sight,
        at: Instant,
    ) -> Option<AttentionKind> {
        if !kind.waits_for_an_answer() && sight == Sight::Watched {
            self.forget(&target);
            return None;
        }
        let queued = self
            .entries
            .iter()
            .any(|entry| entry.target == target && entry.kind == kind);
        if queued {
            return None;
        }
        self.forget(&target);
        self.entries.push(AttentionEntry {
            target,
            kind,
            since: at,
        });
        Some(kind)
    }

    pub fn seen(&mut self, target: &AttentionTarget) {
        self.entries
            .retain(|entry| &entry.target != target || entry.kind.waits_for_an_answer());
    }

    pub fn forget(&mut self, target: &AttentionTarget) {
        self.entries.retain(|entry| &entry.target != target);
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&AttentionTarget) -> bool) {
        self.entries.retain(|entry| keep(&entry.target));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn contains(&self, target: &AttentionTarget) -> bool {
        self.entries.iter().any(|entry| &entry.target == target)
    }

    #[must_use]
    pub fn kind_of(&self, target: &AttentionTarget) -> Option<AttentionKind> {
        self.entries
            .iter()
            .find(|entry| &entry.target == target)
            .map(|entry| entry.kind)
    }

    #[must_use]
    pub fn ordered(&self) -> Vec<&AttentionEntry> {
        let mut order: Vec<&AttentionEntry> = self.entries.iter().collect();
        order.sort_by_key(|entry| entry.kind.rank());
        order
    }

    #[must_use]
    pub fn next_after(&self, current: Option<&AttentionTarget>) -> Option<&AttentionTarget> {
        let order = self.ordered();
        let start = current
            .and_then(|current| order.iter().position(|entry| &entry.target == current))
            .map_or(0, |at| at + 1);
        order
            .iter()
            .cycle()
            .skip(start)
            .take(order.len())
            .map(|entry| &entry.target)
            .find(|target| Some(*target) != current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(env: &str, id: u64) -> AttentionTarget {
        AttentionTarget::new(EnvironmentName::new(env.to_owned()), TerminalId(id))
    }

    fn change(target: &AttentionTarget, previous: AgentState, next: AgentState) -> StateChange {
        StateChange {
            target: target.clone(),
            previous,
            next,
            waiting_for: AttentionKind::Ask,
            sight: Sight::Unwatched,
            at: Instant::now(),
        }
    }

    fn queue_of(raised: &[(AttentionTarget, AttentionKind)]) -> AttentionQueue {
        let mut queue = AttentionQueue::default();
        for (target, kind) in raised {
            queue.raise(target.clone(), *kind, Sight::Unwatched, Instant::now());
        }
        queue
    }

    #[test]
    fn transitions_raise_what_the_agent_waits_for_or_its_finish() {
        let agent = target("main", 1);
        let cases = [
            (
                AgentState::Working,
                AgentState::WaitingForInput,
                Some(AttentionKind::Ask),
            ),
            (
                AgentState::Idle,
                AgentState::WaitingForInput,
                Some(AttentionKind::Ask),
            ),
            (
                AgentState::Working,
                AgentState::Done,
                Some(AttentionKind::Finished),
            ),
            (
                AgentState::WaitingForInput,
                AgentState::Done,
                Some(AttentionKind::Finished),
            ),
            (
                AgentState::Working,
                AgentState::Idle,
                Some(AttentionKind::Finished),
            ),
            (AgentState::Idle, AgentState::Working, None),
            (AgentState::WaitingForInput, AgentState::Idle, None),
            (AgentState::Done, AgentState::Idle, None),
            (AgentState::Done, AgentState::Done, None),
        ];
        for (previous, next, raised) in cases {
            let mut queue = AttentionQueue::default();
            assert_eq!(
                queue.observe(change(&agent, previous, next)),
                raised,
                "{previous:?} -> {next:?}"
            );
        }
    }

    #[test]
    fn waiting_takes_the_kind_the_agent_waits_for() {
        let mut queue = AttentionQueue::default();
        let agent = target("main", 1);
        let raised = queue.observe(StateChange {
            waiting_for: AttentionKind::Permission,
            ..change(&agent, AgentState::Working, AgentState::WaitingForInput)
        });
        assert_eq!(raised, Some(AttentionKind::Permission));
        assert_eq!(queue.kind_of(&agent), Some(AttentionKind::Permission));
    }

    #[test]
    fn raising_the_same_kind_twice_reports_it_once() {
        let agent = target("main", 1);
        let mut queue = queue_of(&[(agent.clone(), AttentionKind::Ask)]);
        let again = queue.raise(agent, AttentionKind::Ask, Sight::Unwatched, Instant::now());
        assert_eq!(again, None);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn working_again_leaves_the_queue() {
        let agent = target("main", 1);
        let mut queue = queue_of(&[(agent.clone(), AttentionKind::Permission)]);
        queue.observe(change(
            &agent,
            AgentState::WaitingForInput,
            AgentState::Working,
        ));
        assert!(queue.is_empty());
    }

    #[test]
    fn what_the_user_watches_is_only_queued_when_it_waits_for_an_answer() {
        let mut queue = AttentionQueue::default();
        let now = Instant::now();
        for kind in [AttentionKind::Finished, AttentionKind::Error] {
            assert_eq!(
                queue.raise(target("main", 1), kind, Sight::Watched, now),
                None
            );
        }
        assert!(queue.is_empty());
        for kind in [AttentionKind::Ask, AttentionKind::Permission] {
            assert_eq!(
                queue.raise(target("main", 2), kind, Sight::Watched, now),
                Some(kind)
            );
        }
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn seeing_clears_finishes_and_errors_but_keeps_what_waits() {
        let finished = target("main", 1);
        let failed = target("main", 2);
        let asking = target("main", 3);
        let mut queue = queue_of(&[
            (finished.clone(), AttentionKind::Finished),
            (failed.clone(), AttentionKind::Error),
            (asking.clone(), AttentionKind::Permission),
        ]);
        for target in [&finished, &failed, &asking] {
            queue.seen(target);
        }
        assert!(!queue.contains(&finished));
        assert!(!queue.contains(&failed));
        assert!(queue.contains(&asking));
    }

    #[test]
    fn next_goes_permissions_asks_errors_finished_then_oldest_and_wraps() {
        let done = target("main", 1);
        let failed = target("main", 2);
        let asking = target("other", 3);
        let permission = target("main", 4);
        let queue = queue_of(&[
            (done.clone(), AttentionKind::Finished),
            (failed.clone(), AttentionKind::Error),
            (asking.clone(), AttentionKind::Ask),
            (permission.clone(), AttentionKind::Permission),
        ]);
        assert_eq!(queue.next_after(None), Some(&permission));
        assert_eq!(queue.next_after(Some(&permission)), Some(&asking));
        assert_eq!(queue.next_after(Some(&asking)), Some(&failed));
        assert_eq!(queue.next_after(Some(&failed)), Some(&done));
        assert_eq!(queue.next_after(Some(&done)), Some(&permission));
    }

    #[test]
    fn next_from_outside_the_queue_starts_at_the_front() {
        let asking = target("main", 2);
        let queue = queue_of(&[(asking.clone(), AttentionKind::Ask)]);
        assert_eq!(queue.next_after(Some(&target("main", 9))), Some(&asking));
    }

    #[test]
    fn next_skips_the_current_target_and_is_empty_without_others() {
        let asking = target("main", 2);
        let queue = queue_of(&[(asking.clone(), AttentionKind::Ask)]);
        assert_eq!(queue.next_after(Some(&asking)), None);
        assert_eq!(AttentionQueue::default().next_after(None), None);
    }

    #[test]
    fn a_new_kind_moves_the_target_to_the_back_of_its_rank() {
        let first = target("main", 1);
        let second = target("main", 2);
        let mut queue = queue_of(&[
            (first.clone(), AttentionKind::Error),
            (second.clone(), AttentionKind::Error),
        ]);
        let raised = queue.raise(
            first.clone(),
            AttentionKind::Error,
            Sight::Unwatched,
            Instant::now(),
        );
        assert_eq!(raised, None);
        queue.raise(
            first.clone(),
            AttentionKind::Finished,
            Sight::Unwatched,
            Instant::now(),
        );
        queue.raise(
            first.clone(),
            AttentionKind::Error,
            Sight::Unwatched,
            Instant::now(),
        );
        assert_eq!(queue.next_after(None), Some(&second));
        assert_eq!(queue.len(), 2);
    }
}
