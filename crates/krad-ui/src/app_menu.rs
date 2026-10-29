use gpui_kit::{App, Menu, MenuItem, OsAction, SystemMenuType};

use crate::actions::{
    CloseWindow, CopySelection, HideApp, HideOtherApps, MinimizeWindow, NewAgent, NewTerminal,
    OpenSettings, PasteClipboard, Quit, SearchTerminal, ShowAllApps, SplitTerminal, TextLarger,
    TextReset, TextSmaller, ToggleActivity, ToggleFps, TogglePalette, ToggleSidePanel,
    ToggleSidebar, ZoomWindow,
};

const APP_MENU: &str = "Krad ADE";
const FILE_MENU: &str = "File";
const EDIT_MENU: &str = "Edit";
const VIEW_MENU: &str = "View";
const WINDOW_MENU: &str = "Window";

const SETTINGS_ITEM: &str = "Settings…";
const SERVICES_ITEM: &str = "Services";
const HIDE_ITEM: &str = "Hide Krad ADE";
const HIDE_OTHERS_ITEM: &str = "Hide Others";
const SHOW_ALL_ITEM: &str = "Show All";
const QUIT_ITEM: &str = "Quit Krad ADE";

const NEW_TERMINAL_ITEM: &str = "New Terminal";
const NEW_AGENT_ITEM: &str = "New Agent…";
const SPLIT_ITEM: &str = "Split Terminal";
const CLOSE_WINDOW_ITEM: &str = "Close Window";

const COPY_ITEM: &str = "Copy";
const PASTE_ITEM: &str = "Paste";
const FIND_ITEM: &str = "Find…";

const PALETTE_ITEM: &str = "Command Palette…";
const SIDEBAR_ITEM: &str = "Toggle Sidebar";
const SIDE_PANEL_ITEM: &str = "Toggle Side Panel";
const ACTIVITY_ITEM: &str = "Toggle Activity";
const TEXT_LARGER_ITEM: &str = "Larger Text";
const TEXT_SMALLER_ITEM: &str = "Smaller Text";
const TEXT_RESET_ITEM: &str = "Actual Text Size";
const FPS_ITEM: &str = "Show Frame Rate";

const MINIMIZE_ITEM: &str = "Minimize";
const ZOOM_WINDOW_ITEM: &str = "Zoom";

fn menu(name: &'static str, items: Vec<MenuItem>) -> Menu {
    Menu {
        name: name.into(),
        items,
        disabled: false,
    }
}

fn menus() -> Vec<Menu> {
    vec![
        menu(
            APP_MENU,
            vec![
                MenuItem::action(SETTINGS_ITEM, OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu(SERVICES_ITEM, SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(HIDE_ITEM, HideApp),
                MenuItem::action(HIDE_OTHERS_ITEM, HideOtherApps),
                MenuItem::action(SHOW_ALL_ITEM, ShowAllApps),
                MenuItem::separator(),
                MenuItem::action(QUIT_ITEM, Quit),
            ],
        ),
        menu(
            FILE_MENU,
            vec![
                MenuItem::action(NEW_TERMINAL_ITEM, NewTerminal),
                MenuItem::action(NEW_AGENT_ITEM, NewAgent),
                MenuItem::action(SPLIT_ITEM, SplitTerminal),
                MenuItem::separator(),
                MenuItem::action(CLOSE_WINDOW_ITEM, CloseWindow),
            ],
        ),
        menu(
            EDIT_MENU,
            vec![
                MenuItem::os_action(COPY_ITEM, CopySelection, OsAction::Copy),
                MenuItem::os_action(PASTE_ITEM, PasteClipboard, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::action(FIND_ITEM, SearchTerminal),
            ],
        ),
        menu(
            VIEW_MENU,
            vec![
                MenuItem::action(PALETTE_ITEM, TogglePalette),
                MenuItem::separator(),
                MenuItem::action(SIDEBAR_ITEM, ToggleSidebar),
                MenuItem::action(SIDE_PANEL_ITEM, ToggleSidePanel),
                MenuItem::action(ACTIVITY_ITEM, ToggleActivity),
                MenuItem::separator(),
                MenuItem::action(TEXT_LARGER_ITEM, TextLarger),
                MenuItem::action(TEXT_SMALLER_ITEM, TextSmaller),
                MenuItem::action(TEXT_RESET_ITEM, TextReset),
                MenuItem::separator(),
                MenuItem::action(FPS_ITEM, ToggleFps),
            ],
        ),
        menu(
            WINDOW_MENU,
            vec![
                MenuItem::action(MINIMIZE_ITEM, MinimizeWindow),
                MenuItem::action(ZOOM_WINDOW_ITEM, ZoomWindow),
            ],
        ),
    ]
}

pub fn install_app_menus(cx: &mut App) {
    if !cfg!(target_os = "macos") {
        return;
    }
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    cx.set_menus(menus());
}
