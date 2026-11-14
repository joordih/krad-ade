use gpui_kit::Context;
use krad_domain::{KraddieId, ToastKind};
use krad_protocol::{Command, KraddieCommand, KraddieState, KraddiesSnapshot, OwnedDigests, Reply};

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

    pub(in crate::environment_shell) fn kraddies_replaced(&mut self, snapshot: KraddiesSnapshot) {
        for state in snapshot.kraddies {
            self.kraddie_heard(state);
        }
        self.kraddies.memory.replace_books(snapshot.digests);
    }

    pub(in crate::environment_shell) fn kraddie_heard(&mut self, state: KraddieState) {
        let id = state.id();
        let KraddieState {
            mind,
            activity,
            last_check_in,
            ..
        } = state;
        self.kraddies
            .profile
            .adopt(&id, mind, activity, last_check_in);
    }

    pub(in crate::environment_shell) fn kraddies_redrawn(&mut self) {
        let gone = self.kraddies.redraw(&self.runtime.kraddies.entries);
        for id in &gone {
            self.kraddies.profile.forget(id);
            self.kraddies.memory.forget_book(id);
            self.forget_automation_selection(id);
        }
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

    pub(in crate::environment_shell) fn digests_heard(&mut self, owned: OwnedDigests) {
        self.kraddies.memory.adopt_book(owned);
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
