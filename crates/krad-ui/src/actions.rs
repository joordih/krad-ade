use gpui_kit::actions;

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
        OpenSettings,
        TextLarger,
        TextSmaller,
        TextReset,
        NewAgent,
        SendTab,
        SendBackTab,
        RenameFile,
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

pub(crate) const CANVAS_KEY_CONTEXT: &str = "KradCanvas";

pub(crate) const VIEWER_KEY_CONTEXT: &str = "KradCodeViewer";

pub(crate) const FILE_TREE_KEY_CONTEXT: &str = "KradFileTree";

pub(crate) const FILE_TAB_KEY_CONTEXT: &str = "KradFileTab";

pub(crate) const ASK_KEY_CONTEXT: &str = "KradAsk";
