mod digest_card;
mod sharing_card;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{KraddieId, MemoryOwner};

use super::super::EnvironmentShell;
use super::layout::{area_layout, AreaLayout, AreaShape};
use crate::components::motion::rise_in;
use crate::components::{icon, solid_card, Button, ButtonVariant, EmptyState, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, ICON_XS, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_SM, SPACE_1, SPACE_1_5,
    SPACE_2, SPACE_3, SPACE_5, TEXT_SM, TEXT_XS,
};

const CARD_PREFIX: &str = "kraddie-memory-card";
const CHAT_PREFIX: &str = "kraddie-memory-chat";
const NEW_CHAT_ID: &str = "kraddie-memory-new-chat";
const CHATS_TITLE: &str = "Chats";
const CHATS_NOTE: &str = "Conversations that belong to it";
const NEW_CHAT_LABEL: &str = "New chat";
const NO_CHATS_TITLE: &str = "No chats yet";
const NO_CHATS_TEXT: &str =
    "Start one: it brings its compass and keepsakes, and leaves a digest behind.";
const DIGESTS_TITLE: &str = "What its chats left behind";
const DIGESTS_NOTE: &str = "Digests it reads at the start of every chat";
const NO_DIGESTS_TITLE: &str = "Nothing written down yet";
const NO_DIGESTS_TEXT: &str =
    "When one of its chats ends or goes quiet, the facts and decisions worth keeping land here.";
const CHAT_IN_OTHER_ENV: &str = "In another environment";
const CHATS_COUNT_SUFFIX: &str = " open";
const DIGEST_CARDS_STEP: usize = 24;
const MORE_DIGESTS_ID: &str = "kraddie-memory-more-digests";
const MORE_DIGESTS_LABEL: &str = "Show older digests";

fn digest_cards_shown(total: usize, step_count: usize) -> usize {
    total.min(DIGEST_CARDS_STEP.saturating_mul(step_count.saturating_add(1)))
}

fn card_header(title: &'static str, note: &'static str, pal: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_1))
        .child(
            div()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_strong))
                .child(title),
        )
        .child(
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .child(note),
        )
}

