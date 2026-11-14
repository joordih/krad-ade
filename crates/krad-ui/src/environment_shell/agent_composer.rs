mod sheet;

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Window};
use krad_domain::{sanitize_worktree_name, AgentPreset, GroupId};

use super::git_actions::GitFollowUp;
use super::sessions::Launch;
use super::EnvironmentShell;
use crate::actions::NewAgent;

const PROMPT_ROWS: usize = 6;
const NAME_WORDS: usize = 4;
const NAME_WORD_SEPARATOR: &str = "-";
const PROMPT_PLACEHOLDER: &str = "What should the agent do?";
const WORKTREE_PLACEHOLDER: &str = "Worktree name (optional, from the prompt)";

pub(super) struct AgentComposer {
    preset: AgentPreset,
    group: Option<GroupId>,
    new_worktree: bool,
    prompt: Entity<TextareaState>,
    worktree_name: Entity<InputState>,
}

fn name_from_prompt(prompt: &str) -> Option<String> {
    let words: Vec<&str> = prompt.split_whitespace().take(NAME_WORDS).collect();
    sanitize_worktree_name(&words.join(NAME_WORD_SEPARATOR)).map(|name| name.to_lowercase())
}

impl EnvironmentShell {
    pub(super) fn on_new_agent(
        &mut self,
        _: &NewAgent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_agent_composer(window, cx);
    }

    pub(super) fn open_agent_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let offered = self.offered_agents(cx);
        let default = self.offered_default_harness(cx);
        let preset = Some(default)
            .filter(|preset| preset.is_agent() && offered.contains(preset))
            .or_else(|| offered.iter().copied().find(|preset| preset.is_agent()))
            .unwrap_or(default);
        let prompt = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(PROMPT_ROWS)
                .placeholder(PROMPT_PLACEHOLDER)
        });
        let focus_target = prompt.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        self.agent_composer = Some(AgentComposer {
            preset,
            group: self.active_group().map(|group| group.id),
            new_worktree: false,
            prompt,
            worktree_name: cx
                .new(|cx| InputState::new(window, cx).placeholder(WORKTREE_PLACEHOLDER)),
        });
        cx.notify();
    }

    fn close_agent_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_composer = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn start_composed_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = self.agent_composer.take() else {
            return;
        };
        window.focus(&self.focus, cx);
        let prompt = composer.prompt.read(cx).value().to_string();
        let env = self.active.clone();
        let launch = Launch::Agent {
            preset: composer.preset,
            prompt: prompt.clone(),
            model: None,
        };
        let repo_group = composer.group.filter(|group| {
            self.envs
                .get(&env)
                .and_then(|live| live.data.group(*group))
                .and_then(|group| self.git_at(&env, &group.root))
                .is_some()
        });
        let typed_name = composer.worktree_name.read(cx).value().to_string();
        let name = sanitize_worktree_name(&typed_name).or_else(|| name_from_prompt(&prompt));
        let label = self.settings.acp.harness_name(composer.preset).into_owned();
        match (composer.new_worktree, repo_group, name) {
            (true, Some(group), Some(name)) => {
                let follow = GitFollowUp::SpawnIn { label, launch };
                self.create_worktree_then(&env, group, &name, follow, cx);
            }
            _ => {
                self.spawn_terminal_in(&env, composer.group, &label, launch, None, cx);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_names_come_from_the_first_words_of_the_prompt() {
        assert_eq!(
            name_from_prompt("Fix the login token refresh bug").as_deref(),
            Some("fix-the-login-token")
        );
        assert_eq!(name_from_prompt("   "), None);
    }
}
