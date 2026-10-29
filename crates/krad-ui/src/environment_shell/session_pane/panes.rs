use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, DragMoveEvent, FontWeight,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use krad_domain::{
    Axis, DividerId, LayoutTemplate, PaneId, SessionPanes, SnapZone, TerminalId, UnitRect,
};
use std::time::Instant;

use super::super::drag::DragState;
use super::super::{EnvironmentShell, LiveEnv};
use super::{tab_shell, StripPlace, TAB_LABEL_MAX_W};
use crate::components::{card, icon, Button, IconName};
use crate::motion::Spring;
use crate::theme::{
    floating_shadow, palette, tinted, ICON, ICON_SM, LINE_SM, RADIUS_LG, RADIUS_SM, RADIUS_XS,
    SPACE_1, SPACE_1_5, SPACE_2, TEXT_SM, WINDOW_INSET,
};

pub(super) const PANE_STRIP_H: f32 = 38.0;
const PANES_ID: &str = "session-panes";
const DIVIDER_PREFIX: &str = "pane-divider";
const TEMPLATE_PREFIX: &str = "pane-template";
const CLOSE_PANE_PREFIX: &str = "pane-close";
const GUEST_TAB_PREFIX: &str = "pane-guest-tab";
const GUEST_BACK_PREFIX: &str = "pane-guest-back";
const GUEST_BACK_TOOLTIP: &str = "Back to the side panel";
const ZONE_REACH: f32 = 0.3;
const DIVIDER_HIT: f32 = 8.0;
const PREVIEW_RESPONSE_S: f32 = 0.18;
const PREVIEW_FILL_ALPHA: u32 = 0x26;
const TEMPLATE_W: f32 = 72.0;
const TEMPLATE_H: f32 = 44.0;
const TEMPLATE_GAP: f32 = 8.0;
const TEMPLATE_PAD: f32 = 8.0;
const TEMPLATE_TOP: f32 = 10.0;
const SLOT_GAP: f32 = 3.0;
const TEMPLATE_BAND_H: f32 = 84.0;
const GHOST_H: f32 = 30.0;
const EMPTY_PANE_TITLE: &str = "Empty pane";
const EMPTY_PANE_TEXT: &str = "Drag a tab here.";
const CLOSE_PANE_TOOLTIP: &str = "Close pane";

#[derive(Clone)]
pub(in crate::environment_shell) struct DraggedTab {
    pub(in crate::environment_shell) id: TerminalId,
    label: SharedString,
    glyph: IconName,
    color: u32,
    shell: WeakEntity<EnvironmentShell>,
}

impl DraggedTab {
    pub(in crate::environment_shell) fn new(
        id: TerminalId,
        label: impl Into<SharedString>,
        glyph: IconName,
        color: u32,
        shell: WeakEntity<EnvironmentShell>,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            glyph,
            color,
            shell,
        }
    }
}

