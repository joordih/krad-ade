mod activity;
mod agent_bus;
mod agent_choices;
mod agent_composer;
mod agent_hooks;
mod arcade;
mod ask;
mod changes_tab;
mod chat;
mod chrome;
mod code_viewer;
mod context_menu;
mod dialog_parts;
mod disk_text;
mod docs_generate;
mod drag;
mod environment_menu;
mod environment_order;
mod file_tab;
mod file_tree;
mod gateway;
mod groups;
mod harness_models;
mod input;
mod layout;
mod markdown_images;
mod mcp;
mod menus;
mod model_picker;
mod name_dialogs;
mod native_alert;
mod notes_view;
mod opencode;
mod palette;
mod rename;
mod review_view;
mod session_look;
mod session_pane;
mod sessions;
mod sessions_view;
mod settings_view;
mod side_panel;
mod sidebar;
mod sidebar_terminals;
mod snapshot;
mod surface;
mod terminal_actions;
mod terminal_body;
mod terminal_card;
mod terminal_images;
mod terminal_lifecycle;
mod terminal_status;
mod terminal_text;
mod top_bar;
mod window_close;
mod worktrees;

use futures::channel::mpsc::{Receiver as PumpReceiver, Sender as PumpSender};
use futures::StreamExt as _;
use gpui_kit::component::command::CommandState;
use gpui_kit::{
    Context, Entity, FocusHandle, Subscription, Task, WeakEntity, Window,
    WindowBackgroundAppearance,
};
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, EnvironmentView, GroupId, PersistedLayout,
    Settings, TerminalId, TerminalMeta, Visibility, DEFAULT_ENVIRONMENT, SCROLLBACK_PERSIST_LINES,
};
use krad_store::LayoutWriter;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use self::chrome::Toast;
use self::drag::DragState;
use self::sidebar::PanelReveal;
use self::window_close::CloseRequest;
use krad_agents::{AgentStatus, Mailbox};
use krad_session::pump_channel;
use krad_session::spawn_ticker;
use krad_session::LiveSession;
use krad_session::PumpMsg;

const FPS_HUD_DEFAULT: Visibility = Visibility::Hidden;
const FULL_REFRESH_HZ: u32 = 30;
const REDUCED_REFRESH_HZ: u32 = 15;

struct LiveEnv {
    data: EnvironmentData,
    sessions: HashMap<TerminalId, LiveSession>,
    chats: HashMap<TerminalId, surface::SurfaceChat>,
    focused: Option<TerminalId>,
    selected_group: Option<GroupId>,
    mailbox: Mailbox,
    working_since: HashMap<TerminalId, Instant>,
    statuses: HashMap<TerminalId, AgentStatus>,
    pending_chats: HashMap<TerminalId, krad_chat::LiveSession>,
    noticed_chats: Vec<TerminalId>,
    images: HashMap<TerminalId, terminal_images::TerminalImages>,
    editors: HashMap<TerminalId, file_tab::FileEditor>,
    diffs: HashMap<TerminalId, changes_tab::FileDiff>,
    activity_docked: Vec<TerminalId>,
    utility_tab: activity::UtilityTab,
    terminals_tab: activity::TerminalsTab,
}

impl LiveEnv {
    fn new(mut data: EnvironmentData, focused: Option<TerminalId>) -> Self {
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
            data,
            sessions: HashMap::new(),
            chats: HashMap::new(),
            focused,
            selected_group,
            mailbox: Mailbox::default(),
            working_since: HashMap::new(),
            statuses: HashMap::new(),
            pending_chats,
            noticed_chats: Vec::new(),
            images: HashMap::new(),
            editors: HashMap::new(),
            diffs: HashMap::new(),
            activity_docked: Vec::new(),
            utility_tab: activity::UtilityTab::Log,
            terminals_tab: activity::TerminalsTab::Hidden,
        }
    }
}

