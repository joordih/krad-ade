use gpui_kit::{AppContext as _, Context};
use krad_chat::{new_session_id, LiveSession};
use krad_domain::review::Issue;
use krad_domain::{
    EnvironmentName, GroupId, Prompt, SessionSpec, SessionTemplate, TemplateLaunch, WorktreeOrigin,
};
use krad_repo::add_worktree;

use super::super::terminal_lifecycle::NodeGroup;
use super::super::EnvironmentShell;
use super::ChatChoices;

const NOT_A_REPO_TEXT: &str =
    "The template opens a worktree, but the group is not in a git repository";

impl EnvironmentShell {
    #[tracing::instrument(skip_all, fields(group = group.as_u64()))]
    pub(super) fn launch_template(
        &mut self,
        group: GroupId,
        template: &SessionTemplate,
        issue: Option<&Issue>,
        cx: &mut Context<Self>,
    ) {
        let launch = match template.launch(issue) {
            Ok(launch) => launch,
            Err(error) => return self.toast(&error.to_string(), cx),
        };
        let env = self.active.clone();
        let Some(branch) = launch.worktree.clone() else {
            self.open_template_session(&env, Some(group), launch, cx);
            return;
        };
        let Some(repo_root) = self.group_repo_root(group) else {
            return self.toast(NOT_A_REPO_TEXT, cx);
        };
        let link = self.env_link(&env);
        let create = cx.background_spawn({
            let repo_root = repo_root.clone();
            async move { add_worktree(&link, &repo_root, branch.as_str()) }
        });
        cx.spawn(async move |this, cx| {
            let created = create.await;
            this.update(cx, |shell, cx| match created {
                Ok(worktree) => {
                    let origin = worktree
                        .branch
                        .map(|branch| WorktreeOrigin { repo_root, branch });
                    let group = shell.add_group_to(&env, worktree.path, origin, cx);
                    shell.open_template_session(&env, group, launch, cx);
                }
                Err(error) => shell.toast(&error.to_string(), cx),
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn open_template_session(
        &mut self,
        env: &EnvironmentName,
        group: Option<GroupId>,
        launch: TemplateLaunch,
        cx: &mut Context<Self>,
    ) {
        let TemplateLaunch {
            session_name,
            harness,
            model,
            mcp_servers,
            prompt,
            ..
        } = launch;
        let spec = SessionSpec { name: session_name };
        if *env == self.active {
            self.new_session(group, spec, cx);
        } else if let Some(live) = self.envs.get_mut(env) {
            live.data.new_session(group, spec);
        }
        let session = LiveSession::Start(new_session_id());
        let place = NodeGroup::Exactly(group);
        let chat = match harness {
            Some(harness) => self.spawn_chat_as(env, place, harness, session, cx),
            None => self.spawn_chat_in(env, place, session, cx),
        };
        let Some((id, live)) = chat.zip(self.envs.get_mut(env)) else {
            return;
        };
        if let Some(prompt) = prompt {
            live.waiting_prompts.insert(id, vec![Prompt::from(prompt)]);
        }
        live.waiting_choices.insert(
            id,
            ChatChoices {
                model,
                mcp_servers,
                effort: None,
            },
        );
        tracing::info!(terminal = id.as_u64(), "session started from a template");
    }
}