impl Render for DraggedTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = palette(cx);
        let mouse = window.mouse_position();
        let (dx, dy) = self.shell.upgrade().map_or((0.0, 0.0), |shell| {
            shell
                .read(cx)
                .ghost_pull((mouse.x.as_f32(), mouse.y.as_f32()))
        });
        div()
            .ml(px(dx))
            .mt(px(dy))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(GHOST_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .bg(rgb(pal.bg_surface))
            .border_1()
            .border_color(pal.border)
            .shadow(floating_shadow(pal))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_strong))
            .child(icon(self.glyph, ICON_SM, self.color))
            .child(self.label.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DropTarget {
    Strip(PaneId),
    Zone(PaneId, SnapZone),
    Template(LayoutTemplate, usize),
}

pub(in crate::environment_shell) struct PaneDrag {
    target: Option<DropTarget>,
    templates: bool,
    frame: [Spring; 4],
    last_frame: Option<Instant>,
}

impl Default for PaneDrag {
    fn default() -> Self {
        Self {
            target: None,
            templates: false,
            frame: [0.0, 0.0, 1.0, 1.0].map(|value| Spring::at_rest(value, PREVIEW_RESPONSE_S)),
            last_frame: None,
        }
    }
}

impl PaneDrag {
    fn aim(&mut self, target: Option<DropTarget>, templates: bool, frame: Option<UnitRect>) {
        let first = self.target.is_none();
        self.target = target;
        self.templates = templates;
        let Some(frame) = frame else {
            return;
        };
        let values = [frame.x, frame.y, frame.w, frame.h];
        for (spring, value) in self.frame.iter_mut().zip(values) {
            if first {
                *spring = Spring::at_rest(value, PREVIEW_RESPONSE_S);
            } else {
                spring.set_target(value);
            }
        }
        self.last_frame.get_or_insert_with(Instant::now);
    }

    fn clear(&mut self) {
        self.target = None;
        self.templates = false;
        self.last_frame = None;
    }

    pub(in crate::environment_shell) fn advance(&mut self, now: Instant) -> bool {
        let Some(last) = self.last_frame else {
            return false;
        };
        let elapsed = now.duration_since(last).as_secs_f32();
        for spring in &mut self.frame {
            spring.step(elapsed);
        }
        let moving = self.frame.iter().any(|spring| !spring.is_settled());
        self.last_frame = moving.then_some(now);
        moving
    }

    fn preview(&self) -> UnitRect {
        let [x, y, w, h] = self.frame.each_ref().map(Spring::value);
        UnitRect { x, y, w, h }
    }
}

const GHOST_PULL: f32 = 0.22;
const GHOST_PULL_MAX: f32 = 48.0;

pub(super) fn pane_px(
    rect: UnitRect,
    origin: (f32, f32),
    size: (f32, f32),
) -> (f32, f32, f32, f32) {
    let gap = WINDOW_INSET / 2.0;
    let edge = |at: f32| if at <= 0.0 || at >= 1.0 { 0.0 } else { gap };
    let left = edge(rect.x);
    let top = edge(rect.y);
    let right = edge(rect.x + rect.w);
    let bottom = edge(rect.y + rect.h);
    (
        origin.0 + rect.x * size.0 + left,
        origin.1 + rect.y * size.1 + top,
        rect.w * size.0 - left - right,
        rect.h * size.1 - top - bottom,
    )
}

fn templates_left(width: f32) -> f32 {
    let count = f32::from(u8::try_from(LayoutTemplate::ALL.len()).unwrap_or(u8::MAX));
    let strip = count * TEMPLATE_W + (count - 1.0) * TEMPLATE_GAP + 2.0 * TEMPLATE_PAD;
    (width - strip) / 2.0
}

fn template_at(x: f32, y: f32, width: f32) -> Option<(LayoutTemplate, usize)> {
    let top = TEMPLATE_TOP + TEMPLATE_PAD;
    if y < top || y > top + TEMPLATE_H {
        return None;
    }
    let mut left = templates_left(width) + TEMPLATE_PAD;
    for template in LayoutTemplate::ALL {
        if x >= left && x <= left + TEMPLATE_W {
            let (ux, uy) = ((x - left) / TEMPLATE_W, (y - top) / TEMPLATE_H);
            let slot = template.slots().iter().position(|slot| {
                ux >= slot.x && ux <= slot.x + slot.w && uy >= slot.y && uy <= slot.y + slot.h
            })?;
            return Some((template, slot));
        }
        left += TEMPLATE_W + TEMPLATE_GAP;
    }
    None
}

impl EnvironmentShell {
    pub(super) fn pane_strip_h(&self) -> f32 {
        let split = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.current_session())
            .is_some_and(|session| session.panes.panes.len() > 1);
        if split {
            PANE_STRIP_H
        } else {
            0.0
        }
    }

    pub(in crate::environment_shell) fn sync_session_panes(&mut self) {
        let env = self.active.clone();
        let target = self.pane_target.take();
        let Some(live) = self.envs.get_mut(&env) else {
            return;
        };
        let docked = live.activity_docked.clone();
        let tabs: Vec<TerminalId> = live
            .session_tabs()
            .into_iter()
            .filter(|id| !docked.contains(id))
            .collect();
        let guests: Vec<TerminalId> = live
            .data
            .side_panel
            .terminals
            .iter()
            .map(|terminal| terminal.id)
            .filter(|id| !docked.contains(id))
            .collect();
        let focused = live.focused;
        let Some(session) = live.data.current_session_mut() else {
            return;
        };
        let into = target.or_else(|| focused.and_then(|id| session.panes.pane_of(id)));
        session.panes.sync(&tabs, &guests, into);
        if let Some(focused) = focused.filter(|id| session.panes.pane_of(*id).is_some()) {
            session.panes.show(focused);
        }
    }

    pub(in crate::environment_shell) fn session_panes_view(
        &self,
        live: &LiveEnv,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = live.data.current_session() else {
            return div().into_any_element();
        };
        let size = self.canvas_px;
        let panes: Vec<AnyElement> = session
            .panes
            .layout()
            .into_iter()
            .map(|(id, rect)| self.pane_card(live, &session.panes, id, rect, size, cx))
            .collect();
        let dividers: Vec<AnyElement> = session
            .panes
            .dividers()
            .into_iter()
            .map(|(divider, axis, ratio, frame)| {
                Self::pane_divider_handle(divider, axis, ratio, frame, size, cx)
            })
            .collect();
        let dragging = cx.has_active_drag();
        div()
            .id(PANES_ID)
            .relative()
            .size_full()
            .on_drag_move(cx.listener(
                |this: &mut Self, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    this.aim_tab_drop(event, cx);
                },
            ))
            .on_drop(cx.listener(|this: &mut Self, dragged: &DraggedTab, _, cx| {
                this.drop_tab(dragged.id, cx);
            }))
            .children(panes)
            .children(dividers)
            .when(dragging, |view| view.children(self.drop_preview(size, cx)))
            .when(dragging && self.pane_drag.templates, |view| {
                view.child(self.template_strip(size, cx))
            })
            .into_any_element()
    }

    fn pane_card(
        &self,
        live: &LiveEnv,
        panes: &SessionPanes,
        id: PaneId,
        rect: UnitRect,
        size: (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let (x, y, w, h) = pane_px(rect, (0.0, 0.0), size);
        let pane = panes.pane(id);
        let shown = pane.and_then(|pane| pane.shown);
        let several = panes.panes.len() > 1;
        let body = match shown {
            Some(tab) => self.pane_body(live, tab, rect, cx),
            None => Self::empty_pane(id, several, cx),
        };
        card(pal, cx)
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(several, |card| {
                card.child(self.pane_strip(live, panes, id, StripPlace::Pane, cx))
            })
            .child(body)
            .into_any_element()
    }

    pub(super) fn guest_tab(
        live: &LiveEnv,
        id: TerminalId,
        shown: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let terminal = live
            .data
            .side_panel
            .terminals
            .iter()
            .find(|terminal| terminal.id == id)?;
        let pal = palette(cx);
        let dragged = DraggedTab::new(
            id,
            terminal.title.clone(),
            IconName::Terminal,
            pal.text_muted,
            cx.entity().downgrade(),
        );
        Some(
            tab_shell((GUEST_TAB_PREFIX, id.as_u64()), shown, pal, cx)
                .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.show_terminal_tab(id, window, cx);
                }))
                .child(icon(IconName::Terminal, ICON_SM, pal.text_muted))
                .child(
                    div()
                        .max_w(px(TAB_LABEL_MAX_W))
                        .truncate()
                        .child(terminal.title.clone()),
                )
                .child(
                    Button::new((GUEST_BACK_PREFIX, id.as_u64()))
                        .icon(IconName::SidebarRight)
                        .small()
                        .tooltip(GUEST_BACK_TOOLTIP)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            cx.stop_propagation();
                            this.edit_panes(cx, |panes| panes.release(id));
                        })),
                )
                .into_any_element(),
        )
    }

    fn empty_pane(id: PaneId, closable: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(SPACE_1))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .child(icon(IconName::Layers, ICON, pal.text_muted))
            .child(
                div()
                    .text_color(rgb(pal.text_strong))
                    .child(EMPTY_PANE_TITLE),
            )
            .child(div().text_color(rgb(pal.text_muted)).child(EMPTY_PANE_TEXT))
            .when(closable, |empty| {
                empty.child(
                    Button::new((CLOSE_PANE_PREFIX, u64::from(id.0)))
                        .icon(IconName::Close)
                        .small()
                        .tooltip(CLOSE_PANE_TOOLTIP)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.edit_panes(cx, |panes| panes.close_pane(id));
                        })),
                )
            })
            .into_any_element()
    }

    fn pane_divider_handle(
        divider: DividerId,
        axis: Axis,
        ratio: f32,
        frame: UnitRect,
        size: (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let base = div()
            .id((DIVIDER_PREFIX, u64::from(divider.0)))
            .absolute()
            .occlude()
            .hover(|style| style.bg(pal.bg_hover))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut Self, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.drag = DragState::ResizePane {
                        divider,
                        axis,
                        frame,
                    };
                }),
            );
        let element = match axis {
            Axis::Row => {
                let at = (frame.x + frame.w * ratio) * size.0;
                base.left(px(at - DIVIDER_HIT / 2.0))
                    .top(px(frame.y * size.1))
                    .w(px(DIVIDER_HIT))
                    .h(px(frame.h * size.1))
                    .cursor_col_resize()
            }
            Axis::Column => {
                let at = (frame.y + frame.h * ratio) * size.1;
                base.top(px(at - DIVIDER_HIT / 2.0))
                    .left(px(frame.x * size.0))
                    .h(px(DIVIDER_HIT))
                    .w(px(frame.w * size.0))
                    .cursor_row_resize()
            }
        };
        element.rounded(px(RADIUS_XS)).into_any_element()
    }

    pub(in crate::environment_shell) fn drag_pane_divider(
        &mut self,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let DragState::ResizePane {
            divider,
            axis,
            frame,
        } = self.drag
        else {
            return false;
        };
        let (origin_x, origin_y) = self.window_origin();
        let (width, height) = self.canvas_px;
        let ratio = match axis {
            Axis::Row => ((x - origin_x) / width.max(1.0) - frame.x) / frame.w.max(f32::EPSILON),
            Axis::Column => {
                ((y - origin_y) / height.max(1.0) - frame.y) / frame.h.max(f32::EPSILON)
            }
        };
        self.edit_panes(cx, |panes| panes.resize(divider, ratio));
        true
    }

    pub(in crate::environment_shell) fn edit_panes(
        &mut self,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut SessionPanes),
    ) {
        let env = self.active.clone();
        if let Some(session) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.data.current_session_mut())
        {
            edit(&mut session.panes);
        }
        cx.notify();
        self.store();
    }

    fn aim_tab_drop(&mut self, event: &DragMoveEvent<DraggedTab>, cx: &mut Context<Self>) {
        let bounds = event.bounds;
        let position = event.event.position;
        if !bounds.contains(&position) {
            if self.pane_drag.target.is_some() {
                self.pane_drag.clear();
                cx.notify();
            }
            return;
        }
        let (width, height) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
        let x = (position.x - bounds.origin.x).as_f32();
        let y = (position.y - bounds.origin.y).as_f32();
        let centre = x > width / 3.0 && x < width * 2.0 / 3.0;
        let templates = y < TEMPLATE_BAND_H && centre;
        let (target, frame) = if templates {
            match template_at(x, y, width) {
                Some((template, slot)) => (
                    Some(DropTarget::Template(template, slot)),
                    template.slots().get(slot).copied(),
                ),
                None => (None, None),
            }
        } else {
            self.zone_under(x / width.max(1.0), y / height.max(1.0), height)
        };
        let changed = self.pane_drag.target != target || self.pane_drag.templates != templates;
        self.pane_drag.aim(target, templates, frame);
        if changed {
            cx.notify();
        }
    }

    fn zone_under(&self, ux: f32, uy: f32, height: f32) -> (Option<DropTarget>, Option<UnitRect>) {
        let Some(session) = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.current_session())
        else {
            return (None, None);
        };
        let hit = session.panes.layout().into_iter().find(|(_, rect)| {
            ux >= rect.x && ux <= rect.x + rect.w && uy >= rect.y && uy <= rect.y + rect.h
        });
        let Some((pane, rect)) = hit else {
            return (None, None);
        };
        let strip = self.pane_strip_h() / height.max(1.0);
        if uy - rect.y < strip {
            return (Some(DropTarget::Strip(pane)), Some(rect));
        }
        let (lx, ly) = ((ux - rect.x) / rect.w, (uy - rect.y) / rect.h);
        match SnapZone::at(lx, ly, ZONE_REACH) {
            Some(zone) => (Some(DropTarget::Zone(pane, zone)), Some(zone.frame(rect))),
            None => (Some(DropTarget::Strip(pane)), Some(rect)),
        }
    }

    fn drop_tab(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let target = self.pane_drag.target;
        self.pane_drag.clear();
        let Some(target) = target else {
            cx.notify();
            return;
        };
        let mut refused = false;
        self.edit_panes(cx, |panes| match target {
            DropTarget::Strip(pane) => {
                refused = panes.move_tab(id, pane, usize::MAX).is_err();
            }
            DropTarget::Zone(pane, zone) => {
                refused = panes.snap(id, pane, zone).is_err();
            }
            DropTarget::Template(template, slot) => panes.apply_template(template, id, slot),
        });
        if refused {
            self.toast(PANES_FULL_TEXT, cx);
        }
        self.focus_terminal(id);
    }

    fn ghost_pull(&self, mouse: (f32, f32)) -> (f32, f32) {
        if !matches!(self.pane_drag.target, Some(DropTarget::Zone(..))) {
            return (0.0, 0.0);
        }
        let (x, y, w, h) = pane_px(
            self.pane_drag.preview(),
            self.window_origin(),
            self.canvas_px,
        );
        let lean = |towards: f32, from: f32| {
            ((towards - from) * GHOST_PULL).clamp(-GHOST_PULL_MAX, GHOST_PULL_MAX)
        };
        (lean(x + w / 2.0, mouse.0), lean(y + h / 2.0, mouse.1))
    }

    pub(in crate::environment_shell) fn end_tab_drag(&mut self, cx: &mut Context<Self>) {
        if self.pane_drag.target.is_some() || self.pane_drag.templates {
            self.pane_drag.clear();
            cx.notify();
        }
    }

    fn drop_preview(&self, size: (f32, f32), cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.pane_drag.target?;
        if matches!(target, DropTarget::Template(..)) {
            return None;
        }
        let pal = palette(cx);
        let (x, y, w, h) = pane_px(self.pane_drag.preview(), (0.0, 0.0), size);
        Some(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .rounded(px(RADIUS_LG))
                .bg(tinted(pal.accent, PREVIEW_FILL_ALPHA))
                .border_2()
                .border_color(rgb(pal.accent))
                .into_any_element(),
        )
    }

    fn template_strip(&self, size: (f32, f32), cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let hovered = match self.pane_drag.target {
            Some(DropTarget::Template(template, slot)) => Some((template, slot)),
            _ => None,
        };
        let templates: Vec<AnyElement> = LayoutTemplate::ALL
            .iter()
            .enumerate()
            .map(|(index, template)| {
                let slots: Vec<AnyElement> = template
                    .slots()
                    .into_iter()
                    .enumerate()
                    .map(|(slot, rect)| {
                        let lit = hovered == Some((*template, slot));
                        div()
                            .absolute()
                            .left(px(rect.x * TEMPLATE_W + SLOT_GAP / 2.0))
                            .top(px(rect.y * TEMPLATE_H + SLOT_GAP / 2.0))
                            .w(px(rect.w * TEMPLATE_W - SLOT_GAP))
                            .h(px(rect.h * TEMPLATE_H - SLOT_GAP))
                            .rounded(px(RADIUS_XS))
                            .when(lit, |slot| slot.bg(rgb(pal.accent)))
                            .when(!lit, |slot| slot.bg(pal.bg_selected))
                            .into_any_element()
                    })
                    .collect();
                div()
                    .id((TEMPLATE_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
                    .relative()
                    .flex_none()
                    .w(px(TEMPLATE_W))
                    .h(px(TEMPLATE_H))
                    .children(slots)
                    .into_any_element()
            })
            .collect();
        div()
            .absolute()
            .top(px(TEMPLATE_TOP))
            .left(px(templates_left(size.0)))
            .flex()
            .flex_row()
            .gap(px(TEMPLATE_GAP))
            .p(px(TEMPLATE_PAD))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_surface))
            .border_1()
            .border_color(pal.border)
            .shadow(floating_shadow(pal))
            .children(templates)
            .into_any_element()
    }
}

const PANES_FULL_TEXT: &str = "A session shows at most four panes.";
