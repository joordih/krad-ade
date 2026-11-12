use gpui_kit::{
    div, px, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{ArchiveRetention, CodeTools, ComposerHeight, ExtensionTools};

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, panel_card, segmented_track, EnvironmentShell, SWATCH_GAP};
use crate::theme::palette;

const BTN_CODE_TOOLS_ON: &str = "code-tools-on";
const BTN_CODE_TOOLS_OFF: &str = "code-tools-off";
const LABEL_CODE_TOOLS_ON: &str = "On";
const LABEL_CODE_TOOLS_OFF: &str = "Off";
const CODE_TOOLS_LABEL: &str = "Code tools for agents";
const BTN_EXTENSION_TOOLS_ON: &str = "extension-tools-on";
const BTN_EXTENSION_TOOLS_OFF: &str = "extension-tools-off";
const EXTENSION_TOOLS_LABEL: &str = "Extension tools for agents";
const EXTENSION_TOOLS_HINT: &str = "Agents the app starts can use the tools that trusted extensions offer, approved by each agent's permission mode. Applies to the next tool list an agent asks for.";
const COMPOSER_HEIGHT_PREFIX: &str = "composer-height";
const COMPOSER_HEIGHT_LABEL: &str = "Chat composer height";
const COMPOSER_HEIGHT_HINT: &str = "How tall the chat composer may grow as you write before it scrolls. It never takes more than a share of the window.";
const RETENTION_PREFIX: &str = "archive-retention";
const RETENTION_LABEL: &str = "Keep archived";
const RETENTION_HINT: &str = "How long each group's Archive keeps closed sessions and chats so they can be reopened. Deleting removes only what the app keeps, never the harness's own conversations. Not at all deletes them on close.";
const CODE_TOOLS_HINT: &str = "Agents the app starts can ask the editor's language servers for diagnostics, definitions, references and renames. Applies to agents started after the change.";

impl EnvironmentShell {
    pub(super) fn chat_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let tools_card = panel_card(
            vec![
                self.code_tools_row(cx),
                self.extension_tools_row(cx),
                self.api_file_row(cx),
            ],
            pal,
        );
        let composer_card = panel_card(
            vec![self.composer_height_row(cx), self.archive_retention_row(cx)],
            pal,
        );
        let spending_card = self.spending_card(cx);
        let plan_usage_card = self.plan_usage_card(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(tools_card)
            .child(composer_card)
            .child(spending_card)
            .child(plan_usage_card)
            .into_any_element()
    }

    fn code_tools_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.code_tools;
        let option = |id: &'static str, label: &'static str, choice: CodeTools| {
            segment(
                id,
                label,
                None,
                current == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.code_tools = choice;
                this.save_settings(cx);
                cx.notify();
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_CODE_TOOLS_ON,
                LABEL_CODE_TOOLS_ON,
                CodeTools::On,
            ))
            .child(option(
                BTN_CODE_TOOLS_OFF,
                LABEL_CODE_TOOLS_OFF,
                CodeTools::Off,
            ))
            .into_any_element();
        form_row(CODE_TOOLS_LABEL, Some(CODE_TOOLS_HINT), segmented, pal)
    }

    fn extension_tools_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.extension_tools;
        let option = |id: &'static str, label: &'static str, choice: ExtensionTools| {
            segment(
                id,
                label,
                None,
                current == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.settings.extension_tools = choice;
                this.save_settings(cx);
                cx.notify();
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(
                BTN_EXTENSION_TOOLS_ON,
                LABEL_CODE_TOOLS_ON,
                ExtensionTools::On,
            ))
            .child(option(
                BTN_EXTENSION_TOOLS_OFF,
                LABEL_CODE_TOOLS_OFF,
                ExtensionTools::Off,
            ))
            .into_any_element();
        form_row(
            EXTENSION_TOOLS_LABEL,
            Some(EXTENSION_TOOLS_HINT),
            segmented,
            pal,
        )
    }

    fn composer_height_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.composer_height;
        let segments = ComposerHeight::ALL.map(|height| {
            segment(
                (COMPOSER_HEIGHT_PREFIX, height as u64),
                height.display_name(),
                None,
                current == height,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_composer_height(height, cx);
            }))
        });
        let segmented = segmented_track(pal).children(segments).into_any_element();
        form_row(
            COMPOSER_HEIGHT_LABEL,
            Some(COMPOSER_HEIGHT_HINT),
            segmented,
            pal,
        )
    }

    fn archive_retention_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.archive_retention;
        let segments = ArchiveRetention::ALL.map(|retention| {
            segment(
                (RETENTION_PREFIX, retention as u64),
                retention.display_name(),
                None,
                current == retention,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_archive_retention(retention, cx);
            }))
        });
        let segmented = segmented_track(pal).children(segments).into_any_element();
        form_row(RETENTION_LABEL, Some(RETENTION_HINT), segmented, pal)
    }

    fn set_archive_retention(&mut self, retention: ArchiveRetention, cx: &mut Context<Self>) {
        if self.settings.archive_retention == retention {
            return;
        }
        self.settings.archive_retention = retention;
        self.save_settings(cx);
        self.sweep_archive(cx);
        cx.notify();
    }

    fn set_composer_height(&mut self, height: ComposerHeight, cx: &mut Context<Self>) {
        if self.settings.composer_height == height {
            return;
        }
        self.settings.composer_height = height;
        self.save_settings(cx);
        cx.notify();
    }
}
