mod activity;
mod agent_bus;
mod agent_composer;
mod agent_hooks;
mod app_update;
mod arcade;
mod ask;
mod changes_tab;
mod chat;
mod chrome;
mod code_viewer;
mod context_menu;
mod dialog_parts;
mod discard;
mod disk_text;
mod docs_generate;
mod drag;
mod editor_keys;
mod editor_popovers;
mod entry_input;
mod environment_menu;
mod environment_order;
mod file_tab;
mod file_tree;
mod gateway;
mod gestures;
mod groups;
mod harness_models;
mod host;
mod template_popup;
pub use host::HostTools;
mod input;
mod language_servers;
mod layout;
mod live_env;
mod markdown_images;
mod mcp;
mod menus;
mod model_picker;
mod name_dialogs;
mod native_alert;
mod notes_view;
mod opencode;
mod palette;
mod performance;
mod rename;
mod review_view;
mod rubber_band;
mod session_look;
mod session_pane;
mod sessions;
mod sessions_view;
mod settings_swipe;
mod settings_view;
mod shell_site;
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
mod toolchains;
mod top_bar;
mod window_close;
mod work_mode;
mod worktrees;

use futures::channel::mpsc::{Receiver as PumpReceiver, Sender as PumpSender};
use futures::StreamExt as _;
use gpui_kit::component::command::CommandState;
use gpui_kit::{
    Context, Entity, FocusHandle, Subscription, Task, WeakEntity, Window,
    WindowBackgroundAppearance,
};
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, EnvironmentView, PersistedLayout, Prompt,
    Settings, SidebarWidth, TerminalId, Visibility, WorkMode, DEFAULT_ENVIRONMENT,
};
use krad_store::LayoutWriter;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use self::chrome::Toast;
use self::drag::DragState;
use self::live_env::{live_meta, LiveEnv};
use self::sidebar::PanelReveal;
use self::window_close::CloseRequest;
use krad_session::pump_channel;
use krad_session::spawn_ticker;
use krad_session::PumpMsg;

const FPS_HUD_DEFAULT: Visibility = Visibility::Hidden;
const FULL_REFRESH_HZ: u32 = 30;
const REDUCED_REFRESH_HZ: u32 = 15;

pub struct EnvironmentShell {
    focus: FocusHandle,
    file_tree_focus: FocusHandle,
    envs: HashMap<EnvironmentName, LiveEnv>,
    order: Vec<EnvironmentName>,
    active: EnvironmentName,
    drag: DragState,
    fps_hud: Visibility,
    window_background: Option<(WindowBackgroundAppearance, bool)>,
    activation_watch: Option<Subscription>,
    palette_open: bool,
    palette_files: Vec<PathBuf>,
    language_servers: language_servers::LanguageServers,
    file_icons: Option<crate::file_icons::FileIcons>,
    editor_popover: Option<editor_popovers::EditorPopover>,
    signature: Option<editor_popovers::SignatureCard>,
    entry_input: Option<entry_input::EntryInput>,
    template_popup: Option<template_popup::TemplatePopup>,
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
    work_mode: WorkMode,
    sidebar_w: SidebarWidth,
    side_reveal: PanelReveal,
    side_search: side_panel::SideSearch,
    changes_expanded: HashSet<PathBuf>,
    pending_discard: Option<discard::Discard>,
    gestures: gestures::Gestures,
    history: side_panel::CommitHistory,
    activity_log: activity::ActivityLog,
    close_request: CloseRequest,
    layout_writer: Option<LayoutWriter>,
    git: HashMap<groups::GitKey, groups::GroupGit>,
    git_task: Option<Task<()>>,
    file_tree: file_tree::FileTree,
    viewer: Option<code_viewer::CodeView>,
    native_agents: Vec<AgentPreset>,
    wsl_agents: HashMap<String, Vec<AgentPreset>>,
    host_agents: HashMap<krad_domain::Host, Vec<AgentPreset>>,
    settings: Settings,
    settings_ui: Option<settings_view::SettingsUi>,
    agent_bus: Option<agent_bus::AgentBus>,
    opencode: opencode::Opencode,
    gateway: gateway::Gateway,
    listed_models: harness_models::ListedModels,
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
    hosts: host::HostUi,
    toolchains: toolchains::Toolchains,
    unreadable_environments: Vec<krad_domain::UnreadableEnvironment>,
    host_tools: host::HostTools,
    arcade: arcade::Arcade,
    app_updates: app_update::AppUpdates,
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
                    shell.service_hosts(cx);
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
        host_tools: host::HostTools,
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
            activation_watch: None,
            palette_open: false,
            palette_files: Vec::new(),
            language_servers: language_servers::LanguageServers::new(),
            file_icons: crate::file_icons::FileIcons::load(),
            editor_popover: None,
            signature: None,
            entry_input: None,
            template_popup: None,
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
            work_mode: WorkMode::default(),
            sidebar_w: SidebarWidth::default(),
            side_reveal: PanelReveal::new(false),
            side_search: side_panel::SideSearch::default(),
            changes_expanded: HashSet::new(),
            pending_discard: None,
            gestures: gestures::Gestures::default(),
            history: side_panel::CommitHistory::default(),
            activity_log: activity::ActivityLog::default(),
            close_request: CloseRequest::Idle,
            layout_writer: LayoutWriter::spawn(),
            git: HashMap::new(),
            git_task: None,
            file_tree: file_tree::FileTree::default(),
            viewer: None,
            native_agents: vec![AgentPreset::Shell],
            wsl_agents: HashMap::new(),
            host_agents: HashMap::new(),
            settings,
            settings_ui: None,
            agent_bus: None,
            opencode: opencode::Opencode::default(),
            gateway: gateway::Gateway::default(),
            listed_models: harness_models::ListedModels::default(),
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
            hosts: host::HostUi::default(),
            toolchains: toolchains::Toolchains::default(),
            unreadable_environments: Vec::new(),
            host_tools,
            app_updates: app_update::AppUpdates::default(),
        };
        shell.start_agent_bus(cx);
        shell.check_gateway(cx);
        shell.start_app_updates(cx);
        Self::load_arcade_scores(cx);
        match seed {
            Some(snapshot) => shell.restore_snapshot(snapshot),
            None => shell.restore_env(EnvironmentData::empty(EnvironmentName::new(
                DEFAULT_ENVIRONMENT.to_owned(),
            ))),
        }
        let active = shell.active.clone();
        shell.show_host(&active, cx);
        shell.pump_task = Some(Self::start_pump(pump_rx, cx));
        Self::detect_agents(cx);
        shell.start_watches(cx);
        shell.load_language_tools(cx);
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
            self.show_host(name, cx);
            cx.notify();
            self.store();
        }
    }

    fn close_env(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        if let Some(live) = self.envs.remove(name) {
            if !self
                .envs
                .values()
                .any(|other| other.data.host == live.data.host)
            {
                host::close_link(&live.link);
            }
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
            let link = self.link_for(&data.host);
            self.envs
                .insert(fresh.clone(), LiveEnv::new(data, None, link));
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
