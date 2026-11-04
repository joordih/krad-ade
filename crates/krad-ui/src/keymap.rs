use gpui_kit::component::input::{
    GoToDefinition, Redo, Replace, ShowCompletions, ToggleCodeActions,
};
use gpui_kit::KeyBinding;

use crate::actions::{
    AcceptCompletion, AskConfirm, AskDeny, CopySelection, DeleteLine, DuplicateLine, FindInFiles,
    FindReferences, MoveLineDown, MoveLineUp, NewAgent, NewFromTemplate, NewTerminal, NextProblem,
    OpenSettings, PasteClipboard, PreviousProblem, QuickOpen, Reformat, RenameFile, RenameSymbol,
    SaveFile, SearchTerminal, SendBackTab, SendTab, ShowTabAt, SplitTerminal, TextLarger,
    TextReset, TextSmaller, ToggleActivity, ToggleFps, ToggleLineComment, TogglePalette,
    ToggleSidePanel, ToggleSidebar, ToggleWorkMode, ASK_KEY_CONTEXT, EDITOR_KEY_CONTEXT,
    FILE_TAB_KEY_CONTEXT, FILE_TREE_KEY_CONTEXT, SHELL_KEY_CONTEXT, VIEWER_KEY_CONTEXT,
};
#[cfg(target_os = "macos")]
use crate::actions::{HideApp, HideOtherApps, MinimizeWindow, Quit};

#[cfg(target_os = "macos")]
mod keys {
    pub(super) const TOGGLE_FPS: &str = "cmd-alt-shift-f";
    pub(super) const PALETTE: &str = "cmd-k";
    pub(super) const OPEN_SETTINGS_HINT: &str = "⌘,";
    pub(super) const NEW_TERMINAL: &str = "cmd-t";
    pub(super) const NEW_TERMINAL_HINT: &str = "⌘T";
    pub(super) const TAB_SLOTS: [&str; 9] = [
        "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
    ];
    pub(super) const TAB_SLOT_HINTS: [&str; 9] =
        ["⌘1", "⌘2", "⌘3", "⌘4", "⌘5", "⌘6", "⌘7", "⌘8", "⌘9"];
    pub(super) const NEW_FROM_TEMPLATE: &str = "cmd-n";
    pub(super) const SPLIT_TERMINAL: &str = "cmd-d";
    pub(super) const COPY: &str = "cmd-c";
    pub(super) const VIEWER_COPY: &str = "cmd-c";
    pub(super) const COPY_HINT: &str = "⌘C";
    pub(super) const PASTE: &str = "cmd-v";
    pub(super) const PASTE_HINT: &str = "⌘V";
    pub(super) const SEARCH: &str = "cmd-f";
    pub(super) const TOGGLE_ACTIVITY: &str = "cmd-shift-e";
    pub(super) const TOGGLE_SIDEBAR: &str = "cmd-ctrl-s";
    pub(super) const TOGGLE_SIDE_PANEL: &str = "cmd-alt-b";
    pub(super) const TOGGLE_WORK_MODE: &str = "cmd-alt-e";
    pub(super) const QUICK_OPEN: &str = "cmd-shift-o";
    pub(super) const REFORMAT: &str = "cmd-alt-l";
    pub(super) const FIND_IN_FILES: &str = "cmd-shift-f";
    pub(super) const GO_TO_DEFINITION: &str = "cmd-b";
    pub(super) const REPLACE: &str = "cmd-r";
    pub(super) const DUPLICATE_LINE: &str = "cmd-d";
    pub(super) const DELETE_LINE: &str = "cmd-backspace";
    pub(super) const TOGGLE_LINE_COMMENT: &str = "cmd-/";
    pub(super) const REDO: &str = "cmd-shift-z";
    pub(super) const SAVE_FILE: &str = "cmd-s";
    pub(super) const OPEN_SETTINGS: &str = "cmd-,";
    pub(super) const TEXT_LARGER: &str = "cmd-alt-=";
    pub(super) const TEXT_SMALLER: &str = "cmd-alt--";
    pub(super) const TEXT_RESET: &str = "cmd-alt-0";
    pub(super) const NEW_AGENT: &str = "cmd-alt-n";
    pub(super) const QUIT: &str = "cmd-q";
    pub(super) const HIDE_APP: &str = "cmd-h";
    pub(super) const HIDE_OTHER_APPS: &str = "cmd-alt-h";
    pub(super) const MINIMIZE_WINDOW: &str = "cmd-m";
}

