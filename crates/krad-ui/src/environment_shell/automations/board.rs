use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, Animation, AnimationExt as _, AnyElement, Context, Div, ElementId, Entity,
    FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use std::time::SystemTime;

use super::super::EnvironmentShell;
use super::grid::grid_rows;
use super::layout::{can_sit_side_by_side, BoardFit, Density, PAGE_PAD};
use super::model::{automation_cards, filtered_cards, tally, AutomationCard, CardClock, Tally};
use crate::components::motion::{counted, rise_in, COUNT_UP_DURATION};
use crate::components::{Button, ButtonVariant, CenteredEmpty, IconName};
use crate::theme::{
    palette, Palette, LINE_LG, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_LG, TEXT_XS,
};

const BOARD_ID: &str = "automation-board";
const CARD_ENTER: &str = "automation-card-enter";
const TALLY_PREFIX: &str = "automation-tally";
const BTN_COMPOSE: &str = "automation-compose";
const BTN_BLUEPRINTS: &str = "automation-blueprints";
const BTN_BESIDE: &str = "automation-beside";
const BTN_EMPTY_COMPOSE: &str = "automation-empty-compose";
const BTN_EMPTY_BLUEPRINTS: &str = "automation-empty-blueprints";
const FILTER_MIN_W: f32 = 200.0;
const HEADING_MIN_W: f32 = 160.0;
const BOTTOM_ROOM: f32 = PAGE_PAD * 2.0;
const FILTER_MAX_W: f32 = 360.0;
const TITLE: &str = "Automations";
const ON_DUTY: &str = "on duty";
const RESTING: &str = "resting";
const RUNNING: &str = "running now";
const TALLY_SEPARATOR: &str = "·";
const COMPOSE_LABEL: &str = "Compose";
const COMPOSE_TOOLTIP: &str = "Write a new automation from a blank canvas";
const BLUEPRINTS_LABEL: &str = "Blueprints";
const BLUEPRINTS_TOOLTIP: &str = "Show or hide ready-made briefs to borrow";
const BESIDE_TOOLTIP: &str = "Keep the board next to the editor";
const ALONE_TOOLTIP: &str = "Give the editor the whole width";
const EMPTY_TITLE: &str = "The roster is empty";
const EMPTY_TEXT: &str =
    "Hand a Kraddie a standing order. It can wake on a clock, a code host event or your say-so.";
const NO_MATCH_TITLE: &str = "Nobody answers to that";
const NO_MATCH_TEXT: &str = "Try part of a name, a Kraddie or words like weekdays or pipeline.";

