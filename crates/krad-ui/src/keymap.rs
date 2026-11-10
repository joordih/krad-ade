use gpui_kit::component::input::{
    GoToDefinition, Redo, Replace, ShowCompletions, ToggleCodeActions,
};
use gpui_kit::KeyBinding;

use crate::actions::{
    AcceptCompletion, AskConfirm, AskDeny, CloseAllTabs, CloseTab, CopySelection, DeleteLine,
    DuplicateLine, FindInFiles, FindReferences, MoveLineDown, MoveLineUp, NavigateBack,
    NavigateForward, NewAgent, NewTerminal, NewTreeFile, NextProblem, NextWaitingAgent,
    OpenSettings, PasteClipboard, PreviousProblem, QuickOpen, Reformat, RenameFile, RenameSymbol,
    SaveFile, SearchTerminal, SendBackTab, SendTab, ShowTabAt, SplitTerminal, TextLarger,
    TextReset, TextSmaller, ToggleActivity, ToggleAttentionInbox, ToggleFps, ToggleLineComment,
    TogglePalette, ToggleSidePanel, ToggleSidebar, ToggleWorkMode, ASK_KEY_CONTEXT,
    EDITOR_KEY_CONTEXT, FILE_TAB_KEY_CONTEXT, FILE_TREE_KEY_CONTEXT, SHELL_KEY_CONTEXT,
    VIEWER_KEY_CONTEXT,
};
#[cfg(target_os = "macos")]
use crate::actions::{HideApp, HideOtherApps, MinimizeWindow, Quit};

