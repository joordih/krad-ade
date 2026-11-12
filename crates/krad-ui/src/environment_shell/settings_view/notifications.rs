use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{
    AttentionKind, AttentionSettings, AttentionSound, KindRule, SoundTiming, Switch, TimeOfDay,
};

use super::super::dialog_parts::{segment, SegmentSize};
use super::flat_pane::FlatPane;
use super::toasts::{TOASTS_DETAIL, TOASTS_TITLE};
use super::{form_row, hint, panel_card, segmented_track, EnvironmentShell};
use crate::app_badge::{BadgeSignal, BADGE_LABEL, SIGNAL};
use crate::attention_sound;
use crate::components::{icon, IconName};
use crate::keymap::{ATTENTION_INBOX_HINT, NEXT_WAITING_AGENT_HINT};
use crate::theme::{
    palette, Palette, ICON_BUTTON_SM, ICON_SM, LINE_SM, RADIUS_XS, SPACE_1, TEXT_SM,
};

const ID_PREFIX: &str = "attention-setting";
const ID_SEPARATOR: &str = "-";
const KEY_SYSTEM: &str = "system";
const KEY_ANSWERS: &str = "answers";
const KEY_QUIET: &str = "quiet";
const KEY_BADGE: &str = "badge";
const KEY_NOTIFY: &str = "notify";
const KEY_SOUND: &str = "sound";
const KEY_TIMING: &str = "timing";
const KEY_START: &str = "start";
const KEY_END: &str = "end";
const KEY_EARLIER: &str = "earlier";
const KEY_LATER: &str = "later";
const KEY_PERMISSION: &str = "permission";
const KEY_ASK: &str = "ask";
const KEY_ERROR: &str = "error";
const KEY_FINISHED: &str = "finished";
const QUIET_STEP_MINUTES: i32 = 30;
const CLOCK_W: f32 = 52.0;

const NOTIFICATIONS_TITLE: &str = "Notifications";
const NOTIFICATIONS_DETAIL: &str = "Shown only while the window is in the background.";
const SYSTEM_LABEL: &str = "System notifications";
const SYSTEM_HINT: &str = "Turns every kind of notification on or off at once.";
const ANSWERS_LABEL: &str = "Answer from notifications";
const ANSWERS_HINT: &str =
    "Permission notifications offer Approve and Deny, so you can answer without opening the app.";
const EVENTS_TITLE: &str = "Events";
const EVENTS_DETAIL: &str = "What each kind of event does.";
const NOTIFY_LABEL: &str = "Notify";
const SOUND_LABEL: &str = "Sound";
const COUNT_LABEL: &str = "Count it";
const SOUNDS_TITLE: &str = "Sounds";
const TIMING_LABEL: &str = "Play sounds";
const TIMING_HINT: &str = "In the background only, or also while you use the app.";
const QUIET_TITLE: &str = "Quiet hours";
const QUIET_DETAIL: &str =
    "No notifications or sounds during these hours; the inbox and the count still fill.";
const QUIET_LABEL: &str = "Quiet hours";
const QUIET_START_LABEL: &str = "From";
const QUIET_END_LABEL: &str = "Until";
const BADGE_TITLE: &str = "App icon";
const BADGE_COUNT_HINT: &str =
    "Shows how many agents are waiting for you. Which events count is set per event above.";
const BADGE_FLASH_HINT: &str = "Flags the window in the taskbar when an agent needs you; a count is not available on this system yet. Which events count is set per event above.";
const SHORTCUTS_TITLE: &str = "Shortcuts";
const SHORTCUTS_NEXT: &str = "Go to the next agent waiting for you";
const SHORTCUTS_INBOX: &str = "Open the attention inbox";
const EARLIER_LABEL: &str = "Earlier";
const LATER_LABEL: &str = "Later";

type SwitchEdit = fn(&mut AttentionSettings, Switch);

#[derive(Clone, Copy)]
enum QuietEdge {
    Start,
    End,
}

fn element_id(parts: &[&str]) -> SharedString {
    SharedString::from(
        [ID_PREFIX]
            .iter()
            .chain(parts)
            .copied()
            .collect::<Vec<_>>()
            .join(ID_SEPARATOR),
    )
}

