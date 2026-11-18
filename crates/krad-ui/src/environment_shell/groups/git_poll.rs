use gpui_kit::Context;
pub(in crate::environment_shell) use krad_domain::GroupGit;
use krad_domain::Host;
use krad_protocol::{Command, GitAct, GitCommand};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use super::super::language_servers::DiskChange;
use super::super::runtime_view::GitMove;
use super::super::EnvironmentShell;

const GROUP_CHECKS_EVERY: Duration = Duration::from_secs(3);

impl EnvironmentShell {
    pub(in crate::environment_shell) fn start_group_checks(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| loop {
            let checked = this.update(cx, |shell, cx| {
                let roots = shell.group_roots();
                shell.detect_wsl_agents(&roots, cx);
                shell.ensure_project_checks(cx);
            });
            if checked.is_err() {
                break;
            }
            cx.background_executor().timer(GROUP_CHECKS_EVERY).await;
        });
        self.git_task = Some(task);
    }

    pub(in crate::environment_shell) fn refresh_git(&mut self, _: &mut Context<Self>) {
        let env = self.active.clone();
        let Some(group) = self.envs.get(&env).and_then(|live| {
            live.selected_group
                .or_else(|| live.data.groups.first().map(|group| group.id))
        }) else {
            return;
        };
        self.runtime.client.post(Command::Git(GitCommand {
            environment: env,
            group,
            act: GitAct::Refresh,
        }));
    }

    pub(in crate::environment_shell) fn git_moved(
        &mut self,
        moved: GitMove,
        cx: &mut Context<Self>,
    ) {
        let GitMove {
            dirs,
            files,
            vanished,
        } = moved;
        self.reload_change_marks(&files, cx);
        let mut by_host: HashMap<Host, Vec<(PathBuf, DiskChange)>> = HashMap::new();
        for (host, path, change) in files {
            by_host.entry(host).or_default().push((path, change));
        }
        for (host, changed) in by_host {
            self.project_files_changed(&host, &changed, cx);
        }
        if vanished {
            self.forget_missing_problems(cx);
        }
        self.forget_diffs();
        self.forget_change_marks();
        self.forget_file_index();
        self.reload_file_dirs(&dirs, cx);
        cx.notify();
    }
}
