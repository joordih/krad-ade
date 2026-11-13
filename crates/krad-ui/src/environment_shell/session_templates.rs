mod launch;
mod sheet;

use gpui_kit::{AppContext as _, Context};
use krad_domain::{review::Issue, GroupId, McpServerName, ModelId, SessionTemplate, ToastKind};
use krad_protocol::ReviewAct;
use krad_repo::{read_session_templates, StoredTemplate};
use std::path::{Path, PathBuf};

use super::review_view::Remote;
use super::EnvironmentShell;

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
            None => self.toast(ToastKind::Warning, NO_GROUP_TEXT, cx),
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
        self.post_review(root, ReviewAct::Issues, cx);
        cx.notify();
    }

    pub(super) fn issues_answered(&mut self, root: &Path, issues: Remote<Vec<Issue>>) {
        if let Some(TemplatePicker {
            root: asked,
            stage: PickerStage::Issues { issues: shown, .. },
            ..
        }) = &mut self.template_picker
        {
            if asked == root {
                *shown = issues;
            }
        }
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