struct KeySet {
    toggle_fps: &'static str,
    palette: &'static str,
    open_settings_hint: &'static str,
    new_terminal: &'static str,
    new_terminal_hint: &'static str,
    navigate_back: &'static str,
    navigate_forward: &'static str,
    tab_slots: [&'static str; 9],
    tab_slot_hints: [&'static str; 9],
    new_tree_file: &'static str,
    split_terminal: &'static str,
    close_tab: &'static str,
    close_tab_hint: &'static str,
    close_all_tabs: &'static str,
    close_all_tabs_hint: &'static str,
    copy: &'static str,
    viewer_copy: &'static str,
    copy_hint: &'static str,
    paste: &'static str,
    paste_hint: &'static str,
    commit_hint: &'static str,
    search: &'static str,
    toggle_activity: &'static str,
    toggle_sidebar: &'static str,
    toggle_side_panel: &'static str,
    toggle_work_mode: &'static str,
    quick_open: &'static str,
    reformat: &'static str,
    find_in_files: &'static str,
    go_to_definition: &'static str,
    replace: &'static str,
    duplicate_line: &'static str,
    delete_line: &'static str,
    toggle_line_comment: &'static str,
    redo: &'static str,
    save_file: &'static str,
    open_settings: &'static str,
    text_larger: &'static str,
    text_smaller: &'static str,
    text_reset: &'static str,
    new_agent: &'static str,
    next_waiting_agent: &'static str,
    attention_inbox: &'static str,
    next_waiting_agent_hint: &'static str,
    attention_inbox_hint: &'static str,
}

#[cfg(any(target_os = "macos", test))]
const MAC_KEYS: KeySet = KeySet {
    toggle_fps: "cmd-alt-shift-f",
    palette: "cmd-k",
    open_settings_hint: "⌘,",
    new_terminal: "cmd-t",
    new_terminal_hint: "⌘T",
    navigate_back: "ctrl--",
    navigate_forward: "ctrl-shift--",
    tab_slots: [
        "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
    ],
    tab_slot_hints: ["⌘1", "⌘2", "⌘3", "⌘4", "⌘5", "⌘6", "⌘7", "⌘8", "⌘9"],
    new_tree_file: "cmd-n",
    split_terminal: "cmd-d",
    close_tab: "cmd-w",
    close_tab_hint: "⌘W",
    close_all_tabs: "cmd-shift-w",
    close_all_tabs_hint: "⌘⇧W",
    copy: "cmd-c",
    viewer_copy: "cmd-c",
    copy_hint: "⌘C",
    paste: "cmd-v",
    paste_hint: "⌘V",
    commit_hint: "⌘↩",
    search: "cmd-f",
    toggle_activity: "cmd-shift-e",
    toggle_sidebar: "cmd-ctrl-s",
    toggle_side_panel: "cmd-alt-b",
    toggle_work_mode: "cmd-alt-e",
    quick_open: "cmd-shift-o",
    reformat: "cmd-alt-l",
    find_in_files: "cmd-shift-f",
    go_to_definition: "cmd-b",
    replace: "cmd-r",
    duplicate_line: "cmd-d",
    delete_line: "cmd-backspace",
    toggle_line_comment: "cmd-/",
    redo: "cmd-shift-z",
    save_file: "cmd-s",
    open_settings: "cmd-,",
    text_larger: "cmd-alt-=",
    text_smaller: "cmd-alt--",
    text_reset: "cmd-alt-0",
    new_agent: "cmd-alt-n",
    next_waiting_agent: "cmd-shift-j",
    attention_inbox: "cmd-alt-a",
    next_waiting_agent_hint: "⌘⇧J",
    attention_inbox_hint: "⌥⌘A",
};

#[cfg(any(not(target_os = "macos"), test))]
const OTHER_KEYS: KeySet = KeySet {
    toggle_fps: "ctrl-alt-shift-f",
    palette: "ctrl-shift-p",
    open_settings_hint: "Ctrl+,",
    new_terminal: "ctrl-shift-t",
    new_terminal_hint: "Ctrl+Shift+T",
    navigate_back: "alt-left",
    navigate_forward: "alt-right",
    tab_slots: [
        "alt-1", "alt-2", "alt-3", "alt-4", "alt-5", "alt-6", "alt-7", "alt-8", "alt-9",
    ],
    tab_slot_hints: [
        "Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9",
    ],
    new_tree_file: "ctrl-shift-n",
    split_terminal: "ctrl-shift-d",
    close_tab: "ctrl-shift-w",
    close_tab_hint: "Ctrl+Shift+W",
    close_all_tabs: "ctrl-alt-shift-w",
    close_all_tabs_hint: "Ctrl+Alt+Shift+W",
    copy: "ctrl-shift-c",
    viewer_copy: "ctrl-c",
    copy_hint: "Ctrl+Shift+C",
    paste: "ctrl-shift-v",
    paste_hint: "Ctrl+Shift+V",
    commit_hint: "Ctrl+Enter",
    search: "ctrl-shift-f",
    toggle_activity: "ctrl-shift-e",
    toggle_sidebar: "ctrl-shift-b",
    toggle_side_panel: "ctrl-alt-b",
    toggle_work_mode: "ctrl-alt-e",
    quick_open: "ctrl-shift-n",
    reformat: "ctrl-alt-l",
    find_in_files: "ctrl-alt-f",
    go_to_definition: "ctrl-b",
    replace: "ctrl-r",
    duplicate_line: "ctrl-d",
    delete_line: "ctrl-y",
    toggle_line_comment: "ctrl-/",
    redo: "ctrl-shift-z",
    save_file: "ctrl-s",
    open_settings: "ctrl-,",
    text_larger: "ctrl-alt-=",
    text_smaller: "ctrl-alt--",
    text_reset: "ctrl-alt-0",
    new_agent: "ctrl-alt-n",
    next_waiting_agent: "ctrl-shift-j",
    attention_inbox: "ctrl-alt-a",
    next_waiting_agent_hint: "Ctrl+Shift+J",
    attention_inbox_hint: "Ctrl+Alt+A",
};

#[cfg(target_os = "macos")]
const KEYS: KeySet = MAC_KEYS;
#[cfg(not(target_os = "macos"))]
const KEYS: KeySet = OTHER_KEYS;

#[cfg(target_os = "macos")]
mod app_keys {
    pub(super) const QUIT: &str = "cmd-q";
    pub(super) const HIDE_APP: &str = "cmd-h";
    pub(super) const HIDE_OTHER_APPS: &str = "cmd-alt-h";
    pub(super) const MINIMIZE_WINDOW: &str = "cmd-m";
}

#[cfg(target_os = "macos")]
fn app_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(app_keys::QUIT, Quit, None),
        KeyBinding::new(app_keys::HIDE_APP, HideApp, None),
        KeyBinding::new(app_keys::HIDE_OTHER_APPS, HideOtherApps, None),
        KeyBinding::new(app_keys::MINIMIZE_WINDOW, MinimizeWindow, None),
    ]
}

