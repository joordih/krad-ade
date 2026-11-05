use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_extensions::api::view::{
    Choice, Element, Field, FieldKind, FieldValue, InputKind, Node, StreamLine, ViewEvent,
};
use krad_extensions::api::{ActionId, NodeKey};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::model::find;
use super::ExtensionView;
use crate::components::{Button, ButtonVariant};
use crate::theme::{Palette, INPUT_H, LINE_XS, RADIUS_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_XS};

const SUBMIT_LABEL: &str = "Submit";
const FIELD_PREFIX: &str = "ext-field";
const SUBMIT_PREFIX: &str = "ext-submit";
const CHOICE_PREFIX: &str = "ext-choice";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum InputSlot {
    Node(NodeKey),
    Field { form: NodeKey, field: NodeKey },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocalValue {
    Toggle(bool),
    Choice(String),
}

pub(super) const fn is_secret(kind: InputKind) -> bool {
    matches!(kind, InputKind::Secret)
}

pub(super) fn input_action(root: &Node, key: &NodeKey) -> Option<ActionId> {
    match &find(root, key)?.element {
        Element::Input { action, .. } => Some(action.clone()),
        _ => None,
    }
}

pub(super) fn stream_lines<'a>(root: &'a Node, key: &NodeKey) -> Option<&'a [StreamLine]> {
    match &find(root, key)?.element {
        Element::Stream { lines, .. } => Some(lines),
        _ => None,
    }
}

pub(super) fn element_id(prefix: &'static str, key: &NodeKey) -> ElementId {
    ElementId::NamedChild(
        Arc::new(ElementId::Name(SharedString::new_static(prefix))),
        SharedString::from(key.to_string()),
    )
}

pub(super) fn indexed_id(parent: ElementId, index: usize) -> ElementId {
    ElementId::NamedChild(Arc::new(parent), SharedString::from(index.to_string()))
}

impl ExtensionView {
    pub(super) fn input_element(&self, key: &NodeKey) -> AnyElement {
        self.inputs.get(&InputSlot::Node(key.clone())).map_or_else(
            || div().h(px(INPUT_H)).into_any_element(),
            |state| Input::new(state).into_any_element(),
        )
    }

    pub(super) fn choice_row(
        id_key: &NodeKey,
        options: &[Choice],
        value: Option<&str>,
        pal: &Palette,
        cx: &mut Context<Self>,
        on_choose: impl Fn(&mut Self, String, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(SPACE_1))
            .children(options.iter().enumerate().map(|(index, choice)| {
                let chosen = value == Some(choice.value.as_str());
                let picked = choice.value.clone();
                let handler = on_choose.clone();
                div()
                    .id(indexed_id(element_id(CHOICE_PREFIX, id_key), index))
                    .px(px(SPACE_2))
                    .py(px(SPACE_1))
                    .rounded(px(RADIUS_XS))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(if chosen {
                        pal.on_accent
                    } else {
                        pal.text_default
                    }))
                    .bg(if chosen {
                        rgb(pal.accent).into()
                    } else {
                        gpui_kit::Hsla::from(pal.bg_hover)
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        handler(this, picked.clone(), cx);
                    }))
                    .child(choice.label.clone())
            }))
            .into_any_element()
    }

    pub(super) fn form_element(
        &self,
        node: &Node,
        submit: &ActionId,
        fields: &[Field],
        submit_label: Option<&str>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let form = node.key.clone();
        let rows = fields
            .iter()
            .map(|field| self.field_row(&form, field, pal, cx))
            .collect::<Vec<_>>();
        let action = submit.clone();
        let submitted_form = form.clone();
        let submitted_fields = fields.to_vec();
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .children(rows)
            .child(
                Button::new(element_id(SUBMIT_PREFIX, &form))
                    .label(SharedString::from(
                        submit_label.unwrap_or(SUBMIT_LABEL).to_owned(),
                    ))
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let values = this.form_values(&submitted_form, &submitted_fields, cx);
                        this.emit(ViewEvent::Form {
                            action: action.clone(),
                            values,
                        });
                    })),
            )
            .into_any_element()
    }

    fn field_row(
        &self,
        form: &NodeKey,
        field: &Field,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slot = InputSlot::Field {
            form: form.clone(),
            field: field.key.clone(),
        };
        let control = match &field.kind {
            FieldKind::Text { .. } | FieldKind::Number { .. } => {
                self.inputs.get(&slot).map_or_else(
                    || div().h(px(INPUT_H)).into_any_element(),
                    |state| Input::new(state).into_any_element(),
                )
            }
            FieldKind::Toggle { value } => {
                let current = match self.fields.get(&slot) {
                    Some(LocalValue::Toggle(local)) => *local,
                    _ => *value,
                };
                let toggled = slot.clone();
                Switch::new(element_id(FIELD_PREFIX, &field.key))
                    .checked(current)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        this.fields
                            .insert(toggled.clone(), LocalValue::Toggle(*checked));
                        cx.notify();
                    }))
                    .into_any_element()
            }
            FieldKind::Select { options, value } => {
                let current = match self.fields.get(&slot) {
                    Some(LocalValue::Choice(local)) => Some(local.as_str()),
                    _ => value.as_deref(),
                };
                let chosen = slot.clone();
                Self::choice_row(
                    &field.key,
                    options,
                    current,
                    pal,
                    cx,
                    move |this, picked, cx| {
                        this.fields
                            .insert(chosen.clone(), LocalValue::Choice(picked));
                        cx.notify();
                    },
                )
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(field.label.clone()),
            )
            .child(control)
            .into_any_element()
    }

    fn form_values(
        &self,
        form: &NodeKey,
        fields: &[Field],
        cx: &gpui_kit::App,
    ) -> BTreeMap<NodeKey, FieldValue> {
        fields
            .iter()
            .map(|field| {
                let slot = InputSlot::Field {
                    form: form.clone(),
                    field: field.key.clone(),
                };
                let typed = || {
                    self.inputs
                        .get(&slot)
                        .map(|state| state.read(cx).value().to_string())
                };
                let value = field_value(&field.kind, typed(), self.fields.get(&slot));
                (field.key.clone(), value)
            })
            .collect()
    }
}

pub(super) fn field_value(
    kind: &FieldKind,
    typed: Option<String>,
    local: Option<&LocalValue>,
) -> FieldValue {
    match (kind, local) {
        (FieldKind::Toggle { .. }, Some(LocalValue::Toggle(value)))
        | (FieldKind::Toggle { value }, _) => FieldValue::Toggle(*value),
        (FieldKind::Select { .. }, Some(LocalValue::Choice(value))) => {
            FieldValue::Text(value.clone())
        }
        (FieldKind::Select { value, .. }, _) => FieldValue::Text(value.clone().unwrap_or_default()),
        (FieldKind::Number { value }, _) => FieldValue::Number(
            typed
                .and_then(|text| text.trim().parse().ok())
                .unwrap_or(*value),
        ),
        (FieldKind::Text { value, .. }, _) => {
            FieldValue::Text(typed.unwrap_or_else(|| value.clone()))
        }
    }
}
