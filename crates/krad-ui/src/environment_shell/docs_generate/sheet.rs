use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{AgentPreset, ModelId};

use super::super::dialog_parts::{
    sheet_body, sheet_footer, sheet_group, sheet_overlay, sheet_panel, sheet_row, sheet_section,
    sheet_title,
};
use super::super::EnvironmentShell;
use super::{DocsDialog, NOTE_CHOICES};
use crate::components::{harness_icon, icon, Button, ButtonVariant, IconName};
use crate::theme::{palette, ICON_SM, ICON_XS, LINE_XS, NAV_ROW_H, SPACE_1_5, SPACE_2, TEXT_XS};

const SHEET_W: f32 = 620.0;
const TRIGGER_RADIUS: f32 = 8.0;
const FOLDER_W: f32 = 200.0;
const TITLE: &str = "Generate documentation";
const SOURCES_LABEL: &str = "Sources";
const NOTES_LABEL: &str = "Notes to write";
const NOTES_DETAIL: &str = "The agent writes these, or updates them when they already exist.";
const OUTPUT_LABEL: &str = "Output";
const FOLDER_LABEL: &str = "Folder";
const FOLDER_DETAIL: &str = "Where the notes go inside the vault; empty means its root.";
const KEEP_LABEL: &str = "Keep notes I edited";
const KEEP_DETAIL: &str = "Generated notes you changed stay as they are.";
const CANCEL_LABEL: &str = "Cancel";
const GENERATE_LABEL: &str = "Generate notes";
const ITEMS_SUFFIX: &str = "items";
const FILE_DETAIL: &str = "File";
const BODY_ID: &str = "docs-dialog-body";
const BTN_CLOSE: &str = "docs-dialog-close";
const BTN_CANCEL: &str = "docs-dialog-cancel";
const BTN_GENERATE: &str = "docs-dialog-generate";
const SOURCE_PREFIX: &str = "docs-source";
const SOURCE_SWITCH_PREFIX: &str = "docs-source-switch";
const NOTE_PREFIX: &str = "docs-note";
const NOTE_SWITCH_PREFIX: &str = "docs-note-switch";
const SWITCH_KEEP: &str = "docs-keep";
const MODEL_PICKER_ID: &str = "docs-model";

fn close(this: &mut EnvironmentShell, _: &mut Window, cx: &mut Context<EnvironmentShell>) {
    this.docs_dialog = None;
    cx.notify();
}

