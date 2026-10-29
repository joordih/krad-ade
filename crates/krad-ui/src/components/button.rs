use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, Action, App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window,
};
use std::rc::Rc;

use super::icon::{icon, IconName};
use super::tooltip::{tooltip_view, tooltip_with_shortcut};
use crate::theme::{
    palette, row_shadow, BUTTON_H, CTA_SM_H, ICON, ICON_BUTTON, ICON_BUTTON_SM, ICON_SM, LINE_SM,
    RADIUS_MD, RADIUS_SM, RADIUS_XS, SPACE_2, SPACE_3, TEXT_SM,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ButtonVariant {
    Primary,
    Secondary,
    #[default]
    Ghost,
    Destructive,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum ButtonSize {
    #[default]
    Default,
    Small,
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(crate) struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<IconName>,
    variant: ButtonVariant,
    size: ButtonSize,
    tooltip: Option<SharedString>,
    shortcut: Option<Box<dyn Action>>,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            tooltip: None,
            shortcut: None,
            on_click: None,
        }
    }

    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub(crate) const fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) const fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub(crate) const fn small(mut self) -> Self {
        self.size = ButtonSize::Small;
        self
    }

    pub(crate) fn shortcut(mut self, action: Box<dyn Action>) -> Self {
        self.shortcut = Some(action);
        self
    }

    pub(crate) fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub(crate) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let pal = palette(cx);
        let icon_only = self.label.is_none();
        let small = self.size == ButtonSize::Small;
        let (height, radius, icon_size) = match (icon_only, small) {
            (true, false) => (ICON_BUTTON, RADIUS_SM, ICON),
            (true, true) => (ICON_BUTTON_SM, RADIUS_XS, ICON_SM),
            (false, false) => (BUTTON_H, RADIUS_MD, ICON),
            (false, true) => (CTA_SM_H, RADIUS_XS, ICON_SM),
        };
        let pad_x = if small { SPACE_2 } else { SPACE_3 };
        let solid = |color: u32| -> Hsla { rgb(color).into() };
        let (fill, hover, pressed, text): (Option<Hsla>, Hsla, Hsla, u32) = match self.variant {
            ButtonVariant::Primary => (
                Some(solid(pal.accent)),
                solid(pal.accent_hover),
                solid(pal.accent_pressed),
                pal.on_accent,
            ),
            ButtonVariant::Secondary => (
                Some(solid(pal.bg_surface)),
                solid(pal.bg_row_selected),
                solid(pal.bg_row_selected),
                pal.text_strong,
            ),
            ButtonVariant::Ghost => (
                None,
                pal.bg_hover.into(),
                pal.bg_selected.into(),
                if icon_only {
                    pal.text_muted
                } else {
                    pal.text_default
                },
            ),
            ButtonVariant::Destructive => (
                Some(solid(pal.error_fg)),
                solid(pal.error_fg),
                solid(pal.error_fg),
                pal.on_accent,
            ),
        };
        let neutral = matches!(
            self.variant,
            ButtonVariant::Ghost | ButtonVariant::Secondary
        );
        let hover_text = if neutral { pal.text_strong } else { text };
        let on_click = self.on_click;
        let tooltip = self.tooltip;
        let shortcut = self.shortcut;
        div()
            .id(self.id)
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .h(px(height))
            .rounded(px(radius))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(text))
            .when(icon_only, |button| button.w(px(height)))
            .when(!icon_only, |button| button.px(px(pad_x)))
            .when_some(fill, Styled::bg)
            .when(self.variant == ButtonVariant::Secondary, |button| {
                button.shadow(row_shadow(pal, cx))
            })
            .when(self.variant == ButtonVariant::Destructive, |button| {
                button.hover(|style| style.opacity(HOVER_DIM))
            })
            .when(self.variant != ButtonVariant::Destructive, |button| {
                button
                    .hover(|style| style.bg(hover).text_color(rgb(hover_text)))
                    .active(|style| style.bg(pressed))
            })
            .when_some(self.icon, |button, name| {
                button.child(icon(name, icon_size, text))
            })
            .when_some(self.label, ParentElement::child)
            .when_some(on_click, |button, handler| {
                button.on_click(move |event, window, cx| handler(event, window, cx))
            })
            .when_some(tooltip, |button, text| {
                button.tooltip(move |_, cx| match shortcut.as_deref() {
                    Some(action) => tooltip_with_shortcut(text.clone(), action, cx),
                    None => tooltip_view(text.clone(), cx),
                })
            })
    }
}

const HOVER_DIM: f32 = 0.9;
