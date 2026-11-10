use gpui_kit::{AppContext as _, Context};
use krad_domain::{ChatDigests, Kraddie, KraddieId, KraddieSettings, MemoryOwner, Switch};

use super::super::EnvironmentShell;

impl EnvironmentShell {
    fn tune_kraddie(
        &mut self,
        id: &KraddieId,
        edit: impl FnOnce(&mut KraddieSettings) -> bool,
        cx: &mut Context<Self>,
    ) {
        let Some(mut kraddie) = self.kraddies.entry(id).map(|entry| entry.kraddie.clone()) else {
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
        cx: &mut Context<Self>,
    ) {
        let memory = &mut self.kraddies.memory;
        if let Some(book) = memory.books.remove(old) {
            memory.books.insert(new.clone(), book);
        }
        for set in [&mut memory.loaded, &mut memory.loading] {
            if set.remove(&MemoryOwner::Kraddie(old.clone())) {
                set.insert(MemoryOwner::Kraddie(new.clone()));
            }
        }
        if memory.expanded.remove(old) {
            memory.expanded.insert(new.clone());
        }
        for live in self.envs.values_mut() {
            live.data.rename_kraddie(old, new);
        }
        let changed: Vec<Kraddie> = self
            .kraddies
            .entries
            .iter_mut()
            .filter_map(|entry| {
                let reads = &mut entry.kraddie.settings.reads;
                let at = reads.iter().position(|read| read == old)?;
                reads[at] = new.clone();
                Some(entry.kraddie.clone())
            })
            .collect();
        let Some(files) = self.kraddies.memory.shelf.clone() else {
            return;
        };
        if changed.is_empty() {
            return;
        }
        cx.background_spawn(async move {
            for kraddie in changed {
                if let Err(error) = files.write(&kraddie) {
                    tracing::warn!(kraddie = %kraddie.id(), %error, "renamed reader not saved");
                }
            }
        })
        .detach();
    }
}