fn live_meta(live: &LiveEnv) -> HashMap<TerminalId, TerminalMeta> {
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

pub struct EnvironmentShell {
    focus: FocusHandle,
    file_tree_focus: FocusHandle,
    envs: HashMap<EnvironmentName, LiveEnv>,
    order: Vec<EnvironmentName>,
    active: EnvironmentName,
    drag: DragState,
    fps_hud: Visibility,
    window_background: Option<(WindowBackgroundAppearance, bool)>,
    material_watch: Option<Subscription>,
    palette_open: bool,
    pub palette: Option<Entity<CommandState>>,
    pub self_ref: Option<WeakEntity<Self>>,
    pump_task: Option<Task<()>>,
    pump_tx: Option<PumpSender<PumpMsg>>,
    toasts: Vec<Toast>,
    toast_seq: u64,
    unseen: HashSet<EnvironmentName>,
    cell_width_px: Option<f32>,
    scroll_remainder: f32,
    canvas_px: (f32, f32),
    frame_inset: (f32, f32),
    search: Option<terminal_actions::SearchQuery>,
    rename: Option<rename::RenameDraft>,
    activity: Visibility,
    pane_drag: session_pane::PaneDrag,
    pane_target: Option<krad_domain::PaneId>,
    activity_reveal: PanelReveal,
    activity_opened: activity::ActivityOpened,
    sidebar: PanelReveal,
    sidebar_peek: sidebar::Peek,
    chat_landings: HashMap<TerminalId, PanelReveal>,
    side_panel_view: EnvironmentView,
    side_reveal: PanelReveal,
    side_search: side_panel::SideSearch,
    changes_expanded: HashSet<PathBuf>,
    activity_log: activity::ActivityLog,
    close_request: CloseRequest,
    layout_writer: Option<LayoutWriter>,
    git: HashMap<PathBuf, groups::GroupGit>,
    git_task: Option<Task<()>>,
    file_tree: file_tree::FileTree,
    viewer: Option<code_viewer::CodeView>,
    native_agents: Vec<AgentPreset>,
    wsl_agents: HashMap<String, Vec<AgentPreset>>,
    settings: Settings,
    settings_ui: Option<settings_view::SettingsUi>,
    agent_bus: Option<agent_bus::AgentBus>,
    opencode: opencode::Opencode,
    gateway: gateway::Gateway,
    harness_models: harness_models::HarnessModels,
    account_usage: surface::AccountUsage,
    context_menu: Option<context_menu::ContextMenu>,
    model_picker: Option<model_picker::ModelPicker>,
    agent_composer: Option<agent_composer::AgentComposer>,
    panel_widths: layout::PanelWidths,
    chat: chat::ChatState,
    notes: notes_view::NotesUi,
    review: review_view::ReviewUi,
    docs_dialog: Option<docs_generate::DocsDialog>,
    name_dialogs: name_dialogs::NameDialogs,
    arcade: arcade::Arcade,
}

impl EnvironmentShell {
    fn start_pump(pump_rx: PumpReceiver<PumpMsg>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut inbox = pump_rx;
            while let Some(first) = inbox.next().await {
                let mut batch = vec![first];
                while let Ok(msg) = inbox.try_recv() {
                    batch.push(msg);
                }
                let Ok(interval) = this.update(cx, |shell, cx| {
                    let mut changed = false;
                    for msg in batch {
                        changed |= shell.apply_pump(msg);
                    }
                    shell.flush_noticed_chats(cx);
                    shell.watch_new_opencode_terminals(cx);
                    if changed {
                        cx.notify();
                    }
                    shell.frame_interval()
                }) else {
                    break;
                };
                cx.background_executor().timer(interval).await;
            }
        })
    }

    pub fn with_seed(
        cx: &mut Context<Self>,
        seed: Option<PersistedLayout>,
        settings: Settings,
    ) -> Self {
        let (pump_tx, pump_rx) = pump_channel();
        spawn_ticker(pump_tx.clone());
        let mut shell = Self {
            focus: cx.focus_handle(),
            file_tree_focus: cx.focus_handle(),
            envs: HashMap::new(),
            order: Vec::new(),
            active: EnvironmentName::new(String::new()),
            drag: DragState::None,
            fps_hud: FPS_HUD_DEFAULT,
            window_background: None,
            material_watch: None,
            palette_open: false,
            palette: None,
            self_ref: None,
            pump_task: None,
            pump_tx: Some(pump_tx),
            toasts: Vec::new(),
            toast_seq: 0,
            unseen: HashSet::new(),
            cell_width_px: None,
            scroll_remainder: 0.0,
            canvas_px: (0.0, 0.0),
            frame_inset: (0.0, 0.0),
            search: None,
            rename: None,
            activity: Visibility::Hidden,
            pane_drag: session_pane::PaneDrag::default(),
            pane_target: None,
            activity_reveal: PanelReveal::new(false),
            activity_opened: activity::ActivityOpened::Deliberately,
            sidebar: PanelReveal::open(),
            sidebar_peek: sidebar::Peek::Off,
            chat_landings: HashMap::new(),
            side_panel_view: EnvironmentView::Sessions,
            side_reveal: PanelReveal::new(false),
            side_search: side_panel::SideSearch::default(),
            changes_expanded: HashSet::new(),
            activity_log: activity::ActivityLog::default(),
            close_request: CloseRequest::Idle,
            layout_writer: LayoutWriter::spawn(),
            git: HashMap::new(),
            git_task: None,
            file_tree: file_tree::FileTree::default(),
            viewer: None,
            native_agents: vec![AgentPreset::Shell],
            wsl_agents: HashMap::new(),
            settings,
            settings_ui: None,
            agent_bus: None,
            opencode: opencode::Opencode::default(),
            gateway: gateway::Gateway::default(),
            harness_models: harness_models::HarnessModels::default(),
            account_usage: surface::AccountUsage::default(),
            context_menu: None,
            model_picker: None,
            agent_composer: None,
            panel_widths: layout::PanelWidths::default(),
            chat: chat::ChatState::default(),
            notes: notes_view::NotesUi::default(),
            review: review_view::ReviewUi::default(),
            docs_dialog: None,
            arcade: arcade::Arcade::new(cx.focus_handle()),
            name_dialogs: name_dialogs::NameDialogs::Closed,
        };
        shell.start_agent_bus(cx);
        shell.check_gateway(cx);
        Self::restore_chat(cx);
        match seed {
            Some(snapshot) => shell.restore_snapshot(snapshot),
            None => shell.restore_env(EnvironmentData::empty(EnvironmentName::new(
                DEFAULT_ENVIRONMENT.to_owned(),
            ))),
        }
        shell.pump_task = Some(Self::start_pump(pump_rx, cx));
        Self::detect_agents(cx);
        shell.start_git_watch(cx);
        shell
    }

    fn frame_interval(&self) -> Duration {
        let terminals_shown = self
            .envs
            .get(&self.active)
            .is_some_and(|live| live.data.view == EnvironmentView::Sessions);
        let hz = if terminals_shown {
            FULL_REFRESH_HZ
        } else {
            REDUCED_REFRESH_HZ
        };
        Duration::from_secs(1) / hz
    }

    pub fn focus_canvas(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
    }

    fn switch_env(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        if self.envs.contains_key(name) {
            let leaving = self.active.clone();
            if let Some(live) = self.envs.get_mut(&leaving) {
                live.data.meta = live_meta(live);
            }
            self.active = name.clone();
            self.unseen.remove(name);
            cx.notify();
            self.store();
        }
    }

    fn close_env(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        if let Some(live) = self.envs.remove(name) {
            for (_, session) in live.sessions {
                session.cancelled.store(true, Ordering::SeqCst);
                if let Some(killer) = session.killer {
                    killer.kill();
                }
            }
        }
        self.order.retain(|entry| entry != name);
        self.unseen.remove(name);
        if self.order.is_empty() {
            let data = EnvironmentData::empty(EnvironmentName::new(DEFAULT_ENVIRONMENT.to_owned()));
            let fresh = data.name.clone();
            self.order.push(fresh.clone());
            self.envs.insert(fresh.clone(), LiveEnv::new(data, None));
        }
        if &self.active == name {
            if let Some(first) = self.order.first().cloned() {
                self.active = first;
            }
        }
        cx.notify();
        self.store();
    }
}
