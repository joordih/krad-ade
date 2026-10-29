use gpui_kit::KeyBinding;

use crate::actions::{
    AskConfirm, AskDeny, CopySelection, HideApp, HideOtherApps, MinimizeWindow, NewAgent,
    NewTerminal, OpenSettings, PasteClipboard, Quit, RenameFile, SaveFile, SearchTerminal,
    SendBackTab, SendTab, SplitTerminal, TextLarger, TextReset, TextSmaller, ToggleActivity,
    ToggleFps, TogglePalette, ToggleSidePanel, ToggleSidebar, ASK_KEY_CONTEXT,
    FILE_TAB_KEY_CONTEXT, FILE_TREE_KEY_CONTEXT, SHELL_KEY_CONTEXT, VIEWER_KEY_CONTEXT,
};

#[cfg(target_os = "macos")]
mod keys {
    pub(super) const TOGGLE_FPS: &str = "cmd-alt-shift-f";
    pub(super) const PALETTE: &str = "cmd-k";
    pub(super) const OPEN_SETTINGS_HINT: &str = "⌘,";
    pub(super) const NEW_TERMINAL: &str = "cmd-t";
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
const ASK_CONFIRM: &str = "enter";
const ASK_DENY: &str = "escape";

pub(crate) const OPEN_SETTINGS_HINT: &str = keys::OPEN_SETTINGS_HINT;
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
        KeyBinding::new(keys::SPLIT_TERMINAL, SplitTerminal, scope),
        KeyBinding::new(keys::COPY, CopySelection, scope),
        KeyBinding::new(keys::PASTE, PasteClipboard, scope),
        KeyBinding::new(keys::SEARCH, SearchTerminal, scope),
        KeyBinding::new(keys::TOGGLE_ACTIVITY, ToggleActivity, scope),
        KeyBinding::new(keys::TOGGLE_SIDEBAR, ToggleSidebar, scope),
        KeyBinding::new(keys::TOGGLE_SIDE_PANEL, ToggleSidePanel, scope),
        KeyBinding::new(keys::OPEN_SETTINGS, OpenSettings, scope),
        KeyBinding::new(keys::TEXT_LARGER, TextLarger, scope),
        KeyBinding::new(keys::TEXT_SMALLER, TextSmaller, scope),
        KeyBinding::new(keys::TEXT_RESET, TextReset, scope),
        KeyBinding::new(keys::NEW_AGENT, NewAgent, scope),
        KeyBinding::new(SEND_TAB, SendTab, scope),
        KeyBinding::new(SEND_BACK_TAB, SendBackTab, scope),
        KeyBinding::new(keys::VIEWER_COPY, CopySelection, Some(VIEWER_KEY_CONTEXT)),
        KeyBinding::new(RENAME_FILE, RenameFile, Some(FILE_TREE_KEY_CONTEXT)),
        KeyBinding::new(keys::SAVE_FILE, SaveFile, Some(FILE_TAB_KEY_CONTEXT)),
        KeyBinding::new(ASK_CONFIRM, AskConfirm, Some(ASK_KEY_CONTEXT)),
        KeyBinding::new(ASK_DENY, AskDeny, Some(ASK_KEY_CONTEXT)),
    ]);
    bindings
}