#[cfg(not(target_os = "macos"))]
fn app_bindings() -> Vec<KeyBinding> {
    Vec::new()
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

pub(crate) const OPEN_SETTINGS_HINT: &str = KEYS.open_settings_hint;
pub(crate) const NEW_TERMINAL_HINT: &str = KEYS.new_terminal_hint;
pub(crate) const TAB_SLOT_HINTS: [&str; 9] = KEYS.tab_slot_hints;
pub(crate) const COPY_HINT: &str = KEYS.copy_hint;
pub(crate) const PASTE_HINT: &str = KEYS.paste_hint;
pub(crate) const CLOSE_TAB_HINT: &str = KEYS.close_tab_hint;
pub(crate) const CLOSE_ALL_TABS_HINT: &str = KEYS.close_all_tabs_hint;
pub(crate) const COMMIT_HINT: &str = KEYS.commit_hint;
pub(crate) const NEXT_WAITING_AGENT_HINT: &str = KEYS.next_waiting_agent_hint;
pub(crate) const ATTENTION_INBOX_HINT: &str = KEYS.attention_inbox_hint;

#[must_use]
pub fn key_bindings() -> Vec<KeyBinding> {
    let mut bindings = app_bindings();
    bindings.extend(bindings_for(&KEYS));
    bindings
}

fn bindings_for(keys: &KeySet) -> Vec<KeyBinding> {
    let scope = Some(SHELL_KEY_CONTEXT);
    let mut bindings = Vec::new();
    bindings.extend([
        KeyBinding::new(keys.toggle_fps, ToggleFps, scope),
        KeyBinding::new(keys.palette, TogglePalette, scope),
        KeyBinding::new(keys.new_terminal, NewTerminal, scope),
        KeyBinding::new(keys.navigate_back, NavigateBack, scope),
        KeyBinding::new(keys.navigate_forward, NavigateForward, scope),
    ]);
    bindings.extend(
        keys.tab_slots
            .iter()
            .enumerate()
            .map(|(slot, key)| KeyBinding::new(key, ShowTabAt { slot }, scope)),
    );
    bindings.extend([
        KeyBinding::new(keys.split_terminal, SplitTerminal, scope),
        KeyBinding::new(keys.close_tab, CloseTab, scope),
        KeyBinding::new(keys.close_all_tabs, CloseAllTabs, scope),
        KeyBinding::new(keys.copy, CopySelection, scope),
        KeyBinding::new(keys.paste, PasteClipboard, scope),
        KeyBinding::new(keys.search, SearchTerminal, scope),
        KeyBinding::new(keys.toggle_activity, ToggleActivity, scope),
        KeyBinding::new(keys.toggle_sidebar, ToggleSidebar, scope),
        KeyBinding::new(keys.toggle_side_panel, ToggleSidePanel, scope),
        KeyBinding::new(keys.toggle_work_mode, ToggleWorkMode, scope),
        KeyBinding::new(keys.quick_open, QuickOpen, scope),
        KeyBinding::new(keys.open_settings, OpenSettings, scope),
        KeyBinding::new(keys.text_larger, TextLarger, scope),
        KeyBinding::new(keys.text_smaller, TextSmaller, scope),
        KeyBinding::new(keys.text_reset, TextReset, scope),
        KeyBinding::new(keys.new_agent, NewAgent, scope),
        KeyBinding::new(keys.next_waiting_agent, NextWaitingAgent, scope),
        KeyBinding::new(keys.attention_inbox, ToggleAttentionInbox, scope),
        KeyBinding::new(SEND_TAB, SendTab, scope),
        KeyBinding::new(SEND_BACK_TAB, SendBackTab, scope),
        KeyBinding::new(keys.viewer_copy, CopySelection, Some(VIEWER_KEY_CONTEXT)),
        KeyBinding::new(RENAME_FILE, RenameFile, Some(FILE_TREE_KEY_CONTEXT)),
        KeyBinding::new(keys.new_tree_file, NewTreeFile, Some(FILE_TREE_KEY_CONTEXT)),
        KeyBinding::new(keys.save_file, SaveFile, Some(FILE_TAB_KEY_CONTEXT)),
        KeyBinding::new(ASK_CONFIRM, AskConfirm, Some(ASK_KEY_CONTEXT)),
        KeyBinding::new(ASK_DENY, AskDeny, Some(ASK_KEY_CONTEXT)),
    ]);
    bindings.extend(editor_bindings(keys));
    bindings
}

fn editor_bindings(keys: &KeySet) -> Vec<KeyBinding> {
    let editor = Some(EDITOR_KEY_CONTEXT);
    vec![
        KeyBinding::new(ACCEPT_COMPLETION, AcceptCompletion, editor),
        KeyBinding::new(SHOW_COMPLETIONS, ShowCompletions, editor),
        KeyBinding::new(keys.go_to_definition, GoToDefinition, editor),
        KeyBinding::new(FIND_REFERENCES, FindReferences, editor),
        KeyBinding::new(CODE_ACTIONS, ToggleCodeActions, editor),
        KeyBinding::new(keys.replace, Replace, editor),
        KeyBinding::new(keys.redo, Redo, editor),
        KeyBinding::new(keys.duplicate_line, DuplicateLine, editor),
        KeyBinding::new(keys.delete_line, DeleteLine, editor),
        KeyBinding::new(MOVE_LINE_UP, MoveLineUp, editor),
        KeyBinding::new(MOVE_LINE_DOWN, MoveLineDown, editor),
        KeyBinding::new(keys.toggle_line_comment, ToggleLineComment, editor),
        KeyBinding::new(NEXT_PROBLEM, NextProblem, editor),
        KeyBinding::new(PREVIOUS_PROBLEM, PreviousProblem, editor),
        KeyBinding::new(RENAME_SYMBOL, RenameSymbol, editor),
        KeyBinding::new(keys.reformat, Reformat, editor),
        KeyBinding::new(keys.find_in_files, FindInFiles, editor),
        KeyBinding::new(keys.find_in_files, FindInFiles, Some(SHELL_KEY_CONTEXT)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn clashes(keys: &KeySet) -> Vec<String> {
        let mut seen: HashMap<(String, String), usize> = HashMap::new();
        for binding in bindings_for(keys) {
            let chord = binding
                .keystrokes()
                .iter()
                .map(gpui_kit::KeybindingKeystroke::unparse)
                .collect::<Vec<_>>()
                .join(" ");
            let context = binding
                .predicate()
                .map_or_else(String::new, |predicate| predicate.to_string());
            *seen.entry((chord, context)).or_default() += 1;
        }
        let mut clashes: Vec<String> = seen
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|((chord, context), _)| format!("{chord} in {context}"))
            .collect();
        clashes.sort();
        clashes
    }

    #[test]
    fn no_chord_is_bound_twice_within_one_context_on_either_platform() {
        assert_eq!(clashes(&MAC_KEYS), Vec::<String>::new(), "macOS");
        assert_eq!(
            clashes(&OTHER_KEYS),
            Vec::<String>::new(),
            "Windows and Linux"
        );
    }
}
