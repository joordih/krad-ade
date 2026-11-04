mod preview;
mod slots;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, Size};
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, ClickEvent, Context, Entity, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, StatefulInteractiveElement, Styled, Subscription,
    Window,
};
use krad_domain::{Theme, ThemeColor, ThemeId};
use krad_store::write_user_theme;

use super::super::context_menu::{MenuAction, MenuEntry, MenuSearch};
use super::super::dialog_parts::{
    field_label, segment, sheet_footer, sheet_overlay, sheet_panel, sheet_title, SegmentSize,
};
use super::{segmented_track, EnvironmentShell};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    add_to_library, current_theme, palette, theme_library, ICON_SM, LINE_SM, RADIUS_SM, SPACE_1,
    SPACE_2, SPACE_3, SPACE_5, SPACE_6, TEXT_SM,
};
use preview::theme_preview;
use slots::{ColorGroup, ColorSlot};

const TITLE: &str = "Create theme";
const NAME_LABEL: &str = "Name";
const NAME_PLACEHOLDER: &str = "My theme";
const BASE_LABEL: &str = "Based on";
const BASE_FILTER: &str = "Filter themes…";
const SAVE_LABEL: &str = "Save theme";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CLOSE: &str = "theme-editor-close";
const BTN_SAVE: &str = "theme-editor-save";
const BTN_CANCEL: &str = "theme-editor-cancel";
const BTN_BASE: &str = "theme-editor-base";
const GROUP_PREFIX: &str = "theme-editor-group";
const ROW_PREFIX: &str = "theme-editor-row";
const ROWS_ID: &str = "theme-editor-rows";
const SAVE_FAILED: &str = "The theme could not be saved";
const SHEET_W: f32 = 860.0;
const ROWS_W: f32 = 380.0;
const ROWS_MAX_H: f32 = 420.0;
const SWATCH: f32 = 18.0;
const HEX_W: f32 = 110.0;
const ID_SEPARATOR: char = '-';

pub(super) struct ThemeEditor {
    draft: Theme,
    base: ThemeId,
    group: ColorGroup,
    name: Entity<InputState>,
    fields: Vec<(ColorSlot, Entity<InputState>)>,
    _edits: Vec<Subscription>,
}

fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with(ID_SEPARATOR) {
            slug.push(ID_SEPARATOR);
        }
    }
    slug.trim_end_matches(ID_SEPARATOR).to_owned()
}

fn unique_id(name: &str, taken: &[ThemeId]) -> ThemeId {
    let base = Some(slug(name))
        .filter(|slug| !slug.is_empty())
        .unwrap_or_else(|| slug(NAME_PLACEHOLDER));
    let free = |candidate: &str| !taken.iter().any(|id| id.0 == candidate);
    let id = std::iter::once(base.clone())
        .chain((2..=taken.len() + 1).map(|number| format!("{base}{ID_SEPARATOR}{number}")))
        .find(|candidate| free(candidate))
        .unwrap_or(base);
    ThemeId(id)
}

fn dismiss(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    if let Some(ui) = this.settings_ui.as_mut() {
        ui.theme_editor = None;
    }
    cx.notify();
}