#[cfg(target_os = "macos")]
fn app_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(keys::QUIT, Quit, None),
        KeyBinding::new(keys::HIDE_APP, HideApp, None),
        KeyBinding::new(keys::HIDE_OTHER_APPS, HideOtherApps, None),
        KeyBinding::new(keys::MINIMIZE_WINDOW, MinimizeWindow, None),
    ]
}

#[cfg(not(target_os = "macos"))]
fn app_bindings() -> Vec<KeyBinding> {
    Vec::new()
}

#[cfg(not(target_os = "macos"))]
mod keys {
    pub(super) const TOGGLE_FPS: &str = "ctrl-alt-shift-f";
    pub(super) const PALETTE: &str = "ctrl-shift-p";
    pub(super) const OPEN_SETTINGS_HINT: &str = "Ctrl+,";
    pub(super) const NEW_TERMINAL: &str = "ctrl-shift-t";
    pub(super) const NEW_TERMINAL_HINT: &str = "Ctrl+Shift+T";
    pub(super) const TAB_SLOTS: [&str; 9] = [
        "alt-1", "alt-2", "alt-3", "alt-4", "alt-5", "alt-6", "alt-7", "alt-8", "alt-9",
    ];
    pub(super) const TAB_SLOT_HINTS: [&str; 9] = [
        "Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9",
    ];
    pub(super) const NEW_FROM_TEMPLATE: &str = "ctrl-shift-n";
    pub(super) const SPLIT_TERMINAL: &str = "ctrl-shift-d";
    pub(super) const COPY: &str = "ctrl-shift-c";
    pub(super) const VIEWER_COPY: &str = "ctrl-c";
    pub(super) const COPY_HINT: &str = "Ctrl+Shift+C";
    pub(super) const PASTE: &str = "ctrl-shift-v";
    pub(super) const PASTE_HINT: &str = "Ctrl+Shift+V";
    pub(super) const SEARCH: &str = "ctrl-shift-f";
    pub(super) const TOGGLE_ACTIVITY: &str = "ctrl-shift-e";
    pub(super) const TOGGLE_SIDEBAR: &str = "ctrl-shift-b";
    pub(super) const TOGGLE_SIDE_PANEL: &str = "ctrl-alt-b";
    pub(super) const TOGGLE_WORK_MODE: &str = "ctrl-alt-e";
    pub(super) const QUICK_OPEN: &str = "ctrl-shift-n";
    pub(super) const REFORMAT: &str = "ctrl-alt-l";
    pub(super) const FIND_IN_FILES: &str = "ctrl-shift-f";
    pub(super) const GO_TO_DEFINITION: &str = "ctrl-b";
    pub(super) const REPLACE: &str = "ctrl-r";
    pub(super) const DUPLICATE_LINE: &str = "ctrl-d";
    pub(super) const DELETE_LINE: &str = "ctrl-y";
    pub(super) const TOGGLE_LINE_COMMENT: &str = "ctrl-/";
    pub(super) const REDO: &str = "ctrl-shift-z";
    pub(super) const SAVE_FILE: &str = "ctrl-s";
    pub(super) const OPEN_SETTINGS: &str = "ctrl-,";
    pub(super) const TEXT_LARGER: &str = "ctrl-alt-=";
    pub(super) const TEXT_SMALLER: &str = "ctrl-alt--";
    pub(super) const TEXT_RESET: &str = "ctrl-alt-0";
    pub(super) const NEW_AGENT: &str = "ctrl-alt-n";
}

const SEND_TAB: &str = "tab";
const SEND_BACK_TAB: &str = "shift-tab";
const RENAME_FILE: &str = "f2";
const FIND_REFERENCES: &str = "alt-f7";
const ACCEPT_COMPLETION: &str = "tab";
const SHOW_COMPLETIONS: &str = "ctrl-space";
const CODE_ACTIONS: &str = "alt-enter";
const MOVE_LINE_UP: &str = "alt-shift-up";
const MOVE_LINE_DOWN: &str = "alt-shift-down";
const NEXT_PROBLEM: &str = "f2";
const PREVIOUS_PROBLEM: &str = "shift-f2";
const RENAME_SYMBOL: &str = "shift-f6";
const ASK_CONFIRM: &str = "enter";
const ASK_DENY: &str = "escape";

pub(crate) const OPEN_SETTINGS_HINT: &str = keys::OPEN_SETTINGS_HINT;
pub(crate) const NEW_TERMINAL_HINT: &str = keys::NEW_TERMINAL_HINT;
pub(crate) const TAB_SLOT_HINTS: [&str; 9] = keys::TAB_SLOT_HINTS;
pub(crate) const COPY_HINT: &str = keys::COPY_HINT;
pub(crate) const PASTE_HINT: &str = keys::PASTE_HINT;

