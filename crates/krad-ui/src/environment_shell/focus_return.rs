use gpui_kit::{Context, FocusHandle, Window};

use super::host::HostDialog;
use super::name_dialogs::NameDialogs;
use super::window_close::CloseRequest;
use super::EnvironmentShell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FocusKeeper<T> {
    open: bool,
    outside: Option<T>,
    previous: Option<T>,
    inside: Option<T>,
}

impl<T> Default for FocusKeeper<T> {
    fn default() -> Self {
        Self {
            open: false,
            outside: None,
            previous: None,
            inside: None,
        }
    }
}

impl<T: Clone + PartialEq> FocusKeeper<T> {
    #[must_use]
    pub(super) fn follow(&mut self, open: bool, focused: Option<T>) -> Option<T> {
        match (self.open, open) {
            (false, false) => {
                self.outside = focused;
                None
            }
            (false, true) => {
                self.open = true;
                self.previous = self.outside.take();
                self.inside = focused;
                None
            }
            (true, true) => {
                if focused.is_some() {
                    self.inside = focused;
                }
                None
            }
            (true, false) => {
                self.open = false;
                let previous = self.previous.take();
                let inside = self.inside.take();
                let lost = focused.is_none() || focused == inside;
                self.outside = focused;
                previous.filter(|_| lost)
            }
        }
    }
}

#[derive(Default)]
pub(super) struct FocusReturns {
    dialogs: FocusKeeper<FocusHandle>,
    model_picker: FocusKeeper<FocusHandle>,
}

impl EnvironmentShell {
    fn dialog_open(&self) -> bool {
        let lead_ending = self
            .envs
            .get(&self.active)
            .is_some_and(|live| live.pending_lead_ending.is_some());
        self.agent_composer.is_some()
            || self.template_picker.is_some()
            || self.variants.sheet_shown()
            || self.docs_dialog.is_some()
            || self.file_tree.deleting()
            || self.session_changes.sheet_shown()
            || self.pending_discard.is_some()
            || lead_ending
            || self.kraddies.editor.is_some()
            || self.kraddies.studio.is_some()
            || !matches!(self.name_dialogs, NameDialogs::Closed)
            || self.extensions.trusting()
            || !matches!(self.hosts.dialog, HostDialog::Closed)
            || self.toolchains.entry_shown()
            || self.harness_checks.onboarding_shown()
            || matches!(self.close_request, CloseRequest::Confirming(_))
    }

    pub(super) fn sync_focus_returns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = window.focused(cx);
        let dialog_open = self.dialog_open();
        let picker_open = self.model_picker.is_some();
        let returns = &mut self.focus_returns;
        let back = [
            returns.dialogs.follow(dialog_open, focused.clone()),
            returns.model_picker.follow(picker_open, focused),
        ];
        if let Some(previous) = back.into_iter().flatten().next() {
            window.focus(&previous, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDITOR: u8 = 1;
    const DIALOG_FIELD: u8 = 2;
    const TERMINAL: u8 = 3;

    #[test]
    fn closing_gives_focus_back_to_what_had_it_before_opening() {
        let mut keeper = FocusKeeper::default();
        assert_eq!(keeper.follow(false, Some(EDITOR)), None);
        assert_eq!(keeper.follow(true, Some(DIALOG_FIELD)), None);
        assert_eq!(keeper.follow(true, Some(DIALOG_FIELD)), None);
        assert_eq!(keeper.follow(false, None), Some(EDITOR));
    }

    #[test]
    fn focus_still_inside_the_closed_surface_is_given_back_too() {
        let mut keeper = FocusKeeper::default();
        let _ = keeper.follow(false, Some(EDITOR));
        let _ = keeper.follow(true, Some(DIALOG_FIELD));
        assert_eq!(keeper.follow(false, Some(DIALOG_FIELD)), Some(EDITOR));
    }

    #[test]
    fn focus_moved_elsewhere_by_the_closing_action_is_kept() {
        let mut keeper = FocusKeeper::default();
        let _ = keeper.follow(false, Some(EDITOR));
        let _ = keeper.follow(true, Some(DIALOG_FIELD));
        assert_eq!(keeper.follow(false, Some(TERMINAL)), None);
    }

    #[test]
    fn nothing_is_given_back_when_nothing_had_focus_before() {
        let mut keeper = FocusKeeper::default();
        let _ = keeper.follow(false, None);
        let _ = keeper.follow(true, Some(DIALOG_FIELD));
        assert_eq!(keeper.follow(false, None), None);
    }

    #[test]
    fn a_second_round_starts_from_the_focus_left_after_the_first() {
        let mut keeper = FocusKeeper::default();
        let _ = keeper.follow(false, Some(EDITOR));
        let _ = keeper.follow(true, Some(DIALOG_FIELD));
        let _ = keeper.follow(false, Some(TERMINAL));
        let _ = keeper.follow(true, Some(DIALOG_FIELD));
        assert_eq!(keeper.follow(false, None), Some(TERMINAL));
    }
}
