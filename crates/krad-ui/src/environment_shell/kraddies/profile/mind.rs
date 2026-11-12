use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, rgb, AnimationExt as _, AnyElement, AppContext as _, Context, InteractiveElement,
    IntoElement, ParentElement, SharedString, SpringAnimation, Styled, Window,
};
use krad_agents::{ErrorCode, KraddieMemoryInfo};
use krad_domain::{KraddieId, MemoryError, ToastKind, MAX_MEMORY_LINES};

use super::super::super::EnvironmentShell;
use super::super::layout::PageLayout;
use super::super::motion::{entering, SLIDE_SPRING};
use super::super::tile::{caption, grid, tile, Tile};
use super::LineDraft;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, LINE_SM, RADIUS_LG, SPACE_1, SPACE_2, SPACE_3, TEXT_SM};

const TILE_PREFIX: &str = "kraddie-keepsakes";
const MEMORY_TITLE: &str = "Keepsakes";
const MEMORY_DETAIL: &str =
    "Short notes it carries from run to run. It can tuck one away itself with krad kraddie remember.";
const MEMORY_ADD: &str = "kraddie-memory-add";
const MEMORY_ADD_TOOLTIP: &str = "Keep this";
const METER_ID: &str = "kraddie-keepsake-meter";
const METER_H: f32 = 6.0;
const ROW_PREFIX: &str = "kraddie-keepsake";
const MEMORY_EDIT_PREFIX: &str = "kraddie-memory-edit";
const MEMORY_DELETE_PREFIX: &str = "kraddie-memory-delete";
const MEMORY_SAVE: &str = "kraddie-memory-save-line";
const MEMORY_CANCEL: &str = "kraddie-memory-cancel-line";
const EDIT_TOOLTIP: &str = "Edit";
const DELETE_TOOLTIP: &str = "Let it go";
const SAVE_LABEL: &str = "Save";
const CANCEL_LABEL: &str = "Cancel";
const EMPTY_MEMORY: &str = "No keepsakes yet. Add the first one above.";
const NO_KRADDIE: &str = "No Kraddie with that name";
const MEMORY_ROW_PREFIX: &str = "kraddie-keepsake-row";

fn fraction(part: usize, whole: usize) -> f32 {
    let as_f32 = |value: usize| u16::try_from(value).map_or(f32::from(u16::MAX), f32::from);
    if whole == 0 {
        return 0.0;
    }
    (as_f32(part) / as_f32(whole)).clamp(0.0, 1.0)
}

const fn memory_code(error: MemoryError) -> ErrorCode {
    match error {
        MemoryError::Empty | MemoryError::TooLong => ErrorCode::BadRequest,
        MemoryError::Full => ErrorCode::Forbidden,
        MemoryError::NoSuchLine => ErrorCode::NotFound,
    }
}

impl EnvironmentShell {
    fn kraddie_named(&self, name: &str) -> Option<KraddieId> {
        self.kraddies
            .entries
            .iter()
            .find(|entry| entry.kraddie.name.matches(name) || entry.kraddie.id().as_str() == name)
            .map(|entry| entry.kraddie.id())
    }

    fn memory_info(&self, id: &KraddieId) -> KraddieMemoryInfo {
        KraddieMemoryInfo {
            kraddie: id.to_string(),
            lines: self.kraddie_mind(id).memory.lines().to_vec(),
            limit: MAX_MEMORY_LINES,
        }
    }

    pub(in crate::environment_shell) fn kraddie_memory_from_bus(
        &self,
        name: &str,
    ) -> Result<KraddieMemoryInfo, (ErrorCode, String)> {
        let id = self
            .kraddie_named(name)
            .ok_or_else(|| (ErrorCode::NotFound, NO_KRADDIE.to_owned()))?;
        Ok(self.memory_info(&id))
    }

