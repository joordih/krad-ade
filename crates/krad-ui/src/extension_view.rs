mod collections;
mod form;
mod layout;
mod model;
mod slider;
mod style;

use futures::channel::mpsc::UnboundedSender;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    AnyView, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, UniformListScrollHandle, Window,
};
use krad_extensions::api::protocol::RowsResult;
use krad_extensions::api::view::{Element, FieldKind, Node, Patch, Sort, ViewEvent};
use krad_extensions::api::{NodeKey, TerminalHandle};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::theme::palette;
pub(crate) use form::InputSlot;
use model::ViewModel;
use slider::SliderView;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ViewOutput {
    Event(ViewEvent),
    Rows { node: NodeKey, start: u32, end: u32 },
}

pub(crate) struct ExtensionView {
    model: ViewModel,
    terminals: HashMap<TerminalHandle, AnyView>,
    output: UnboundedSender<ViewOutput>,
    folder: Option<PathBuf>,
    inputs: HashMap<InputSlot, Entity<InputState>>,
    fields: HashMap<InputSlot, form::LocalValue>,
    sorts: HashMap<NodeKey, Sort>,
    streams: HashMap<NodeKey, UniformListScrollHandle>,
    sliders: HashMap<NodeKey, Entity<SliderView>>,
    inputs_stale: bool,
    subscriptions: Vec<Subscription>,
}

impl ExtensionView {
    pub(crate) fn new(root: Node, output: UnboundedSender<ViewOutput>) -> Self {
        Self {
            model: ViewModel::new(root),
            terminals: HashMap::new(),
            output,
            folder: None,
            inputs: HashMap::new(),
            fields: HashMap::new(),
            sorts: HashMap::new(),
            streams: HashMap::new(),
            sliders: HashMap::new(),
            inputs_stale: true,
            subscriptions: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn with_folder(mut self, folder: PathBuf) -> Self {
        self.folder = Some(folder);
        self
    }

    pub(crate) fn replace_root(&mut self, root: Node, cx: &mut Context<Self>) {
        self.model.replace_root(root);
        self.inputs.clear();
        self.fields.clear();
        self.sorts.clear();
        self.streams.clear();
        self.sliders.clear();
        self.subscriptions.clear();
        self.inputs_stale = true;
        self.sync_sliders(cx);
        cx.notify();
    }

    pub(crate) fn apply(&mut self, patches: Vec<Patch>, cx: &mut Context<Self>) {
        self.model.apply(patches);
        for stream in self.model.take_appended() {
            let Some(scroll) = self.streams.get(&stream) else {
                continue;
            };
            if scroll.is_scrolled_to_end().unwrap_or(true) {
                scroll.scroll_to_bottom();
            }
        }
        self.inputs_stale = true;
        self.sync_sliders(cx);
        self.prune_sliders();
        cx.notify();
    }

    fn sync_sliders(&mut self, cx: &mut Context<Self>) {
        for key in self.model.take_received_sliders() {
            let Some(Element::Slider(spec)) =
                model::find(&self.model.root, &key).map(|node| &node.element)
            else {
                continue;
            };
            let spec = spec.clone();
            if let Some(slider) = self.sliders.get(&key) {
                slider.update(cx, |slider, cx| slider.receive(spec, cx));
            } else {
                let output = self.output.clone();
                let slider = cx.new(|cx| SliderView::new(key.clone(), spec, output, cx));
                self.sliders.insert(key, slider);
            }
        }
    }

    fn prune_sliders(&mut self) {
        let mut present = HashSet::new();
        model::slider_keys(&self.model.root, &mut present);
        self.sliders.retain(|key, _| present.contains(key));
    }

    pub(crate) fn set_rows(&mut self, node: NodeKey, rows: RowsResult, cx: &mut Context<Self>) {
        self.model.set_rows(node, rows);
        cx.notify();
    }

    pub(crate) fn set_terminal(
        &mut self,
        handle: TerminalHandle,
        view: AnyView,
        cx: &mut Context<Self>,
    ) {
        self.terminals.insert(handle, view);
        cx.notify();
    }

    fn send(&self, output: ViewOutput) {
        if self.output.unbounded_send(output).is_err() {
            tracing::trace!("extension view output dropped after its program stopped");
        }
    }

    fn emit(&self, event: ViewEvent) {
        self.send(ViewOutput::Event(event));
    }

    fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    fn prepare_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.inputs_stale {
            return;
        }
        self.inputs_stale = false;
        let mut streams = Vec::new();
        model::stream_keys(&self.model.root, &mut streams);
        for stream in streams {
            self.streams.entry(stream).or_default();
        }
        let mut wanted = Vec::new();
        collect_inputs(&self.model.root, &mut wanted);
        for (slot, initial) in wanted {
            if self.inputs.contains_key(&slot) {
                continue;
            }
            let field = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(initial.value)
                    .placeholder(initial.placeholder)
                    .masked(initial.masked)
            });
            if let InputSlot::Node(_) = &slot {
                let watched = slot.clone();
                let subscription = cx.subscribe_in(
                    &field,
                    window,
                    move |this, state, event: &InputEvent, _, cx| {
                        this.input_event(&watched, state, event, cx);
                    },
                );
                self.subscriptions.push(subscription);
            }
            self.inputs.insert(slot, field);
        }
    }

    fn input_event(
        &mut self,
        slot: &InputSlot,
        state: &Entity<InputState>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        let InputSlot::Node(key) = slot else {
            return;
        };
        let Some(action) = form::input_action(&self.model.root, key) else {
            return;
        };
        let text = state.read(cx).value().to_string();
        match event {
            InputEvent::Change => self.emit(ViewEvent::Input { action, text }),
            InputEvent::PressEnter { shift: false, .. } => {
                self.emit(ViewEvent::Submit { action, text });
            }
            InputEvent::PressEnter { .. } | InputEvent::Focus | InputEvent::Blur => {}
        }
    }
}

struct InitialInput {
    value: String,
    placeholder: String,
    masked: bool,
}

fn collect_inputs(node: &Node, wanted: &mut Vec<(InputSlot, InitialInput)>) {
    match &node.element {
        Element::Input {
            value,
            placeholder,
            kind,
            ..
        } => wanted.push((
            InputSlot::Node(node.key.clone()),
            InitialInput {
                value: value.clone(),
                placeholder: placeholder.clone().unwrap_or_default(),
                masked: form::is_secret(*kind),
            },
        )),
        Element::Form { fields, .. } => {
            for field in fields {
                let initial = match &field.kind {
                    FieldKind::Text {
                        value,
                        placeholder,
                        kind,
                    } => InitialInput {
                        value: value.clone(),
                        placeholder: placeholder.clone().unwrap_or_default(),
                        masked: form::is_secret(*kind),
                    },
                    FieldKind::Number { value } => InitialInput {
                        value: value.to_string(),
                        placeholder: String::new(),
                        masked: false,
                    },
                    FieldKind::Toggle { .. } | FieldKind::Select { .. } => continue,
                };
                wanted.push((
                    InputSlot::Field {
                        form: node.key.clone(),
                        field: field.key.clone(),
                    },
                    initial,
                ));
            }
        }
        _ => {}
    }
    for child in node.children() {
        collect_inputs(child, wanted);
    }
}

impl Render for ExtensionView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_inputs(window, cx);
        self.sync_sliders(cx);
        let pal = palette(cx);
        gpui_kit::div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(self.element(&self.model.root, pal, cx))
    }
}

#[cfg(test)]
mod tests;
