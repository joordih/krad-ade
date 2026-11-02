use gpui_kit::{div, px, rgb, AnyElement, Context, IntoElement, ParentElement, Styled};
use krad_domain::TerminalId;
use krad_extensions::MissingToolchain;
use std::path::Path;

use super::super::{EnvironmentShell, LiveEnv};
use super::{ServerKey, ServerSlot};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_1, SPACE_2, TEXT_SM};

const BTN_INSTALL: &str = "language-server-install";
const BTN_NOT_NOW: &str = "language-server-not-now";
const BTN_RETRY: &str = "language-server-retry";
const BTN_CHOOSE: &str = "toolchain-choose";
const BTN_INSTALL_TOOLCHAIN: &str = "toolchain-install";
const INSTALL_LABEL: &str = "Install";
const NOT_NOW_LABEL: &str = "Not now";
const RETRY_LABEL: &str = "Retry";
const CHOOSE_LABEL: &str = "Choose folder…";

enum Notice {
    Toolchain(MissingToolchain, String),
    Offer(String),
    Busy(String),
    Failed(String),
}

fn notice(key: &ServerKey, slot: &ServerSlot, language: &str) -> Option<Notice> {
    let server = &key.plan.package;
    match slot {
        ServerSlot::NeedsToolchain(missing) => Some(Notice::Toolchain(
            missing.clone(),
            format!(
                "{language} needs {}, which was not found. Install it or choose the folder where you have it.",
                missing.name
            ),
        )),
        ServerSlot::InstallingToolchain(name) => Some(Notice::Busy(format!("Installing {name}…"))),
        ServerSlot::AwaitingConsent => Some(Notice::Offer(format!(
            "Install {server} for completions, diagnostics and go to definition in {language} files?"
        ))),
        ServerSlot::Installing => Some(Notice::Busy(format!("Installing {server}…"))),
        ServerSlot::Failed(reason) => Some(Notice::Failed(format!("{server} is not running: {reason}"))),
        ServerSlot::Locating | ServerSlot::Starting | ServerSlot::Running(_) => None,
    }
}

impl EnvironmentShell {
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
        let bar =
            match found {
                Notice::Toolchain(missing, text) => {
                    let (choose, install) = (key.clone(), key.clone());
                    let (chosen_id, install_id) = (missing.id.clone(), missing.id.clone());
                    let (chosen_name, install_name) = (missing.name.clone(), missing.name.clone());
                    let installable = missing.installable;
                    bar.text_color(rgb(pal.text_default))
                        .child(div().flex_1().min_w_0().child(text))
                        .child(Self::not_now(key, cx))
                        .child(
                            Button::new(BTN_CHOOSE)
                                .label(CHOOSE_LABEL)
                                .small()
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.choose_toolchain_folder(
                                        choose.clone(),
                                        chosen_id.clone(),
                                        chosen_name.clone(),
                                        cx,
                                    );
                                })),
                        )
                        .children(installable.then(|| {
                            Button::new(BTN_INSTALL_TOOLCHAIN)
                                .label(format!("{INSTALL_LABEL} {install_name}"))
                                .small()
                                .variant(ButtonVariant::Primary)
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    this.install_toolchain(
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
                Notice::Failed(text) => bar
                    .text_color(rgb(pal.warning_fg))
                    .child(div().flex_1().min_w_0().child(text))
                    .child(Button::new(BTN_RETRY).label(RETRY_LABEL).small().on_click(
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            this.retry_server(key.clone(), cx);
                        }),
                    )),
            };
        Some(bar.into_any_element())
    }
}
