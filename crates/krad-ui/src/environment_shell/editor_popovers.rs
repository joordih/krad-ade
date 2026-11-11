mod code_actions;
mod completion;
mod content;
mod hover;
mod signature;
mod snippet;

use gpui_kit::component::input::EditorState;
use gpui_kit::{px, size, AnyElement, App, Bounds, Context, Div, Entity, Pixels, Styled, Window};
use krad_domain::TerminalId;
use std::ops::Range;

use super::language_servers::Document;
use super::EnvironmentShell;
use crate::components::floating::{self, floating_panel, Anchor, Edge, Placed, Side};
use crate::theme::Palette;
pub(super) use code_actions::ActionMenu;
pub(super) use hover::HoverCard;
pub(super) use signature::SignatureCard;

pub(super) enum EditorPopover {
    Hover(HoverCard),
    Actions(ActionMenu),
    Completion(completion::CompletionMenu),
}

impl EditorPopover {
    const fn tab(&self) -> TerminalId {
        match self {
            Self::Hover(card) => card.tab,
            Self::Actions(menu) => menu.tab,
            Self::Completion(menu) => menu.tab,
        }
    }
}

fn popover_panel(pal: &Palette, cx: &App) -> Div {
    floating_panel(pal, cx)
}

fn place_on(
    side: Side,
    anchor: Bounds<Pixels>,
    inset: (f32, f32),
    root: (f32, f32),
    wanted: (f32, f32),
) -> Placed {
    floating::place(
        Anchor::Bounds(anchor).relative_to(inset),
        size(px(wanted.0), px(wanted.1)),
        size(px(root.0), px(root.1)),
        side,
    )
}

fn place(
    anchor: Bounds<Pixels>,
    inset: (f32, f32),
    root: (f32, f32),
    wanted: (f32, f32),
) -> Placed {
    place_on(Side::Below, anchor, inset, root, wanted)
}

fn place_above(
    anchor: Bounds<Pixels>,
    inset: (f32, f32),
    root: (f32, f32),
    wanted: (f32, f32),
) -> Placed {
    place_on(Side::Above, anchor, inset, root, wanted)
}

fn placed(panel: Div, placement: Placed) -> Div {
    let panel = panel
        .left(px(placement.left))
        .max_h(px(placement.max_height));
    match placement.edge {
        Edge::Top(top) => panel.top(px(top)),
        Edge::Bottom(bottom) => panel.bottom(px(bottom)),
    }
}

impl EnvironmentShell {
    fn popover_editor(&self, tab: TerminalId) -> Option<(Entity<EditorState>, Document)> {
        let live = self.envs.get(&self.active)?;
        let (shown, _) = live.shown_file()?;
        if shown != tab {
            return None;
        }
        let editor = live.editors.get(&tab)?;
        Some((
            editor.state.clone()?,
            Document::new(&live.data.host, &editor.path),
        ))
    }

    fn tab_showing(&self, document: &Document) -> Option<TerminalId> {
        let live = self.envs.get(&self.active)?;
        if live.data.host != document.host {
            return None;
        }
        let (tab, path) = live.shown_file()?;
        (path == document.path).then_some(tab)
    }

    fn anchor_bounds(
        state: &Entity<EditorState>,
        anchor: &Range<usize>,
        cx: &App,
    ) -> Option<Bounds<Pixels>> {
        state.read(cx).range_to_bounds(anchor)
    }

    pub(super) fn close_editor_popovers(&mut self, cx: &mut Context<Self>) {
        if self.editor_popover.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn editor_popover_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let popover = self.editor_popover.as_ref()?;
        let (state, _) = self.popover_editor(popover.tab())?;
        let root = self.root_size(window);
        match popover {
            EditorPopover::Hover(card) => self.hover_overlay(card, &state, root, cx),
            EditorPopover::Actions(menu) => self.actions_overlay(menu, &state, root, cx),
            EditorPopover::Completion(menu) => self.completion_overlay(menu, &state, root, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::FLOATING_EDGE_MARGIN;
    use gpui_kit::point;

    fn line(top: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(100.0), px(top)), size(px(40.0), px(20.0)))
    }

    #[test]
    fn a_popover_opens_below_its_line_unless_only_the_room_above_fits_it() {
        let root = (1000.0, 800.0);
        let below = place(line(100.0), (0.0, 0.0), root, (400.0, 300.0));
        assert_eq!(below.edge, Edge::Top(124.0));
        assert!((below.max_height - 300.0).abs() < f32::EPSILON);
        let above = place(line(700.0), (0.0, 0.0), root, (400.0, 300.0));
        assert_eq!(above.edge, Edge::Bottom(104.0));
        let pushed = place(line(100.0), (0.0, 0.0), (300.0, 800.0), (400.0, 300.0));
        assert!((pushed.left - FLOATING_EDGE_MARGIN).abs() < f32::EPSILON);
    }

    #[test]
    fn a_signature_card_prefers_the_room_above_its_line() {
        let root = (1000.0, 800.0);
        let above = place_above(line(400.0), (0.0, 0.0), root, (400.0, 200.0));
        assert_eq!(above.edge, Edge::Bottom(404.0));
        let below = place_above(line(50.0), (0.0, 0.0), root, (400.0, 200.0));
        assert_eq!(below.edge, Edge::Top(74.0));
    }
}
