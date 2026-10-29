use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Subscription, Window,
};
use krad_domain::{Note, Property, PROVENANCE_KEYS};

use super::super::super::EnvironmentShell;
use super::ROW_PREFIX;
use crate::components::{icon, Button, IconName};
use crate::theme::{palette, ICON_XS, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS};

const VALUE_PREFIX: &str = "inspector-value";
const BTN_ADD_PROPERTY: &str = "inspector-add-property";
const REMOVE_PREFIX: &str = "inspector-remove-property";
const ADD_PROPERTY_LABEL: &str = "Add property";
const NEW_PROPERTY_KEY: &str = "property";
const KEY_W: f32 = 84.0;

pub(in crate::environment_shell) struct PropertyEdit {
    key: String,
    field: Entity<InputState>,
    _changes: Subscription,
}

impl EnvironmentShell {
    pub(super) fn properties_list(&self, path: &str, cx: &mut Context<Self>) -> AnyElement {
        let Some(text) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.text(cx))
        else {
            return div().into_any_element();
        };
        let note = Note::parse(&text);
        let rows: Vec<AnyElement> = note
            .properties
            .entries()
            .iter()
            .enumerate()
            .map(|(index, property)| self.property_row(path, index, property, cx))
            .collect();
        let added = path.to_owned();
        div()
            .flex()
            .flex_col()
            .children(rows)
            .child(
                div().pt(px(SPACE_1)).child(
                    Button::new(BTN_ADD_PROPERTY)
                        .icon(IconName::Add)
                        .label(ADD_PROPERTY_LABEL)
                        .small()
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.change_properties(&added, window, cx, |properties| {
                                properties.set(NEW_PROPERTY_KEY, String::new());
                            });
                            this.edit_property(&added, NEW_PROPERTY_KEY, window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn property_row(
        &self,
        path: &str,
        index: usize,
        property: &Property,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let fixed = PROVENANCE_KEYS.contains(&property.key.as_str());
        let editing = self
            .notes
            .property_edit
            .as_ref()
            .filter(|edit| edit.key == property.key)
            .map(|edit| edit.field.clone());
        let key = property.key.clone();
        let removed = property.key.clone();
        let value = property.value.trim().to_owned();
        let note_path = path.to_owned();
        let removed_path = path.to_owned();
        div()
            .id((ROW_PREFIX, index))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_2))
            .py(px(SPACE_1))
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .child(
                div()
                    .flex_none()
                    .w(px(KEY_W))
                    .truncate()
                    .text_color(rgb(pal.text_muted))
                    .child(property.key.clone()),
            )
            .child(match editing {
                Some(field) => div()
                    .flex_1()
                    .child(Input::new(&field).small())
                    .into_any_element(),
                None => div()
                    .id((VALUE_PREFIX, index))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if fixed {
                        pal.text_muted
                    } else {
                        pal.text_default
                    }))
                    .when(!fixed, |cell| {
                        cell.cursor_text().on_click(cx.listener(
                            move |this: &mut Self, _, window, cx| {
                                this.edit_property(&note_path, &key, window, cx);
                            },
                        ))
                    })
                    .child(value)
                    .into_any_element(),
            })
            .when(!fixed, |row| {
                row.child(
                    div()
                        .id((REMOVE_PREFIX, index))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                            this.change_properties(&removed_path, window, cx, |properties| {
                                properties.remove(&removed);
                            });
                        }))
                        .child(icon(IconName::Close, ICON_XS, pal.text_muted)),
                )
            })
            .into_any_element()
    }

    fn change_properties(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut krad_domain::Properties),
    ) {
        let Some(state) = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.state.clone())
        else {
            return;
        };
        let mut note = Note::parse(&state.read(cx).value());
        change(&mut note.properties);
        let text = note.to_markdown();
        state.update(cx, |state, cx| state.set_value(text, window, cx));
    }

    fn edit_property(
        &mut self,
        path: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self
            .notes
            .editors
            .get(path)
            .and_then(|editor| editor.text(cx))
            .map(|text| Note::parse(&text))
            .and_then(|note| note.properties.get(key).map(str::to_owned))
            .unwrap_or_default();
        let field = cx.new(|cx| InputState::new(window, cx).default_value(value));
        let focus = field.clone();
        cx.defer_in(window, move |_, window, cx| {
            focus.update(cx, |field, cx| field.focus(window, cx));
        });
        let (path, key_owned) = (path.to_owned(), key.to_owned());
        let changes = cx.subscribe_in(
            &field,
            window,
            move |this: &mut Self, field, event: &InputEvent, window, cx| {
                let done = matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur);
                if !done {
                    return;
                }
                let value = field.read(cx).value().to_string();
                this.notes.property_edit = None;
                let key = key_owned.clone();
                this.change_properties(&path, window, cx, |properties| properties.set(&key, value));
                cx.notify();
            },
        );
        self.notes.property_edit = Some(PropertyEdit {
            key: key.to_owned(),
            field,
            _changes: changes,
        });
        cx.notify();
    }
}
