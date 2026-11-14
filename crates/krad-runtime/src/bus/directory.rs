use krad_agents::{AgentFeed, AgentInfo, AgentKey, StateReason, Timestamp};

#[derive(Debug)]
pub(crate) struct Directory {
    feed: AgentFeed,
    agents: Vec<(AgentKey, AgentInfo)>,
}

fn tracked(key: &AgentKey, agent: AgentInfo) -> AgentInfo {
    AgentInfo {
        environment: Some(key.environment.clone()),
        terminal: key.terminal,
        is_self: false,
        ..agent
    }
}

impl Directory {
    pub(crate) const fn new(feed: AgentFeed) -> Self {
        Self {
            feed,
            agents: Vec::new(),
        }
    }

    pub(crate) fn observe(
        &mut self,
        key: &AgentKey,
        seen: Option<AgentInfo>,
        hint: Option<StateReason>,
    ) {
        let seen = seen.map(|agent| tracked(key, agent));
        let index = self.agents.iter().position(|(known, _)| known == key);
        match (index, &seen) {
            (Some(index), Some(agent)) => self.agents[index].1 = agent.clone(),
            (Some(index), None) => {
                self.agents.remove(index);
            }
            (None, Some(agent)) => self.agents.push((key.clone(), agent.clone())),
            (None, None) => {}
        }
        self.feed.observe(key.clone(), seen, hint, Timestamp::now());
    }

    pub(crate) fn reconcile(&mut self, environments: &[String], agents: Vec<AgentInfo>) {
        let seen: Vec<(AgentKey, AgentInfo)> = agents
            .into_iter()
            .filter_map(|agent| {
                let key = AgentKey::new(agent.environment.as_deref()?, agent.terminal);
                let agent = tracked(&key, agent);
                Some((key, agent))
            })
            .collect();
        self.feed.reconcile(
            environments,
            seen.iter().map(|(_, agent)| agent.clone()).collect(),
            Timestamp::now(),
        );
        self.agents = seen;
    }

    pub(crate) fn agents_in(&self, environment: &str, me: Option<u64>) -> Vec<AgentInfo> {
        self.agents
            .iter()
            .filter(|(key, _)| key.environment == environment)
            .map(|(key, agent)| AgentInfo {
                is_self: Some(key.terminal) == me,
                ..agent.clone()
            })
            .collect()
    }

    pub(crate) fn every_agent(&self) -> Vec<AgentInfo> {
        self.agents.iter().map(|(_, agent)| agent.clone()).collect()
    }
}