impl EnvironmentShell {
    pub(super) fn automation_board(
        &self,
        filter: &Entity<InputState>,
        fit: BoardFit,
        full_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let query = filter.read(cx).value().to_string();
        let now = SystemTime::now();
        let clock = CardClock {
            next_in: &|kraddie, entry| self.automation_next_in(kraddie, entry),
            age: &|started| now.duration_since(started).ok(),
        };
        let everything = automation_cards(&self.kraddies.entries, "", &clock);
        let filtered = (!query.trim().is_empty()).then(|| filtered_cards(&everything, &query));
        let cards = filtered.as_deref().unwrap_or(everything.as_slice());
        let content = if everything.is_empty() {
            Self::board_empty(EMPTY_TITLE, EMPTY_TEXT, cx)
        } else if cards.is_empty() {
            CenteredEmpty::new(NO_MATCH_TITLE, NO_MATCH_TEXT).into_any_element()
        } else {
            self.board_cards(cards, fit, pal, cx)
        };
        div()
            .id(BOARD_ID)
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .px(px(PAGE_PAD))
            .pt(px(PAGE_PAD))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(fit.inner))
                    .mx_auto()
                    .gap(px(SPACE_3))
                    .child(self.board_toolbar(filter, full_width, tally(&everything), pal, cx))
                    .child(self.blueprint_shelf(fit, pal, cx))
                    .child(div().flex_none().w_full().min_w_0().child(content))
                    .child(div().flex_none().h(px(BOTTOM_ROOM))),
            )
            .into_any_element()
    }

    fn board_toolbar(
        &self,
        filter: &Entity<InputState>,
        full_width: f32,
        tally: Tally,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let heading = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_1))
            .child(
                div()
                    .text_size(px(TEXT_LG))
                    .line_height(px(LINE_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(TITLE),
            )
            .child(self.tally_line(tally, pal));
        let shelf_open = self.automations.shelf.open;
        let blueprints = Button::new(BTN_BLUEPRINTS)
            .label(BLUEPRINTS_LABEL)
            .icon(if shelf_open {
                IconName::ChevronUp
            } else {
                IconName::Magic
            })
            .variant(if shelf_open {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .tooltip(BLUEPRINTS_TOOLTIP)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_blueprints(cx)));
        let beside =
            (self.automations.editor.is_some() && can_sit_side_by_side(full_width)).then(|| {
                let on = self.automations.side_by_side;
                Button::new(BTN_BESIDE)
                    .icon(if on {
                        IconName::Expand
                    } else {
                        IconName::SidebarLeft
                    })
                    .tooltip(if on { ALONE_TOOLTIP } else { BESIDE_TOOLTIP })
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_side_by_side(cx)))
            });
        let compose = Button::new(BTN_COMPOSE)
            .label(COMPOSE_LABEL)
            .icon(IconName::Add)
            .variant(ButtonVariant::Primary)
            .tooltip(COMPOSE_TOOLTIP)
            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                this.start_automation(None, window, cx);
            }));
        let search = div()
            .flex_1()
            .min_w(px(FILTER_MIN_W))
            .max_w(px(FILTER_MAX_W))
            .child(Input::new(filter));
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .children(beside)
            .child(blueprints)
            .child(compose);
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_3))
            .child(heading.min_w(px(HEADING_MIN_W)))
            .child(search)
            .child(actions.flex_none())
            .into_any_element()
    }

    fn tally_line(&self, tally: Tally, pal: &Palette) -> Div {
        let numerals = &self.automations.numerals;
        let figure = |key: usize, value: usize, label: &'static str, color: u32| {
            let animated = numerals.get(value).is_some();
            let shown = numerals.clone();
            let base = div()
                .flex()
                .flex_row()
                .gap(px(SPACE_1))
                .text_color(rgb(color));
            let number: AnyElement = if animated {
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .with_animation(
                        ElementId::NamedInteger(
                            TALLY_PREFIX.into(),
                            u64::try_from(key * numerals.len() + value).unwrap_or_default(),
                        ),
                        Animation::new(COUNT_UP_DURATION),
                        move |digit, t| {
                            digit.child(shown.get(counted(t, value)).cloned().unwrap_or_default())
                        },
                    )
                    .into_any_element()
            } else {
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(value.to_string()))
                    .into_any_element()
            };
            base.child(number).child(label)
        };
        let separator = || {
            div()
                .text_color(rgb(pal.text_disabled))
                .child(TALLY_SEPARATOR)
        };
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_1_5))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(figure(0, tally.on_duty, ON_DUTY, pal.text_default))
            .child(separator())
            .child(figure(1, tally.resting, RESTING, pal.text_muted))
            .when(tally.running > 0, |line| {
                line.child(separator())
                    .child(figure(2, tally.running, RUNNING, pal.success_fg))
            })
    }

    fn board_cards(
        &self,
        cards: &[AutomationCard],
        fit: BoardFit,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.editing_automation().cloned();
        let grid = fit.cards(cards.len());
        let tiles: Vec<AnyElement> = cards
            .iter()
            .enumerate()
            .map(|(index, card)| {
                let chosen = selected.as_ref() == Some(&card.key);
                let tile = match fit.density {
                    Density::Cards => self.automation_card(index, card, chosen, pal, cx),
                    Density::Rows => self.automation_row(index, card, chosen, pal, cx),
                };
                let cell = grid.cell(index);
                rise_in(
                    ElementId::NamedInteger(
                        CARD_ENTER.into(),
                        u64::try_from(index).unwrap_or_default(),
                    ),
                    index,
                    div()
                        .relative()
                        .flex()
                        .flex_none()
                        .w(px(cell.w))
                        .when(fit.density == Density::Cards, |wrapper| {
                            wrapper.h(px(cell.h))
                        })
                        .child(tile),
                )
            })
            .collect();
        match fit.density {
            Density::Cards => grid_rows(grid, tiles).into_any_element(),
            Density::Rows => div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .children(tiles)
                .into_any_element(),
        }
    }

    fn board_empty(title: &'static str, text: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let actions = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(
                Button::new(BTN_EMPTY_BLUEPRINTS)
                    .label(BLUEPRINTS_LABEL)
                    .icon(IconName::Magic)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.automations.shelf.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Button::new(BTN_EMPTY_COMPOSE)
                    .label(COMPOSE_LABEL)
                    .icon(IconName::Add)
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.start_automation(None, window, cx);
                    })),
            );
        CenteredEmpty::new(title, text)
            .action(actions)
            .into_any_element()
    }
}
