mod activity;
mod agent_bus;
mod agent_composer;
mod agent_hooks;
mod app_events;
mod app_update;
mod arcade;
mod archive;
mod ask;
mod attention;
mod backdrop;
mod changes_tab;
mod chat;
mod chat_search;
mod cloud;
mod code_viewer;
mod content_shift;
mod context_menu;
mod dialog_parts;
mod discard;
mod disk_text;
mod docs_generate;
mod drag;
mod editor_keys;
mod editor_popovers;
mod entry_input;
mod environment_edits;
mod environment_menu;
mod environment_order;
mod extensions;
mod file_tab;
mod file_tree;
mod focus_return;
mod gateway;
mod gestures;
mod git_actions;
mod groups;
mod harness_health;
mod harness_models;
mod host;
mod template_popup;
pub use host::HostTools;
mod automations;
mod input;
mod kraddie_memory;
mod kraddies;
mod language_servers;
mod layout;
mod live_env;
mod live_terminal;
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
mod runtime_view;
mod session_changes;
mod session_look;
mod session_pane;
mod session_templates;
mod sessions;
mod sessions_view;
mod settings_swipe;
mod settings_view;
mod shell_site;
mod side_panel;
mod sidebar;
mod sidebar_terminals;
mod snapshot;
mod spending;
mod surface;
mod terminal_actions;
mod terminal_body;
mod terminal_card;
mod terminal_images;
mod terminal_lifecycle;
mod terminal_status;
mod terminal_text;
mod toasts;
mod toolchains;
mod top_bar;
mod usage_bar;
mod variants;
mod window_close;
mod work_mode;
mod workers;
mod worktrees;

use gpui_kit::component::command::CommandState;
use gpui_kit::{Context, Entity, FocusHandle, Subscription, Task, WeakEntity, Window};
use krad_domain::{
    AgentPreset, EnvironmentData, EnvironmentName, EnvironmentView, Layout, Prompt, Settings,
    SidebarWidth, TerminalId, Visibility, WorkMode, DEFAULT_ENVIRONMENT,
};
use krad_runtime::RuntimeHandle;
use krad_store::{AcpTranscripts, LayoutFile, LayoutWriter};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use self::drag::DragState;
use self::live_env::LiveEnv;
use self::sidebar::PanelReveal;
use self::toasts::Toasts;
use self::window_close::CloseRequest;

use crate::components::{new_copy_feedback, CopyFeedback};

const FPS_HUD_DEFAULT: Visibility = Visibility::Hidden;
const FULL_REFRESH_HZ: u32 = 30;
const REDUCED_REFRESH_HZ: u32 = 15;
const UI_TICK: Duration = Duration::from_secs(1);

