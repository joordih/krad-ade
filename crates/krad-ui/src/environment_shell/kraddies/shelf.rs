use gpui_kit::Context;
use krad_domain::{Automation, AutomationId, AutomationKey, Kraddie, KraddieId, Switch};
use krad_protocol::KraddieCommand;

use super::super::EnvironmentShell;

impl EnvironmentShell {
    pub(in crate::environment_shell) fn save_kraddie(
        &mut self,
        kraddie: Kraddie,
        replaced: Option<&KraddieId>,
        cx: &mut Context<Self>,
    ) {
        let id = kraddie.id();
        if let Some(old) = replaced.filter(|old| **old != id) {
            self.kraddies.profile.rename(old, &id);
            self.rename_kraddie_memory(old, &id);
        }
        let command = KraddieCommand::Save {
            kraddie: Box::new(kraddie),
            replaced: replaced.cloned(),
        };
        self.tell_kraddies(command, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn import_kraddie(
        &mut self,
        kraddie: Kraddie,
        automations: Vec<Automation>,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::Import {
            kraddie: Box::new(kraddie),
            automations,
        };
        self.tell_kraddies(command, cx);
    }

    pub(in crate::environment_shell) fn delete_kraddie(
        &mut self,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        self.kraddie_removed(id, cx);
        let command = KraddieCommand::Delete {
            kraddie: id.clone(),
        };
        self.tell_kraddies(command, cx);
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
        let entry = self.runtime.kraddies.entry(kraddie)?;
        if !stays {
            automation.id =
                AutomationId::after(entry.automations.iter().map(|known| &known.automation.id));
        }
        let key = AutomationKey::new(kraddie.clone(), automation.id);
        let command = KraddieCommand::SaveAutomation {
            kraddie: kraddie.clone(),
            automation: Box::new(automation),
            previous: previous.cloned(),
        };
        self.tell_kraddies(command, cx);
        cx.notify();
        Some(key)
    }

    pub(in crate::environment_shell) fn delete_automation(
        &mut self,
        key: &AutomationKey,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::DeleteAutomation {
            automation: key.clone(),
        };
        self.tell_kraddies(command, cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn switch_automation(
        &mut self,
        key: &AutomationKey,
        switch: Switch,
        cx: &mut Context<Self>,
    ) {
        let command = KraddieCommand::SwitchAutomation {
            automation: key.clone(),
            switch,
        };
        self.tell_kraddies(command, cx);
        cx.notify();
    }
}
