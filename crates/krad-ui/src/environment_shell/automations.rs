mod board;
mod card;
mod editor;
mod grid;
mod layout;
mod model;
mod shelf;
mod wording;

use gpui_kit::component::input::InputState;
use gpui_kit::{
    canvas, div, px, AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement,
    SharedString, Styled, Task, Window,
};
use krad_domain::{
    AgentPreset, AutomationKey, Effort, EnvironmentView, KraddieId, KraddieSchedule,
    PermissionMode, TemplateCatalog, TemplateCategory, Trigger,
};
use krad_gitlab::GitlabConfig;
use krad_store::{read_github_login, read_gitlab_login};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use super::EnvironmentShell;
use crate::actions::ToggleAutomations;
use crate::components::card as island;
use crate::theme::palette;
use layout::{arrangement, Arrangement};

pub(super) use editor::AutomationEditor;

const FILTER_PLACEHOLDER: &str = "Find by name, Kraddie or cue";
const NUMERALS_SHOWN: usize = 64;
const MEASURE_TOLERANCE: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CodeHostLogins {
    pub(super) github: bool,
    pub(super) gitlab: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AutomationMenu {
    AddTrigger(Trigger),
    AddSchedule(KraddieSchedule),
    Harness(Option<AgentPreset>),
    Effort(Option<Effort>),
    Permission(Option<PermissionMode>),
    Group(Option<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shelf {
    open: bool,
    category: TemplateCategory,
}

pub(super) struct AutomationsUi {
    catalog: TemplateCatalog,
    filter: Option<Entity<InputState>>,
    editor: Option<Box<AutomationEditor>>,
    shelf: Shelf,
    side_by_side: bool,
    logins: CodeHostLogins,
    ticker: Option<Task<()>>,
    numerals: Rc<[SharedString]>,
    island_width: Rc<Cell<Option<f32>>>,
}

fn island_probe(measured: Rc<Cell<Option<f32>>>) -> AnyElement {
    canvas(
        move |bounds, window, _| {
            let width = bounds.size.width.as_f32();
            let known = measured.get();
            if known.is_none_or(|known| (known - width).abs() >= MEASURE_TOLERANCE) {
                measured.set(Some(width));
                window.refresh();
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

impl Default for AutomationsUi {
    fn default() -> Self {
        let catalog = TemplateCatalog::built_in().unwrap_or_else(|error| {
            tracing::error!(%error, "automation templates not read");
            TemplateCatalog::default()
        });
        Self {
            catalog,
            filter: None,
            editor: None,
            shelf: Shelf {
                open: false,
                category: TemplateCategory::Popular,
            },
            side_by_side: false,
            logins: CodeHostLogins::default(),
            ticker: None,
            island_width: Rc::new(Cell::new(None)),
            numerals: (0..=NUMERALS_SHOWN)
                .map(|number| SharedString::from(number.to_string()))
                .collect(),
        }
    }
}

impl EnvironmentShell {
    pub(super) fn on_toggle_automations(
        &mut self,
        _: &ToggleAutomations,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_automations(cx);
    }

    pub(super) fn toggle_automations(&mut self, cx: &mut Context<Self>) {
        let view = if self.active_view() == EnvironmentView::Automations {
            EnvironmentView::Sessions
        } else {
            EnvironmentView::Automations
        };
        self.hide_extension_main_view();
        self.set_view(view, cx);
    }

    pub(super) fn automations_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let filter = self
            .automations
            .filter
            .get_or_insert_with(|| {
                cx.new(|cx| InputState::new(window, cx).placeholder(FILTER_PLACEHOLDER))
            })
            .clone();
        let width = self
            .automations
            .island_width
            .get()
            .unwrap_or_else(|| self.content_size(window).0);
        let editing = self.automations.editor.is_some();
        let body = match arrangement(width, editing, self.automations.side_by_side) {
            Arrangement::Board(fit) => self.automation_board(&filter, fit, width, cx),
            Arrangement::Editor { rail, .. } => self.automation_editor(rail, false, cx),
            Arrangement::SideBySide {
                board,
                editor_w,
                rail,
            } => div()
                .flex()
                .flex_row()
                .size_full()
                .min_h_0()
                .child(
                    div()
                        .flex_none()
                        .w(px(board.width))
                        .h_full()
                        .border_r_1()
                        .border_color(pal.divider)
                        .child(self.automation_board(&filter, board, width, cx)),
                )
                .child(
                    div()
                        .flex_none()
                        .w(px(editor_w))
                        .h_full()
                        .child(self.automation_editor(rail, true, cx)),
                )
                .into_any_element(),
        };
        self.keep_countdowns_ticking(cx);
        island(pal, cx)
            .relative()
            .size_full()
            .child(island_probe(Rc::clone(&self.automations.island_width)))
            .child(body)
            .into_any_element()
    }

    fn toggle_blueprints(&mut self, cx: &mut Context<Self>) {
        self.automations.shelf.open = !self.automations.shelf.open;
        cx.notify();
    }

    fn pick_blueprint_category(&mut self, category: TemplateCategory, cx: &mut Context<Self>) {
        self.automations.shelf.category = category;
        cx.notify();
    }

    fn toggle_side_by_side(&mut self, cx: &mut Context<Self>) {
        self.automations.side_by_side = !self.automations.side_by_side;
        cx.notify();
    }

    pub(super) fn close_automation_editor(&mut self, cx: &mut Context<Self>) {
        self.automations.editor = None;
        cx.notify();
    }

    pub(super) fn forget_automation_selection(&mut self, kraddie: &KraddieId) {
        if self
            .editing_automation()
            .is_some_and(|key| &key.kraddie == kraddie)
        {
            self.automations.editor = None;
        }
    }

    pub(super) fn editing_automation(&self) -> Option<&AutomationKey> {
        self.automations
            .editor
            .as_ref()
            .and_then(|editor| editor.editing())
    }

    fn automation_next_in(
        &self,
        kraddie: &super::kraddies::KraddieEntry,
        entry: &super::kraddies::AutomationEntry,
    ) -> Option<Duration> {
        let at = super::kraddies::next_run(kraddie, entry, self.kraddies.opened_at())?;
        let at = SystemTime::from(at);
        Some(at.duration_since(SystemTime::now()).unwrap_or_default())
    }

    fn soonest_countdown(&self) -> Option<Duration> {
        self.kraddies
            .entries
            .iter()
            .flat_map(|kraddie| {
                kraddie
                    .automations
                    .iter()
                    .map(move |entry| (kraddie, entry))
            })
            .filter_map(|(kraddie, entry)| self.automation_next_in(kraddie, entry))
            .min()
    }

    fn keep_countdowns_ticking(&mut self, cx: &mut Context<Self>) {
        if self.automations.ticker.is_some() {
            return;
        }
        let Some(soonest) = self.soonest_countdown() else {
            return;
        };
        let wait = wording::countdown_tick(soonest);
        self.automations.ticker = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |shell, cx| {
                shell.automations.ticker = None;
                if shell.active_view() == EnvironmentView::Automations {
                    cx.notify();
                }
            })
            .unwrap_or_default();
        }));
    }

    fn refresh_code_host_logins(&mut self, cx: &mut Context<Self>) {
        let gitlab = GitlabConfig::try_from(&self.settings.gitlab).ok();
        let read = cx.background_spawn(async move {
            let github = read_github_login()
                .map_err(|error| tracing::debug!(%error, "GitHub login not read"))
                .is_ok_and(|login| login.is_some());
            let gitlab = gitlab.is_some_and(|config| {
                read_gitlab_login(&config.host)
                    .map_err(|error| tracing::debug!(%error, "GitLab login not read"))
                    .is_ok_and(|login| login.is_some())
            });
            CodeHostLogins { github, gitlab }
        });
        cx.spawn(async move |this, cx| {
            let logins = read.await;
            this.update(cx, |shell, cx| {
                shell.automations.logins = logins;
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn run_automation_menu(
        &mut self,
        choice: AutomationMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = &mut self.automations.editor else {
            return;
        };
        editor.apply(choice, window, cx);
        cx.notify();
    }
}
