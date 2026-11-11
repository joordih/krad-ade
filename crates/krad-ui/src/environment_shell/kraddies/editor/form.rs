use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, Div, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{AgentPreset, KraddieWorkspace};

use super::super::super::dialog_parts::{
    field_label, segment, sheet_body, sheet_footer, sheet_overlay, sheet_panel, sheet_title,
    SegmentSize,
};
use super::super::super::harness_models::AcpListing;
use super::super::super::model_picker::{mark, HarnessModels, ModelChoice};
use super::super::super::EnvironmentShell;
use super::super::motion::{entering, shaking};
use super::KraddieEditor;
use crate::components::{harness_icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_1, SPACE_2, TEXT_XS};

const SHEET_W: f32 = 600.0;
const NEW_TITLE: &str = "Hatch a Kraddie";
const EDIT_TITLE: &str = "Rename or retrain";
const BTN_CLOSE: &str = "kraddie-editor-close";
const BTN_SAVE: &str = "kraddie-editor-save";
const BTN_CANCEL: &str = "kraddie-editor-cancel";
const BODY_ID: &str = "kraddie-editor-body";
const SAVE_LABEL: &str = "Save";
const HATCH_LABEL: &str = "Hatch";
const IDEAS_LABEL: &str = "Need an idea?";
const SHUFFLE_BTN: &str = "kraddie-editor-shuffle";
const SHUFFLE_TOOLTIP: &str = "Other names";
const IDEA_PREFIX: &str = "kraddie-name-idea";
const REFUSAL_ID: &str = "kraddie-editor-refusal";
const CANCEL_LABEL: &str = "Cancel";
const NAME_LABEL: &str = "Name";
const AGENT_LABEL: &str = "Agent";
const MODEL_LABEL: &str = "Model";
const WORKSPACE_LABEL: &str = "Home, where each run gets a fresh worktree";
const NO_WORKSPACE: &str = "Open a group of a git repository so it has a home to work in";
const OTHER_WORKSPACE_PREFIX: &str = "Another environment: ";
const AGENT_PREFIX: &str = "kraddie-agent";
const GROUP_PREFIX: &str = "kraddie-group";
const MODEL_PREFIX: &str = "kraddie-model";
const MAX_MODEL_CHIPS: usize = 12;
const DEFAULT_SUFFIX: &str = " · default";
const LISTING_MODELS: &str = "Asking the agent for its models…";
const NO_MODELS: &str = "The agent offers no models to choose from";
const MODELS_FAILED: &str = "Its models could not be listed";

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.close_kraddie_editor(cx);
}

fn chips() -> Div {
    div().flex().flex_row().flex_wrap().gap(px(SPACE_1))
}