pub struct EnvironmentShell {
    focus: FocusHandle,
    file_tree_focus: FocusHandle,
    envs: HashMap<EnvironmentName, LiveEnv>,
    order: Vec<EnvironmentName>,
    active: EnvironmentName,
    drag: DragState,
    fps_hud: Visibility,
    backdrop: backdrop::Backdrop,
    activation_watch: Option<Subscription>,
    palette_open: bool,
    palette_files: Vec<PathBuf>,
    palette_tasks: Vec<krad_domain::ProjectTask>,
    language_servers: language_servers::LanguageServers,
    file_icons: Option<crate::file_icons::FileIcons>,
    editor_popover: Option<editor_popovers::EditorPopover>,
    signature: Option<editor_popovers::SignatureCard>,
    entry_input: Option<entry_input::EntryInput>,
    template_popup: Option<template_popup::TemplatePopup>,
    pub palette: Option<Entity<CommandState>>,
    pub self_ref: Option<WeakEntity<Self>>,
    ticker: Option<Task<()>>,
    toasts: Toasts,
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
    content_shift: content_shift::ContentShift,
    sidebar_w: SidebarWidth,
    side_reveal: PanelReveal,
    side_search: side_panel::SideSearch,
    git_ui: git_actions::GitUi,
    git_requests: git_actions::GitRequests,
    pending_discard: Option<discard::Discard>,
    session_changes: session_changes::SessionChangesUi,
    gestures: gestures::Gestures,
    history: side_panel::CommitHistory,
    activity_log: activity::ActivityLog,
    close_request: CloseRequest,
    view_writer: Option<LayoutWriter>,
    acp_transcripts: Option<AcpTranscripts>,
    git: HashMap<groups::GitKey, groups::GroupGit>,
    git_task: Option<Task<()>>,
    file_tree: file_tree::FileTree,
    viewer: Option<code_viewer::CodeView>,
    native_agents: Vec<AgentPreset>,
    wsl_agents: HashMap<String, Vec<AgentPreset>>,
    host_agents: HashMap<krad_domain::Host, Vec<AgentPreset>>,
    detected_acp: Vec<krad_domain::AcpAgent>,
    settings: Settings,
    settings_ui: Option<settings_view::SettingsUi>,
    bus_work: agent_bus::BusWork,
    opencode: opencode::Opencode,
    gateway: gateway::Gateway,
    listed_models: harness_models::ListedModels,
    acp_adapters: HashMap<krad_domain::AcpAgentId, krad_domain::ManagedAdapter>,
    harness_checks: harness_health::HarnessChecks,
    context_menu: Option<context_menu::ContextMenu>,
    focus_returns: focus_return::FocusReturns,
    copy_feedback: Entity<CopyFeedback>,
    model_picker: Option<model_picker::ModelPicker>,
    effort_popover: Option<surface::EffortPopover>,
    agent_composer: Option<agent_composer::AgentComposer>,
    template_picker: Option<session_templates::TemplatePicker>,
    panel_widths: layout::PanelWidths,
    chat: chat::ChatState,
    notes: notes_view::NotesUi,
    chat_search: chat_search::ChatSearch,
    review: review_view::ReviewUi,
    docs_dialog: Option<docs_generate::DocsDialog>,
    name_dialogs: name_dialogs::NameDialogs,
    hosts: host::HostUi,
    toolchains: toolchains::Toolchains,
    host_tools: host::HostTools,
    arcade: arcade::Arcade,
    app_updates: app_update::AppUpdates,
    extensions: extensions::Extensions,
    attention: attention::AttentionUi,
    app_events: app_events::AppEventsUi,
    kraddies: kraddies::KraddiesUi,
    automations: automations::AutomationsUi,
    variants: variants::VariantsUi,
    cloud: cloud::CloudRunners,
    runtime: runtime_view::RuntimeLink,
}

