use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, Div, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;
use krad_extensions::MissingServerRuntime;
use std::path::Path;

use super::super::{EnvironmentShell, LiveEnv};
use super::{ServerFailure, ServerKey, ServerSlot};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, Palette, LINE_SM, SPACE_1, SPACE_2, TEXT_SM};

const BTN_INSTALL: &str = "language-server-install";
const BTN_NOT_NOW: &str = "language-server-not-now";
const BTN_RETRY: &str = "language-server-retry";
const BTN_CHOOSE: &str = "server_runtime-choose";
const BTN_INSTALL_SERVER_RUNTIME: &str = "server_runtime-install";
const INSTALL_LABEL: &str = "Install";
const NOT_NOW_LABEL: &str = "Not now";
const RETRY_LABEL: &str = "Retry";
const CHOOSE_LABEL: &str = "Choose folder…";
const BTN_DETAILS: &str = "language-server-details";
const BTN_COPY_DETAILS: &str = "language-server-copy-details";
const DETAILS_ID: &str = "language-server-failure-details";
const DETAILS_LABEL: &str = "Details";
const HIDE_DETAILS_LABEL: &str = "Hide details";
const COPY_DETAILS_LABEL: &str = "Copy details";
const COPIED_TEXT: &str = "Copied to the clipboard";
const DETAILS_MAX_HEIGHT: f32 = 160.0;

enum Notice {
    ServerRuntime(MissingServerRuntime, String),
    Offer(String),
    Busy(String),
    Failed(String, Option<String>),
}

fn notice(key: &ServerKey, slot: &ServerSlot, language: &str) -> Option<Notice> {
    let server = &key.plan.package;
    match slot {
        ServerSlot::NeedsServerRuntime(missing) => Some(Notice::ServerRuntime(
            missing.clone(),
            runtime_message(server, language, missing),
        )),
        ServerSlot::InstallingServerRuntime(name) => Some(Notice::Busy(format!("Installing {name}…"))),
        ServerSlot::AwaitingConsent => Some(Notice::Offer(format!(
            "Install {server} for completions, diagnostics and go to definition in {language} files?"
        ))),
        ServerSlot::Installing => Some(Notice::Busy(format!("Installing {server}…"))),
        ServerSlot::Failed(ServerFailure { summary, details }) => Some(Notice::Failed(
            format!("{server} is not running: {summary}"),
            details.clone(),
        )),
        ServerSlot::Locating | ServerSlot::Starting(_) | ServerSlot::Running(_) => None,
    }
}

fn runtime_message(server: &str, language: &str, missing: &MissingServerRuntime) -> String {
    let name = &missing.name;
    match (&missing.needs, missing.outdated, missing.installable) {
        (Some(needs), true, true) => format!(
            "{server} needs {name} {needs}, and the one found is older. The app can install its own {name} just to run {server}; your projects keep theirs."
        ),
        (Some(needs), true, false) => format!(
            "{server} needs {name} {needs}, and the one found is older. Choose a folder with a newer {name}."
        ),
        (Some(needs), false, _) => format!(
            "{server} needs {name} {needs}, which was not found. Install it or choose the folder where you have it."
        ),
        (None, _, _) => format!(
            "{language} needs {name}, which was not found. Install it or choose the folder where you have it."
        ),
    }
}

