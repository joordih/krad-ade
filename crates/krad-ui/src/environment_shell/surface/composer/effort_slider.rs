mod embedded;
mod fast;
mod layout;
mod popover;
mod state;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, ease_out_quint, px, rgb, Animation, AnimationExt as _, AnyElement, ClickEvent, Context,
    FocusHandle, FontWeight, Hsla, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Window,
};
use krad_domain::{Effort, EffortSnap, TerminalId};
use std::time::Duration;

use super::super::super::EnvironmentShell;
use super::super::facts::Facts;
use super::super::{Backend, SurfaceCommand};
use super::control_pill;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, tinted, Palette, ICON_XS, LINE_XS, TEXT_XS};
use layout::{level_share, level_tone, mix, trigger, Trigger};
use state::{PopoverMove, PopoverState};

const TRIGGER_PREFIX: &str = "surface-effort";
const NOTICE_PREFIX: &str = "surface-effort-notice";
const METER_BAR_W: f32 = 2.0;
const METER_MIN_H: f32 = 4.0;
const METER_MAX_H: f32 = 11.0;
const METER_GAP: f32 = 1.5;
const METER_IDLE_ALPHA: u32 = 0x40;
const NOTICE_FADE: Duration = Duration::from_millis(240);
const DEFAULT_EFFORT_LABEL: &str = "Default";
const TOOLTIP_EFFORT: &str = "Reasoning effort";
const TOOLTIP_NO_EFFORT: &str = "This model has no reasoning effort levels to choose from";
const TOOLTIP_ONE_EFFORT: &str = "This model thinks at a single effort level";
const NOT_AVAILABLE: &str = "is not available for this model";
const MOVED_TO: &str = "so the effort is set to";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct ChatTuning {
    pub(in crate::environment_shell) fixed_effort: Option<Effort>,
    pub(in crate::environment_shell) thinking: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffortPlacement {
    Floating,
    InPicker,
}

pub(in crate::environment_shell) struct EffortPopover {
    state: PopoverState,
    origin: (f32, f32),
    focus: FocusHandle,
    placement: EffortPlacement,
}

fn notice_text(snap: EffortSnap) -> Option<String> {
    match snap {
        EffortSnap::Kept => None,
        EffortSnap::Moved { from, to } => Some(format!(
            "{} {NOT_AVAILABLE}, {MOVED_TO} {}",
            from.label(),
            to.label()
        )),
        EffortSnap::Dropped(from) => Some(format!("{} {NOT_AVAILABLE}", from.label())),
    }
}

fn level_color(share: f32, pal: &Palette) -> Hsla {
    mix(rgb(pal.text_muted), rgb(pal.accent), level_tone(share)).into()
}

fn current_step(facts: &Facts) -> Option<usize> {
    let shown = facts.effort.or(facts.default_effort)?;
    facts.efforts.iter().position(|step| *step == shown)
}

fn meter(at: Option<usize>, steps: usize, pal: &Palette) -> AnyElement {
    let lit = at.map_or(0.0, |at| level_share(at, steps));
    let color = level_color(lit, pal);
    let idle: Hsla = tinted(pal.text_muted, METER_IDLE_ALPHA).into();
    let bars = (0..steps).map(|step| {
        let height = (METER_MAX_H - METER_MIN_H).mul_add(level_share(step, steps), METER_MIN_H);
        let on = at.is_some_and(|at| step <= at);
        div()
            .w(px(METER_BAR_W))
            .h(px(height))
            .rounded_full()
            .bg(if on { color } else { idle })
    });
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_end()
        .gap(px(METER_GAP))
        .h(px(METER_MAX_H))
        .children(bars)
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn effort_trigger(
        &self,
        id: TerminalId,
        facts: &Facts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pal = palette(cx);
        let steps = facts.efforts.len();
        let label = facts.effort.map_or(DEFAULT_EFFORT_LABEL, Effort::label);
        let pill = match trigger(facts.offers_effort, steps) {
            Trigger::Hidden => return None,
            Trigger::Disabled => {
                let tooltip = if steps == 0 {
                    TOOLTIP_NO_EFFORT
                } else {
                    TOOLTIP_ONE_EFFORT
                };
                control_pill((TRIGGER_PREFIX, id.as_u64()), pal)
                    .cursor_default()
                    .text_color(rgb(pal.text_disabled))
                    .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
                    .child(icon(IconName::Flash, ICON_XS, pal.text_disabled))
                    .child(label)
            }
            Trigger::Enabled => {
                let open = self
                    .effort_popover
                    .as_ref()
                    .is_some_and(|popover| popover.state.chat == id);
                control_pill((TRIGGER_PREFIX, id.as_u64()), pal)
                    .when(open, |pill| pill.bg(pal.bg_selected))
                    .tooltip(|_, cx| tooltip_view(TOOLTIP_EFFORT.into(), cx))
                    .on_click(cx.listener(
                        move |this: &mut Self, event: &ClickEvent, window, cx| {
                            let at = event.position();
                            let origin = (at.x.as_f32(), at.y.as_f32());
                            this.open_effort_popover(id, origin, window, cx);
                        },
                    ))
                    .child(icon(
                        IconName::Flash,
                        ICON_XS,
                        if facts.fast == Some(true) {
                            pal.accent
                        } else {
                            pal.text_muted
                        },
                    ))
                    .child(div().font_weight(FontWeight::MEDIUM).child(label))
                    .child(meter(current_step(facts), steps, pal))
                    .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
            }
        };
        let notice = facts.effort_notice.and_then(notice_text).map(|text| {
            div()
                .flex_none()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_subtle))
                .child(text)
                .with_animation(
                    (NOTICE_PREFIX, id.as_u64()),
                    Animation::new(NOTICE_FADE).with_easing(ease_out_quint()),
                    Styled::opacity,
                )
        });
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .child(pill)
                .children(notice)
                .into_any_element(),
        )
    }

    fn open_effort_popover(
        &mut self,
        id: TerminalId,
        origin: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .effort_popover
            .as_ref()
            .is_some_and(|popover| popover.state.chat == id)
        {
            return self.close_effort_popover(cx);
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.effort_popover = Some(EffortPopover {
            state: PopoverState::new(id),
            origin,
            focus,
            placement: EffortPlacement::Floating,
        });
        cx.notify();
    }

    pub(in crate::environment_shell) fn embed_effort(
        &mut self,
        chat: TerminalId,
        focus: FocusHandle,
    ) {
        self.effort_popover = Some(EffortPopover {
            state: PopoverState::new(chat),
            origin: (0.0, 0.0),
            focus,
            placement: EffortPlacement::InPicker,
        });
    }

    pub(in crate::environment_shell) fn drop_embedded_effort(&mut self) {
        if self
            .effort_popover
            .as_ref()
            .is_some_and(|popover| popover.placement == EffortPlacement::InPicker)
        {
            self.effort_popover = None;
        }
    }

    fn close_effort_popover(&mut self, cx: &mut Context<Self>) {
        self.effort_popover = None;
        cx.notify();
    }

    fn chat_facts(&self, id: TerminalId) -> Option<Facts> {
        self.envs
            .get(&self.active)
            .and_then(|live| live.chats.get(&id))
            .map(|chat| self.composer_facts(id, chat))
    }

    pub(in crate::environment_shell) fn chat_tuning(&self, id: TerminalId) -> Option<ChatTuning> {
        let facts = self.chat_facts(id)?;
        let fixed_effort = match facts.efforts.as_slice() {
            [only] => Some(*only),
            _ => None,
        };
        Some(ChatTuning {
            fixed_effort,
            thinking: facts.thinking,
        })
    }

    fn effort_was(&self, id: TerminalId) -> Option<Effort> {
        match &self.envs.get(&self.active)?.chats.get(&id)?.backend {
            Backend::Structured(structured) => structured.effort_was,
            Backend::Transcript(_) => None,
        }
    }

    fn follow_popover(&mut self, step: PopoverMove, cx: &mut Context<Self>) {
        let Some(chat) = self
            .effort_popover
            .as_ref()
            .map(|popover| popover.state.chat)
        else {
            return;
        };
        match step {
            PopoverMove::Close => self.close_effort_popover(cx),
            PopoverMove::Choose(effort) => {
                self.command_surface(chat, SurfaceCommand::Effort(effort), cx);
            }
            PopoverMove::Redraw => cx.notify(),
            PopoverMove::Ignore => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_says_which_level_is_missing_and_what_it_became() {
        assert_eq!(notice_text(EffortSnap::Kept), None);
        assert_eq!(
            notice_text(EffortSnap::Moved {
                from: Effort::Ultra,
                to: Effort::Max,
            })
            .as_deref(),
            Some("Ultra is not available for this model, so the effort is set to Max")
        );
        assert_eq!(
            notice_text(EffortSnap::Dropped(Effort::Minimal)).as_deref(),
            Some("Minimal is not available for this model")
        );
    }
}