    pub(in crate::environment_shell) fn remember_from_bus(
        &mut self,
        name: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Result<KraddieMemoryInfo, (ErrorCode, String)> {
        let id = self
            .kraddie_named(name)
            .ok_or_else(|| (ErrorCode::NotFound, NO_KRADDIE.to_owned()))?;
        let mut memory = self.kraddie_mind(&id).memory;
        memory
            .add(text)
            .map_err(|error| (memory_code(error), error.to_string()))?;
        self.edit_mind(&id, |mind| mind.memory = memory, cx);
        tracing::info!(kraddie = %id, "kraddie remembered a line");
        Ok(self.memory_info(&id))
    }

    fn add_memory_line(&mut self, id: &KraddieId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.kraddies.profile.new_line.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let mut memory = self.kraddie_mind(id).memory;
        match memory.add(&text) {
            Ok(_) => {
                self.edit_mind(id, |mind| mind.memory = memory, cx);
                input.update(cx, |input, cx| input.set_value("", window, cx));
            }
            Err(error) => self.toast(ToastKind::Error, &error.to_string(), cx),
        }
    }

    fn start_line_edit(
        &mut self,
        id: &KraddieId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(line) = self.kraddie_mind(id).memory.lines().get(index).cloned() else {
            return;
        };
        let text = cx.new(|cx| InputState::new(window, cx).default_value(line));
        self.kraddies.profile.editing_line = Some(LineDraft { index, text });
        cx.notify();
    }

    fn save_line_edit(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let Some(draft) = self.kraddies.profile.editing_line.take() else {
            return;
        };
        let text = draft.text.read(cx).value().to_string();
        let mut memory = self.kraddie_mind(id).memory;
        match memory.edit(draft.index, &text) {
            Ok(()) => self.edit_mind(id, |mind| mind.memory = memory, cx),
            Err(error) => {
                self.kraddies.profile.editing_line = Some(draft);
                self.toast(ToastKind::Error, &error.to_string(), cx);
            }
        }
    }

    fn forget_line(&mut self, id: &KraddieId, index: usize, cx: &mut Context<Self>) {
        let mut memory = self.kraddie_mind(id).memory;
        if memory.remove(index).is_ok() {
            self.kraddies.profile.editing_line = None;
            self.edit_mind(id, |mind| mind.memory = memory, cx);
        }
    }

    pub(super) fn keepsakes_section(
        &self,
        id: &KraddieId,
        layout: &PageLayout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let memory = self.kraddie_mind(id).memory;
        let editing = self.kraddies.profile.editing_line.as_ref();
        let key = id.to_string();
        let rows: Vec<AnyElement> = memory
            .lines()
            .iter()
            .enumerate()
            .map(
                |(index, line)| match editing.filter(|draft| draft.index == index) {
                    Some(draft) => Self::line_editor(id, &draft.text, cx),
                    None => entering(
                        Self::memory_row(id, index, line, pal, cx),
                        (SharedString::from(format!("{ROW_PREFIX}-{key}")), index),
                        index,
                    )
                    .into_any_element(),
                },
            )
            .collect();
        let target = id.clone();
        let adder = self.kraddies.profile.new_line.as_ref().map(|input| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .child(div().flex_1().child(Input::new(input)))
                .child(
                    Button::new(MEMORY_ADD)
                        .icon(IconName::Add)
                        .variant(ButtonVariant::Primary)
                        .tooltip(MEMORY_ADD_TOOLTIP)
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.add_memory_line(&target, window, cx);
                        })),
                )
        });
        let filled = fraction(memory.len(), MAX_MEMORY_LINES);
        let meter = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(METER_H))
                    .rounded_full()
                    .bg(rgb(pal.chip_bg))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(pal.accent))
                            .with_spring(
                                (SharedString::from(format!("{METER_ID}-{key}")), 0),
                                SpringAnimation::new(SLIDE_SPRING).to(filled),
                                |bar, filled| bar.w(gpui_kit::relative(filled.clamp(0.0, 1.0))),
                            ),
                    ),
            )
            .child(caption(memory.meter(), pal));
        let listed = if rows.is_empty() {
            caption(EMPTY_MEMORY, pal)
                .py(px(SPACE_2))
                .into_any_element()
        } else {
            div().flex().flex_col().children(rows).into_any_element()
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(meter)
            .children(adder)
            .child(listed)
            .into_any_element();
        grid(
            layout.gutter,
            vec![tile(
                Tile {
                    id: SharedString::from(format!("{TILE_PREFIX}-{key}-keepsakes")),
                    index: 0,
                    width: layout.content_w,
                    title: MEMORY_TITLE,
                    note: Some(MEMORY_DETAIL.into()),
                    accessory: None,
                    body,
                },
                pal,
            )],
        )
        .into_any_element()
    }

    fn memory_row(
        id: &KraddieId,
        index: usize,
        line: &str,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let (edit, forget) = (id.clone(), id.clone());
        let controls = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .child(
                Button::new((MEMORY_EDIT_PREFIX, index))
                    .icon(IconName::Pencil)
                    .small()
                    .tooltip(EDIT_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.start_line_edit(&edit, index, window, cx);
                    })),
            )
            .child(
                Button::new((MEMORY_DELETE_PREFIX, index))
                    .icon(IconName::Minus)
                    .small()
                    .tooltip(DELETE_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.forget_line(&forget, index, cx);
                    })),
            );
        div()
            .id((MEMORY_ROW_PREFIX, index))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .rounded(px(RADIUS_LG))
            .hover(|style| style.bg(pal.bg_hover))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(line.to_owned()),
            )
            .child(controls)
    }

    fn line_editor(
        id: &KraddieId,
        text: &gpui_kit::Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let target = id.clone();
        div()
            .py(px(SPACE_1))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_3))
                    .child(div().flex_1().child(Input::new(text)))
                    .child(
                        Button::new(MEMORY_CANCEL)
                            .label(CANCEL_LABEL)
                            .small()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.kraddies.profile.editing_line = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(MEMORY_SAVE)
                            .label(SAVE_LABEL)
                            .small()
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.save_line_edit(&target, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keepsake_meter_fills_with_what_it_carries() {
        assert!(fraction(0, MAX_MEMORY_LINES).abs() < f32::EPSILON);
        assert!((fraction(MAX_MEMORY_LINES, MAX_MEMORY_LINES) - 1.0).abs() < f32::EPSILON);
        assert!((fraction(50, 200) - 0.25).abs() < f32::EPSILON);
        assert!(fraction(3, 0).abs() < f32::EPSILON);
    }

    #[test]
    fn memory_refusals_keep_stable_error_codes() {
        assert_eq!(memory_code(MemoryError::Empty), ErrorCode::BadRequest);
        assert_eq!(memory_code(MemoryError::TooLong), ErrorCode::BadRequest);
        assert_eq!(memory_code(MemoryError::Full), ErrorCode::Forbidden);
        assert_eq!(memory_code(MemoryError::NoSuchLine), ErrorCode::NotFound);
    }
}
