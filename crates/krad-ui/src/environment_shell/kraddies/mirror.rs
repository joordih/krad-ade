use gpui_kit::Context;
use krad_domain::{KraddieId, ToastKind};
use krad_protocol::{Command, KraddieCommand, KraddieState, KraddiesSnapshot, OwnedDigests, Reply};

use super::super::EnvironmentShell;
use super::{AutomationEntry, KraddieEntry};

const FAILED_RUN_SEPARATOR: &str = ": ";
const NOTES_TOLD: usize = 12;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn tell_kraddies(
        &mut self,
        command: KraddieCommand,
        cx: &mut Context<Self>,
    ) {
        let pending = self.runtime.handle.send(
            self.runtime.handle.next_request(),
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
        if let Some(opened_at) = snapshot.opened_at {
            self.kraddies.opened_at = opened_at;
        }
        let kept: Vec<KraddieId> = snapshot.kraddies.iter().map(KraddieState::id).collect();
        let gone: Vec<KraddieId> = self
            .kraddies
            .entries
            .iter()
            .map(|entry| entry.kraddie.id())
            .filter(|id| !kept.contains(id))
            .collect();
        for id in &gone {
            self.kraddie_removed(id);
        }
        for state in snapshot.kraddies {
            self.kraddie_heard(state);
        }
        self.kraddies.memory.replace_books(snapshot.digests);
    }

    pub(in crate::environment_shell) fn kraddie_heard(&mut self, state: KraddieState) {
        let id = state.id();
        let KraddieState {
            kraddie,
            automations,
            check_ins,
            mind,
            activity,
            last_check_in,
        } = state;
        let automations: Vec<AutomationEntry> = automations
            .into_iter()
            .map(|state| AutomationEntry {
                automation: state.automation,
                history: state.history,
            })
            .collect();
        let entries = &mut self.kraddies.entries;
        match entries.iter_mut().find(|entry| entry.kraddie.id() == id) {
            Some(entry) if entry.kraddie.design == kraddie.design => {
                entry.kraddie = kraddie;
                entry.automations = automations;
                entry.check_ins = check_ins;
            }
            _ => {
                entries.retain(|entry| entry.kraddie.id() != id);
                let mut entry = KraddieEntry::new(kraddie, automations);
                entry.check_ins = check_ins;
                entries.push(entry);
                entries.sort_by_key(|entry| entry.kraddie.name.as_str().to_lowercase());
            }
        }
        self.kraddies
            .profile
            .adopt(&id, mind, activity, last_check_in);
    }

    pub(in crate::environment_shell) fn kraddie_removed(&mut self, id: &KraddieId) {
        self.kraddies
            .entries
            .retain(|entry| entry.kraddie.id() != *id);
        self.kraddies.profile.forget(id);
        self.kraddies.memory.forget_book(id);
        self.forget_automation_selection(id);
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
}
