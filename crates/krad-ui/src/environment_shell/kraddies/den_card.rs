use chrono::{DateTime, Local};
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{KraddieId, KraddieMood, KraddieRun};

use super::super::EnvironmentShell;
use super::motion::{entering, idling};
use super::profile::{mood_color, mood_line};
use super::tile::{caption, lifted};
use super::KraddieEntry;
use crate::components::{Button, IconName};
use crate::kraddie_avatar::kraddie_avatar;
use crate::theme::{
    Palette, LINE_LG, LINE_XS, RADIUS_XL, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, SPACE_5, TEXT_LG,
    TEXT_XS,
};
use gpui_kit::component::switch::Switch;

const AVATAR_SIDE: f32 = 56.0;
const MOOD_DOT: f32 = 7.0;
const HISTORY_SHOWN: usize = 3;
const SLACK: f32 = 0.5;
const CARD_PREFIX: &str = "kraddie-den-card";
const BOB_PREFIX: &str = "kraddie-den-bob";
const SWITCH_PREFIX: &str = "kraddie-switch";
const PAGE_TOOLTIP: &str = "Visit its page";
const RUN_TOOLTIP: &str = "Wake it up now";
const EDIT_TOOLTIP: &str = "Rename, or change its agent and home";
const DESIGN_TOOLTIP: &str = "Paint it in the studio";
const EXPORT_TOOLTIP: &str = "Save it as a .kraddie file";
const DELETE_TOOLTIP: &str = "Say goodbye to it";
const SWITCH_ON_TOOLTIP: &str = "Awake: its routines run";
const SWITCH_OFF_TOOLTIP: &str = "Resting: its routines wait";
const ROUTINES_LABEL: &str = "routines";
const RUNS_LABEL: &str = "runs";
const NO_RUNS: &str = "Has not run yet";
const CAPTION_SEPARATOR: &str = " · ";
const STARTED_FORMAT: &str = "%a %d %b %H:%M";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KraddieAction {
    Page,
    Run,
    Edit,
    Design,
    Export,
    Delete,
}

impl KraddieAction {
    const ALL: [Self; 6] = [
        Self::Page,
        Self::Run,
        Self::Edit,
        Self::Design,
        Self::Export,
        Self::Delete,
    ];

    const fn prefix(self) -> &'static str {
        match self {
            Self::Page => "kraddie-page",
            Self::Run => "kraddie-run",
            Self::Edit => "kraddie-edit",
            Self::Design => "kraddie-design",
            Self::Export => "kraddie-export",
            Self::Delete => "kraddie-delete",
        }
    }

    const fn glyph(self) -> IconName {
        match self {
            Self::Page => IconName::Expand,
            Self::Run => IconName::Play,
            Self::Edit => IconName::Settings,
            Self::Design => IconName::Pencil,
            Self::Export => IconName::OpenExternal,
            Self::Delete => IconName::Minus,
        }
    }

    const fn tooltip(self) -> &'static str {
        match self {
            Self::Page => PAGE_TOOLTIP,
            Self::Run => RUN_TOOLTIP,
            Self::Edit => EDIT_TOOLTIP,
            Self::Design => DESIGN_TOOLTIP,
            Self::Export => EXPORT_TOOLTIP,
            Self::Delete => DELETE_TOOLTIP,
        }
    }
}

fn started_label(run: &KraddieRun) -> String {
    DateTime::<Local>::from(run.started)
        .format(STARTED_FORMAT)
        .to_string()
}

impl EnvironmentShell {
    fn kraddie_caption(&self, entry: &KraddieEntry) -> String {
        let harness = self.settings.acp.harness_name(entry.kraddie.harness);
        [
            harness.into_owned(),
            format!("{} {ROUTINES_LABEL}", entry.automations.len()),
            format!("{} {RUNS_LABEL}", entry.run_count()),
        ]
        .join(CAPTION_SEPARATOR)
    }