const fn kind_key(kind: AttentionKind) -> &'static str {
    match kind {
        AttentionKind::Permission => KEY_PERMISSION,
        AttentionKind::Ask => KEY_ASK,
        AttentionKind::Error => KEY_ERROR,
        AttentionKind::Finished => KEY_FINISHED,
    }
}

impl EnvironmentShell {
    pub(super) fn notifications_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let settings = self.settings.attention;
        let main = panel_card(
            [
                Self::switch_row(
                    KEY_SYSTEM,
                    SYSTEM_LABEL,
                    SYSTEM_HINT,
                    settings.notifications,
                    |settings, switch| settings.notifications = switch,
                    cx,
                ),
                Self::switch_row(
                    KEY_ANSWERS,
                    ANSWERS_LABEL,
                    ANSWERS_HINT,
                    settings.actions,
                    |settings, switch| settings.actions = switch,
                    cx,
                ),
            ],
            pal,
        );
        let mut sounds = vec![panel_card([self.timing_row(cx)], pal)];
        if let Some(note) = attention_sound::MISSING_NOTE {
            sounds.push(hint(note, pal));
        }
        let quiet = panel_card(
            [
                Self::switch_row(
                    KEY_QUIET,
                    QUIET_LABEL,
                    QUIET_DETAIL,
                    settings.quiet_hours.switch,
                    |settings, switch| settings.quiet_hours.switch = switch,
                    cx,
                ),
                self.quiet_row(QuietEdge::Start, cx),
                self.quiet_row(QuietEdge::End, cx),
            ],
            pal,
        );
        let badge_hint = if SIGNAL == BadgeSignal::Count {
            BADGE_COUNT_HINT
        } else {
            BADGE_FLASH_HINT
        };
        let badge = panel_card(
            [Self::switch_row(
                KEY_BADGE,
                BADGE_LABEL,
                badge_hint,
                settings.badge,
                |settings, switch| settings.badge = switch,
                cx,
            )],
            pal,
        );
        let shortcuts = panel_card(
            [
                form_row(
                    SHORTCUTS_NEXT,
                    None,
                    shortcut(NEXT_WAITING_AGENT_HINT, pal),
                    pal,
                ),
                form_row(
                    SHORTCUTS_INBOX,
                    None,
                    shortcut(ATTENTION_INBOX_HINT, pal),
                    pal,
                ),
            ],
            pal,
        );
        let mut pane = FlatPane::default();
        pane.section(NOTIFICATIONS_TITLE, Some(NOTIFICATIONS_DETAIL), pal);
        pane.item(main);
        pane.section(EVENTS_TITLE, Some(EVENTS_DETAIL), pal);
        for kind in AttentionKind::ALL {
            pane.heading(kind.display_name(), pal);
            pane.item(self.kind_card(kind, cx));
        }
        pane.section(TOASTS_TITLE, Some(TOASTS_DETAIL), pal);
        pane.item(self.toasts_card(cx));
        pane.section(SOUNDS_TITLE, None, pal);
        for sound in sounds {
            pane.item(sound);
        }
        pane.section(QUIET_TITLE, None, pal);
        pane.item(quiet);
        pane.section(BADGE_TITLE, None, pal);
        pane.item(badge);
        pane.section(SHORTCUTS_TITLE, None, pal);
        pane.item(shortcuts);
        pane.into_element()
    }

    fn edit_attention(
        &mut self,
        edit: impl FnOnce(&mut AttentionSettings),
        cx: &mut Context<Self>,
    ) {
        edit(&mut self.settings.attention);
        self.save_settings(cx);
        cx.notify();
    }

    fn switch_row(
        key: &'static str,
        label: &str,
        detail: &str,
        current: Switch,
        apply: SwitchEdit,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let segments = Switch::ALL.map(|choice| {
            segment(
                element_id(&[key, choice.display_name()]),
                choice.display_name(),
                None,
                current == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_attention(|settings| apply(settings, choice), cx);
            }))
        });
        let control = segmented_track(pal).children(segments).into_any_element();
        form_row(label, Some(detail), control, pal)
    }

    fn kind_card(&self, kind: AttentionKind, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rule = self.settings.attention.rules.of(kind);
        let key = kind_key(kind);
        let switch = |row: &'static str,
                      current: Switch,
                      apply: fn(&mut KindRule, Switch),
                      cx: &mut Context<Self>| {
            let segments = Switch::ALL.map(|choice| {
                segment(
                    element_id(&[key, row, choice.display_name()]),
                    choice.display_name(),
                    None,
                    current == choice,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_attention(|settings| apply(settings.rules.of_mut(kind), choice), cx);
                }))
            });
            segmented_track(pal).children(segments).into_any_element()
        };
        let notify = switch(
            KEY_NOTIFY,
            rule.notify,
            |rule, switch| rule.notify = switch,
            cx,
        );
        let badge = switch(
            KEY_BADGE,
            rule.badge,
            |rule, switch| rule.badge = switch,
            cx,
        );
        let sounds = AttentionSound::ALL.map(|sound| {
            segment(
                element_id(&[key, KEY_SOUND, sound.display_name()]),
                sound.display_name(),
                None,
                rule.sound == sound,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_attention(|settings| settings.rules.of_mut(kind).sound = sound, cx);
                attention_sound::play(sound);
            }))
        });
        let sound = segmented_track(pal).children(sounds).into_any_element();
        panel_card(
            [
                form_row(NOTIFY_LABEL, None, notify, pal),
                form_row(SOUND_LABEL, None, sound, pal),
                form_row(COUNT_LABEL, None, badge, pal),
            ],
            pal,
        )
    }

    fn timing_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.attention.sound_timing;
        let segments = SoundTiming::ALL.map(|timing| {
            segment(
                element_id(&[KEY_TIMING, timing.display_name()]),
                timing.display_name(),
                None,
                current == timing,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.edit_attention(|settings| settings.sound_timing = timing, cx);
            }))
        });
        let control = segmented_track(pal).children(segments).into_any_element();
        form_row(TIMING_LABEL, Some(TIMING_HINT), control, pal)
    }

    fn quiet_row(&self, edge: QuietEdge, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let quiet = self.settings.attention.quiet_hours;
        let (label, key, time) = match edge {
            QuietEdge::Start => (QUIET_START_LABEL, KEY_START, quiet.start),
            QuietEdge::End => (QUIET_END_LABEL, KEY_END, quiet.end),
        };
        let step = |direction: &'static str,
                    glyph: IconName,
                    tooltip: &'static str,
                    by: i32,
                    cx: &mut Context<Self>| {
            div()
                .id(element_id(&[KEY_QUIET, key, direction]))
                .flex()
                .items_center()
                .justify_center()
                .size(px(ICON_BUTTON_SM))
                .rounded(px(RADIUS_XS))
                .cursor_pointer()
                .hover(|style| style.bg(pal.bg_hover))
                .tooltip(move |_, cx| crate::components::tooltip_view(tooltip.into(), cx))
                .child(icon(glyph, ICON_SM, pal.text_muted))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.edit_attention(
                        |settings| {
                            let quiet = &mut settings.quiet_hours;
                            let moved = |time: TimeOfDay| time.shifted(by);
                            match edge {
                                QuietEdge::Start => quiet.start = moved(quiet.start),
                                QuietEdge::End => quiet.end = moved(quiet.end),
                            }
                        },
                        cx,
                    );
                }))
        };
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1))
            .child(step(
                KEY_EARLIER,
                IconName::Minus,
                EARLIER_LABEL,
                -QUIET_STEP_MINUTES,
                cx,
            ))
            .child(
                div()
                    .w(px(CLOCK_W))
                    .flex()
                    .justify_center()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(SharedString::from(time.to_string())),
            )
            .child(step(
                KEY_LATER,
                IconName::Add,
                LATER_LABEL,
                QUIET_STEP_MINUTES,
                cx,
            ))
            .into_any_element();
        form_row(label, None, control, pal)
    }
}

fn shortcut(keys: &'static str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .text_color(rgb(pal.text_muted))
        .child(keys)
        .into_any_element()
}
