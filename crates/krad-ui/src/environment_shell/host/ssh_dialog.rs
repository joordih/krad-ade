use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Subscription, Window,
};
use krad_domain::{ssh_config_aliases, EnvironmentData, Host, SshDestination, SshDestinationError};
use krad_host::{read_ssh_config, HostLink};
use std::path::PathBuf;

use super::super::dialog_parts::{
    field_label, sheet_footer, sheet_overlay, sheet_panel, sheet_title,
};
use super::super::terminal_card::home_dir;
use super::super::{EnvironmentShell, LiveEnv};
use super::HostDialog;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_5, SPACE_6,
    TEXT_SM, TEXT_XS,
};

const DIALOG_W: f32 = 440.0;
const TITLE: &str = "New SSH environment";
const SUBTITLE: &str = "Its groups, terminals and agents run on that host";
const DESTINATION_LABEL: &str = "Destination";
const PLACEHOLDER: &str = "user@host, host:port or an alias from ~/.ssh/config";
const SUGGESTIONS_LABEL: &str = "From ~/.ssh/config";
const CREATE_LABEL: &str = "Create";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "ssh-dialog-close";
const BTN_CANCEL: &str = "ssh-dialog-cancel";
const BTN_CREATE: &str = "ssh-dialog-create";
const SUGGESTION_ID: &str = "ssh-dialog-suggestion";
const SUGGESTIONS_SHOWN: usize = 8;

pub(in crate::environment_shell) struct SshDialog {
    field: Entity<InputState>,
    error: Option<SshDestinationError>,
    suggestions: Vec<String>,
    _events: Subscription,
}

pub(in crate::environment_shell) fn ssh_config_suggestions() -> Vec<String> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    ssh_config_aliases(&read_ssh_config(&HostLink::Local, &PathBuf::from(home)))
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_host_dialog(cx);
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn open_ssh_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let focus_target = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus_target.update(cx, |field, cx| field.focus(window, cx));
        });
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, _, event: &InputEvent, _, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_ssh_dialog(cx),
                InputEvent::Change => this.clear_ssh_error(cx),
                _ => {}
            },
        );
        self.hosts.dialog = HostDialog::NewEnvironment(SshDialog {
            field,
            error: None,
            suggestions: Vec::new(),
            _events: events,
        });
        let read = cx.background_spawn(async move { ssh_config_suggestions() });
        cx.spawn(async move |this, cx| {
            let suggestions = read.await;
            this.update(cx, |shell, cx| {
                if let HostDialog::NewEnvironment(dialog) = &mut shell.hosts.dialog {
                    dialog.suggestions = suggestions;
                    cx.notify();
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn close_host_dialog(&mut self, cx: &mut Context<Self>) {
        self.hosts.dialog = HostDialog::Closed;
        cx.notify();
    }

    fn clear_ssh_error(&mut self, cx: &mut Context<Self>) {
        if let HostDialog::NewEnvironment(dialog) = &mut self.hosts.dialog {
            if dialog.error.take().is_some() {
                cx.notify();
            }
        }
    }

    fn commit_ssh_dialog(&mut self, cx: &mut Context<Self>) {
        let HostDialog::NewEnvironment(dialog) = &mut self.hosts.dialog else {
            return;
        };
        let typed = dialog.field.read(cx).value().to_string();
        match typed.parse::<SshDestination>() {
            Ok(destination) => self.create_ssh_environment(destination, cx),
            Err(error) => {
                dialog.error = Some(error);
                cx.notify();
            }
        }
    }

    fn create_ssh_environment(&mut self, destination: SshDestination, cx: &mut Context<Self>) {
        self.hosts.dialog = HostDialog::Closed;
        let name = self.unused_env_name();
        let mut data = EnvironmentData::empty(name.clone());
        data.title = Some(destination.to_string());
        data.host = Host::Ssh { destination };
        self.order.push(name.clone());
        let link = self.link_for(&data.host);
        self.envs
            .insert(name.clone(), LiveEnv::new(data, None, link));
        self.switch_env(&name, cx);
    }

    pub(in crate::environment_shell) fn ssh_dialog_sheet(
        dialog: &SshDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let error = dialog.error.map(|error| {
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.error_fg))
                .child(error.to_string())
        });
        let suggestions: Vec<AnyElement> = dialog
            .suggestions
            .iter()
            .take(SUGGESTIONS_SHOWN)
            .enumerate()
            .map(|(index, alias)| {
                let picked = alias.clone();
                div()
                    .id((SUGGESTION_ID, index))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .px(px(SPACE_2))
                    .py(px(SPACE_1))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(icon(IconName::Plug, ICON_SM, pal.text_muted))
                    .child(alias.clone())
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if let Ok(destination) = picked.parse::<SshDestination>() {
                            this.create_ssh_environment(destination, cx);
                        }
                    }))
                    .into_any_element()
            })
            .collect();
        let has_suggestions = !suggestions.is_empty();
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .child(field_label(DESTINATION_LABEL, pal))
            .child(Input::new(&dialog.field))
            .children(error)
            .when(has_suggestions, |body| {
                body.child(field_label(SUGGESTIONS_LABEL, pal))
                    .child(div().flex().flex_col().children(suggestions))
            });
        let sheet = sheet_panel(DIALOG_W, pal)
            .child(sheet_title(
                TITLE,
                Some(SUBTITLE.to_owned()),
                BTN_CLOSE,
                dismiss,
                cx,
            ))
            .child(body)
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.close_host_dialog(cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_CREATE)
                            .label(CREATE_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.commit_ssh_dialog(cx);
                            })),
                    ),
            );
        sheet_overlay(sheet, dismiss, cx)
    }
}
