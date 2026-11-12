mod acp;
mod api_file;
mod backdrop_image;
mod beta;
mod chat;
mod editor;
mod extensions;
mod flat_pane;
mod fonts;
mod gateway;
mod general;
mod github;
mod gitlab;
mod harnesses;
mod integrations;
mod java_engine;
mod languages;
mod material;
mod mcp;
mod navigation;
mod notifications;
mod page;
mod plan_usage;
mod search;
mod sections;
mod skills;
mod spending;
mod terminal;
mod theme_editor;
mod themes;
mod toasts;
mod toolchains;
mod tuning;
mod webhooks;
mod worktrees;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, BoxShadow, Context, Div, ElementId,
    InteractiveElement, IntoElement, ParentElement, SharedString, Stateful, Styled, Window,
};
use krad_domain::{AppearanceMode, ToastKind};
use krad_store::write_settings;

use super::dialog_parts::{sheet_group, sheet_row};
use super::EnvironmentShell;
use crate::actions::OpenSettings;
use crate::theme::{
    appearance, card_shadow, mode_appearance, system_is_dark, Appearance, Palette, CAPSULE_PAD,
    LINE_SM, LINE_XS, RADIUS_XL, SPACE_0_5, TEXT_SM, TEXT_XS,
};
use acp::AcpUi;
use gateway::GatewayUi;
use github::GithubUi;
pub(super) use gitlab::gitlab_token_saver;
use gitlab::GitlabUi;
use mcp::McpUi;
use webhooks::WebhooksUi;

const SWATCH_GAP: f32 = 10.0;
const CHOICE_PAD: f32 = 14.0;
const CHOICE_GAP: f32 = 10.0;
const CHOICE_RING: f32 = 1.5;
const SETTINGS_NOT_SAVED: &str = "Couldn't save the settings";

pub(super) use navigation::SettingsPane;

pub(in crate::environment_shell) use fonts::FontZone;

pub(super) struct SettingsUi {
    pane: SettingsPane,
    search: search::SearchUi,
    skills: skills::SkillsUi,
    worktrees: worktrees::WorktreesUi,
    gitlab: GitlabUi,
    github: GithubUi,
    gateway: GatewayUi,
    mcp: McpUi,
    acp: AcpUi,
    webhooks: WebhooksUi,
    tint: material::TintUi,
    tuning: tuning::TuningUi,
    backdrop: backdrop_image::BackdropUi,
    languages: languages::LanguagesUi,
    extensions: extensions::ExtensionsUi,
    fonts: crate::font_catalog::FontCatalog,
    theme_editor: Option<theme_editor::ThemeEditor>,
    scroll: gpui_kit::ScrollHandle,
}

impl From<AppearanceMode> for Appearance {
    fn from(mode: AppearanceMode) -> Self {
        match mode {
            AppearanceMode::Light | AppearanceMode::System => Self::Light,
            AppearanceMode::Dark => Self::Dark,
        }
    }
}

impl From<Appearance> for AppearanceMode {
    fn from(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self::Light,
            Appearance::Dark => Self::Dark,
        }
    }
}

fn hint(text: &str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_subtle))
        .child(text.to_owned())
        .into_any_element()
}

fn panel_card(rows: impl IntoIterator<Item = AnyElement>, pal: &Palette) -> AnyElement {
    sheet_group(rows, pal)
}

pub(super) fn segmented_track(pal: &Palette) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(SPACE_0_5))
        .p(px(CAPSULE_PAD))
        .rounded_full()
        .bg(pal.bg_hover)
}

fn choice_card(
    id: impl Into<ElementId>,
    header: impl IntoElement,
    description: &str,
    chosen: bool,
    pal: &Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(CHOICE_GAP))
        .p(px(CHOICE_PAD))
        .rounded(px(RADIUS_XL))
        .cursor_pointer()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_strong))
        .when(chosen, |card| {
            let mut ring = card_shadow(pal);
            ring.push(
                BoxShadow::new(px(0.0), px(0.0), rgb(pal.accent).into())
                    .spread_radius(px(CHOICE_RING)),
            );
            card.bg(rgb(pal.bg_surface)).shadow(ring)
        })
        .when(!chosen, |card| card.bg(rgb(pal.bg_panel)))
        .child(header)
        .child(
            div()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_subtle))
                .child(description.to_owned()),
        )
}