#[must_use]
pub fn key_bindings() -> Vec<KeyBinding> {
    let scope = Some(SHELL_KEY_CONTEXT);
    let mut bindings = app_bindings();
    bindings.extend([
        KeyBinding::new(keys::TOGGLE_FPS, ToggleFps, scope),
        KeyBinding::new(keys::PALETTE, TogglePalette, scope),
        KeyBinding::new(keys::NEW_TERMINAL, NewTerminal, scope),
    ]);
    bindings.extend(
        keys::TAB_SLOTS
            .iter()
            .enumerate()
            .map(|(slot, key)| KeyBinding::new(key, ShowTabAt { slot }, scope)),
    );
    bindings.extend([
        KeyBinding::new(keys::SPLIT_TERMINAL, SplitTerminal, scope),
        KeyBinding::new(keys::COPY, CopySelection, scope),
        KeyBinding::new(keys::PASTE, PasteClipboard, scope),
        KeyBinding::new(keys::SEARCH, SearchTerminal, scope),
        KeyBinding::new(keys::TOGGLE_ACTIVITY, ToggleActivity, scope),
        KeyBinding::new(keys::TOGGLE_SIDEBAR, ToggleSidebar, scope),
        KeyBinding::new(keys::TOGGLE_SIDE_PANEL, ToggleSidePanel, scope),
        KeyBinding::new(keys::TOGGLE_WORK_MODE, ToggleWorkMode, scope),
        KeyBinding::new(keys::QUICK_OPEN, QuickOpen, scope),
        KeyBinding::new(keys::OPEN_SETTINGS, OpenSettings, scope),
        KeyBinding::new(keys::TEXT_LARGER, TextLarger, scope),
        KeyBinding::new(keys::TEXT_SMALLER, TextSmaller, scope),
        KeyBinding::new(keys::TEXT_RESET, TextReset, scope),
        KeyBinding::new(keys::NEW_AGENT, NewAgent, scope),
        KeyBinding::new(SEND_TAB, SendTab, scope),
        KeyBinding::new(SEND_BACK_TAB, SendBackTab, scope),
        KeyBinding::new(keys::VIEWER_COPY, CopySelection, Some(VIEWER_KEY_CONTEXT)),
        KeyBinding::new(RENAME_FILE, RenameFile, Some(FILE_TREE_KEY_CONTEXT)),
        KeyBinding::new(
            keys::NEW_FROM_TEMPLATE,
            NewFromTemplate,
            Some(FILE_TREE_KEY_CONTEXT),
        ),
        KeyBinding::new(keys::SAVE_FILE, SaveFile, Some(FILE_TAB_KEY_CONTEXT)),
        KeyBinding::new(ASK_CONFIRM, AskConfirm, Some(ASK_KEY_CONTEXT)),
        KeyBinding::new(ASK_DENY, AskDeny, Some(ASK_KEY_CONTEXT)),
    ]);
    bindings.extend(editor_bindings());
    bindings
}

fn editor_bindings() -> Vec<KeyBinding> {
    let editor = Some(EDITOR_KEY_CONTEXT);
    vec![
        KeyBinding::new(ACCEPT_COMPLETION, AcceptCompletion, editor),
        KeyBinding::new(SHOW_COMPLETIONS, ShowCompletions, editor),
        KeyBinding::new(keys::GO_TO_DEFINITION, GoToDefinition, editor),
        KeyBinding::new(FIND_REFERENCES, FindReferences, editor),
        KeyBinding::new(CODE_ACTIONS, ToggleCodeActions, editor),
        KeyBinding::new(keys::REPLACE, Replace, editor),
        KeyBinding::new(keys::REDO, Redo, editor),
        KeyBinding::new(keys::DUPLICATE_LINE, DuplicateLine, editor),
        KeyBinding::new(keys::DELETE_LINE, DeleteLine, editor),
        KeyBinding::new(MOVE_LINE_UP, MoveLineUp, editor),
        KeyBinding::new(MOVE_LINE_DOWN, MoveLineDown, editor),
        KeyBinding::new(keys::TOGGLE_LINE_COMMENT, ToggleLineComment, editor),
        KeyBinding::new(NEXT_PROBLEM, NextProblem, editor),
        KeyBinding::new(PREVIOUS_PROBLEM, PreviousProblem, editor),
        KeyBinding::new(RENAME_SYMBOL, RenameSymbol, editor),
        KeyBinding::new(keys::REFORMAT, Reformat, editor),
        KeyBinding::new(keys::FIND_IN_FILES, FindInFiles, editor),
        KeyBinding::new(keys::FIND_IN_FILES, FindInFiles, Some(SHELL_KEY_CONTEXT)),
    ]
}
