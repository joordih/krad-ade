use gpui_kit::Context;
use krad_domain::{ChatDigests, KraddieId, KraddieSettings, Switch};

use super::super::EnvironmentShell;

impl EnvironmentShell {
    fn tune_kraddie(
        &mut self,
        id: &KraddieId,
        edit: impl FnOnce(&mut KraddieSettings) -> bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut kraddie) = self
            .runtime
            .kraddies
            .entry(id)
            .map(|entry| entry.kraddie.clone())
        else {
            return;
        };
        if edit(&mut kraddie.settings) {
            self.save_kraddie(kraddie, Some(id), cx);
        }
    }

    pub(super) fn set_memory_sharing(
        &mut self,
        reader: &KraddieId,
        from: &KraddieId,
        switch: Switch,
        cx: &mut Context<Self>,
    ) {
        let own = reader.clone();
        let from = from.clone();
        self.tune_kraddie(
            reader,
            move |settings| match switch {
                Switch::On => settings.share_from(&own, from),
                Switch::Off => settings.unshare_from(&from),
            },
            cx,
        );
        tracing::info!(reader = %reader, switch = switch.display_name(), "kraddie memory sharing changed");
    }

    pub(super) fn set_chat_digests(
        &mut self,
        id: &KraddieId,
        digests: ChatDigests,
        cx: &mut Context<Self>,
    ) {
        self.tune_kraddie(
            id,
            move |settings| {
                let changed = settings.digests != digests;
                settings.digests = digests;
                changed
            },
            cx,
        );
    }

    pub(in crate::environment_shell) fn rename_kraddie_memory(
        &mut self,
        old: &KraddieId,
        new: &KraddieId,
    ) {
        let memory = &mut self.kraddies.memory;
        if let Some(book) = memory.books.remove(old) {
            memory.books.insert(new.clone(), book);
        }
        if memory.expanded.remove(old) {
            memory.expanded.insert(new.clone());
        }
    }
}