impl EnvironmentShell {
    pub(super) fn open_theme_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let base = current_theme(cx).clone();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(NAME_PLACEHOLDER));
        let mut edits = Vec::new();
        let fields = ColorSlot::all()
            .into_iter()
            .map(|slot| {
                let value = slot
                    .get(&base)
                    .map_or_else(String::new, |color| color.to_string());
                let field = cx.new(|cx| InputState::new(window, cx).default_value(value));
                edits.push(cx.subscribe(
                    &field,
                    move |this: &mut Self, field, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            let text = field.read(cx).value().to_string();
                            this.edit_theme_color(slot, &text, cx);
                        }
                    },
                ));
                (slot, field)
            })
            .collect();
        if let Some(ui) = self.settings_ui.as_mut() {
            ui.theme_editor = Some(ThemeEditor {
                base: base.id.clone(),
                draft: base,
                group: ColorGroup::Interface,
                name,
                fields,
                _edits: edits,
            });
        }
        cx.notify();
    }

    fn edit_theme_color(&mut self, slot: ColorSlot, text: &str, cx: &mut Context<Self>) {
        let Ok(color) = text.trim().parse::<ThemeColor>() else {
            return;
        };
        if let Some(editor) = self
            .settings_ui
            .as_mut()
            .and_then(|ui| ui.theme_editor.as_mut())
        {
            slot.set(&mut editor.draft, color);
            cx.notify();
        }
    }

    pub(in crate::environment_shell) fn rebase_theme_draft(
        &mut self,
        base: &ThemeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(theme) = theme_library(cx)
            .iter()
            .find(|theme| &theme.id == base)
            .cloned()
        else {
            return;
        };
        let Some(editor) = self
            .settings_ui
            .as_mut()
            .and_then(|ui| ui.theme_editor.as_mut())
        else {
            return;
        };
        editor.base = theme.id.clone();
        editor.draft = theme;
        let values: Vec<(Entity<InputState>, String)> = editor
            .fields
            .iter()
            .map(|(slot, field)| {
                let value = slot
                    .get(&editor.draft)
                    .map_or_else(String::new, |color| color.to_string());
                (field.clone(), value)
            })
            .collect();
        for (field, value) in values {
            field.update(cx, |field, cx| field.set_value(value, window, cx));
        }
        cx.notify();
    }

    fn save_theme_draft(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self
            .settings_ui
            .as_mut()
            .and_then(|ui| ui.theme_editor.take())
        else {
            return;
        };
        let typed = editor.name.read(cx).value().to_string();
        let name = Some(typed.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| NAME_PLACEHOLDER.to_owned());
        let taken: Vec<ThemeId> = theme_library(cx)
            .iter()
            .map(|theme| theme.id.clone())
            .collect();
        let mut theme = editor.draft;
        theme.id = unique_id(&name, &taken);
        theme.name = name;
        theme.credit = None;
        add_to_library(theme.clone(), cx);
        self.select_theme(&theme, cx);
        let writing = cx.background_spawn(async move { write_user_theme(&theme) });
        cx.spawn(async move |this, cx| {
            let Err(error) = writing.await else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.push_toast(format!("{SAVE_FAILED}: {error}"));
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn base_picker(editor: &ThemeEditor, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let shown = theme_library(cx)
            .iter()
            .find(|theme| theme.id == editor.base)
            .map_or_else(String::new, |theme| theme.name.clone());
        let entries: Vec<MenuEntry> = theme_library(cx)
            .iter()
            .map(|theme| {
                MenuEntry::checked(
                    theme.name.clone(),
                    theme.id == editor.base,
                    MenuAction::RebaseTheme(theme.id.clone()),
                )
            })
            .collect();
        div()
            .id(BTN_BASE)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .flex_1()
            .h(px(SWATCH + SPACE_2 * 2.0))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .border_1()
            .border_color(pal.border)
            .bg(pal.bg_hover)
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                    let Point { x, y }: Point<Pixels> = event.position();
                    this.open_searchable_menu(
                        (x.as_f32(), y.as_f32()),
                        entries.clone(),
                        MenuSearch {
                            placeholder: BASE_FILTER,
                            create: None,
                            footer: Vec::new(),
                        },
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(pal.text_strong))
                    .child(shown),
            )
            .child(icon(IconName::ChevronDown, ICON_SM, pal.text_muted))
            .into_any_element()
    }

    fn color_rows(editor: &ThemeEditor, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows = editor
            .fields
            .iter()
            .enumerate()
            .filter(|(_, (slot, _))| slot.group() == editor.group)
            .map(|(index, (slot, field))| {
                let color = slot.get(&editor.draft).map_or(0, |color| color.0);
                div()
                    .id((ROW_PREFIX, u64::try_from(index).unwrap_or(u64::MAX)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_none()
                            .size(px(SWATCH))
                            .rounded(px(RADIUS_SM))
                            .border_1()
                            .border_color(pal.border)
                            .bg(rgb(color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .text_color(rgb(pal.text_default))
                            .child(slot.label()),
                    )
                    .child(
                        div()
                            .w(px(HEX_W))
                            .child(Input::new(field).with_size(Size::Small)),
                    )
            });
        div()
            .id(ROWS_ID)
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .w(px(ROWS_W))
            .max_h(px(ROWS_MAX_H))
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    }

    fn group_tabs(editor: &ThemeEditor, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tabs = ColorGroup::ALL.map(|group| {
            segment(
                (GROUP_PREFIX, group as u64),
                group.label(),
                None,
                editor.group == group,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if let Some(editor) = this
                    .settings_ui
                    .as_mut()
                    .and_then(|ui| ui.theme_editor.as_mut())
                {
                    editor.group = group;
                }
                cx.notify();
            }))
        });
        segmented_track(pal).children(tabs).into_any_element()
    }

    pub(super) fn theme_editor_sheet(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.settings_ui.as_ref()?.theme_editor.as_ref()?;
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let body = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_5))
            .px(px(SPACE_6))
            .pb(px(SPACE_5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_3))
                    .child(field_label(NAME_LABEL, pal))
                    .child(Input::new(&editor.name))
                    .child(field_label(BASE_LABEL, pal))
                    .child(Self::base_picker(editor, cx))
                    .child(Self::group_tabs(editor, cx))
                    .child(Self::color_rows(editor, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(theme_preview(&editor.draft, mono)),
            );
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(TITLE, None, BTN_CLOSE, dismiss, cx))
            .child(body)
            .child(
                sheet_footer(pal)
                    .justify_end()
                    .child(
                        Button::new(BTN_CANCEL)
                            .label(CANCEL_LABEL)
                            .variant(ButtonVariant::Secondary)
                            .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                                dismiss(this, window, cx);
                            })),
                    )
                    .child(
                        Button::new(BTN_SAVE)
                            .label(SAVE_LABEL)
                            .variant(ButtonVariant::Primary)
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.save_theme_draft(cx);
                            })),
                    ),
            );
        Some(sheet_overlay(sheet, dismiss, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_theme_gets_a_readable_id_that_no_other_theme_uses() {
        let taken = [
            ThemeId("night-sky".to_owned()),
            ThemeId("night-sky-2".to_owned()),
        ];
        assert_eq!(
            unique_id("Night  Sky!", &taken),
            ThemeId("night-sky-3".to_owned())
        );
        assert_eq!(unique_id("Ocean", &taken), ThemeId("ocean".to_owned()));
        assert_eq!(unique_id("  ", &taken), ThemeId("my-theme".to_owned()));
    }
}