fn area_card(width: f32, pal: &Palette) -> Div {
    solid_card(pal)
        .flex()
        .flex_col()
        .flex_none()
        .w(px(width))
        .min_w_0()
        .gap(px(SPACE_3))
        .p(px(SPACE_5))
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_memory_area(
        &self,
        id: &KraddieId,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = area_layout(width);
        let chats = self.chats_card(id, &layout, cx);
        let digests = self.digests_card(id, &layout, cx);
        let sharing = self.sharing_card(id, &layout, cx);
        let cards = [chats, digests, sharing]
            .into_iter()
            .enumerate()
            .map(|(index, card)| {
                rise_in(
                    ElementId::Name(format!("{CARD_PREFIX}-{id}-{index}").into()),
                    index,
                    div().relative().child(card),
                )
            });
        match layout.shape {
            AreaShape::Stacked => div()
                .flex()
                .flex_col()
                .gap(px(layout.gap))
                .children(cards)
                .into_any_element(),
            AreaShape::Trio => div()
                .flex()
                .flex_row()
                .items_start()
                .gap(px(layout.gap))
                .children(cards)
                .into_any_element(),
            AreaShape::Paired => {
                let mut cards: Vec<AnyElement> = cards.collect();
                let sharing = cards.pop();
                let digests = cards.pop();
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(layout.gap))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .gap(px(layout.gap))
                            .children(cards)
                            .children(sharing),
                    )
                    .children(digests)
                    .into_any_element()
            }
        }
    }

    fn chats_card(
        &self,
        id: &KraddieId,
        layout: &AreaLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let chats = self.kraddie_chats(id);
        let target = id.clone();
        let new_chat = Button::new(NEW_CHAT_ID)
            .label(NEW_CHAT_LABEL)
            .icon(IconName::Add)
            .variant(ButtonVariant::Primary)
            .small()
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.start_kraddie_chat(&target, cx);
            }));
        let header = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_start()
            .justify_between()
            .gap(px(SPACE_2))
            .child(card_header(CHATS_TITLE, CHATS_NOTE, pal))
            .child(new_chat);
        let body = if chats.is_empty() {
            EmptyState::new(IconName::Chat, NO_CHATS_TITLE, NO_CHATS_TEXT).into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .children(
                    chats
                        .into_iter()
                        .enumerate()
                        .map(|(index, (env, session))| {
                            let here = env == self.active;
                            let session_id = session.id;
                            let detail: SharedString = if here {
                                format!(
                                    "{}{CHATS_COUNT_SUFFIX}",
                                    super::super::sessions_view::session_terminals(session).count()
                                )
                                .into()
                            } else {
                                CHAT_IN_OTHER_ENV.into()
                            };
                            let row = div()
                                .id(ElementId::Name(
                                    format!("{CHAT_PREFIX}-{id}-{}", session_id.as_u64()).into(),
                                ))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(SPACE_2))
                                .h(px(NAV_ROW_H))
                                .px(px(SPACE_2))
                                .rounded(px(RADIUS_SM))
                                .text_size(px(TEXT_SM))
                                .line_height(px(LINE_SM))
                                .text_color(rgb(pal.text_strong))
                                .when(here, |row| {
                                    row.cursor_pointer()
                                        .hover(|style| style.bg(pal.bg_hover))
                                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                            this.reopen_kraddie_chat(&env, session_id, cx);
                                        }))
                                })
                                .child(icon(IconName::Chat, ICON_SM, pal.text_muted))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(session.name.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(px(TEXT_XS))
                                        .line_height(px(LINE_XS))
                                        .text_color(rgb(pal.text_muted))
                                        .child(detail),
                                )
                                .child(icon(IconName::ChevronRight, ICON_XS, pal.text_muted));
                            rise_in(
                                ElementId::Name(format!("{CHAT_PREFIX}-in-{id}-{index}").into()),
                                index,
                                row,
                            )
                        }),
                )
                .into_any_element()
        };
        area_card(layout.chats_w, pal)
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn digests_card(
        &self,
        id: &KraddieId,
        layout: &AreaLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let owner = MemoryOwner::Kraddie(id.clone());
        let total = self
            .kraddies
            .memory
            .book(&owner)
            .map_or(0, |book| book.digests().len());
        let steps = self
            .kraddies
            .memory
            .older_digests
            .get(id)
            .copied()
            .unwrap_or_default();
        let shown = digest_cards_shown(total, steps);
        let digests: Vec<AnyElement> = self.kraddies.memory.book(&owner).map_or_default(|book| {
            book.digests()
                .iter()
                .rev()
                .take(shown)
                .enumerate()
                .map(|(index, digest)| {
                    self.digest_card(&owner, digest, index, layout.digest_card_w, cx)
                })
                .collect()
        });
        let more = (shown < total).then(|| {
            let target = id.clone();
            Button::new(MORE_DIGESTS_ID)
                .label(MORE_DIGESTS_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    *this
                        .kraddies
                        .memory
                        .older_digests
                        .entry(target.clone())
                        .or_default() += 1;
                    cx.notify();
                }))
        });
        let body = if digests.is_empty() {
            EmptyState::new(IconName::Book, NO_DIGESTS_TITLE, NO_DIGESTS_TEXT).into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(layout.gap))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(layout.gap))
                        .children(digests),
                )
                .children(more)
                .into_any_element()
        };
        area_card(layout.digests_w, pal)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .child(card_header(DIGESTS_TITLE, DIGESTS_NOTE, pal)),
            )
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_cards_are_built_a_step_at_a_time() {
        assert_eq!(digest_cards_shown(0, 0), 0);
        assert_eq!(digest_cards_shown(5, 0), 5);
        assert_eq!(digest_cards_shown(100, 0), DIGEST_CARDS_STEP);
        assert_eq!(digest_cards_shown(100, 1), 2 * DIGEST_CARDS_STEP);
        assert_eq!(digest_cards_shown(30, 3), 30);
        assert_eq!(digest_cards_shown(usize::MAX, usize::MAX), usize::MAX);
    }
}
