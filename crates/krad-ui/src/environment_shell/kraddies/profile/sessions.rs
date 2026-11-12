use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    div, px, AnyElement, AppContext as _, Context, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{KraddieId, ModelId, QuietSpan, ToastKind};

use super::super::super::native_alert::Confirmation;
use super::super::super::EnvironmentShell;
use super::super::rows::SWITCH_W;
use super::super::segmented::sliding_width;
use super::super::segmented::{sliding_track, Sliding};
use super::super::tile::{button_w, setting_row, SettingRow};
use crate::components::{Button, ButtonVariant, Stretch};
use crate::theme::{palette, SPACE_2};

const SESSION_MODEL_W: f32 = 200.0;
const SESSIONS_TITLE: &str = "Its sessions";
const SESSIONS_NOTE: &str = "Where the work it starts shows up";
const SIDEBAR_LABEL: &str = "List them in the sidebar";
const SIDEBAR_PREFIX: &str = "kraddie-sessions-sidebar";
const FOLDER_LABEL: &str = "Tuck them under its row";
const FOLDER_DETAIL: &str = "Nested below it in the sidebar's Kraddies list";
const FOLDER_PREFIX: &str = "kraddie-sessions-folder";
const SESSION_MODEL_LABEL: &str = "Model for its sessions";
const SESSION_MODEL_PLACEHOLDER: &str = "Same as its brain";
const SESSION_MODEL_SAVE: &str = "kraddie-session-model-save";
const SAVE_LABEL: &str = "Save";
const BAD_MODEL: &str = "That model id is not valid";
const ROUNDS_TITLE: &str = "Rounds";
const ROUNDS_NOTE: &str =
    "A read-only stroll over its work that leaves you a suggestion, nothing more";
const ROUNDS_LABEL: &str = "Make rounds on its own";
const ROUNDS_PREFIX: &str = "kraddie-rounds";
const QUIET_LABEL: &str = "After a quiet spell of";
const QUIET_PREFIX: &str = "kraddie-rounds-quiet";
const CONVERSATION_TITLE: &str = "Fresh start";
const CONVERSATION_NOTE: &str = "Clears what it said in past runs";
const RESET_LABEL: &str = "Start over";
const RESET_DETAIL: &str = "Its compass, keepsakes and routines stay put";
const RESET_BTN: &str = "kraddie-reset";
const RESET_CONFIRM_BTN: &str = "kraddie-reset-confirm";
const RESET_CANCEL_BTN: &str = "kraddie-reset-cancel";
const RESET_TITLE: &str = "Start this Kraddie over?";
const RESET_TEXT: &str =
    "Its run history is cleared. The compass, keepsakes and routines are kept.";
const RESET_CONFIRM: &str = "Start over";
const CANCEL: &str = "Cancel";

pub(super) type Part = (&'static str, Option<&'static str>, AnyElement);

