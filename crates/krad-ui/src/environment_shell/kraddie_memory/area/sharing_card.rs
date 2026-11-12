use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{ChatDigests, KraddieId, KraddieMood, KraddieWorkspace, Switch, ToastKind};

use super::super::super::EnvironmentShell;
use super::super::layout::AreaLayout;
use super::{area_card, card_header};
use crate::components::{icon, toggle_switch, tooltip_view, Button, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{
    palette, Palette, ICON_XS, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_SM, SPACE_1, SPACE_1_5, SPACE_2,
    SPACE_3, TEXT_SM, TEXT_XS,
};

const SHARING_TITLE: &str = "Memory and reach";
const SHARING_NOTE: &str = "What it writes down, whose memory it reads and where it works";
const DIGESTS_LABEL: &str = "Write a digest when a chat ends or rests";
const DIGESTS_PREFIX: &str = "kraddie-digests-switch";
const READS_HEADING: &str = "Can read memory of";
const READS_HINT: &str = "One way: it reads them, they do not read it back";
const READ_PREFIX: &str = "kraddie-reads";
const NO_OTHERS: &str = "No other Kraddie to read from yet";
const READ_BY_HEADING: &str = "Read by";
const READ_BY_NOBODY: &str = "No other Kraddie reads its memory";
const FOLDER_HEADING: &str = "Working folder";
const FOLDER_NONE: &str = "Each chat picks its group";
const FOLDER_USE_ID: &str = "kraddie-folder-use";
const FOLDER_USE_LABEL: &str = "Use the open group";
const FOLDER_CLEAR_ID: &str = "kraddie-folder-clear";
const FOLDER_CLEAR_LABEL: &str = "Let each chat pick";
const FOLDER_SHARED_HINT: &str = "Its automations work here too";
const NO_GROUP_OPEN: &str = "Open a group first; its folder becomes the Kraddie's";
const READ_ON_TIP: &str = "Stop reading this Kraddie's memory";
const READ_OFF_TIP: &str = "Read this Kraddie's memory in every chat";
const ARROW_AVATAR: f32 = 18.0;
const LIST_SEPARATOR: &str = ", ";

fn section(heading: &'static str, pal: &Palette) -> Div {
    div().flex().flex_col().gap(px(SPACE_1_5)).child(
        div()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_subtle))
            .child(heading),
    )
}

fn muted(text: impl Into<SharedString>, pal: &Palette) -> Div {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text.into())
}

