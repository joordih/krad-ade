use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::EnvironmentView;
use std::rc::Rc;

use super::super::EnvironmentShell;
use super::layout::{place, CardSpan, PageLayout};
use super::motion::{entering, idling};
use super::profile::width_probe;
use super::tile::grid;
use crate::actions::ToggleKraddies;
use crate::components::{card, Button, ButtonVariant, IconName};
use crate::kraddie_avatar::{kraddie_avatar, KraddieSprite};
use crate::theme::{
    palette, Palette, LINE_LG, LINE_SM, SPACE_1, SPACE_2, SPACE_3, SPACE_6, TEXT_LG, TEXT_SM,
};

const EMPTY_AVATAR: f32 = 96.0;
const TITLE: &str = "Kraddies";
const SUBTITLE: &str = "Little workers with a face, keepsakes and routines of their own";
const BTN_NEW: &str = "kraddies-new";
const BTN_EMPTY_NEW: &str = "kraddies-empty-new";
const BTN_IMPORT: &str = "kraddies-import";
const NEW_LABEL: &str = "Hatch a Kraddie";
const IMPORT_LABEL: &str = "Import…";
const EMPTY_TITLE: &str = "No Kraddies hatched yet";
const EMPTY_TEXT: &str =
    "A Kraddie keeps its own routines, notes and face. Hatch one, name it and give it a repository to look after.";
const PAGE_ID: &str = "kraddies-view";
const EMPTY_BOB: &str = "kraddie-den-empty-bob";
const EMPTY_ID: &str = "kraddie-den-empty";
pub(super) const NAME_TAKEN: &str = "Another Kraddie already answers to that name";

impl EnvironmentShell {
    pub(in crate::environment_shell) fn on_toggle_kraddies(
        &mut self,
        _: &ToggleKraddies,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_kraddies(cx);
    }

    pub(in crate::environment_shell) fn toggle_kraddies(&mut self, cx: &mut Context<Self>) {
        let view = if self.active_view() == EnvironmentView::Kraddies {
            EnvironmentView::Sessions
        } else {
            EnvironmentView::Kraddies
        };
        self.hide_extension_main_view();
        self.set_view(view, cx);
    }

    pub(in crate::environment_shell) fn kraddies_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(page) = self.kraddie_page(window, cx) {
            return page;
        }
        let pal = palette(cx);
        let layout = self.kraddie_layout(window);
        let body = if self.kraddies.entries.is_empty() {
            self.den_empty(pal, cx)
        } else {
            self.den_grid(&layout, pal, cx)
        };
        let column = div()
            .flex()
            .flex_col()
            .w(px(layout.content_w))
            .mx_auto()
            .gap(px(SPACE_6))
            .child(Self::den_header(pal, cx))
            .child(body);
        card(pal, cx)
            .size_full()
            .relative()
            .child(width_probe(&self.kraddies.profile.width))
            .child(
                div()
                    .id(ElementId::from(PAGE_ID))
                    .size_full()
                    .overflow_y_scroll()
                    .py(px(layout.padding))
                    .child(column),
            )
            .into_any_element()
    }

    fn den_header(pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
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
            .child(
                div()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(SUBTITLE),
            );
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_2))
            .child(heading)
            .child(
                Button::new(BTN_IMPORT)
                    .label(IMPORT_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|_: &mut Self, _, _, cx| Self::import_kraddie_file(cx))),
            )
            .child(
                Button::new(BTN_NEW)
                    .label(NEW_LABEL)
                    .icon(IconName::Kraddie)
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.open_kraddie_editor(None, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn den_empty(&self, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let sprite = self.kraddies.placeholder();
        let avatar = idling(
            kraddie_avatar(&sprite, EMPTY_AVATAR, self.kraddies.frame()),
            EMPTY_BOB,
        );
        let body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(SPACE_3))
            .py(px(SPACE_6))
            .child(avatar)
            .child(
                div()
                    .text_size(px(TEXT_LG))
                    .line_height(px(LINE_LG))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(EMPTY_TITLE),
            )
            .child(
                div()
                    .max_w(px(EMPTY_TEXT_W))
                    .text_center()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_muted))
                    .child(EMPTY_TEXT),
            )
            .child(
                Button::new(BTN_EMPTY_NEW)
                    .label(NEW_LABEL)
                    .icon(IconName::Kraddie)
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.open_kraddie_editor(None, window, cx);
                    })),
            );
        entering(body, EMPTY_ID, 0).into_any_element()
    }

    fn den_grid(
        &self,
        layout: &PageLayout,
        pal: &'static Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let spans = place(
            &vec![CardSpan::One; self.kraddies.entries.len()],
            layout.columns,
        );
        let cards = self
            .kraddies
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let width = layout.span_w(spans[index], layout.columns);
                self.den_card(index, entry, width, pal, cx)
            })
            .collect();
        grid(layout.gutter, cards).into_any_element()
    }
}

const EMPTY_TEXT_W: f32 = 420.0;

pub(super) fn placeholder_sprite() -> Rc<KraddieSprite> {
    let design = krad_domain::KraddieDesign::kraddie(krad_domain::BodyStyle::default());
    Rc::new(KraddieSprite::new(&design, krad_domain::KraddieMood::Idle))
}
