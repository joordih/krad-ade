use gpui_kit::Context;
use krad_domain::{KraddieId, ToastKind};
use krad_protocol::{Command, KraddieCommand, Reply};

use super::super::runtime_view::KraddieBook;
use super::super::EnvironmentShell;

const FAILED_RUN_SEPARATOR: &str = ": ";
const NOTES_TOLD: usize = 12;
const KRADDIE_NOT_SAVED: &str = "A Kraddie change was not saved";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn tell_kraddies(
        &mut self,
        command: KraddieCommand,
        cx: &mut Context<Self>,
    ) {
        let pending = self.runtime.client.send_as(
            self.runtime.client.next_request(),
            Command::Kraddie(command),
        );
        cx.spawn(async move |this, cx| {
            let Ok(Reply::Refused { message, .. }) = pending.await else {
                return;
            };
            this.update(cx, |shell, cx| shell.toast(ToastKind::Error, &message, cx))
                .unwrap_or_default();
        })
        .detach();
    }

    pub(in crate::environment_shell) fn tell_notes_to_kraddies(&mut self, cx: &mut Context<Self>) {
        let titles = self
            .notes
            .index
            .notes()
            .iter()
            .filter(|note| !note.generated)
            .take(NOTES_TOLD)
            .map(|note| note.title.clone())
            .collect();
        self.tell_kraddies(KraddieCommand::UseNotes { titles }, cx);
    }

    pub(in crate::environment_shell) fn kraddies_redrawn(&mut self) {
        let gone = self.kraddies.redraw(&self.runtime.kraddies.entries);
        for id in &gone {
            self.kraddies.profile.forget(id);
            self.kraddies.memory.forget_book(id);
            self.forget_automation_selection(id);
        }
    }

    pub(in crate::environment_shell) fn edit_kraddie_book<R>(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut KraddieBook) -> R,
    ) -> R {
        let edited = self
            .runtime
            .view
            .update(cx, |model, _| model.kraddies.edit(edit));
        self.runtime.reread(cx);
        edited
    }

    pub(in crate::environment_shell) fn kraddie_removed(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.runtime
            .view
            .update(cx, |model, _| model.kraddies.removed(id));
        self.runtime.reread(cx);
        self.kraddies_redrawn();
    }

    pub(in crate::environment_shell) fn kraddie_run_failed(
        &mut self,
        task: &str,
        reason: &str,
        cx: &mut Context<Self>,
    ) {
        let told = format!("{task}{FAILED_RUN_SEPARATOR}{reason}");
        self.toast(ToastKind::Error, &told, cx);
    }

    pub(in crate::environment_shell) fn kraddie_not_saved(
        &mut self,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        let told = format!("{KRADDIE_NOT_SAVED}{FAILED_RUN_SEPARATOR}{error}");
        self.toast(ToastKind::Error, &told, cx);
    }
}