impl EnvironmentShell {
    fn start_ticker(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(UI_TICK).await;
            let ticked = this.update(cx, |shell, cx| {
                let changed = shell.apply_tick();
                shell.flush_live_work(cx);
                if changed {
                    cx.notify();
                }
            });
            if ticked.is_err() {
                break;
            }
        })
    }

    fn flush_live_work(&mut self, cx: &mut Context<Self>) {
        self.flush_noticed_chats(cx);
        self.flush_attention(cx);
        self.service_hosts(cx);
        self.watch_new_opencode_terminals(cx);
    }

    pub fn with_seed(
        cx: &mut Context<Self>,
        layout: Layout,
        settings: Settings,
        host_tools: host::HostTools,
        runtime: RuntimeHandle,
    ) -> Self {
        let mut shell = Self::assemble(cx, settings, host_tools, runtime);
        shell.start(layout, cx);
        shell
    }

    fn assemble(
        cx: &mut Context<Self>,
        settings: Settings,
        host_tools: host::HostTools,
        runtime: RuntimeHandle,
    ) -> Self {
        Self {
            focus: cx.focus_handle(),
            file_tree_focus: cx.focus_handle(),
            envs: HashMap::new(),
            order: Vec::new(),
            active: EnvironmentName::new(String::new()),
            drag: DragState::None,
            fps_hud: FPS_HUD_DEFAULT,
            backdrop: backdrop::Backdrop::default(),
            activation_watch: None,
            palette_open: false,
            palette_files: Vec::new(),
            palette_tasks: Vec::new(),
            language_servers: language_servers::LanguageServers::new(),
            file_icons: crate::file_icons::FileIcons::load(),
            editor_popover: None,
            signature: None,
            entry_input: None,
            template_popup: None,
            palette: None,
            self_ref: None,
            ticker: None,
            toasts: Toasts::default(),
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
            content_shift: content_shift::ContentShift::default(),
            sidebar_w: SidebarWidth::default(),
            side_reveal: PanelReveal::new(false),
            side_search: side_panel::SideSearch::default(),
            git_ui: git_actions::GitUi::default(),
            git_requests: git_actions::GitRequests::default(),
            pending_discard: None,
            session_changes: session_changes::SessionChangesUi::default(),
            gestures: gestures::Gestures::default(),
            history: side_panel::CommitHistory::default(),
            activity_log: activity::ActivityLog::default(),
            close_request: CloseRequest::Idle,
            view_writer: LayoutWriter::spawn(LayoutFile::View),
            acp_transcripts: AcpTranscripts::spawn(),
            git: HashMap::new(),
            git_task: None,
            file_tree: file_tree::FileTree::default(),
            viewer: None,
            native_agents: vec![AgentPreset::Shell],
            wsl_agents: HashMap::new(),
            host_agents: HashMap::new(),
            detected_acp: settings.acp.all_agents(),
            settings,
            settings_ui: None,
            bus_work: agent_bus::BusWork::default(),
            opencode: opencode::Opencode::default(),
            gateway: gateway::Gateway::default(),
            listed_models: harness_models::ListedModels::default(),
            acp_adapters: HashMap::new(),
            harness_checks: harness_health::HarnessChecks::default(),
            context_menu: None,
            focus_returns: focus_return::FocusReturns::default(),
            copy_feedback: new_copy_feedback(cx),
            model_picker: None,
            effort_popover: None,
            agent_composer: None,
            template_picker: None,
            panel_widths: layout::PanelWidths::default(),
            chat: chat::ChatState::default(),
            notes: notes_view::NotesUi::default(),
            chat_search: chat_search::ChatSearch::default(),
            review: review_view::ReviewUi::default(),
            docs_dialog: None,
            arcade: arcade::Arcade::new(cx.focus_handle()),
            name_dialogs: name_dialogs::NameDialogs::Closed,
            hosts: host::HostUi::default(),
            toolchains: toolchains::Toolchains::default(),
            host_tools,
            app_updates: app_update::AppUpdates::default(),
            extensions: extensions::Extensions::default(),
            attention: attention::AttentionUi::default(),
            app_events: app_events::AppEventsUi::default(),
            kraddies: kraddies::KraddiesUi::default(),
            automations: automations::AutomationsUi::default(),
            variants: variants::VariantsUi::default(),
            cloud: cloud::CloudRunners::default(),
            runtime: runtime_view::RuntimeLink::new(runtime),
        }
    }

    fn start(&mut self, layout: Layout, cx: &mut Context<Self>) {
        self.runtime.handle.use_host_context(self.host_context());
        self.start_services(cx);
        self.restore_layout(layout);
        let active = self.active.clone();
        self.show_host(&active, cx);
        self.connect_worker_hosts(cx);
        self.ticker = Some(Self::start_ticker(cx));
        self.start_loading(cx);
    }

    fn start_services(&mut self, cx: &mut Context<Self>) {
        cx.observe(&self.copy_feedback, |_, _, cx| cx.notify())
            .detach();
        self.follow_runtime(cx);
        Self::settle_runtime_on_quit(cx);
        self.use_webhooks();
        self.start_agent_bus(cx);
        self.check_gateway(cx);
        self.start_app_updates(cx);
        Self::load_arcade_scores(cx);
    }

    fn start_loading(&mut self, cx: &mut Context<Self>) {
        self.detect_agents();
        self.start_harness_onboarding(cx);
        self.start_watches(cx);
        self.load_language_tools(cx);
        self.load_extensions(cx);
        self.start_kraddie_clocks(cx);
        self.start_archive_sweeps(cx);
    }

    pub(super) fn frame_interval(&self) -> Duration {
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
            self.active = name.clone();
            self.unseen.remove(name);
            self.show_host(name, cx);
            self.place_usage();
            cx.notify();
            self.store();
        }
    }

    fn close_env(&mut self, name: &EnvironmentName, cx: &mut Context<Self>) {
        self.forget_environment_archive(name, cx);
        if let Some(live) = self.envs.remove(name) {
            for id in live.sessions.keys() {
                self.end_terminal(name, *id);
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