fn row_key(index: usize) -> u64 {
    u64::try_from(index).unwrap_or(u64::MAX)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn docs_generate_sheet(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.docs_dialog.as_ref()?;
        let pal = palette(cx);
        let group = self
            .active_group()
            .map_or_default(|group| group.name.clone());
        let sheet = sheet_panel(SHEET_W, pal)
            .child(sheet_title(TITLE, None, BTN_CLOSE, close, cx))
            .child(
                sheet_body(BODY_ID)
                    .child(Self::docs_sources(dialog, &group, cx))
                    .child(Self::docs_notes(dialog, cx))
                    .child(Self::docs_output(dialog, cx)),
            )
            .child(Self::docs_footer(dialog, cx));
        Some(sheet_overlay(sheet, close, cx))
    }

    fn docs_sources(dialog: &DocsDialog, group: &str, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = dialog
            .sources
            .iter()
            .enumerate()
            .map(|(index, (entry, chosen))| {
                let toggle = move |this: &mut Self, cx: &mut Context<Self>| {
                    if let Some((_, chosen)) = this
                        .docs_dialog
                        .as_mut()
                        .and_then(|dialog| dialog.sources.get_mut(index))
                    {
                        *chosen = !*chosen;
                    }
                    cx.notify();
                };
                let detail: SharedString = if entry.is_dir {
                    format!("{} {ITEMS_SUFFIX}", entry.children).into()
                } else {
                    FILE_DETAIL.into()
                };
                sheet_row(entry.relative.clone(), Some(detail), None, pal)
                    .id((SOURCE_PREFIX, row_key(index)))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| toggle(this, cx)))
                    .child(
                        Switch::new((SOURCE_SWITCH_PREFIX, row_key(index)))
                            .checked(*chosen)
                            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                                toggle(this, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        let description = format!("What the agent reads in {group}.");
        sheet_section(SOURCES_LABEL, Some(&description), pal)
            .child(sheet_group(rows, pal))
            .into_any_element()
    }

    fn docs_notes(dialog: &DocsDialog, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows: Vec<AnyElement> = dialog
            .notes
            .iter()
            .enumerate()
            .map(|(index, (note, chosen))| {
                let toggle = move |this: &mut Self, cx: &mut Context<Self>| {
                    if let Some((_, chosen)) = this
                        .docs_dialog
                        .as_mut()
                        .and_then(|dialog| dialog.notes.get_mut(index))
                    {
                        *chosen = !*chosen;
                    }
                    cx.notify();
                };
                let detail = NOTE_CHOICES
                    .get(index)
                    .map(|(_, detail)| SharedString::from(*detail));
                sheet_row(*note, detail, None, pal)
                    .id((NOTE_PREFIX, row_key(index)))
                    .cursor_pointer()
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| toggle(this, cx)))
                    .child(
                        Switch::new((NOTE_SWITCH_PREFIX, row_key(index)))
                            .checked(*chosen)
                            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                                toggle(this, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();
        sheet_section(NOTES_LABEL, Some(NOTES_DETAIL), pal)
            .child(sheet_group(rows, pal))
            .into_any_element()
    }

    fn docs_output(dialog: &DocsDialog, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let folder = sheet_row(
            FOLDER_LABEL,
            Some(FOLDER_DETAIL.into()),
            Some(
                div()
                    .w(px(FOLDER_W))
                    .child(Input::new(&dialog.folder).small())
                    .into_any_element(),
            ),
            pal,
        )
        .into_any_element();
        let keep = sheet_row(
            KEEP_LABEL,
            Some(KEEP_DETAIL.into()),
            Some(
                Switch::new(SWITCH_KEEP)
                    .checked(dialog.keep_edited)
                    .on_click(cx.listener(|this: &mut Self, checked: &bool, _, cx| {
                        if let Some(dialog) = &mut this.docs_dialog {
                            dialog.keep_edited = *checked;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ),
            pal,
        )
        .into_any_element();
        sheet_section(OUTPUT_LABEL, None, pal)
            .child(sheet_group([folder, keep], pal))
            .into_any_element()
    }

    fn docs_model_trigger(
        preset: AgentPreset,
        model: Option<&ModelId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let (mark, mark_color) = harness_icon(preset, pal);
        div()
            .id(MODEL_PICKER_ID)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(TRIGGER_RADIUS))
            .bg(pal.bg_hover)
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(px(TEXT_XS))
            .line_height(px(LINE_XS))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(pal.text_strong))
            .child(icon(mark, ICON_SM, mark_color))
            .child(preset.display_name())
            .children(model.map(|model| {
                div()
                    .font_family(mono)
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(pal.text_muted))
                    .child(model.to_string())
            }))
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_muted))
            .on_click(cx.listener(|this: &mut Self, event: &ClickEvent, _, cx| {
                let at = event.position();
                this.open_docs_model_picker((at.x.as_f32(), at.y.as_f32()), cx);
            }))
            .into_any_element()
    }

    fn docs_footer(dialog: &DocsDialog, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let picker = dialog
            .preset
            .map(|preset| Self::docs_model_trigger(preset, dialog.model.as_ref(), cx));
        let ready = dialog.preset.is_some() && dialog.notes.iter().any(|(_, chosen)| *chosen);
        sheet_footer(pal)
            .children(picker)
            .child(div().flex_1())
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(
                        cx.listener(|this: &mut Self, _, window, cx| close(this, window, cx)),
                    ),
            )
            .when(ready, |footer| {
                footer.child(
                    Button::new(BTN_GENERATE)
                        .label(GENERATE_LABEL)
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.start_docs_generation(cx);
                        })),
                )
            })
            .into_any_element()
    }
}
