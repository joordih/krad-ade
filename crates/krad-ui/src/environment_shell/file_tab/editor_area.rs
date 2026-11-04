use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::{div, px, Context, Entity, KeyDownEvent, ParentElement, Styled, Window};
use krad_domain::TextScale;

use super::super::language_servers::Document;
use super::super::rename::{KEY_ENTER, KEY_ESCAPE};
use super::super::{EnvironmentShell, LiveEnv};
use super::minimap::minimap;
use super::problem_lens::problem_lens;
use super::problem_marks::problem_marks;
use super::problem_underlines::problem_underlines;
use super::FileEditor;
use crate::theme::SPACE_1_5;

const EDITOR_TEXT_PX: f32 = 14.0;
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
    ) -> gpui_kit::Div {
        let overlays = editor.problems.clone().map(|problems| {
            [
                problem_underlines(state.clone(), problems.clone()),
                problem_lens(state.clone(), problems.clone(), settings.problem_lens),
                problem_marks(state.clone(), problems),
            ]
        });
        let library = self.is_library_document(&Document::new(&live.data.host, &editor.path));
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pl(px(SPACE_1_5))
                    .child(
                        Editor::new(state)
                            .bordered(false)
                            .readonly(library)
                            .text_size(px(EDITOR_TEXT_PX * scale.as_f32()))
                            .h_full(),
                    )
                    .children(overlays.into_iter().flatten()),
            )
            .children(
                editor
                    .minimap
                    .clone()
                    .filter(|_| settings.minimap.is_shown())
                    .map(|shown| minimap(shown, state.clone())),
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
