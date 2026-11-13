use krad_domain::{EnvironmentData, GroupId, PermissionMode, TerminalId};
use std::collections::HashMap;
use std::time::Instant;

mod environment_copy;

pub(super) use environment_copy::{EnvironmentCopy, NodesMoved};

use super::live_terminal::LiveTerminal;
use super::{
    activity, archive, changes_tab, file_tab, session_templates, surface,
    terminal_images, workers, Prompt,
};

pub(super) struct LiveEnv {
    pub(super) data: EnvironmentCopy,
    pub(super) sessions: HashMap<TerminalId, LiveTerminal>,
    pub(super) chats: HashMap<TerminalId, surface::SurfaceChat>,
    pub(super) focused: Option<TerminalId>,
    pub(super) selected_group: Option<GroupId>,
    pub(super) working_since: HashMap<TerminalId, Instant>,
    pub(super) pending_chats: HashMap<TerminalId, krad_chat::LiveSession>,
    pub(super) waiting_prompts: HashMap<TerminalId, Vec<Prompt>>,
    pub(super) waiting_modes: HashMap<TerminalId, PermissionMode>,
    pub(super) waiting_choices: HashMap<TerminalId, session_templates::ChatChoices>,
    pub(super) noticed_chats: Vec<TerminalId>,
    pub(super) images: HashMap<TerminalId, terminal_images::TerminalImages>,
    pub(super) editors: HashMap<TerminalId, file_tab::FileEditor>,
    pub(super) diffs: HashMap<TerminalId, changes_tab::FileDiff>,
    pub(super) activity_docked: Vec<TerminalId>,
    pub(super) utility_tab: activity::UtilityTab,
    pub(super) terminals_tab: activity::TerminalsTab,
    pub(super) pending_cursors: HashMap<TerminalId, krad_lsp::lsp_types::Position>,
    pub(super) navigation: HashMap<krad_domain::SessionId, krad_domain::NavigationHistory>,
    pub(super) worker_launches: HashMap<TerminalId, workers::LaunchReply>,
    pub(super) pending_lead_ending: Option<workers::LeadEnding>,
    pub(super) archive_sheet: Option<archive::ArchiveSheet>,
    pub(super) extension_sidebar: Option<super::extensions::OpenViewId>,
    pub(super) extension_side: Option<super::extensions::OpenViewId>,
}

impl LiveEnv {
    pub(super) fn new(data: EnvironmentData, focused: Option<TerminalId>) -> Self {
        let selected_group = focused
            .and_then(|id| data.group_of(id))
            .or_else(|| data.groups.first())
            .map(|group| group.id);
        let pending_chats = surface::restored_chats(&data.nodes);
        Self {
            data: EnvironmentCopy::new(data),
            sessions: HashMap::new(),
            chats: HashMap::new(),
            focused,
            selected_group,
            working_since: HashMap::new(),
            pending_chats,
            waiting_prompts: HashMap::new(),
            waiting_modes: HashMap::new(),
            waiting_choices: HashMap::new(),
            noticed_chats: Vec::new(),
            images: HashMap::new(),
            editors: HashMap::new(),
            diffs: HashMap::new(),
            activity_docked: Vec::new(),
            utility_tab: activity::UtilityTab::Log,
            terminals_tab: activity::TerminalsTab::Hidden,
            pending_cursors: HashMap::new(),
            navigation: HashMap::new(),
            worker_launches: HashMap::new(),
            pending_lead_ending: None,
            archive_sheet: None,
            extension_sidebar: None,
            extension_side: None,
        }
    }
}
