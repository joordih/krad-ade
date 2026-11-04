use gpui_kit::{
    div, px, rgb, AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, Window,
};
use krad_domain::JavaEngine;

use super::super::dialog_parts::{segment, SegmentSize};
use super::super::native_alert::Confirmation;
use super::{form_row, panel_card, segmented_track, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, LINE_SM, SPACE_2, TEXT_SM};

const ENGINE_LABEL: &str = "Java engine";
const ENGINE_HINT: &str = "The language server Java files use. IntelliJ runs krad-java-server, which must be on PATH, and is unstable: it can report problems that are not real. Changing it restarts the language servers.";
const ENGINE_PREFIX: &str = "java-engine";
const CONFIRM_TITLE: &str = "Use the unstable IntelliJ engine for Java?";
const CONFIRM_DETAIL: &str = "krad-java-server is still unstable: it may report problems that are not real, use more memory and fail to import some projects. The language servers restart to switch.";
const CONFIRM_LABEL: &str = "Use IntelliJ";
const CANCEL_LABEL: &str = "Cancel";
const BTN_CONFIRM: &str = "java-engine-confirm";
const BTN_CANCEL: &str = "java-engine-cancel";

impl EnvironmentShell {
    fn set_java_engine(&mut self, engine: JavaEngine, cx: &mut Context<Self>) {
        if let Some(ui) = &mut self.settings_ui {
            ui.languages.confirming_unstable = false;
        }
        if self.settings.java_engine == engine {
            cx.notify();
            return;
        }
        self.settings.java_engine = engine;
        self.save_settings(cx);
        self.restart_language_servers(cx);
        cx.notify();
    }

    fn choose_java_engine(
        &mut self,
        engine: JavaEngine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !engine.is_unstable() || self.settings.java_engine == engine {
            self.set_java_engine(engine, cx);
            return;
        }
        let confirmation = Confirmation {
            title: CONFIRM_TITLE,
            detail: CONFIRM_DETAIL,
            confirm: CONFIRM_LABEL,
            cancel: CANCEL_LABEL,
        };
        let asked =
            Self::confirm_natively(&confirmation, window, cx, move |this, confirmed, _, cx| {
                if confirmed {
                    this.set_java_engine(engine, cx);
                }
            });
        if !asked {
            if let Some(ui) = &mut self.settings_ui {
                ui.languages.confirming_unstable = true;
            }
            cx.notify();
        }
    }

    pub(super) fn java_engine_card(&self, confirming: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.java_engine;
        let engines = JavaEngine::ALL.map(|engine| {
            segment(
                (ENGINE_PREFIX, engine as u64),
                engine.display_name(),
                None,
                current == engine,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.choose_java_engine(engine, window, cx);
            }))
        });
        let row = form_row(
            ENGINE_LABEL,
            Some(ENGINE_HINT),
            segmented_track(pal).children(engines).into_any_element(),
            pal,
        );
        let confirm = confirming.then(|| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_2))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.warning_fg))
                .child(div().flex_1().child(CONFIRM_DETAIL))
                .child(
                    Button::new(BTN_CANCEL)
                        .label(CANCEL_LABEL)
                        .small()
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            if let Some(ui) = &mut this.settings_ui {
                                ui.languages.confirming_unstable = false;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(BTN_CONFIRM)
                        .label(CONFIRM_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.set_java_engine(JavaEngine::IntelliJ, cx);
                        })),
                )
                .into_any_element()
        });
        panel_card(std::iter::once(row).chain(confirm), pal)
    }
}
