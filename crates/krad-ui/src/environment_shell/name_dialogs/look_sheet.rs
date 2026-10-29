use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{Hue, SessionIcon, SessionId, SessionLook};

use super::super::dialog_parts::{
    sheet_body, sheet_footer, sheet_overlay, sheet_panel, sheet_section, sheet_title,
};
use super::super::session_look::{session_icon, session_wash};
use super::super::EnvironmentShell;
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{hue_rgb, palette, Palette, ICON, RADIUS_SM, SPACE_2};

const LOOK_DIALOG_W: f32 = 440.0;
const LOOK_TITLE: &str = "Customize session";
const COLOR_LABEL: &str = "Color";
const COLOR_TEXT: &str = "Shown as a translucent wash behind the session in the sidebar.";
const ICON_LABEL: &str = "Icon";
const ICON_TEXT: &str = "Shown before the session's name.";
const DONE_LABEL: &str = "Done";
const BODY_ID: &str = "session-look-body";
const BTN_LOOK_CLOSE: &str = "session-look-close";
const BTN_LOOK_DONE: &str = "session-look-done";
const SWATCH_PREFIX: &str = "session-color";
const NO_COLOR_ID: &str = "session-color-none";
const GLYPH_PREFIX: &str = "session-icon";
const NO_ICON_ID: &str = "session-icon-none";
const SWATCH: f32 = 28.0;
const SWATCH_DOT: f32 = 16.0;
const GLYPH_CELL: f32 = 36.0;
const SELECTED_RING: f32 = 2.0;

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_name_dialog(cx);
}

fn pick_cell(size: f32, selected: bool, pal: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(RADIUS_SM))
        .cursor_pointer()
        .border(px(SELECTED_RING))
        .border_color(if selected {
            rgb(pal.accent).into()
        } else {
            gpui_kit::transparent_black()
        })
        .hover(|style| style.bg(pal.bg_hover))
}

impl EnvironmentShell {
    pub(super) fn look_sheet(
        &self,
        session: SessionId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let look = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.session(session))?
            .look;
        let pal = palette(cx);
        let sheet = sheet_panel(LOOK_DIALOG_W, pal)
            .child(sheet_title(LOOK_TITLE, None, BTN_LOOK_CLOSE, dismiss, cx))
            .child(
                sheet_body(BODY_ID)
                    .child(
                        sheet_section(COLOR_LABEL, Some(COLOR_TEXT), pal)
                            .child(Self::color_picks(session, look, pal, cx)),
                    )
                    .child(
                        sheet_section(ICON_LABEL, Some(ICON_TEXT), pal)
                            .child(Self::icon_picks(session, look, pal, cx)),
                    ),
            )
            .child(
                sheet_footer(pal).justify_end().child(
                    Button::new(BTN_LOOK_DONE)
                        .label(DONE_LABEL)
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.close_name_dialog(cx);
                        })),
                ),
            );
        Some(sheet_overlay(sheet, dismiss, cx))
    }

    fn color_picks(
        session: SessionId,
        look: SessionLook,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let none = pick_cell(SWATCH, look.color.is_none(), pal)
            .id(NO_COLOR_ID)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_look(session, |look| look.color = None, cx);
            }))
            .child(icon(IconName::Close, SWATCH_DOT, pal.text_muted));
        let swatches = Hue::ALL.iter().enumerate().map(|(index, &color)| {
            pick_cell(SWATCH, look.color == Some(color), pal)
                .id((SWATCH_PREFIX, index))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_look(session, |look| look.color = Some(color), cx);
                }))
                .child(
                    div()
                        .size(px(SWATCH_DOT))
                        .rounded_full()
                        .bg(rgb(hue_rgb(color, pal)))
                        .border_1()
                        .border_color(session_wash(color, false, pal)),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_2))
            .child(none)
            .children(swatches)
            .into_any_element()
    }

    fn icon_picks(
        session: SessionId,
        look: SessionLook,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let none = pick_cell(GLYPH_CELL, look.icon.is_none(), pal)
            .id(NO_ICON_ID)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_look(session, |look| look.icon = None, cx);
            }))
            .child(icon(IconName::Close, ICON, pal.text_muted));
        let glyphs = SessionIcon::ALL.iter().enumerate().map(|(index, &glyph)| {
            let selected = look.icon == Some(glyph);
            pick_cell(GLYPH_CELL, selected, pal)
                .id((GLYPH_PREFIX, index))
                .when(selected, |cell| cell.bg(pal.bg_selected))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_look(session, |look| look.icon = Some(glyph), cx);
                }))
                .child(icon(session_icon(glyph), ICON, pal.text_default))
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_2))
            .child(none)
            .children(glyphs)
            .into_any_element()
    }

    fn edit_look(
        &mut self,
        session: SessionId,
        edit: impl FnOnce(&mut SessionLook),
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(found) = self.envs.get_mut(&env).and_then(|live| {
            live.data
                .sessions
                .iter_mut()
                .find(|found| found.id == session)
        }) else {
            return;
        };
        edit(&mut found.look);
        self.store();
        cx.notify();
    }
}
