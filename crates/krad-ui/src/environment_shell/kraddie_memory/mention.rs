use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{
    kraddie_chip_text, matching_kraddies, mention_pack, mentioned_kraddies, EnvironmentName,
    KraddieChip, KraddieId, KraddieMood, KraddieName, MemoryOwner, MemorySize, PackRequest,
    TerminalId, MENTION_PACK_TOKENS,
};
use std::rc::Rc;
use std::time::SystemTime;

use super::super::EnvironmentShell;
use crate::components::motion::drop_in;
use crate::components::{icon, tooltip_view, IconName};
use crate::kraddie_avatar::{kraddie_avatar, KraddieSprite};
use crate::theme::{palette, ICON_XS, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS};

const CHIP_PREFIX: &str = "kraddie-chip";
const CHIP_REMOVE_PREFIX: &str = "kraddie-chip-remove";
const CHIP_AVATAR: f32 = 16.0;
const CHIP_REMOVE: &str = "Leave this Kraddie's memory out";
const PACK_SEPARATOR: &str = "\n\n";

pub(in crate::environment_shell) struct KraddieSuggestion {
    pub(in crate::environment_shell) id: KraddieId,
    pub(in crate::environment_shell) name: KraddieName,
    pub(in crate::environment_shell) size: MemorySize,
    pub(in crate::environment_shell) sprite: Rc<KraddieSprite>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_suggestions(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<KraddieSuggestion> {
        let names = self
            .kraddies
            .entries
            .iter()
            .map(|entry| &entry.kraddie.name);
        matching_kraddies(names, query)
            .into_iter()
            .take(limit)
            .filter_map(|name| {
                let id = name.id();
                let entry = self.kraddies.entry(&id)?;
                Some(KraddieSuggestion {
                    size: self.memory_size(&id),
                    sprite: Rc::clone(entry.sprite(KraddieMood::Idle)),
                    name: name.clone(),
                    id,
                })
            })
            .collect()
    }

    fn kraddie_chip(&self, id: &KraddieId) -> Option<KraddieChip> {
        let entry = self.kraddies.entry(id)?;
        let owner = MemoryOwner::Kraddie(id.clone());
        let library = self.memory_library();
        let request = PackRequest {
            home: &owner,
            query: "",
            budget: MENTION_PACK_TOKENS,
            now: SystemTime::now(),
        };
        Some(KraddieChip {
            kraddie: id.clone(),
            name: entry.kraddie.name.clone(),
            size: self.memory_size(id),
            pack: mention_pack(
                entry.kraddie.name.as_str(),
                library.of_owner(&owner),
                &request,
            ),
        })
    }

    pub(in crate::environment_shell) fn attach_kraddie_chip(
        &mut self,
        composer: TerminalId,
        id: &KraddieId,
    ) -> Option<String> {
        let chip = self.kraddie_chip(id)?;
        let text = chip.text();
        let chips = self.kraddies.memory.chips.entry(composer).or_default();
        if !chips.iter().any(|known| known.kraddie == *id) {
            chips.push(chip);
        }
        Some(text)
    }

    fn remove_kraddie_chip(
        &mut self,
        composer: TerminalId,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) {
        if let Some(chips) = self.kraddies.memory.chips.get_mut(&composer) {
            chips.retain(|chip| chip.kraddie != *id);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn with_kraddie_packs(
        &mut self,
        env: &EnvironmentName,
        composer: TerminalId,
        text: String,
    ) -> String {
        let known: Vec<KraddieId> = self
            .kraddies
            .entries
            .iter()
            .map(|entry| entry.kraddie.id())
            .collect();
        let chosen: Vec<KraddieId> = self
            .kraddies
            .memory
            .chips
            .remove(&composer)
            .unwrap_or_default()
            .into_iter()
            .map(|chip| chip.kraddie)
            .filter(|id| text.contains(&kraddie_chip_text(id)))
            .chain(mentioned_kraddies(&text, &known))
            .fold(Vec::new(), |mut chosen, id| {
                if !chosen.contains(&id) {
                    chosen.push(id);
                }
                chosen
            });
        if chosen.is_empty() {
            return text;
        }
        if let Some(live) = self.envs.get_mut(env) {
            live.data.note_kraddie_mentions(composer, &chosen);
        }
        let packs: Vec<String> = chosen
            .iter()
            .filter_map(|id| self.kraddie_chip(id))
            .filter(|chip| !chip.pack.is_empty())
            .map(|chip| chip.pack.text)
            .collect();
        tracing::debug!(
            chat = composer.as_u64(),
            mentioned = chosen.len(),
            "kraddie memory attached"
        );
        std::iter::once(text)
            .chain(packs)
            .collect::<Vec<_>>()
            .join(PACK_SEPARATOR)
    }

    pub(in crate::environment_shell) fn kraddie_chip_tray(
        &self,
        composer: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let chips = self
            .kraddies
            .memory
            .chips
            .get(&composer)
            .filter(|chips| !chips.is_empty())?;
        let pal = palette(cx);
        let rows = chips.iter().filter_map(|chip| {
            let entry = self.kraddies.entry(&chip.kraddie)?;
            let sprite = entry.sprite(KraddieMood::Idle);
            let tooltip = SharedString::from(chip.tooltip());
            let target = chip.kraddie.clone();
            let key = format!("{}-{}", composer.as_u64(), chip.kraddie);
            let remove = div()
                .id(ElementId::Name(
                    format!("{CHIP_REMOVE_PREFIX}-{key}").into(),
                ))
                .flex_none()
                .cursor_pointer()
                .rounded_full()
                .hover(|style| style.bg(pal.bg_hover))
                .tooltip(|_, cx| tooltip_view(CHIP_REMOVE.into(), cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.remove_kraddie_chip(composer, &target, cx);
                }))
                .child(icon(IconName::Close, ICON_XS, pal.text_muted));
            let chip_row = div()
                .id(ElementId::Name(format!("{CHIP_PREFIX}-{key}").into()))
                .relative()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_1_5))
                .pl(px(SPACE_1))
                .pr(px(SPACE_1_5))
                .py(px(SPACE_1))
                .rounded_full()
                .border_1()
                .border_color(pal.border)
                .bg(rgb(pal.bg_surface))
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_strong))
                .when(chip.pack.truncated(), |row| {
                    row.border_color(rgb(pal.accent))
                })
                .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
                .child(kraddie_avatar(sprite, CHIP_AVATAR, self.kraddies_frame()))
                .child(chip.name.to_string())
                .child(
                    div()
                        .text_color(rgb(pal.text_muted))
                        .child(chip.size.to_string()),
                )
                .child(remove);
            Some(drop_in(
                ElementId::Name(format!("{CHIP_PREFIX}-in-{key}").into()),
                chip_row,
            ))
        });
        Some(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(SPACE_2))
                .px(px(SPACE_1))
                .children(rows)
                .into_any_element(),
        )
    }
}