impl EnvironmentShell {
    pub(super) fn sharing_card(
        &self,
        id: &KraddieId,
        layout: &AreaLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let Some(entry) = self.kraddies.entry(id) else {
            return div().into_any_element();
        };
        let settings = &entry.kraddie.settings;
        let automatic = settings.digests.is_automatic();
        let target = id.clone();
        let digests = div()
            .id(ElementId::Name(format!("{DIGESTS_PREFIX}-{id}").into()))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(SPACE_2))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                let next = if automatic {
                    ChatDigests::Off
                } else {
                    ChatDigests::Automatic
                };
                this.set_chat_digests(&target, next, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(DIGESTS_LABEL),
            )
            .child(toggle_switch(
                ElementId::Name(format!("{DIGESTS_PREFIX}-knob-{id}").into()),
                automatic,
                pal,
            ));
        area_card(layout.sharing_w, pal)
            .child(card_header(SHARING_TITLE, SHARING_NOTE, pal))
            .child(digests)
            .child(self.reads_section(id, cx))
            .child(self.read_by_section(id, pal))
            .child(self.folder_section(id, cx))
            .into_any_element()
    }

    fn reads_section(&self, id: &KraddieId, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(reader) = self.kraddies.entry(id) else {
            return div().into_any_element();
        };
        let own = reader.sprite(KraddieMood::Idle);
        let frame = self.kraddies_frame();
        let rows: Vec<AnyElement> = self
            .kraddies
            .entries
            .iter()
            .filter(|other| other.kraddie.id() != *id)
            .map(|other| {
                let other_id = other.kraddie.id();
                let reads = reader.kraddie.settings.reads.contains(&other_id);
                let (reader_id, from) = (id.clone(), other_id.clone());
                let tip = if reads { READ_ON_TIP } else { READ_OFF_TIP };
                let arrow = if reads { pal.accent } else { pal.text_disabled };
                div()
                    .id(ElementId::Name(
                        format!("{READ_PREFIX}-{id}-{other_id}").into(),
                    ))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .h(px(NAV_ROW_H))
                    .px(px(SPACE_1_5))
                    .rounded(px(RADIUS_SM))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .tooltip(move |_, cx| tooltip_view(tip.into(), cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let switch = if reads { Switch::Off } else { Switch::On };
                        this.set_memory_sharing(&reader_id, &from, switch, cx);
                    }))
                    .child(kraddie_avatar(own, ARROW_AVATAR, frame))
                    .child(icon(IconName::ChevronRight, ICON_XS, arrow))
                    .child(kraddie_avatar(
                        other.sprite(KraddieMood::Idle),
                        ARROW_AVATAR,
                        frame,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(if reads {
                                pal.text_strong
                            } else {
                                pal.text_default
                            }))
                            .child(other.kraddie.name.to_string()),
                    )
                    .child(toggle_switch(
                        ElementId::Name(format!("{READ_PREFIX}-knob-{id}-{other_id}").into()),
                        reads,
                        pal,
                    ))
                    .into_any_element()
            })
            .collect();
        let body = if rows.is_empty() {
            muted(NO_OTHERS, pal).into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .children(rows)
                .into_any_element()
        };
        section(READS_HEADING, pal)
            .child(muted(READS_HINT, pal))
            .child(body)
            .into_any_element()
    }

    fn read_by_section(&self, id: &KraddieId, pal: &Palette) -> AnyElement {
        let readers: Vec<String> = self
            .kraddies
            .entries
            .iter()
            .filter(|other| other.kraddie.settings.reads.contains(id))
            .map(|other| other.kraddie.name.to_string())
            .collect();
        let text = if readers.is_empty() {
            READ_BY_NOBODY.to_owned()
        } else {
            readers.join(LIST_SEPARATOR)
        };
        section(READ_BY_HEADING, pal)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .when(!readers.is_empty(), |row| {
                        row.child(icon(IconName::ChevronLeft, ICON_XS, pal.accent))
                    })
                    .child(muted(text, pal)),
            )
            .into_any_element()
    }

    fn folder_section(&self, id: &KraddieId, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let folder = self
            .kraddies
            .entry(id)
            .and_then(|entry| entry.kraddie.workspace.as_ref())
            .map(|place| place.repo.display().to_string());
        let has_folder = folder.is_some();
        let (use_id, clear_id) = (id.clone(), id.clone());
        let buttons = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(
                Button::new(ElementId::Name(format!("{FOLDER_USE_ID}-{id}").into()))
                    .label(FOLDER_USE_LABEL)
                    .icon(IconName::Folder)
                    .small()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.use_open_group_as_folder(&use_id, cx);
                    })),
            )
            .when(has_folder, |row| {
                row.child(
                    Button::new(ElementId::Name(format!("{FOLDER_CLEAR_ID}-{id}").into()))
                        .label(FOLDER_CLEAR_LABEL)
                        .small()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.set_kraddie_folder(&clear_id, None, cx);
                        })),
                )
            });
        section(FOLDER_HEADING, pal)
            .gap(px(SPACE_2))
            .child(
                div()
                    .truncate()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(folder.unwrap_or_else(|| FOLDER_NONE.to_owned())),
            )
            .when(has_folder, |section| {
                section.child(muted(FOLDER_SHARED_HINT, pal))
            })
            .child(buttons)
            .mt(px(SPACE_3))
            .into_any_element()
    }

    fn use_open_group_as_folder(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let Some(root) = self.active_group().map(|group| group.root.clone()) else {
            return self.toast(ToastKind::Warning, NO_GROUP_OPEN, cx);
        };
        let place = KraddieWorkspace {
            environment: self.active.clone(),
            repo: root,
        };
        self.set_kraddie_folder(id, Some(place), cx);
    }

    fn set_kraddie_folder(
        &mut self,
        id: &KraddieId,
        place: Option<KraddieWorkspace>,
        cx: &mut Context<Self>,
    ) {
        let Some(mut kraddie) = self.kraddies.entry(id).map(|entry| entry.kraddie.clone()) else {
            return;
        };
        if kraddie.workspace == place {
            return;
        }
        kraddie.workspace = place;
        self.save_kraddie(kraddie, Some(id), cx);
    }
}
