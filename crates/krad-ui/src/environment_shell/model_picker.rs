mod view;

use gpui_kit::{Context, FocusHandle, KeyDownEvent, Window};
use krad_chat::ModelSource;
use krad_domain::{AgentPreset, ModelId, TerminalId};

use super::surface::SurfaceCommand;
use super::EnvironmentShell;

const KEY_ESCAPE: &str = "escape";
const KEY_BACKSPACE: &str = "backspace";
const KEY_ENTER: &str = "enter";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";
const KEY_LEFT: &str = "left";
const KEY_RIGHT: &str = "right";
const KEY_TAB: &str = "tab";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ModelChoice {
    pub(super) id: Option<ModelId>,
    pub(super) label: String,
    pub(super) current: bool,
    pub(super) section: Option<ModelSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HarnessModels {
    pub(super) harness: AgentPreset,
    pub(super) models: Vec<ModelChoice>,
}

pub(super) fn grouped(
    picks: impl IntoIterator<Item = (AgentPreset, ModelChoice)>,
) -> Vec<HarnessModels> {
    let mut groups: Vec<HarnessModels> = Vec::new();
    for (harness, choice) in picks {
        match groups.iter_mut().find(|group| group.harness == harness) {
            Some(group) => group.models.push(choice),
            None => groups.push(HarnessModels {
                harness,
                models: vec![choice],
            }),
        }
    }
    groups
}

#[derive(Debug, PartialEq, Eq)]
struct Choosing {
    groups: Vec<HarnessModels>,
    harness: AgentPreset,
    query: String,
    highlighted: usize,
}

impl Choosing {
    fn new(groups: Vec<HarnessModels>) -> Option<Self> {
        let harness = groups
            .iter()
            .find(|group| group.models.iter().any(|choice| choice.current))
            .or(groups.first())?
            .harness;
        let mut choosing = Self {
            groups,
            harness,
            query: String::new(),
            highlighted: 0,
        };
        choosing.show_harness(harness);
        Some(choosing)
    }

    fn shown(&self) -> Vec<&ModelChoice> {
        let query = self.query.trim().to_lowercase();
        let mut shown: Vec<&ModelChoice> = self
            .groups
            .iter()
            .find(|group| group.harness == self.harness)
            .map_or_default(|group| {
                group
                    .models
                    .iter()
                    .filter(|choice| {
                        query.is_empty()
                            || choice.label.to_lowercase().contains(&query)
                            || choice
                                .id
                                .as_ref()
                                .is_some_and(|id| id.as_str().to_lowercase().contains(&query))
                    })
                    .collect()
            });
        shown.sort_by_key(|choice| choice.section.as_ref().map(ModelSource::rank));
        shown
    }

    fn show_harness(&mut self, harness: AgentPreset) {
        self.harness = harness;
        self.query.clear();
        self.highlighted = self
            .shown()
            .iter()
            .position(|choice| choice.current)
            .unwrap_or_default();
    }

    fn step_harness(&mut self, forward: bool) {
        let count = self.groups.len();
        let Some(at) = self
            .groups
            .iter()
            .position(|group| group.harness == self.harness)
        else {
            return;
        };
        let next = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        self.show_harness(self.groups[next].harness);
    }

    fn step_row(&mut self, down: bool) {
        let count = self.shown().len();
        if count == 0 {
            return;
        }
        self.highlighted = if down {
            (self.highlighted + 1) % count
        } else {
            (self.highlighted + count - 1) % count
        };
    }

    fn type_text(&mut self, typed: &str) {
        self.query.push_str(typed);
        self.highlighted = 0;
    }

    fn erase(&mut self) {
        self.query.pop();
        self.highlighted = 0;
    }

    fn picked(&self, index: usize) -> Option<(AgentPreset, Option<ModelId>)> {
        let choice = self.shown().get(index).map(|choice| choice.id.clone())?;
        Some((self.harness, choice))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PickFor {
    Chat(TerminalId),
    Docs,
}

pub(super) struct ModelPicker {
    target: PickFor,
    origin: (f32, f32),
    choosing: Choosing,
    focus: FocusHandle,
    focus_pending: bool,
}

impl EnvironmentShell {
    pub(super) fn open_model_picker(
        &mut self,
        target: PickFor,
        origin: (f32, f32),
        groups: Vec<HarnessModels>,
        cx: &mut Context<Self>,
    ) {
        if groups
            .iter()
            .any(|group| group.harness == AgentPreset::Opencode)
        {
            self.refresh_opencode_models(cx);
        }
        let Some(choosing) = Choosing::new(groups) else {
            return;
        };
        self.model_picker = Some(ModelPicker {
            target,
            origin,
            choosing,
            focus: cx.focus_handle(),
            focus_pending: true,
        });
        cx.notify();
    }

    fn close_model_picker(&mut self, cx: &mut Context<Self>) {
        self.model_picker = None;
        cx.notify();
    }

    fn pick_model(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(picker) = self.model_picker.take() else {
            return;
        };
        let Some((harness, model)) = picker.choosing.picked(index) else {
            return cx.notify();
        };
        match (picker.target, model) {
            (PickFor::Chat(chat), Some(model)) => {
                self.command_surface(chat, SurfaceCommand::Model(harness, model), cx);
            }
            (PickFor::Chat(_), None) => {}
            (PickFor::Docs, model) => self.pick_docs_agent(harness, model),
        }
        cx.notify();
    }

    fn show_picker_harness(&mut self, harness: AgentPreset, cx: &mut Context<Self>) {
        if let Some(picker) = self.model_picker.as_mut() {
            picker.choosing.show_harness(harness);
        }
        cx.notify();
    }

    fn highlight_model(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(picker) = self.model_picker.as_mut() {
            if picker.choosing.highlighted != index {
                picker.choosing.highlighted = index;
                cx.notify();
            }
        }
    }

    fn on_model_picker_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let Some(choosing) = self
            .model_picker
            .as_mut()
            .map(|picker| &mut picker.choosing)
        else {
            return;
        };
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            KEY_ESCAPE => return self.close_model_picker(cx),
            KEY_ENTER => {
                let index = choosing.highlighted;
                return self.pick_model(index, cx);
            }
            KEY_UP => choosing.step_row(false),
            KEY_DOWN => choosing.step_row(true),
            KEY_LEFT => choosing.step_harness(false),
            KEY_RIGHT => choosing.step_harness(true),
            KEY_TAB => choosing.step_harness(!modifiers.shift),
            KEY_BACKSPACE => choosing.erase(),
            _ => {
                if let Some(typed) = event
                    .keystroke
                    .key_char
                    .as_ref()
                    .filter(|_| !modifiers.platform && !modifiers.control)
                {
                    choosing.type_text(typed);
                }
            }
        }
        cx.notify();
    }

    pub(super) fn sync_model_picker_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self
            .model_picker
            .as_mut()
            .filter(|picker| picker.focus_pending)
        {
            picker.focus_pending = false;
            window.focus(&picker.focus, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(id: &str, label: &str, current: bool) -> ModelChoice {
        ModelChoice {
            id: Some(ModelId::parse(id).unwrap_or_else(|| unreachable!("{id} is a model id"))),
            label: label.to_owned(),
            current,
            section: None,
        }
    }

    fn sold(id: &str, label: &str, section: ModelSource) -> (AgentPreset, ModelChoice) {
        (
            AgentPreset::Opencode,
            ModelChoice {
                section: Some(section),
                ..choice(id, label, false)
            },
        )
    }

    fn plan(name: &str, order: usize) -> ModelSource {
        ModelSource::Plan {
            name: name.to_owned(),
            order,
        }
    }

    #[test]
    fn models_show_by_plan_order_then_providers_then_free() {
        let picker = Choosing::new(grouped([
            sold("opencode/beam-free", "Beam", ModelSource::Free),
            sold("opencode/claude-opus-5-5", "Opus 5.5", plan("Zen", 1)),
            sold(
                "anthropic/claude-sonnet-5-5",
                "Sonnet 5.5",
                ModelSource::Provider("Anthropic".to_owned()),
            ),
            sold(
                "openai/gpt-6",
                "GPT 6",
                ModelSource::Provider("OpenAI".to_owned()),
            ),
            sold("opencode-go/kimi-k3", "Kimi K3", plan("Go", 0)),
        ]))
        .unwrap_or_else(|| unreachable!("there are models"));
        let shown: Vec<&str> = picker
            .shown()
            .iter()
            .map(|choice| choice.label.as_str())
            .collect();
        assert_eq!(
            shown,
            ["Kimi K3", "Opus 5.5", "Sonnet 5.5", "GPT 6", "Beam"]
        );
    }

    fn groups() -> Vec<HarnessModels> {
        grouped([
            (AgentPreset::ClaudeCode, choice("default", "Default", false)),
            (
                AgentPreset::Opencode,
                choice("anthropic/claude-opus-5-5", "Opus 5.5", true),
            ),
            (AgentPreset::ClaudeCode, choice("opus", "Opus 5.5", false)),
            (
                AgentPreset::Opencode,
                choice("opencode/big-pickle", "Big Pickle", false),
            ),
        ])
    }

    #[test]
    fn models_gather_under_their_harness_in_the_order_they_came() {
        let groups = groups();
        let harnesses: Vec<AgentPreset> = groups.iter().map(|group| group.harness).collect();
        assert_eq!(harnesses, [AgentPreset::ClaudeCode, AgentPreset::Opencode]);
        assert_eq!(groups[0].models.len(), 2);
        assert_eq!(groups[1].models[1].label, "Big Pickle");
    }

    #[test]
    fn the_picker_opens_on_the_current_model_filters_and_cycles_harnesses() {
        let mut picker =
            Choosing::new(groups()).unwrap_or_else(|| unreachable!("there are models"));
        assert_eq!(picker.harness, AgentPreset::Opencode);
        assert_eq!(picker.highlighted, 0);
        picker.type_text("pick");
        let shown: Vec<&str> = picker
            .shown()
            .iter()
            .map(|choice| choice.label.as_str())
            .collect();
        assert_eq!(shown, ["Big Pickle"]);
        assert_eq!(
            picker.picked(0).map(|(harness, _)| harness),
            Some(AgentPreset::Opencode)
        );
        picker.step_harness(true);
        assert_eq!(picker.harness, AgentPreset::ClaudeCode);
        assert_eq!(picker.query, "");
        picker.step_row(false);
        assert_eq!(picker.highlighted, 1);
        assert_eq!(Choosing::new(Vec::new()), None);
    }
}
