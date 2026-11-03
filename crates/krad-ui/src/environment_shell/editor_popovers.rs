mod code_actions;
mod content;
mod hover;

use gpui_kit::component::input::EditorState;
use gpui_kit::{px, AnyElement, App, Bounds, Context, Div, Entity, Pixels, Styled, Window};
use krad_domain::TerminalId;
use std::ops::Range;

use super::language_servers::Document;
use super::EnvironmentShell;
use crate::theme::{popover_fill, popover_shadow, Palette, SPACE_1_5};
pub(super) use code_actions::ActionMenu;
pub(super) use hover::HoverCard;

const POPOVER_RADIUS: f32 = 12.0;
const POPOVER_GAP: f32 = 4.0;
const EDGE_MARGIN: f32 = 8.0;

pub(super) enum EditorPopover {
    Hover(HoverCard),
    Actions(ActionMenu),
}

impl EditorPopover {
    const fn tab(&self) -> TerminalId {
        match self {
            Self::Hover(card) => card.tab,
            Self::Actions(menu) => menu.tab,
        }
    }
}

fn popover_panel(pal: &Palette, cx: &App) -> Div {
    gpui_kit::div()
        .absolute()
        .flex()
        .flex_col()
        .p(px(SPACE_1_5))
        .rounded(px(POPOVER_RADIUS))
        .bg(popover_fill(pal, cx))
        .border_1()
        .border_color(pal.divider)
        .shadow(popover_shadow(pal, cx))
}

#[derive(Clone, Copy)]
struct Placement {
    left: f32,
    top: Option<f32>,
    bottom: Option<f32>,
    max_height: f32,
}

fn place(
    anchor: Bounds<Pixels>,
    inset: (f32, f32),
    root: (f32, f32),
    wanted: (f32, f32),
) -> Placement {
    let (width, height) = wanted;
    let left = (anchor.left().as_f32() - inset.0)
        .min(root.0 - width - EDGE_MARGIN)
        .max(EDGE_MARGIN);
    let below = anchor.bottom().as_f32() - inset.1 + POPOVER_GAP;
    let above = anchor.top().as_f32() - inset.1 - POPOVER_GAP;
    let room_below = root.1 - below - EDGE_MARGIN;
    let room_above = above - EDGE_MARGIN;
    if room_below >= height || room_below >= room_above {
        Placement {
            left,
            top: Some(below),
            bottom: None,
            max_height: room_below.min(height).max(0.0),
        }
    } else {
        Placement {
            left,
            top: None,
            bottom: Some(root.1 - above),
            max_height: room_above.min(height).max(0.0),
        }
    }
}

fn placed(panel: Div, placement: Placement) -> Div {
    let panel = panel
        .left(px(placement.left))
        .max_h(px(placement.max_height));
    let panel = match placement.top {
        Some(top) => panel.top(px(top)),
        None => panel,
    };
    match placement.bottom {
        Some(bottom) => panel.bottom(px(bottom)),
        None => panel,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, size};

    fn line(top: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(100.0), px(top)), size(px(40.0), px(20.0)))
    }

    #[test]
    fn a_popover_opens_below_its_line_unless_only_the_room_above_fits_it() {
        let root = (1000.0, 800.0);
        let below = place(line(100.0), (0.0, 0.0), root, (400.0, 300.0));
        assert_eq!(below.top, Some(124.0));
        assert!((below.max_height - 300.0).abs() < f32::EPSILON);
        let above = place(line(700.0), (0.0, 0.0), root, (400.0, 300.0));
        assert_eq!(above.top, None);
        assert_eq!(above.bottom, Some(104.0));
        let pushed = place(line(100.0), (0.0, 0.0), (300.0, 800.0), (400.0, 300.0));
        assert!((pushed.left - EDGE_MARGIN).abs() < f32::EPSILON);
    }
}
