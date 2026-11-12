use gpui_kit::{AppContext as _, Context, PromptButton, PromptLevel, Window};
use krad_domain::{ServerRuntimeId, ToastKind};

use super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::EnvironmentShell;
use super::commands::ExtensionMenu;
use super::prompts::{Prompt, PromptAnswer};
use super::{Deferred, Slot, SlotKey};

const NATIVE_ALERTS: bool = cfg!(target_os = "macos");
const CONFIRMED: usize = 0;
const INSTALL_LABEL: &str = "Install";
const CANCEL_LABEL: &str = "Not now";
const INSTALL_ON: &str = "on";
const INSTALL_FOR: &str = "for";
const THIS_COMPUTER: &str = "this computer";
const INSTALLING_TEXT: &str = "Installing";
const INSTALL_FAILED_TEXT: &str = "could not be installed";
const MENU_ORIGIN_X_SHARE: f32 = 0.5;
const MENU_ORIGIN_Y_SHARE: f32 = 0.3;

pub(super) struct PendingInstall {
    pub(super) key: SlotKey,
    pub(super) runtime: ServerRuntimeId,
    pub(super) waiting: Vec<Deferred>,
}

fn install_question(runtime: &str, host: &str, extension: &str) -> String {
    format!("{INSTALL_LABEL} {runtime} {INSTALL_ON} {host} {INSTALL_FOR} {extension}?")
}

impl EnvironmentShell {
    pub(super) fn offer_runtime_install(
        &mut self,
        key: &SlotKey,
        runtime: ServerRuntimeId,
        waiting: Vec<Deferred>,
        cx: &mut Context<Self>,
    ) {
        let pending = PendingInstall {
            key: key.clone(),
            runtime,
            waiting,
        };
        self.queue_extension_prompt(Prompt::InstallRuntime(pending), cx);
    }

    fn runtime_question(&self, pending: &PendingInstall) -> String {
        let runtime = self.language_tools().map_or_else(
            || pending.runtime.to_string(),
            |tools| tools.server_runtime_name(&pending.runtime),
        );
        let host = pending
            .key
            .environment
            .as_ref()
            .map_or_else(|| THIS_COMPUTER.to_owned(), |name| self.env_label(name));
        install_question(
            &runtime,
            &host,
            &self.extensions.title_of(&pending.key.extension),
        )
    }

    pub(super) fn show_runtime_install(
        &mut self,
        pending: PendingInstall,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let question = self.runtime_question(&pending);
        self.extensions.prompts.install = Some(pending);
        if NATIVE_ALERTS {
            let answer = window.prompt(
                PromptLevel::Info,
                &question,
                None,
                &[
                    PromptButton::ok(INSTALL_LABEL),
                    PromptButton::cancel(CANCEL_LABEL),
                ],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                let install = matches!(answer.await, Ok(CONFIRMED));
                this.update(cx, |shell, cx| shell.runtime_install_answered(install, cx))
                    .unwrap_or_default();
            })
            .detach();
            return;
        }
        let answer = |install: bool| {
            MenuAction::Extension(Box::new(ExtensionMenu::Answer(
                PromptAnswer::InstallRuntime(install),
            )))
        };
        let entries = vec![
            MenuEntry::Header(question),
            MenuEntry::item(INSTALL_LABEL, None, answer(true)),
            MenuEntry::Separator,
            MenuEntry::item(CANCEL_LABEL, None, answer(false)),
        ];
        let size = window.viewport_size();
        let origin = (
            size.width.as_f32() * MENU_ORIGIN_X_SHARE,
            size.height.as_f32() * MENU_ORIGIN_Y_SHARE,
        );
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    pub(super) fn runtime_install_answered(&mut self, install: bool, cx: &mut Context<Self>) {
        let Some(pending) = self.extensions.prompts.install.take() else {
            return;
        };
        if !install {
            cx.notify();
            return;
        }
        let (Some(tools), Some((link, platform))) =
            (self.language_tools(), self.program_link(&pending.key))
        else {
            return;
        };
        let name = tools.server_runtime_name(&pending.runtime);
        self.push_toast(ToastKind::Info, format!("{INSTALLING_TEXT} {name}"));
        let chosen = self.settings.server_runtimes.clone();
        let runtime = pending.runtime.clone();
        let installing = cx.background_spawn(async move {
            tools.install_server_runtime_on(&link, platform, &runtime, &chosen)
        });
        cx.spawn(async move |this, cx| {
            let installed = installing.await;
            this.update(cx, |shell, cx| match installed {
                Ok(()) => {
                    if matches!(
                        shell.extensions.slots.get(&pending.key),
                        Some(Slot::Failed(_))
                    ) {
                        shell.extensions.slots.remove(&pending.key);
                    }
                    shell.start_program(pending.key, pending.waiting, cx);
                }
                Err(error) => {
                    tracing::warn!(%error, "server runtime install for an extension failed");
                    shell.toast(
                        ToastKind::Error,
                        &format!("{name} {INSTALL_FAILED_TEXT}: {error}"),
                        cx,
                    );
                }
            })
            .unwrap_or_default();
        })
        .detach();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_question_names_the_runtime_the_host_and_the_extension() {
        assert_eq!(
            install_question("Node.js", "staging", "Docker"),
            "Install Node.js on staging for Docker?"
        );
    }
}