impl EnvironmentShell {
    fn failure_bar(
        &self,
        bar: Div,
        key: ServerKey,
        text: String,
        details: Option<String>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let shown = self.language_servers.shown_details.contains(&key);
        let toggle = key.clone();
        let has_details = details.is_some();
        let bar = bar
            .text_color(rgb(pal.warning_fg))
            .child(div().flex_1().min_w_0().child(text))
            .children(has_details.then(|| {
                Button::new(BTN_DETAILS)
                    .label(if shown {
                        HIDE_DETAILS_LABEL
                    } else {
                        DETAILS_LABEL
                    })
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.toggle_failure_details(&toggle, cx);
                    }))
            }))
            .child(
                Button::new(BTN_RETRY)
                    .label(RETRY_LABEL)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.retry_server(key.clone(), cx);
                    })),
            );
        match details.filter(|_| shown) {
            Some(details) => {
                let copied = details.clone();
                bar.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .child(
                            div()
                                .id(DETAILS_ID)
                                .max_h(px(DETAILS_MAX_HEIGHT))
                                .overflow_y_scroll()
                                .text_color(rgb(pal.text_muted))
                                .child(details),
                        )
                        .child(
                            Button::new(BTN_COPY_DETAILS)
                                .label(COPY_DETAILS_LABEL)
                                .small()
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copied.clone(),
                                    ));
                                    this.push_toast(COPIED_TEXT.to_owned());
                                    cx.notify();
                                })),
                        ),
                )
            }
            None => bar,
        }
    }

    fn toggle_failure_details(&mut self, key: &ServerKey, cx: &mut Context<Self>) {
        let shown = &mut self.language_servers.shown_details;
        if !shown.remove(key) {
            shown.insert(key.clone());
        }
        cx.notify();
    }

    fn not_now(key: ServerKey, cx: &mut Context<Self>) -> Button {
        Button::new(BTN_NOT_NOW)
            .label(NOT_NOW_LABEL)
            .small()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.decline_server(&key, cx)))
    }

    pub(in crate::environment_shell) fn language_notice(
        &self,
        live: &LiveEnv,
        id: TerminalId,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = self.server_key(live, id, path)?;
        let slot = self.language_servers.slots.get(&key)?;
        let language = self.language_servers.catalog.for_path(path)?.name.clone();
        let found = notice(&key, slot, &language)?;
        let pal = palette(cx);
        let bar = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .bg(pal.bg_hover)
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM));
        let bar = match found {
            Notice::ServerRuntime(missing, text) => {
                let (choose, install) = (key.clone(), key.clone());
                let (chosen_id, install_id) = (missing.id.clone(), missing.id.clone());
                let (chosen_name, install_name) = (missing.name.clone(), missing.name.clone());
                let installable = missing.installable;
                let local = key.host.is_local();
                bar.text_color(rgb(pal.text_default))
                    .child(div().flex_1().min_w_0().child(text))
                    .child(Self::not_now(key, cx))
                    .children(local.then(|| {
                        Button::new(BTN_CHOOSE)
                            .label(CHOOSE_LABEL)
                            .small()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.choose_server_runtime_folder(
                                    choose.clone(),
                                    chosen_id.clone(),
                                    chosen_name.clone(),
                                    cx,
                                );
                            }))
                    }))
                    .children(installable.then(|| {
                        Button::new(BTN_INSTALL_SERVER_RUNTIME)
                            .label(format!("{INSTALL_LABEL} {install_name}"))
                            .small()
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.install_server_runtime(
                                    install.clone(),
                                    install_id.clone(),
                                    install_name.clone(),
                                    cx,
                                );
                            }))
                    }))
            }
            Notice::Offer(text) => {
                let install = key.clone();
                bar.text_color(rgb(pal.text_default))
                    .child(div().flex_1().min_w_0().child(text))
                    .child(Self::not_now(key, cx))
                    .child(
                        Button::new(BTN_INSTALL)
                            .label(INSTALL_LABEL)
                            .small()
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.install_server(install.clone(), cx);
                            })),
                    )
            }
            Notice::Busy(text) => bar
                .text_color(rgb(pal.text_muted))
                .child(div().flex_1().child(text)),
            Notice::Failed(text, details) => self.failure_bar(bar, key, text, details, pal, cx),
        };
        Some(bar.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::ServerRuntimeId;

    fn missing(needs: Option<&str>, outdated: bool) -> MissingServerRuntime {
        MissingServerRuntime {
            id: ServerRuntimeId::new("python"),
            name: "Python".to_owned(),
            installable: true,
            needs: needs.and_then(|text| text.parse().ok()),
            outdated,
        }
    }

    #[test]
    fn an_old_runtime_is_explained_with_the_range_the_server_needs() {
        let text = runtime_message("basedpyright", "Python", &missing(Some(">=3.10"), true));
        assert!(text.starts_with("basedpyright needs Python >=3.10, and the one found is older."));
        let plain = runtime_message("basedpyright", "Python", &missing(None, false));
        assert!(plain.starts_with("Python needs Python, which was not found."));
    }
}