    pub(super) fn den_card(
        &self,
        index: usize,
        entry: &KraddieEntry,
        width: f32,
        pal: &'static Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.kraddie.id();
        let key = SharedString::from(id.to_string());
        let mood = self.kraddie_mood(entry);
        let sprite = entry.sprite(mood);
        self.kraddies.note_drawn(sprite);
        let avatar = idling(
            kraddie_avatar(sprite, AVATAR_SIDE, self.kraddies.frame()),
            (key.clone(), index),
        );
        let on = entry.kraddie.switch.is_on();
        let target = id.clone();
        let switch = Switch::new(SharedString::from(format!("{SWITCH_PREFIX}-{key}")))
            .checked(on)
            .tooltip(if on {
                SWITCH_ON_TOOLTIP
            } else {
                SWITCH_OFF_TOOLTIP
            })
            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                cx.stop_propagation();
                this.toggle_kraddie_switch(&target, cx);
            }));
        let switch = div()
            .id(SharedString::from(format!("{SWITCH_PREFIX}-hit-{key}")))
            .flex_none()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(switch)
            .into_any_element();
        let identity = Self::den_identity(entry, mood, switch, pal);
        let top = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_3))
            .child(avatar)
            .child(identity);
        let actions = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .children(KraddieAction::ALL.map(|action| Self::kraddie_button(action, &key, &id, cx)));
        let page = id.clone();
        let frame = lifted(
            div().id((SharedString::from(format!("{CARD_PREFIX}-{key}")), index)),
            pal,
        )
        .flex()
        .flex_col()
        .flex_none()
        .w(px((width - SLACK).max(0.0)))
        .gap(px(SPACE_3))
        .p(px(SPACE_5))
        .rounded(px(RADIUS_XL))
        .cursor_pointer()
        .on_click(cx.listener(move |this: &mut Self, _, _, cx| this.open_kraddie_page(&page, cx)))
        .child(top)
        .child(caption(self.kraddie_caption(entry), pal))
        .child(Self::kraddie_history(entry, pal))
        .child(actions)
        .children(self.farewell_row(&id, &key, pal, cx));
        entering(
            frame,
            (SharedString::from(format!("{BOB_PREFIX}-{key}")), index),
            index,
        )
        .into_any_element()
    }

    fn den_identity(
        entry: &KraddieEntry,
        mood: KraddieMood,
        switch: AnyElement,
        pal: &Palette,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TEXT_LG))
                            .line_height(px(LINE_LG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(entry.kraddie.name.to_string()),
                    )
                    .child(switch),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_default))
                    .child(
                        div()
                            .flex_none()
                            .size(px(MOOD_DOT))
                            .rounded_full()
                            .bg(rgb(mood_color(mood, pal))),
                    )
                    .child(mood_line(mood)),
            )
    }

    fn kraddie_button(
        action: KraddieAction,
        key: &SharedString,
        id: &KraddieId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = id.clone();
        Button::new(SharedString::from(format!("{}-{key}", action.prefix())))
            .icon(action.glyph())
            .small()
            .tooltip(action.tooltip())
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                cx.stop_propagation();
                this.perform_kraddie_action(action, &target, window, cx);
            }))
            .into_any_element()
    }

    fn perform_kraddie_action(
        &mut self,
        action: KraddieAction,
        id: &KraddieId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            KraddieAction::Page => self.open_kraddie_page(id, cx),
            KraddieAction::Run => self.run_kraddie_now(id, cx),
            KraddieAction::Edit => self.open_kraddie_editor(Some(id), window, cx),
            KraddieAction::Design => self.open_kraddie_studio(id, cx),
            KraddieAction::Export => self.export_kraddie_file(id, cx),
            KraddieAction::Delete => self.ask_delete_kraddie(id, window, cx),
        }
    }

    fn kraddie_history(entry: &KraddieEntry, pal: &Palette) -> AnyElement {
        let recent: Vec<String> = entry
            .runs_newest_first()
            .into_iter()
            .take(HISTORY_SHOWN)
            .map(|run| {
                [
                    started_label(run).as_str(),
                    run.wake.label(),
                    run.outcome.label(),
                ]
                .join(CAPTION_SEPARATOR)
            })
            .collect();
        if recent.is_empty() {
            return caption(NO_RUNS, pal).into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .children(recent.into_iter().map(|line| caption(line, pal).truncate()))
            .into_any_element()
    }

    fn toggle_kraddie_switch(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let Some(mut kraddie) = self.kraddies.entry(id).map(|entry| entry.kraddie.clone()) else {
            return;
        };
        kraddie.switch = kraddie.switch.toggled();
        self.save_kraddie(kraddie, Some(id), cx);
    }
}