fn form_row(label: &str, detail: Option<&str>, control: AnyElement, pal: &Palette) -> AnyElement {
    sheet_row(
        label.to_owned(),
        detail.map(|detail| SharedString::from(detail.to_owned())),
        Some(control),
        pal,
    )
    .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn on_open_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(window, cx);
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let gitlab = self.gitlab_ui(window, cx);
        let github = Self::github_ui();
        let gateway = self.gateway_ui(window, cx);
        let mcp = Self::mcp_ui(window, cx);
        let acp = Self::acp_ui(window, cx);
        let webhooks = Self::webhooks_ui(window, cx);
        let tint = self.tint_ui(cx);
        let tuning = self.tuning_ui(window, cx);
        let backdrop = self.backdrop_ui(cx);
        let languages = Self::languages_ui(window, cx);
        let search = Self::settings_search_ui(window, cx);
        let skills = Self::skills_ui(window, cx);
        let worktrees = self.worktrees_ui(window, cx);
        self.settings_ui = Some(SettingsUi {
            pane: SettingsPane::General,
            search,
            skills,
            worktrees,
            gitlab,
            github,
            gateway,
            mcp,
            acp,
            webhooks,
            tint,
            tuning,
            backdrop,
            languages,
            extensions: extensions::ExtensionsUi::default(),
            fonts: crate::font_catalog::font_catalog(cx),
            theme_editor: None,
            scroll: gpui_kit::ScrollHandle::new(),
        });
        self.refresh_gitlab_status(cx);
        self.refresh_github_status(cx);
        self.load_group_remotes(cx);
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_settings(cx);
        window.focus(&self.focus, cx);
    }

    pub(super) fn dismiss_settings(&mut self, cx: &mut Context<Self>) -> bool {
        let dismissed = self.drop_settings();
        if dismissed {
            cx.notify();
        }
        dismissed
    }

    pub(super) fn drop_settings(&mut self) -> bool {
        if self.settings_ui.take().is_none() {
            return false;
        }
        self.review.forget_login();
        true
    }

    pub(super) fn save_settings(&mut self, cx: &mut Context<Self>) {
        self.redetect_after_acp_change(cx);
        let settings = self.settings.clone();
        let writing = cx.background_spawn(async move { write_settings(&settings) });
        cx.spawn(async move |this, cx| {
            let Err(error) = writing.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.push_toast(ToastKind::Error, format!("{SETTINGS_NOT_SAVED}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn sync_system_appearance(&mut self, cx: &mut Context<Self>) {
        if self.settings.appearance != AppearanceMode::System {
            return;
        }
        let wanted = mode_appearance(AppearanceMode::System, system_is_dark(cx));
        if wanted != appearance(cx) {
            self.apply_active_theme(cx);
        }
    }

    fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.settings.appearance = mode;
        self.apply_active_theme(cx);
        self.save_settings(cx);
    }

    pub(super) fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        let next = match appearance(cx) {
            Appearance::Light => AppearanceMode::Dark,
            Appearance::Dark => AppearanceMode::Light,
        };
        self.set_appearance(next, cx);
    }

    pub(super) fn select_pane(&mut self, pane: SettingsPane, cx: &mut Context<Self>) {
        if let Some(ui) = &mut self.settings_ui {
            ui.pane = pane;
        }
        if pane == SettingsPane::Languages {
            self.refresh_server_runtime_statuses(cx);
        }
        if pane == SettingsPane::Harnesses {
            self.check_harness_health(cx);
        }
        if pane == SettingsPane::Acp {
            self.check_acp_packages(cx);
        }
        if pane == SettingsPane::Skills {
            self.refresh_skill_library(cx);
        }
        cx.notify();
    }
}
