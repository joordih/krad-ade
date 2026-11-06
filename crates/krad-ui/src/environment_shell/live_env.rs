use krad_agents::{AgentStatus, Mailbox};
use krad_domain::{
    EnvironmentData, GroupId, PermissionMode, TerminalId, TerminalMeta, SCROLLBACK_PERSIST_LINES,
};
use krad_session::LiveSession;
use std::collections::HashMap;
use std::time::Instant;

use super::{
    activity, agent_bus, changes_tab, file_tab, host, sessions, surface, terminal_images, workers,
    Prompt,
};

pub(super) struct LiveEnv {
    pub(super) data: EnvironmentData,
    pub(super) link: krad_host::HostLink,
    pub(super) host: host::HostState,
    pub(super) remote_bus: agent_bus::RemoteBusSlot,
    pub(super) remote_bus_port: Option<u16>,
    pub(super) deferred_launches: Vec<sessions::DeferredLaunch>,
    pub(super) sessions: HashMap<TerminalId, LiveSession>,
    pub(super) chats: HashMap<TerminalId, surface::SurfaceChat>,
    pub(super) focused: Option<TerminalId>,
    pub(super) selected_group: Option<GroupId>,
    pub(super) mailbox: Mailbox,
    pub(super) working_since: HashMap<TerminalId, Instant>,
    pub(super) statuses: HashMap<TerminalId, AgentStatus>,
    pub(super) pending_chats: HashMap<TerminalId, krad_chat::LiveSession>,
    pub(super) waiting_prompts: HashMap<TerminalId, Vec<Prompt>>,
    pub(super) waiting_modes: HashMap<TerminalId, PermissionMode>,
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
    pub(super) extension_sidebar: Option<super::extensions::OpenViewId>,
    pub(super) extension_side: Option<super::extensions::OpenViewId>,
}

impl LiveEnv {
    pub(super) fn new(
        mut data: EnvironmentData,
        focused: Option<TerminalId>,
        link: krad_host::HostLink,
    ) -> Self {
        data.enter_sessions();
        data.drop_file_tabs(true);
        if let Some(id) = focused {
            data.reveal_terminal(id);
        }
        let selected_group = focused
            .and_then(|id| data.group_of(id))
            .or_else(|| data.groups.first())
            .map(|group| group.id);
        let pending_chats = surface::restored_chats(&data.nodes);
        Self {
            link,
            host: host::HostState::default(),
            remote_bus: agent_bus::RemoteBusSlot::Absent,
            remote_bus_port: None,
            deferred_launches: Vec::new(),
            data,
            sessions: HashMap::new(),
            chats: HashMap::new(),
            focused,
            selected_group,
            mailbox: Mailbox::default(),
            working_since: HashMap::new(),
            statuses: HashMap::new(),
            pending_chats,
            waiting_prompts: HashMap::new(),
            waiting_modes: HashMap::new(),
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
            extension_sidebar: None,
            extension_side: None,
        }
    }
}

pub(super) fn live_meta(live: &LiveEnv) -> HashMap<TerminalId, TerminalMeta> {
    let mut meta = live.data.meta.clone();
    for (id, session) in &live.sessions {
        meta.insert(
            *id,
            TerminalMeta {
                cwd: session.cwd.clone().unwrap_or_default(),
                scrollback: session.engine.scrollback(SCROLLBACK_PERSIST_LINES),
            },
        );
    }
    meta
}
