use std::path::PathBuf;

use super::EnvironmentData;
use crate::agent::AgentPreset;
use crate::ids::{ResumeId, TerminalId};

pub const RESTORE_MARKER: &str = "── restored session ──";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelaunchIntent {
    Shell,
    Resume {
        preset: AgentPreset,
        resume: Option<ResumeId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relaunch {
    pub id: TerminalId,
    pub intent: RelaunchIntent,
    pub cwd: Option<PathBuf>,
    pub prefill: Vec<String>,
}

impl EnvironmentData {
    #[must_use]
    pub fn relaunches(&self, running: impl Fn(TerminalId) -> bool) -> Vec<Relaunch> {
        self.nodes
            .iter()
            .filter(|node| node.kind.is_terminal() && !running(node.id))
            .map(|node| {
                let meta = self.meta.get(&node.id);
                let group_root = node
                    .group
                    .and_then(|group| self.group(group))
                    .map(|group| group.root.clone());
                let cwd = meta
                    .filter(|meta| !meta.cwd.is_empty())
                    .map(|meta| PathBuf::from(&meta.cwd))
                    .or(group_root);
                let harness = node.harness.filter(|preset| preset.is_agent());
                let prefill = meta
                    .filter(|_| harness.is_none())
                    .map_or_else(Vec::new, |meta| {
                        let mut lines = meta.scrollback.clone();
                        lines.push(RESTORE_MARKER.to_owned());
                        lines
                    });
                let intent =
                    harness.map_or(RelaunchIntent::Shell, |preset| RelaunchIntent::Resume {
                        preset,
                        resume: node.resume.clone(),
                    });
                Relaunch {
                    id: node.id,
                    intent,
                    cwd,
                    prefill,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{seed_terminals, TerminalMeta, TerminalNode};
    use crate::ids::{EnvironmentName, InvalidResumeId};

    fn node(id: u64) -> TerminalNode {
        let (mut nodes, _) = seed_terminals();
        let mut node = nodes.remove(0);
        node.id = TerminalId(id);
        node
    }

    fn environment() -> EnvironmentData {
        EnvironmentData::empty(EnvironmentName::new("main".to_owned()))
    }

    fn agent(id: u64, resume: Option<ResumeId>) -> TerminalNode {
        let mut agent = node(id);
        agent.harness = Some(AgentPreset::ClaudeCode);
        agent.resume = resume;
        agent
    }

    #[test]
    fn a_shell_comes_back_in_its_folder_with_its_scrollback_and_the_marker() {
        let mut data = environment();
        data.nodes.push(node(1));
        data.meta.insert(
            TerminalId(1),
            TerminalMeta {
                cwd: "/work".to_owned(),
                scrollback: vec!["$ ls".to_owned()],
            },
        );
        assert_eq!(
            data.relaunches(|_| false),
            vec![Relaunch {
                id: TerminalId(1),
                intent: RelaunchIntent::Shell,
                cwd: Some(PathBuf::from("/work")),
                prefill: vec!["$ ls".to_owned(), RESTORE_MARKER.to_owned()],
            }]
        );
    }

    #[test]
    fn an_agent_resumes_its_conversation_without_a_prefill() -> Result<(), InvalidResumeId> {
        let resume: ResumeId = "abc".parse()?;
        let mut data = environment();
        data.nodes.push(agent(2, Some(resume.clone())));
        data.nodes.push(agent(3, None));
        data.meta.insert(
            TerminalId(2),
            TerminalMeta {
                cwd: String::new(),
                scrollback: vec!["old".to_owned()],
            },
        );
        let relaunches = data.relaunches(|_| false);
        assert_eq!(
            relaunches[0].intent,
            RelaunchIntent::Resume {
                preset: AgentPreset::ClaudeCode,
                resume: Some(resume),
            }
        );
        assert_eq!(relaunches[0].prefill, Vec::<String>::new());
        assert_eq!(relaunches[0].cwd, None);
        assert_eq!(
            relaunches[1].intent,
            RelaunchIntent::Resume {
                preset: AgentPreset::ClaudeCode,
                resume: None,
            }
        );
        Ok(())
    }

    #[test]
    fn a_running_terminal_is_left_alone() {
        let mut data = environment();
        data.nodes.push(node(1));
        data.nodes.push(node(2));
        let ids: Vec<TerminalId> = data
            .relaunches(|id| id == TerminalId(1))
            .into_iter()
            .map(|relaunch| relaunch.id)
            .collect();
        assert_eq!(ids, vec![TerminalId(2)]);
    }
}
