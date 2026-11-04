mod line_edits;

use gpui_kit::component::input::{EditorState, Enter, IndentInline};
use gpui_kit::{Context, Entity, Window};
use krad_lsp::lsp_types::Position;
use std::path::PathBuf;

use super::EnvironmentShell;
use crate::actions::{
    AcceptCompletion, DeleteLine, DuplicateLine, MoveLineDown, MoveLineUp, NextProblem,
    PreviousProblem, ToggleLineComment,
};
use line_edits::LineEdit;

impl EnvironmentShell {
    fn shown_editor(&self) -> Option<(PathBuf, Entity<EditorState>)> {
        let live = self.envs.get(&self.active)?;
        let (id, path) = live.shown_file()?;
        let state = live.editors.get(&id)?.state.clone()?;
        Some((path, state))
    }

    fn edit_lines(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&EditorState, &PathBuf) -> Option<LineEdit>,
    ) {
        let Some((path, state)) = self.shown_editor() else {
            return;
        };
        let Some(LineEdit { edits, cursor }) = change(state.read(cx), &path) else {
            return;
        };
        state.update(cx, |state, cx| {
            state.apply_lsp_edits(&edits, window, cx);
            state.set_cursor_position(cursor, window, cx);
        });
    }

    pub(super) fn on_accept_completion(
        &mut self,
        _: &AcceptCompletion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.accept_completion(window, cx) {
            return;
        }
        let menu_open = self
            .shown_editor()
            .is_some_and(|(_, state)| state.read(cx).completion_menu_state().open);
        if menu_open {
            window.dispatch_action(
                Box::new(Enter {
                    secondary: false,
                    shift: false,
                }),
                cx,
            );
        } else {
            window.dispatch_action(Box::new(IndentInline), cx);
        }
    }

    pub(super) fn on_duplicate_line(
        &mut self,
        _: &DuplicateLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_lines(window, cx, |state, _| {
            Some(line_edits::duplicate_line(
                state.text(),
                state.cursor_position(),
            ))
        });
    }

    pub(super) fn on_delete_line(
        &mut self,
        _: &DeleteLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_lines(window, cx, |state, _| {
            Some(line_edits::delete_line(
                state.text(),
                state.cursor_position(),
            ))
        });
    }

    pub(super) fn on_move_line_up(
        &mut self,
        _: &MoveLineUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_lines(window, cx, |state, _| {
            line_edits::move_line(state.text(), state.cursor_position(), true)
        });
    }

    pub(super) fn on_move_line_down(
        &mut self,
        _: &MoveLineDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_lines(window, cx, |state, _| {
            line_edits::move_line(state.text(), state.cursor_position(), false)
        });
    }

    pub(super) fn on_toggle_line_comment(
        &mut self,
        _: &ToggleLineComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefix = self
            .shown_editor()
            .and_then(|(path, _)| self.language_of(&path))
            .and_then(|language| language.line_comment);
        self.edit_lines(window, cx, move |state, _| {
            let prefix = prefix.as_deref()?;
            Some(line_edits::toggle_comment(
                state.text(),
                state.cursor_position(),
                prefix,
            ))
        });
    }

    fn jump_to_problem(&self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, state)) = self.shown_editor() else {
            return;
        };
        let here = state.read(cx).cursor_position();
        let starts: Vec<Position> = self
            .language_servers
            .diagnostics
            .of(&self.active_document(&path))
            .into_iter()
            .map(|problem| problem.range.start)
            .collect();
        let key = |at: &Position| (at.line, at.character);
        let target = if forward {
            starts
                .iter()
                .find(|at| key(at) > key(&here))
                .or_else(|| starts.first())
        } else {
            starts
                .iter()
                .rev()
                .find(|at| key(at) < key(&here))
                .or_else(|| starts.last())
        };
        if let Some(target) = target.copied() {
            state.update(cx, |state, cx| {
                state.set_cursor_position(target, window, cx);
            });
        }
    }

    pub(super) fn on_next_problem(
        &mut self,
        _: &NextProblem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_problem(true, window, cx);
    }

    pub(super) fn on_previous_problem(
        &mut self,
        _: &PreviousProblem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_problem(false, window, cx);
    }
}
