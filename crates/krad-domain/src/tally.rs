use crate::agent::AgentState;
use crate::environment::EnvironmentData;
use crate::ids::GroupId;
use std::time::Duration;

const SECS_PER_MINUTE: u64 = 60;
const SECS_PER_HOUR: u64 = 60 * SECS_PER_MINUTE;
const SECS_PER_DAY: u64 = 24 * SECS_PER_HOUR;
const NOW_LABEL: &str = "now";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StateTally {
    pub idle: usize,
    pub working: usize,
    pub waiting: usize,
    pub done: usize,
}

impl StateTally {
    pub fn of(states: impl IntoIterator<Item = AgentState>) -> Self {
        states
            .into_iter()
            .fold(Self::default(), |mut tally, state| {
                match state {
                    AgentState::Idle => tally.idle += 1,
                    AgentState::Working => tally.working += 1,
                    AgentState::WaitingForInput => tally.waiting += 1,
                    AgentState::Done => tally.done += 1,
                }
                tally
            })
    }

    #[must_use]
    pub const fn total(self) -> usize {
        self.idle + self.working + self.waiting + self.done
    }

    #[must_use]
    pub fn active_counts(self) -> Vec<(AgentState, usize)> {
        [
            (AgentState::WaitingForInput, self.waiting),
            (AgentState::Working, self.working),
            (AgentState::Done, self.done),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .collect()
    }
}

impl EnvironmentData {
    #[must_use]
    pub fn tally_of_group(&self, group: Option<GroupId>) -> StateTally {
        StateTally::of(
            self.nodes
                .iter()
                .filter(|node| node.group == group)
                .map(|node| node.state),
        )
    }

    #[must_use]
    pub fn tally(&self) -> StateTally {
        StateTally::of(self.nodes.iter().map(|node| node.state))
    }
}

#[must_use]
pub fn relative_age(age: Duration) -> String {
    let secs = age.as_secs();
    if secs < SECS_PER_MINUTE {
        NOW_LABEL.to_owned()
    } else if secs < SECS_PER_HOUR {
        format!("{}m", secs / SECS_PER_MINUTE)
    } else if secs < SECS_PER_DAY {
        format!("{}h", secs / SECS_PER_HOUR)
    } else {
        format!("{}d", secs / SECS_PER_DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::seed_environment;
    use std::path::PathBuf;

    #[test]
    fn tally_counts_each_state_and_lists_only_active_ones() {
        let tally = StateTally::of([
            AgentState::Idle,
            AgentState::Working,
            AgentState::WaitingForInput,
            AgentState::WaitingForInput,
        ]);
        assert_eq!(tally.total(), 4);
        assert_eq!(
            tally.active_counts(),
            [(AgentState::WaitingForInput, 2), (AgentState::Working, 1)]
        );
        assert_eq!(StateTally::default().active_counts(), []);
    }

    #[test]
    fn group_tally_only_counts_its_own_terminals() {
        let mut environment = seed_environment();
        for node in &mut environment.nodes {
            node.state = AgentState::Idle;
        }
        let group = environment.add_group(PathBuf::from("/work/api"), None);
        environment.nodes[0].group = Some(group);
        environment.nodes[0].state = AgentState::WaitingForInput;
        environment.nodes[1].state = AgentState::Working;
        let grouped = environment.tally_of_group(Some(group));
        assert_eq!((grouped.total(), grouped.waiting), (1, 1));
        let ungrouped = environment.tally_of_group(None);
        assert_eq!(ungrouped.total(), environment.nodes.len() - 1);
        assert_eq!(environment.tally().working, 1);
    }

    #[test]
    fn ages_round_down_to_the_largest_unit() {
        assert_eq!(relative_age(Duration::from_secs(59)), "now");
        assert_eq!(relative_age(Duration::from_mins(1)), "1m");
        assert_eq!(relative_age(Duration::from_secs(3_599)), "59m");
        assert_eq!(relative_age(Duration::from_hours(1)), "1h");
        assert_eq!(relative_age(Duration::from_hours(72)), "3d");
    }
}
