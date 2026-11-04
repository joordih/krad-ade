use gpui_kit::{actions, Action};

actions!(
    krad,
    [
        ToggleFps,
        TogglePalette,
        NewTerminal,
        SplitTerminal,
        CloseWindow,
        CopySelection,
        PasteClipboard,
        SearchTerminal,
        ToggleActivity,
        ToggleSidebar,
        ToggleSidePanel,
        ToggleMinimap,
        ToggleWorkMode,
        QuickOpen,
        FindReferences,
        AcceptCompletion,
        DuplicateLine,
        DeleteLine,
        MoveLineUp,
        MoveLineDown,
        ToggleLineComment,
        NextProblem,
        PreviousProblem,
        RenameSymbol,
        Reformat,
        FindInFiles,
        OpenSettings,
        TextLarger,
        TextSmaller,
        TextReset,
        NewAgent,
        NavigateBack,
        NavigateForward,
        SendTab,
        SendBackTab,
        RenameFile,
        NewTreeFile,
        SaveFile,
        AskConfirm,
        AskDeny,
        Quit,
        HideApp,
        HideOtherApps,
        ShowAllApps,
        MinimizeWindow,
        ZoomWindow
    ]
);

#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = krad, no_json)]
pub(crate) struct ShowTabAt {
    pub(crate) slot: usize,
}

pub(crate) const SHELL_KEY_CONTEXT: &str = "KradShell";

pub(crate) const VIEWER_KEY_CONTEXT: &str = "KradCodeViewer";

pub(crate) const FILE_TREE_KEY_CONTEXT: &str = "KradFileTree";

pub(crate) const FILE_TAB_KEY_CONTEXT: &str = "KradFileTab";

pub(crate) const EDITOR_KEY_CONTEXT: &str = "KradFileTab > Input";

pub(crate) const ASK_KEY_CONTEXT: &str = "KradAsk";
