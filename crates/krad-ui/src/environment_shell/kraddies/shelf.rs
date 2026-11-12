use gpui_kit::{AppContext as _, Context};
use krad_domain::{
    Automation, AutomationId, AutomationKey, Kraddie, KraddieHistory, KraddieId, Switch, ToastKind,
};

use krad_store::KraddieRename;

use super::super::EnvironmentShell;
use super::{AutomationEntry, KraddieEntry};

const SAVE_FAILED: &str = "The Kraddie could not be saved";
const RENAME_FAILED: &str =
    "The Kraddie could not be renamed; its files keep the old name until it is saved again";
const AUTOMATIONS_FAILED: &str = "The automations could not be saved";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn save_histories(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.kraddies.shelf.clone() else {
            self.kraddies.dirty.clear();
            return;
        };
        let changed: Vec<(AutomationKey, KraddieHistory)> =
            std::mem::take(&mut self.kraddies.dirty)
                .into_iter()
                .filter_map(|key| {
                    let history = self.kraddies.automation(&key)?.1.history.clone();
                    Some((key, history))
                })
                .collect();
        if changed.is_empty() {
            return;
        }
        cx.background_spawn(async move {
            for (key, history) in changed {
                if let Err(error) = store.write_history(&key, &history) {
                    tracing::warn!(automation = %key, %error, "automation history not saved");
                }
            }
        })
        .detach();
    }

    fn persist_automations(&self, kraddie: &KraddieId, cx: &mut Context<Self>) {
        let (Some(store), Some(entry)) =
            (self.kraddies.shelf.clone(), self.kraddies.entry(kraddie))
        else {
            return;
        };
        let automations: Vec<Automation> = entry
            .automations
            .iter()
            .map(|entry| entry.automation.clone())
            .collect();
        let kraddie = kraddie.clone();
        let write =
            cx.background_spawn(async move { store.write_automations(&kraddie, &automations) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = write.await {
                tracing::warn!(%error, "automations not saved");
                this.update(cx, |shell, cx| {
                    shell.toast(ToastKind::Error, AUTOMATIONS_FAILED, cx);
                })
                .unwrap_or_default();
            }
        })
        .detach();
    }

    pub(in crate::environment_shell) fn save_kraddie(
        &mut self,
        kraddie: Kraddie,
        replaced: Option<&KraddieId>,
        cx: &mut Context<Self>,
    ) {
        let automations = replaced
            .and_then(|old| {
                let index = self
                    .kraddies
                    .entries
                    .iter()
                    .position(|entry| entry.kraddie.id() == *old)?;
                Some(self.kraddies.entries.remove(index).automations)
            })
            .unwrap_or_default();
        self.adopt_new_kraddie(kraddie, automations, replaced, cx);
    }

    pub(in crate::environment_shell) fn adopt_new_kraddie(
        &mut self,
        kraddie: Kraddie,
        automations: Vec<AutomationEntry>,
        replaced: Option<&KraddieId>,
        cx: &mut Context<Self>,
    ) {
        let id = kraddie.id();
        let renamed = replaced.filter(|old| **old != id).cloned();
        if let Some(old) = &renamed {
            self.kraddies.limiter.end_kraddie(old);
            self.kraddies.profile.rename(old, &id);
            self.rename_kraddie_memory(old, &id, cx);
        }
        let moved: Vec<Automation> = automations
            .iter()
            .map(|entry| entry.automation.clone())
            .collect();
        let histories: Vec<(AutomationKey, KraddieHistory)> = automations
            .iter()
            .map(|entry| {
                let key = AutomationKey::new(id.clone(), entry.automation.id);
                (key, entry.history.clone())
            })
            .collect();
        self.kraddies
            .entries
            .retain(|entry| entry.kraddie.id() != id);
        self.kraddies
            .entries
            .push(KraddieEntry::new(kraddie.clone(), automations));
        self.kraddies
            .entries
            .sort_by_key(|entry| entry.kraddie.name.as_str().to_lowercase());
        let Some(old) = renamed else {
            self.kraddies
                .dirty
                .extend(histories.into_iter().map(|(key, _)| key));
            self.write_kraddie(kraddie, cx);
            self.persist_automations(&id, cx);
            self.save_histories(cx);
            cx.notify();
            return;
        };
        if let Some(store) = self.kraddies.shelf.clone() {
            let renaming = cx.background_spawn(async move {
                store.rename(&KraddieRename {
                    old: &old,
                    kraddie: &kraddie,
                    automations: &moved,
                    histories: &histories,
                })
            });
            cx.spawn(async move |this, cx| {
                if let Err(error) = renaming.await {
                    tracing::warn!(%error, "kraddie not renamed");
                    this.update(cx, |shell, cx| {
                        shell.toast(ToastKind::Error, RENAME_FAILED, cx);
                    })
                    .unwrap_or_default();
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn write_kraddie(&self, kraddie: Kraddie, cx: &mut Context<Self>) {
        let Some(store) = self.kraddies.shelf.clone() else {
            return;
        };
        let write = cx.background_spawn(async move { store.write(&kraddie) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = write.await {
                tracing::warn!(%error, "kraddie not saved");
                this.update(cx, |shell, cx| {
                    shell.toast(ToastKind::Error, SAVE_FAILED, cx);
                })
                .unwrap_or_default();
            }
        })
        .detach();
    }

    pub(in crate::environment_shell) fn delete_kraddie(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.kraddies
            .entries
            .retain(|entry| entry.kraddie.id() != *id);
        self.kraddies.limiter.end_kraddie(id);
        self.kraddies.profile.forget(id);
        self.forget_automation_selection(id);
        let Some(store) = self.kraddies.shelf.clone() else {
            return;
        };
        let id = id.clone();
        cx.background_spawn(async move {
            if let Err(error) = store.delete(&id) {
                tracing::warn!(kraddie = %id, %error, "kraddie not deleted");
            }
        })
        .detach();
        cx.notify();
    }

    pub(in crate::environment_shell) fn save_automation(
        &mut self,
        kraddie: &KraddieId,
        mut automation: Automation,
        previous: Option<&AutomationKey>,
        cx: &mut Context<Self>,
    ) -> Option<AutomationKey> {
        let stays = previous.is_some_and(|key| key.kraddie == *kraddie);
        let history = previous
            .and_then(|key| self.take_automation(key, !stays, cx))
            .unwrap_or_default();
        let entry = self.kraddies.entry_mut(kraddie)?;
        if !stays {
            automation.id =
                AutomationId::after(entry.automations.iter().map(|known| &known.automation.id));
        }
        let key = AutomationKey::new(kraddie.clone(), automation.id);
        let at = entry
            .automations
            .iter()
            .position(|known| known.automation.id >= automation.id)
            .unwrap_or(entry.automations.len());
        entry.automations.insert(
            at,
            AutomationEntry {
                automation,
                history,
            },
        );
        self.kraddies.dirty.insert(key.clone());
        self.persist_automations(kraddie, cx);
        self.save_histories(cx);
        cx.notify();
        Some(key)
    }

    fn take_automation(
        &mut self,
        key: &AutomationKey,
        forget_history: bool,
        cx: &mut Context<Self>,
    ) -> Option<KraddieHistory> {
        let entry = self.kraddies.entry_mut(&key.kraddie)?;
        let at = entry
            .automations
            .iter()
            .position(|known| known.automation.id == key.automation)?;
        let taken = entry.automations.remove(at);
        self.persist_automations(&key.kraddie, cx);
        if let Some(store) = self.kraddies.shelf.clone().filter(|_| forget_history) {
            let key = key.clone();
            cx.background_spawn(async move {
                if let Err(error) = store.delete_history(&key) {
                    tracing::warn!(automation = %key, %error, "automation history not removed");
                }
            })
            .detach();
        }
        Some(taken.history)
    }

    pub(in crate::environment_shell) fn delete_automation(
        &mut self,
        key: &AutomationKey,
        cx: &mut Context<Self>,
    ) {
        self.kraddies.limiter.end(key);
        self.kraddies.dirty.remove(key);
        self.take_automation(key, true, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn switch_automation(
        &mut self,
        key: &AutomationKey,
        switch: Switch,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(entry) = self.kraddies.automation_mut(key) else {
            return false;
        };
        entry.automation.switch = switch;
        self.persist_automations(&key.kraddie, cx);
        cx.notify();
        true
    }
}
