use gpui_kit::component::input::{Editor, EditorState, RopeExt as _};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, Context, Entity, InteractiveElement as _, KeyDownEvent, MouseButton, MouseDownEvent,
    ParentElement, Styled, Window,
};
use krad_domain::TextScale;

use super::super::language_servers::Document;
use super::super::rename::{KEY_ENTER, KEY_ESCAPE};
use super::super::{EnvironmentShell, LiveEnv};
use super::line_numbers::{gutter_width, line_numbers};
use super::minimap::minimap;
use super::problem_lens::problem_lens;
use super::problem_marks::problem_marks;
use super::problem_underlines::problem_underlines;
use super::FileEditor;
use crate::theme::{font_features, EDITOR_FONT_SIZE};

const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";

impl EnvironmentShell {
    pub(super) fn editor_area(
        &self,
        live: &LiveEnv,
        editor: &FileEditor,
        state: &Entity<EditorState>,
        settings: krad_domain::EditorSettings,
        scale: TextScale,
        cx: &Context<Self>,
    ) -> gpui_kit::Div {
        let overlays = editor.problems.clone().map(|problems| {
            [
                problem_underlines(state.clone(), problems.clone()),
                problem_lens(state.clone(), problems.clone(), settings.problem_lens),
                problem_marks(state.clone(), problems),
            ]
        });
        let font_size = self.settings.editor_font.size_or(EDITOR_FONT_SIZE).as_f32();
        let library = self.is_library_document(&Document::new(&live.data.host, &editor.path));
        let text_px = font_size * scale.as_f32();
        let lines = state.read(cx).text().lines_len();
        let gutter = gutter_width(lines, text_px);
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(text_px))
                    .child(line_numbers(state.clone(), gutter, text_px)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        Editor::new(state)
                            .bordered(false)
                            .readonly(library)
                            .text_size(px(text_px))
                            .font_features(font_features(self.settings.editor_font.ligatures))
                            .h_full(),
                    )
                    .children(overlays.into_iter().flatten()),
            )
            .children(
                editor
                    .minimap
                    .clone()
                    .filter(|_| settings.minimap.is_shown())
                    .map(|shown| {
                        div()
                            .flex_none()
                            .h_full()
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|this: &mut Self, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.minimap_menu(event, cx);
                                }),
                            )
                            .child(minimap(shown, state.clone()))
                    }),
            )
    }

    pub(super) fn on_completion_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.completions_open() {
            if event.keystroke.key == KEY_ESCAPE {
                self.close_signature(cx);
            }
            return;
        }
        match event.keystroke.key.as_str() {
            KEY_UP => self.step_completion(false, cx),
            KEY_DOWN => self.step_completion(true, cx),
            KEY_ENTER => {
                self.accept_completion(window, cx);
            }
            KEY_ESCAPE => self.close_editor_popovers(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}