fn edit(
    this: &mut EnvironmentShell,
    cx: &mut Context<EnvironmentShell>,
    change: impl FnOnce(&mut KraddieEditor),
) {
    if let Some(editor) = &mut this.kraddies.editor {
        change(editor);
    }
    cx.notify();
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn kraddie_editor_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.kraddies.editor.as_ref()?;
        let pal = palette(cx);
        let title = if editor.replacing.is_some() {
            EDIT_TITLE
        } else {
            NEW_TITLE
        };
        let body = sheet_body(BODY_ID).child(
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .child(field_label(NAME_LABEL, pal))
                .child(Input::new(&editor.name))
                .children(Self::refusal_note(editor, pal))
                .children(Self::name_idea_chips(editor, pal, cx))
                .child(field_label(AGENT_LABEL, pal))
                .child(chips().children(self.kraddie_agent_chips(editor, pal, cx)))
                .child(field_label(MODEL_LABEL, pal))
                .child(Input::new(&editor.model))
                .children(self.kraddie_model_chips(editor, pal, cx))
                .children(self.kraddie_model_note(editor, pal))
                .child(field_label(WORKSPACE_LABEL, pal))
                .child(self.kraddie_workspace_chips(editor, pal, cx)),
        );
        let footer = sheet_footer(pal)
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.close_kraddie_editor(cx)),
                    ),
            )
            .child(
                Button::new(BTN_SAVE)
                    .label(if editor.replacing.is_some() {
                        SAVE_LABEL
                    } else {
                        HATCH_LABEL
                    })
                    .variant(ButtonVariant::Primary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, _, cx| this.save_kraddie_editor(cx)),
                    ),
            );
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(title, None, BTN_CLOSE, close, cx))
            .child(body)
            .child(footer);
        Some(sheet_overlay(sheet, close, cx))
    }

    fn refusal_note(editor: &KraddieEditor, pal: &Palette) -> Option<AnyElement> {
        let reason = editor.refusal.clone()?;
        let note = div()
            .text_size(px(TEXT_XS))
            .text_color(rgb(pal.error_fg))
            .child(reason);
        Some(shaking(note, (REFUSAL_ID, editor.refusals)).into_any_element())
    }

    fn name_idea_chips(
        editor: &KraddieEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if editor.replacing.is_some() || editor.ideas.is_empty() {
            return None;
        }
        let shuffles = usize::try_from(editor.shuffles).unwrap_or_default();
        let ideas = editor.ideas.iter().enumerate().map(|(index, idea)| {
            let idea = *idea;
            let chip = segment(
                (IDEA_PREFIX, index),
                idea,
                None,
                false,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.pick_name_idea(idea, window, cx);
            }));
            entering(
                chip,
                (
                    SharedString::from(format!("{IDEA_PREFIX}-{shuffles}")),
                    index,
                ),
                index,
            )
            .into_any_element()
        });
        Some(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(SPACE_1))
                .child(
                    div()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_muted))
                        .child(IDEAS_LABEL),
                )
                .children(ideas)
                .child(
                    Button::new(SHUFFLE_BTN)
                        .icon(IconName::Refresh)
                        .small()
                        .tooltip(SHUFFLE_TOOLTIP)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.shuffle_name_ideas(cx);
                        })),
                )
                .into_any_element(),
        )
    }

    fn kraddie_agent_chips(
        &self,
        editor: &KraddieEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let agents = self.kraddie_agents();
        let chosen = editor
            .harness
            .as_offered(&agents, self.settings.offered_twin);
        agents
            .into_iter()
            .enumerate()
            .map(|(index, preset)| {
                let (glyph, _) = harness_icon(preset, pal);
                segment(
                    (AGENT_PREFIX, index),
                    self.settings.acp.harness_name(preset),
                    Some(glyph),
                    chosen == preset,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.choose_kraddie_harness(preset, cx);
                }))
                .into_any_element()
            })
            .collect()
    }

    fn kraddie_model_chips(
        &self,
        editor: &KraddieEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let typed = editor.model.read(cx).value().trim().to_owned();
        let models = self.kraddie_model_rows(editor.harness);
        if models.is_empty() {
            return None;
        }
        let picks = models
            .into_iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                let model = choice.id?;
                let selected = model.as_str() == typed;
                let label = if choice.marks.default {
                    format!("{}{DEFAULT_SUFFIX}", choice.label)
                } else {
                    choice.label
                };
                let chip = segment(
                    (MODEL_PREFIX, index),
                    SharedString::from(label),
                    choice.marks.favorite.then_some(IconName::StarFilled),
                    selected,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.pick_kraddie_model(&model, window, cx);
                }));
                Some(chip.into_any_element())
            });
        Some(chips().children(picks).into_any_element())
    }

    fn kraddie_model_rows(&self, harness: AgentPreset) -> Vec<ModelChoice> {
        let mut group = HarnessModels {
            harness,
            name: self.settings.acp.harness_name(harness).into(),
            models: self
                .model_choices(harness, None)
                .into_iter()
                .filter(|choice| choice.id.is_some())
                .collect(),
        };
        mark(&mut group, &self.settings.model_preferences);
        group
            .models
            .sort_by_key(|choice| (!choice.marks.favorite, !choice.marks.default));
        group.models.truncate(MAX_MODEL_CHIPS);
        group.models
    }

    fn kraddie_model_note(&self, editor: &KraddieEditor, pal: &Palette) -> Option<AnyElement> {
        let AgentPreset::Acp(agent) = editor.harness else {
            return None;
        };
        let host = self.active_link().host();
        let text = match self.listed_models.acp_listing(&host, agent)? {
            AcpListing::Listing => LISTING_MODELS.to_owned(),
            AcpListing::NoModels => NO_MODELS.to_owned(),
            AcpListing::Failed(reason) => format!("{MODELS_FAILED}: {reason}"),
        };
        Some(
            div()
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.text_muted))
                .child(text)
                .into_any_element(),
        )
    }

    fn kraddie_workspace_chips(
        &self,
        editor: &KraddieEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let env = self.active.clone();
        let groups: Vec<(String, KraddieWorkspace)> = self
            .envs
            .get(&env)
            .map_or_default(|live| live.data.groups.as_slice())
            .iter()
            .map(|group| {
                let repo = self
                    .git_at(&env, &group.root)
                    .map_or_else(|| group.root.clone(), |git| git.repo_root.clone());
                let workspace = KraddieWorkspace {
                    environment: env.clone(),
                    repo,
                };
                (group.name.clone(), workspace)
            })
            .collect();
        let elsewhere = editor
            .workspace
            .as_ref()
            .filter(|workspace| workspace.environment != env)
            .map(|workspace| {
                format!(
                    "{OTHER_WORKSPACE_PREFIX}{} · {}",
                    workspace.environment.as_str(),
                    workspace.repo.display()
                )
            });
        let empty = groups.is_empty();
        let chosen: Vec<AnyElement> = groups
            .into_iter()
            .enumerate()
            .map(|(index, (name, workspace))| {
                let selected = editor.workspace.as_ref() == Some(&workspace);
                segment(
                    (GROUP_PREFIX, index),
                    SharedString::from(name),
                    None,
                    selected,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let workspace = workspace.clone();
                    edit(this, cx, |editor| editor.workspace = Some(workspace));
                }))
                .into_any_element()
            })
            .collect();
        let note = |text: String| {
            div()
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.text_muted))
                .child(text)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(chips().children(chosen))
            .children(elsewhere.map(note))
            .when(empty, |column| column.child(note(NO_WORKSPACE.to_owned())))
            .into_any_element()
    }
}
