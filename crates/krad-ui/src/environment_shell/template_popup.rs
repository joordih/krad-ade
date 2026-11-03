mod choices;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Window,
};
use krad_domain::Host;
use std::path::PathBuf;

use super::entry_input::entry_name;
use super::rename::KEY_ESCAPE;
use super::EnvironmentShell;
use crate::theme::{
    palette, popover_fill, popover_shadow, LINE_SM, LINE_XS, SPACE_1, SPACE_2, SPACE_3, TEXT_BASE,
    TEXT_SM, TEXT_XS,
};
use choices::TemplateChoice;

const POPUP_W: f32 = 420.0;
const POPUP_TOP: f32 = 120.0;
const POPUP_RADIUS: f32 = 14.0;
const ROW_H: f32 = 28.0;
const ROW_RADIUS: f32 = 8.0;
const LIST_MAX_H: f32 = 320.0;
const POPUP_ID: &str = "template-popup";
const ROW_PREFIX: &str = "template-choice";
const NAME_PLACEHOLDER: &str = "Name";
const NEW_TITLE: &str = "New";
const LOADING_TEXT: &str = "Looking for file templates…";
const NONE_TEXT: &str = "No file templates for this folder";
const KEY_UP: &str = "up";
const KEY_DOWN: &str = "down";

pub(super) struct TemplatePopup {
    host: Host,
    folder: PathBuf,
    choices: Vec<TemplateChoice>,
    hidden: Vec<krad_domain::LanguageId>,
    pending: usize,
    selected: usize,
    field: Entity<InputState>,
    _events: Subscription,
}

fn popup_title<'a>(groups: impl IntoIterator<Item = &'a str>) -> String {
    let mut groups = groups.into_iter();
    let Some(first) = groups.next() else {
        return NEW_TITLE.to_owned();
    };
    if groups.all(|group| group == first) {
        format!("{NEW_TITLE} {first}")
    } else {
        NEW_TITLE.to_owned()
    }
}

const fn wrapped(selected: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (selected + 1) % count
    } else {
        (selected + count - 1) % count
    }
}

impl TemplatePopup {
    fn title(&self) -> String {
        popup_title(self.choices.iter().map(|choice| choice.group.as_str()))
    }

    fn step(&mut self, forward: bool) {
        self.selected = wrapped(self.selected, self.choices.len(), forward);
    }
}

impl EnvironmentShell {
    pub(super) fn open_template_popup(
        &mut self,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self
            .envs
            .get(&self.active)
            .map(|live| live.data.host.clone())
        else {
            return;
        };
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let events = cx.subscribe_in(
            &field,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.create_from_template(window, cx);
                }
            },
        );
        field.update(cx, |field, cx| field.focus(window, cx));
        self.template_popup = Some(TemplatePopup {
            host,
            folder,
            choices: Vec::new(),
            hidden: Vec::new(),
            pending: 0,
            selected: 0,
            field,
            _events: events,
        });
        self.gather_template_choices(cx);
        cx.notify();
    }

    pub(super) fn close_template_popup(&mut self, cx: &mut Context<Self>) {
        if self.template_popup.take().is_some() {
            cx.notify();
        }
    }

    fn on_template_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(popup) = &mut self.template_popup else {
            return;
        };
        match event.keystroke.key.as_str() {
            KEY_UP => popup.step(false),
            KEY_DOWN => popup.step(true),
            KEY_ESCAPE => {
                self.close_template_popup(cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn create_from_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popup) = &self.template_popup else {
            return;
        };
        let text = popup.field.read(cx).value().to_string();
        let (Some(name), Some(choice)) = (entry_name(&text), popup.choices.get(popup.selected))
        else {
            return;
        };
        let (name, choice, folder, host) = (
            name.to_owned(),
            choice.clone(),
            popup.folder.clone(),
            popup.host.clone(),
        );
        self.run_template_choice(&host, &folder, &choice, &name, window, cx);
    }

    fn template_row(
        index: usize,
        choice: &TemplateChoice,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        div()
            .id((ROW_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .h(px(ROW_H))
            .px(px(SPACE_2))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_strong))
            .when(selected, |row| row.bg(pal.bg_hover))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                if let Some(popup) = &mut this.template_popup {
                    popup.selected = index;
                }
                this.create_from_template(window, cx);
            }))
            .child(SharedString::from(choice.label.clone()))
            .into_any_element()
    }

    pub(super) fn template_popup_overlay(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let popup = self.template_popup.as_ref()?;
        let pal = palette(cx);
        let (root_w, _) = self.root_size(window);
        let mut rows = Vec::new();
        let mut group: Option<&str> = None;
        for (index, choice) in popup.choices.iter().enumerate() {
            if group != Some(choice.group.as_str()) {
                group = Some(choice.group.as_str());
                rows.push(
                    div()
                        .flex_none()
                        .px(px(SPACE_2))
                        .pt(px(SPACE_2))
                        .pb(px(SPACE_1 / 2.0))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .text_color(rgb(pal.text_subtle))
                        .child(SharedString::from(choice.group.clone()))
                        .into_any_element(),
                );
            }
            rows.push(Self::template_row(
                index,
                choice,
                index == popup.selected,
                cx,
            ));
        }
        let note = popup.choices.is_empty().then(|| {
            div()
                .px(px(SPACE_2))
                .py(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .text_color(rgb(pal.text_muted))
                .child(if popup.pending > 0 {
                    LOADING_TEXT
                } else {
                    NONE_TEXT
                })
        });
        Some(
            div()
                .absolute()
                .inset_0()
                .capture_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                    this.on_template_key(event, cx);
                }))
                .child(div().absolute().inset_0().occlude().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| this.close_template_popup(cx)),
                ))
                .child(
                    div()
                        .id(POPUP_ID)
                        .absolute()
                        .left(px(((root_w - POPUP_W) / 2.0).max(SPACE_3)))
                        .top(px(POPUP_TOP))
                        .w(px(POPUP_W))
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_2))
                        .p(px(SPACE_3))
                        .rounded(px(POPUP_RADIUS))
                        .bg(popover_fill(pal, cx))
                        .border_1()
                        .border_color(pal.divider)
                        .shadow(popover_shadow(pal, cx))
                        .occlude()
                        .child(
                            div()
                                .text_size(px(TEXT_BASE))
                                .text_color(rgb(pal.text_strong))
                                .child(popup.title()),
                        )
                        .child(Input::new(&popup.field).cleanable(false))
                        .child(
                            div()
                                .id((POPUP_ID, 1u64))
                                .flex()
                                .flex_col()
                                .max_h(px(LIST_MAX_H))
                                .overflow_y_scroll()
                                .children(rows)
                                .children(note),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_group_names_the_popup_and_several_keep_it_generic() {
        assert_eq!(popup_title(["Java Class", "Java Class"]), "New Java Class");
        assert_eq!(popup_title(["Java Class", "Package"]), "New");
        assert_eq!(popup_title([]), "New");
    }

    #[test]
    fn moving_through_choices_wraps_around_both_ways() {
        assert_eq!(wrapped(0, 3, false), 2);
        assert_eq!(wrapped(2, 3, true), 0);
        assert_eq!(wrapped(0, 0, true), 0);
    }
}