fn lines(rows: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_2))
        .children(rows)
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn sessions_part(
        &self,
        id: &KraddieId,
        card_inner: f32,
        cx: &mut Context<Self>,
    ) -> Part {
        let pal = palette(cx);
        let Some(entry) = self.kraddies.entry(id) else {
            return (SESSIONS_TITLE, None, div().into_any_element());
        };
        let sessions = entry.kraddie.settings.sessions;
        let sidebar = Self::switch_control(
            SIDEBAR_PREFIX,
            sessions.sidebar,
            id,
            |settings, on| {
                settings.sessions.sidebar = on;
            },
            cx,
        );
        let folder = Self::switch_control(
            FOLDER_PREFIX,
            sessions.folder,
            id,
            |settings, on| {
                settings.sessions.folder = on;
            },
            cx,
        );
        let model = self
            .kraddies
            .profile
            .session_model
            .as_ref()
            .filter(|(kraddie, _)| kraddie == id)
            .map(|(_, input)| {
                let target = id.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(div().w(px(SESSION_MODEL_W)).child(Input::new(input)))
                    .child(
                        Button::new(SESSION_MODEL_SAVE)
                            .label(SAVE_LABEL)
                            .small()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.save_session_model(&target, cx);
                            })),
                    )
                    .into_any_element()
            });
        let model_w = SESSION_MODEL_W + SPACE_2 + button_w(SAVE_LABEL, cx);
        let rows = [
            setting_row(
                SettingRow::new(SIDEBAR_LABEL, SWITCH_W, card_inner),
                Some(sidebar),
                pal,
                cx,
            ),
            setting_row(
                SettingRow::new(FOLDER_LABEL, SWITCH_W, card_inner).detail(FOLDER_DETAIL),
                Some(folder),
                pal,
                cx,
            ),
            setting_row(
                SettingRow::new(SESSION_MODEL_LABEL, model_w, card_inner),
                model,
                pal,
                cx,
            ),
        ];
        (SESSIONS_TITLE, Some(SESSIONS_NOTE), lines(rows))
    }

    pub(super) fn open_session_model_draft(
        &mut self,
        id: &KraddieId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .kraddies
            .entry(id)
            .and_then(|entry| entry.kraddie.settings.session_model.as_ref())
            .map_or_default(ToString::to_string);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(SESSION_MODEL_PLACEHOLDER)
                .default_value(current)
        });
        self.kraddies.profile.session_model = Some((id.clone(), input));
    }

    fn save_session_model(&mut self, id: &KraddieId, cx: &mut Context<Self>) {
        let Some((_, input)) = self.kraddies.profile.session_model.as_ref() else {
            return;
        };
        let text = input.read(cx).value().trim().to_owned();
        let model = if text.is_empty() {
            None
        } else {
            let Some(model) = ModelId::parse(&text) else {
                return self.toast(ToastKind::Warning, BAD_MODEL, cx);
            };
            Some(model)
        };
        self.edit_kraddie_settings(id, |settings| settings.session_model = model, cx);
    }

    pub(super) fn rounds_part(
        &self,
        id: &KraddieId,
        container: f32,
        cx: &mut Context<Self>,
    ) -> Part {
        let pal = palette(cx);
        let Some(entry) = self.kraddies.entry(id) else {
            return (ROUNDS_TITLE, None, div().into_any_element());
        };
        let rounds = entry.kraddie.settings.rounds;
        let switch = Self::switch_control(
            ROUNDS_PREFIX,
            rounds.switch,
            id,
            |settings, on| {
                settings.rounds.switch = on;
            },
            cx,
        );
        let labels = QuietSpan::ALL.map(QuietSpan::label);
        let quiet_row = SettingRow::new(QUIET_LABEL, sliding_width(&labels, cx), container);
        let room = quiet_row.room(cx);
        let sliding = Sliding {
            id: QUIET_PREFIX,
            labels: &labels,
            short: &labels,
            chosen: QuietSpan::ALL
                .iter()
                .position(|span| *span == rounds.after)
                .unwrap_or_default(),
            container: room,
            stretch: Stretch::Natural,
            menu_open: self.kraddies.profile.open_menu == Some(QUIET_PREFIX),
        };
        let quiet = sliding_track(
            &sliding,
            pal,
            cx,
            |index, segment| {
                let span = QuietSpan::ALL[index];
                let target = id.clone();
                segment.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.kraddies.profile.open_menu = None;
                    this.edit_kraddie_settings(
                        &target,
                        |settings| settings.rounds.after = span,
                        cx,
                    );
                }))
            },
            |button| {
                button.on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.kraddies.profile.toggle_menu(QUIET_PREFIX);
                    cx.notify();
                }))
            },
        );
        let rows = [
            setting_row(
                SettingRow::new(ROUNDS_LABEL, SWITCH_W, container),
                Some(switch),
                pal,
                cx,
            ),
            setting_row(quiet_row, Some(quiet), pal, cx),
        ];
        (ROUNDS_TITLE, Some(ROUNDS_NOTE), lines(rows))
    }

    pub(super) fn conversation_part(
        &self,
        id: &KraddieId,
        card_inner: f32,
        cx: &mut Context<Self>,
    ) -> Part {
        let pal = palette(cx);
        let target = id.clone();
        let asking = self.kraddies.profile.resetting.as_ref() == Some(id);
        let control = if asking {
            let confirmed = id.clone();
            div()
                .flex()
                .flex_row()
                .gap(px(SPACE_2))
                .child(
                    Button::new(RESET_CANCEL_BTN)
                        .label(CANCEL)
                        .small()
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.kraddies.profile.resetting = None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(RESET_CONFIRM_BTN)
                        .label(RESET_CONFIRM)
                        .small()
                        .variant(ButtonVariant::Destructive)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.reset_kraddie_conversation(&confirmed, cx);
                        })),
                )
                .into_any_element()
        } else {
            Button::new(RESET_BTN)
                .label(RESET_LABEL)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.ask_reset_conversation(&target, window, cx);
                }))
                .into_any_element()
        };
        let detail = if asking { RESET_TEXT } else { RESET_DETAIL };
        let control_w = if asking {
            button_w(CANCEL, cx) + SPACE_2 + button_w(RESET_CONFIRM, cx)
        } else {
            button_w(RESET_LABEL, cx)
        };
        (
            CONVERSATION_TITLE,
            Some(CONVERSATION_NOTE),
            lines([setting_row(
                SettingRow::new(RESET_LABEL, control_w, card_inner).detail(detail),
                Some(control),
                pal,
                cx,
            )]),
        )
    }

    fn ask_reset_conversation(
        &mut self,
        id: &KraddieId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let confirmation = Confirmation {
            title: RESET_TITLE,
            detail: RESET_TEXT,
            confirm: RESET_CONFIRM,
            cancel: CANCEL,
        };
        let target = id.clone();
        let asked =
            Self::confirm_natively(&confirmation, window, cx, move |this, confirmed, _, cx| {
                if confirmed {
                    this.reset_kraddie_conversation(&target, cx);
                }
            });
        if !asked {
            self.kraddies.profile.resetting = Some(id.clone());
            cx.notify();
        }
    }
}
