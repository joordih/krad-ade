use gpui_kit::component::input::Textarea;
use gpui_kit::{div, px, AnyElement, ClickEvent, Context, IntoElement, ParentElement, Styled};
use krad_domain::{AgentPreset, AutomationDraft, Effort, PermissionMode};

use super::super::super::ask::mode_label;
use super::super::super::context_menu::{MenuAction, MenuEntry, MenuPlacement};
use super::super::super::EnvironmentShell;
use super::super::AutomationMenu;
use super::parts::{chips, click_origin, hint};
use super::AutomationEditor;
use crate::components::{harness_icon, Button, IconName};
use crate::theme::{Palette, SPACE_2};

const BRIEF_HINT: &str =
    "Skills, @file mentions and slash commands such as /review behave just as they do in a chat.";
const OWN_AGENT: &str = "The Kraddie's own agent";
const USUAL_EFFORT: &str = "Usual effort";
const USUAL_PERMISSIONS: &str = "Usual permissions";
const BTN_HARNESS: &str = "automation-harness";
const BTN_EFFORT: &str = "automation-effort";
const BTN_MODE: &str = "automation-mode";
const MODES: [PermissionMode; 6] = [
    PermissionMode::Manual,
    PermissionMode::AcceptEdits,
    PermissionMode::Plan,
    PermissionMode::Auto,
    PermissionMode::DontAsk,
    PermissionMode::BypassPermissions,
];

fn choice(label: impl Into<String>, checked: bool, pick: AutomationMenu) -> MenuEntry {
    MenuEntry::checked(label, checked, MenuAction::Automation(pick))
}

impl EnvironmentShell {
    pub(super) fn brief_step(
        &self,
        editor: &AutomationEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let harness = editor.draft.harness.map_or_else(
            || OWN_AGENT.to_owned(),
            |preset| self.settings.acp.harness_name(preset).into_owned(),
        );
        let glyph = editor
            .draft
            .harness
            .map_or(IconName::UserGroup, |preset| harness_icon(preset, pal).0);
        let effort = editor.draft.effort.map_or(USUAL_EFFORT, Effort::label);
        let mode = editor
            .draft
            .permission_mode
            .map_or(USUAL_PERMISSIONS, mode_label);
        let menu_button =
            |id: &'static str,
             label: String,
             glyph: IconName,
             open: fn(&mut Self, (f32, f32), &mut Context<Self>)| {
                Button::new(id)
                    .label(label)
                    .icon(glyph)
                    .small()
                    .on_click(
                        cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                            open(this, click_origin(event), cx);
                        }),
                    )
            };
        let controls = chips()
            .child(menu_button(
                BTN_HARNESS,
                harness,
                glyph,
                Self::open_harness_choice,
            ))
            .child(menu_button(
                BTN_EFFORT,
                effort.to_owned(),
                IconName::Flash,
                Self::open_effort_choice,
            ))
            .child(menu_button(
                BTN_MODE,
                mode.to_owned(),
                IconName::Lock,
                Self::open_mode_choice,
            ));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(Textarea::new(&editor.brief))
            .child(controls)
            .child(hint(BRIEF_HINT, pal))
            .into_any_element()
    }

    fn editor_draft(&self) -> Option<&AutomationDraft> {
        self.automations.editor.as_ref().map(|editor| &editor.draft)
    }

    fn open_harness_choice(&mut self, origin: (f32, f32), cx: &mut Context<Self>) {
        let agents = self.kraddie_agents();
        let current: Option<AgentPreset> = self
            .editor_draft()
            .and_then(|draft| draft.harness)
            .map(|harness| harness.as_offered(&agents, self.settings.offered_twin));
        let mut entries = vec![choice(
            OWN_AGENT,
            current.is_none(),
            AutomationMenu::Harness(None),
        )];
        entries.extend(agents.into_iter().map(|preset| {
            choice(
                self.settings.acp.harness_name(preset),
                current == Some(preset),
                AutomationMenu::Harness(Some(preset)),
            )
        }));
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    fn open_effort_choice(&mut self, origin: (f32, f32), cx: &mut Context<Self>) {
        let current = self.editor_draft().and_then(|draft| draft.effort);
        let mut entries = vec![choice(
            USUAL_EFFORT,
            current.is_none(),
            AutomationMenu::Effort(None),
        )];
        entries.extend(Effort::ALL.into_iter().map(|effort| {
            choice(
                effort.label(),
                current == Some(effort),
                AutomationMenu::Effort(Some(effort)),
            )
        }));
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }

    fn open_mode_choice(&mut self, origin: (f32, f32), cx: &mut Context<Self>) {
        let current = self.editor_draft().and_then(|draft| draft.permission_mode);
        let mut entries = vec![choice(
            USUAL_PERMISSIONS,
            current.is_none(),
            AutomationMenu::Permission(None),
        )];
        entries.extend(MODES.into_iter().map(|mode| {
            choice(
                mode_label(mode),
                current == Some(mode),
                AutomationMenu::Permission(Some(mode)),
            )
        }));
        self.open_menu(origin, MenuPlacement::Below, entries, cx);
    }
}
