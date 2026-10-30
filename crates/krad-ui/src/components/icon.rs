use gpui_kit::{px, rgb, svg, Styled, Svg};
use krad_domain::AgentPreset;

use crate::theme::Palette;

const CLAUDE_BRAND: u32 = 0x00d9_7757;

macro_rules! icons {
    ($($(#[$doc:meta])* $name:ident => $file:literal,)+) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub(crate) enum IconName {
            $($(#[$doc])* $name,)+
        }

        impl IconName {
            pub(crate) const ALL: &'static [Self] = &[$(Self::$name,)+];

            pub(crate) const fn path(self) -> &'static str {
                match self {
                    $(Self::$name => concat!("krad-icons/", $file, ".svg"),)+
                }
            }

            pub(crate) const fn bytes(self) -> &'static [u8] {
                match self {
                    $(Self::$name => include_bytes!(concat!(
                        "../../../../assets/icons/", $file, ".svg"
                    )),)+
                }
            }
        }
    };
}

icons! {
    Close => "cancel-01",
    Add => "add-01",
    Minus => "minus-sign",
    Search => "search-01",
    Sun => "sun-03",
    Moon => "moon-02",
    PanelBottom => "layout-bottom",
    Mosaic => "grid-view",
    ChevronUp => "arrow-up-01",
    ChevronDown => "arrow-down-01",
    Terminal => "computer-terminal-01",
    Folder => "folder-01",
    FolderAdd => "folder-add",
    FileAdd => "file-add",
    CollapseAll => "collapse-all",
    ListView => "list-view",
    TreeView => "tree-view",
    Refresh => "refresh",
    Workflow => "workflow-square-10",
    Play => "play-circle",
    Diamond => "diamond-01",
    SidebarLeft => "sidebar-left",
    SidebarRight => "sidebar-right",
    Home => "home-01",
    ChevronLeft => "arrow-left-01",
    ChevronRight => "arrow-right-01",
    File => "file-01",
    GitBranch => "git-branch",
    Settings => "settings-02",
    Plug => "plug-socket",
    UserGroup => "user-group",
    Bell => "notification-01",
    Tasks => "task-done-01",
    Tick => "tick-02",
    Loading => "loading-03",
    RadioButton => "radio-button",
    CheckCircle => "checkmark-circle-02",
    CancelCircle => "cancel-circle",
    Expand => "arrow-expand-01",
    Chat => "bubble-chat",
    Flag => "flag-02",
    Book => "book-open-01",
    Note => "note-01",
    Magic => "ai-magic",
    Attach => "attachment-01",
    Send => "sent",
    Flash => "flash",
    Repeat => "repeat",
    Merge => "git-merge",
    PullRequest => "git-pull-request",
    ThumbsUp => "thumbs-up",
    Comment => "comment-01",
    OpenExternal => "square-arrow-up-right",
    Exchange => "exchange-01",
    Support => "customer-support",
    Stop => "stop-circle",
    Target => "target-02",
    Flow => "flow-connection",
    Layers => "layers-01",
    Crown => "crown",
    Key => "key-01",
    Copy => "copy-01",
    Pencil => "pencil-edit-01",
    Lock => "square-lock-02",
    BrandClaude => "brand-claude",
    BrandOpenai => "brand-openai",
    BrandOpencode => "brand-opencode",
}

pub(crate) fn icon(name: IconName, size: f32, color: u32) -> Svg {
    svg()
        .path(name.path())
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
}

pub(crate) const fn harness_icon(preset: AgentPreset, pal: &Palette) -> (IconName, u32) {
    match preset {
        AgentPreset::ClaudeCode => (IconName::BrandClaude, CLAUDE_BRAND),
        AgentPreset::Codex => (IconName::BrandOpenai, pal.text_strong),
        AgentPreset::Opencode => (IconName::BrandOpencode, pal.text_strong),
        AgentPreset::Gateway => (IconName::Flow, pal.accent),
        AgentPreset::Shell => (IconName::Terminal, pal.text_subtle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_icon_has_a_distinct_path_and_an_svg() {
        let paths: HashSet<&str> = IconName::ALL.iter().map(|name| name.path()).collect();
        assert_eq!(paths.len(), IconName::ALL.len(), "two icons share a file");
        for name in IconName::ALL {
            let svg = String::from_utf8_lossy(name.bytes());
            assert!(
                svg.trim_start().starts_with("<svg"),
                "{name:?} is not an SVG"
            );
            assert!(svg.contains("currentColor"), "{name:?} cannot be tinted");
        }
    }
}
