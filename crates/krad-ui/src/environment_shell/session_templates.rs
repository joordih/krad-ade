mod launch;
mod sheet;

use gpui_kit::{AppContext as _, Context};
use krad_domain::review::Issue;
use krad_domain::{CodeHost, GitlabSettings, GroupId, McpServerName, ModelId, SessionTemplate};
use krad_gitlab::GitlabConfig;
use krad_host::HostLink;
use krad_repo::{read_session_templates, StoredTemplate};
use std::path::{Path, PathBuf};

use super::review_view::{origin_repo, Remote};
use super::EnvironmentShell;

const NOT_ON_A_CODE_HOST: &str =
    "The group's origin remote does not point at a GitLab or GitHub project";
const GITLAB_NOT_SIGNED_IN: &str =
    "Sign in on the GitLab page of Settings to start sessions from issues.";
const GITHUB_NOT_SIGNED_IN: &str =
    "Sign in on the GitHub page of Settings to start sessions from issues.";
const NO_GROUP_TEXT: &str = "Select a group to start a session from a template";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ChatChoices {
    pub(super) model: Option<ModelId>,
    pub(super) mcp_servers: Option<Vec<McpServerName>>,
    pub(super) effort: Option<krad_domain::Effort>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickerOpening {
    WhenTemplatesExist,
    Templates,
    Issues,
}

enum PickerStage {
    Templates,
    Issues {
        template: Box<SessionTemplate>,
        issues: Remote<Vec<Issue>>,
    },
}

pub(super) struct TemplatePicker {
    group: GroupId,
    root: PathBuf,
    templates: Vec<StoredTemplate>,
    stage: PickerStage,
}

fn read_open_issues(
    link: &HostLink,
    root: &Path,
    gitlab: &GitlabSettings,
) -> Result<Vec<Issue>, String> {
    let repo = origin_repo(link, root, GitlabConfig::try_from(gitlab).ok())
        .ok_or_else(|| NOT_ON_A_CODE_HOST.to_owned())?;
    let client = repo
        .login
        .client()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            match repo.login.code_host() {
                CodeHost::Gitlab => GITLAB_NOT_SIGNED_IN,
                CodeHost::Github => GITHUB_NOT_SIGNED_IN,
            }
            .to_owned()
        })?;
    client
        .issues(&repo.project)
        .map_err(|error| error.to_string())
}

impl EnvironmentShell {
    pub(super) fn new_session_in(&mut self, group: Option<GroupId>, cx: &mut Context<Self>) {
        match group {
            Some(group) => self.read_templates(group, PickerOpening::WhenTemplatesExist, cx),
            None => {
                self.start_chat_session(None, cx);
            }
        }
    }

    pub(super) fn open_template_picker(&mut self, cx: &mut Context<Self>) {
        self.open_picker_for_selected(PickerOpening::Templates, cx);
    }

    pub(super) fn open_issue_picker(&mut self, cx: &mut Context<Self>) {
        self.open_picker_for_selected(PickerOpening::Issues, cx);
    }

    fn open_picker_for_selected(&mut self, opening: PickerOpening, cx: &mut Context<Self>) {
        match self.active_group().map(|group| group.id) {
            Some(group) => self.read_templates(group, opening, cx),
            None => self.toast(NO_GROUP_TEXT, cx),
        }
    }

    fn read_templates(&mut self, group: GroupId, opening: PickerOpening, cx: &mut Context<Self>) {
        let Some(root) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.group(group))
            .map(|group| group.root.clone())
        else {
            return;
        };
        let link = self.active_link();
        let read = cx.background_spawn({
            let root = root.clone();
            async move { read_session_templates(&link, &root) }
        });
        cx.spawn(async move |this, cx| {
            let templates = read.await;
            this.update(cx, |shell, cx| {
                let templates = templates.unwrap_or_else(|error| {
                    tracing::warn!(%error, "session templates could not be read");
                    Vec::new()
                });
                shell.templates_read(group, root, templates, opening, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn templates_read(
        &mut self,
        group: GroupId,
        root: PathBuf,
        templates: Vec<StoredTemplate>,
        opening: PickerOpening,
        cx: &mut Context<Self>,
    ) {
        if opening == PickerOpening::WhenTemplatesExist && templates.is_empty() {
            self.start_chat_session(Some(group), cx);
            return;
        }
        self.template_picker = Some(TemplatePicker {
            group,
            root,
            templates,
            stage: PickerStage::Templates,
        });
        if opening == PickerOpening::Issues {
            self.pick_issue_for(SessionTemplate::for_issue(), cx);
        }
        cx.notify();
    }

    fn pick_issue_for(&mut self, template: SessionTemplate, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.template_picker else {
            return;
        };
        picker.stage = PickerStage::Issues {
            template: Box::new(template),
            issues: Remote::Loading,
        };
        let root = picker.root.clone();
        let link = self.active_link();
        let gitlab = self.settings.gitlab.clone();
        let read = cx.background_spawn(async move { read_open_issues(&link, &root, &gitlab) });
        cx.spawn(async move |this, cx| {
            let issues = read.await;
            this.update(cx, |shell, cx| {
                if let Some(TemplatePicker {
                    stage: PickerStage::Issues { issues: shown, .. },
                    ..
                }) = &mut shell.template_picker
                {
                    *shown = issues.map_or_else(Remote::Failed, Remote::Ready);
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    fn back_to_templates(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = &mut self.template_picker {
            picker.stage = PickerStage::Templates;
        }
        cx.notify();
    }

    fn close_template_picker(&mut self, cx: &mut Context<Self>) {
        self.template_picker = None;
        cx.notify();
    }

    fn start_blank_session(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.template_picker.take() else {
            return;
        };
        self.start_chat_session(Some(picker.group), cx);
    }

    fn start_template(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(template) = self
            .template_picker
            .as_ref()
            .and_then(|picker| picker.templates.get(index))
            .and_then(|stored| stored.template.as_ref().ok())
            .cloned()
        else {
            return;
        };
        if template.uses_issue() {
            self.pick_issue_for(template, cx);
            return;
        }
        let Some(picker) = self.template_picker.take() else {
            return;
        };
        self.launch_template(picker.group, &template, None, cx);
    }

    fn start_from_issue(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(TemplatePicker {
            group,
            stage:
                PickerStage::Issues {
                    template,
                    issues: Remote::Ready(issues),
                },
            ..
        }) = self.template_picker.take()
        else {
            return;
        };
        if let Some(issue) = issues.get(index) {
            self.launch_template(group, &template, Some(issue), cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_need_a_group_whose_origin_is_on_a_code_host() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!("krad-issues-none-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let read = read_open_issues(&HostLink::Local, &root, &GitlabSettings::default());
        std::fs::remove_dir_all(&root)?;
        assert_eq!(read, Err(NOT_ON_A_CODE_HOST.to_owned()));
        Ok(())
    }
}
