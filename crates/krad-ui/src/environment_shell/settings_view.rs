mod gateway;
mod general;
mod github;
mod gitlab;
mod integrations;
mod material;
mod mcp;
mod page;
mod terminal;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, BoxShadow, Context, Div, ElementId,
    InteractiveElement, IntoElement, ParentElement, SharedString, Stateful, Styled, Window,
};
use krad_domain::AppearanceMode;
use krad_store::write_settings;

use super::dialog_parts::{sheet_group, sheet_row};
use super::EnvironmentShell;
use crate::actions::OpenSettings;
use crate::components::IconName;
use crate::theme::{
    appearance, apply_appearance, card_shadow, Appearance, Palette, LINE_SM, LINE_XS, RADIUS_SM,
    RADIUS_XL, SPACE_0_5, TEXT_SM, TEXT_XS,
};
use gateway::GatewayUi;
use github::GithubUi;
pub(super) use gitlab::gitlab_token_saver;
use gitlab::GitlabUi;
use mcp::McpUi;

const SWATCH_COLUMNS: usize = 3;
const SWATCH_GAP: f32 = 10.0;
const SWATCH_PREVIEW_H: f32 = 64.0;
const SWATCH_PAD: f32 = 10.0;
const CHOICE_PAD: f32 = 14.0;
const CHOICE_GAP: f32 = 10.0;
const CHOICE_RING: f32 = 1.5;
const SETTINGS_NOT_SAVED: &str = "Couldn't save the settings";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPane {
    General,
    Terminal,
    Integrations,
    Gateway,
    Mcp,
}

impl SettingsPane {
    const fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Terminal => "Terminal",
            Self::Integrations => "Integrations",
            Self::Gateway => "Gateway",
            Self::Mcp => "MCP",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::General => "How the app looks and how you work in it.",
            Self::Terminal => "The font, copy and paste, and the colours of terminals.",
            Self::Integrations => "The code hosts reviews and merge requests come from.",
            Self::Gateway => {
                "Chat with the models of a gateway you run, through the app's own agent."
            }
            Self::Mcp => "The MCP servers whose tools Gateway chats can use.",
        }
    }

    const fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Terminal => IconName::Terminal,
            Self::Integrations => IconName::Merge,
            Self::Gateway => IconName::Flow,
            Self::Mcp => IconName::Plug,
        }
    }
}

pub(super) struct SettingsUi {
    pane: SettingsPane,
    gitlab: GitlabUi,
    github: GithubUi,
    gateway: GatewayUi,
    mcp: McpUi,
    tint: material::TintUi,
}

impl From<AppearanceMode> for Appearance {
    fn from(mode: AppearanceMode) -> Self {
        match mode {
            AppearanceMode::Light => Self::Light,
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
        .p(px(SPACE_0_5))
        .rounded(px(RADIUS_SM))
        .bg(pal.bg_hover)
}

fn swatch_grid(swatches: Vec<AnyElement>) -> Vec<AnyElement> {
    let mut swatches = swatches.into_iter().peekable();
    let mut grid = Vec::new();
    while swatches.peek().is_some() {
        let row: Vec<AnyElement> = swatches.by_ref().take(SWATCH_COLUMNS).collect();
        let missing = SWATCH_COLUMNS - row.len();
        grid.push(
            div()
                .flex()
                .flex_row()
                .gap(px(SWATCH_GAP))
                .children(row)
                .children((0..missing).map(|_| div().flex_1()))
                .into_any_element(),
        );
    }
    grid
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
        let tint = self.tint_ui(cx);
        self.settings_ui = Some(SettingsUi {
            pane: SettingsPane::General,
            gitlab,
            github,
            gateway,
            mcp,
            tint,
        });
        self.refresh_gitlab_status(cx);
        self.refresh_github_status(cx);
        self.load_group_remotes(cx);
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_ui = None;
        self.review.forget_login();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn save_settings(&self, cx: &mut Context<Self>) {
        let settings = self.settings.clone();
        let writing = cx.background_spawn(async move { write_settings(&settings) });
        cx.spawn(async move |this, cx| {
            let Err(error) = writing.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.push_toast(format!("{SETTINGS_NOT_SAVED}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn set_appearance(&mut self, mode: AppearanceMode, cx: &mut Context<Self>) {
        self.settings.appearance = mode;
        apply_appearance(mode.into(), cx);
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
        cx.notify();
    }
}
