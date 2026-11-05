use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use krad_domain::{Hue, SessionIcon, SessionId, SessionLook};

use super::super::rename::KEY_ESCAPE;
use super::super::session_look::session_icon;
use super::super::workers::LeadEnding;
use super::super::EnvironmentShell;
use super::NameDialogs;
use crate::components::{icon, IconName};
use crate::theme::{
    hue_rgb, palette, popover_fill, popover_shadow, Palette, ICON, ICON_SM, LINE_SM, RADIUS_SM,
    RADIUS_XL, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
};

const POPOVER_ID: &str = "session-look";
const POPOVER_W: f32 = 312.0;
const POPOVER_H: f32 = 236.0;
const EDGE_MARGIN: f32 = 8.0;
const GLYPH_CELL: f32 = 32.0;
const SWATCH: f32 = 22.0;
const SELECTED_RING: f32 = 2.0;
const RULE: f32 = 1.0;
const GLYPH_PREFIX: &str = "session-look-icon";
const SWATCH_PREFIX: &str = "session-look-color";
const NO_COLOR_ID: &str = "session-look-color-none";
const NO_ICON_ID: &str = "session-look-icon-none";
const CLOSE_ROW_ID: &str = "session-look-close";
const CLOSE_LABEL: &str = "Close session";
const NAME_PLACEHOLDER: &str = "Session name";

pub(in crate::environment_shell) struct LookPopover {
    session: SessionId,
    origin: (f32, f32),
    field: Entity<InputState>,
    _events: Subscription,
}

fn ring(selected: bool, pal: &Palette) -> gpui_kit::Hsla {
    if selected {
        rgb(pal.accent).into()
    } else {
        gpui_kit::transparent_black()
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn customize_session(
        &mut self,
        session: SessionId,
        origin: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.session(session))
            .map(|found| found.name.clone())
        else {
            return;
        };
        let field = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(NAME_PLACEHOLDER)
                .default_value(name)
        });
        field.update(cx, |field, cx| field.focus(window, cx));
        let events = cx.subscribe_in(
            &field,
            window,
            move |this: &mut Self, field, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let typed = field.read(cx).value().trim().to_owned();
                    if !typed.is_empty() {
                        this.edit_session(session, |found| found.name = typed, cx);
                    }
                }
                InputEvent::PressEnter { .. } => this.close_name_dialog(cx),
                InputEvent::Blur | InputEvent::Focus => {}
            },
        );
        self.name_dialogs = NameDialogs::Look(LookPopover {
            session,
            origin,
            field,
            _events: events,
        });
        cx.notify();
    }

    fn edit_session(
        &mut self,
        session: SessionId,
        edit: impl FnOnce(&mut krad_domain::Session),
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
        edit(found);
        self.store();
        cx.notify();
    }

    fn edit_look(
        &mut self,
        session: SessionId,
        edit: impl FnOnce(&mut SessionLook),
        cx: &mut Context<Self>,
    ) {
        self.edit_session(session, |found| edit(&mut found.look), cx);
    }

    pub(super) fn look_popover(
        &self,
        popover: &LookPopover,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let live = self.envs.get(&self.active)?;
        let look = live.data.session(popover.session)?.look;
        let closable = live.data.can_close_session(popover.session);
        let pal = palette(cx);
        let (root_w, root_h) = self.root_size(window);
        let left = (popover.origin.0 - self.frame_inset.0)
            .min(root_w - POPOVER_W - EDGE_MARGIN)
            .max(EDGE_MARGIN);
        let top = (popover.origin.1 - self.frame_inset.1)
            .min(root_h - POPOVER_H - EDGE_MARGIN)
            .max(EDGE_MARGIN);
        let session = popover.session;
        let panel = div()
            .id(POPOVER_ID)
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(POPOVER_W))
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .p(px(SPACE_2))
            .rounded(px(RADIUS_XL))
            .bg(popover_fill(pal, cx))
            .border_1()
            .border_color(pal.divider)
            .shadow(popover_shadow(pal, cx))
            .occlude()
            .child(Input::new(&popover.field).cleanable(false))
            .child(Self::glyph_picks(session, look, pal, cx))
            .child(Self::color_picks(session, look, pal, cx))
            .when(closable, |panel| {
                panel
                    .child(div().h(px(RULE)).mx(px(SPACE_1)).bg(pal.divider))
                    .child(Self::close_row(session, pal, cx))
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .capture_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == KEY_ESCAPE {
                        cx.stop_propagation();
                        this.close_name_dialog(cx);
                    }
                }))
                .child(div().absolute().inset_0().occlude().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| this.close_name_dialog(cx)),
                ))
                .child(panel)
                .into_any_element(),
        )
    }

    fn glyph_picks(
        session: SessionId,
        look: SessionLook,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tone = look.color.map_or(pal.text_default, |hue| hue_rgb(hue, pal));
        let none = Self::glyph_cell(NO_ICON_ID.into(), look.icon.is_none(), pal)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_look(session, |look| look.icon = None, cx);
            }))
            .child(icon(IconName::Layers, ICON, tone))
            .into_any_element();
        let cells = SessionIcon::ALL.iter().enumerate().map(|(index, &glyph)| {
            Self::glyph_cell((GLYPH_PREFIX, index).into(), look.icon == Some(glyph), pal)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_look(session, |look| look.icon = Some(glyph), cx);
                }))
                .child(icon(session_icon(glyph), ICON, tone))
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .child(none)
            .children(cells)
            .into_any_element()
    }

    fn glyph_cell(
        id: gpui_kit::ElementId,
        selected: bool,
        pal: &Palette,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(GLYPH_CELL))
            .rounded(px(RADIUS_SM))
            .cursor_pointer()
            .when(selected, |cell| cell.bg(pal.bg_selected))
            .when(!selected, |cell| cell.hover(|style| style.bg(pal.bg_hover)))
    }

    fn color_picks(
        session: SessionId,
        look: SessionLook,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let swatch = |selected: bool| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(SWATCH + 2.0 * SELECTED_RING))
                .rounded_full()
                .cursor_pointer()
                .border(px(SELECTED_RING))
                .border_color(ring(selected, pal))
        };
        let none = swatch(look.color.is_none())
            .id(NO_COLOR_ID)
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_look(session, |look| look.color = None, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(SWATCH))
                    .rounded_full()
                    .bg(pal.bg_hover)
                    .child(icon(IconName::Close, ICON_SM, pal.text_muted)),
            );
        let hues = Hue::ALL.iter().enumerate().map(|(index, &hue)| {
            swatch(look.color == Some(hue))
                .id((SWATCH_PREFIX, index))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_look(session, |look| look.color = Some(hue), cx);
                }))
                .child(
                    div()
                        .size(px(SWATCH))
                        .rounded_full()
                        .bg(rgb(hue_rgb(hue, pal))),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_1))
            .child(none)
            .children(hues)
            .into_any_element()
    }

    fn close_row(session: SessionId, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(CLOSE_ROW_ID)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_3))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.error_fg))
            .hover(|style| style.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.close_name_dialog(cx);
                this.ask_end_lead(LeadEnding::CloseSession(session), window, cx);
            }))
            .child(icon(IconName::Close, ICON_SM, pal.error_fg))
            .child(SharedString::from(CLOSE_LABEL))
            .into_any_element()
    }
}
