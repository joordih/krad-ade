mod placing;
mod snap;

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{StackDirection, ToastKind, WindowSide};

use super::EnvironmentShell;
use crate::components::floating::floating_surface;
use crate::components::{icon, Button, IconName};
use crate::theme::{palette, Palette, ICON_SM, LINE_SM, SPACE_1, SPACE_2, SPACE_3, TEXT_SM};

use placing::ToastPlacing;
use snap::{Anchor, Frame};

const TOAST_PREFIX: &str = "toast";
const CLOSE_PREFIX: &str = "toast-close";
const ACTION_PREFIX: &str = "toast-action";
const HOVER_GROUP: &str = "toast-hover";
const TOAST_MAX_W: f32 = 340.0;
const TOAST_CAP: usize = 5;

type ToastRun = Rc<dyn Fn(&mut EnvironmentShell, &mut Window, &mut Context<EnvironmentShell>)>;

#[derive(Clone)]
pub(super) struct ToastAction {
    label: SharedString,
    run: ToastRun,
}

impl ToastAction {
    pub(super) fn new(
        label: impl Into<SharedString>,
        run: impl Fn(&mut EnvironmentShell, &mut Window, &mut Context<EnvironmentShell>) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            run: Rc::new(run),
        }
    }
}

#[derive(Default)]
pub(super) struct Toasts {
    shown: Vec<Toast>,
    seq: u64,
    placing: Option<ToastPlacing>,
}

struct Toast {
    seq: u64,
    kind: ToastKind,
    text: String,
    action: Option<ToastAction>,
    born: Instant,
    held_since: Option<Instant>,
}

impl Toast {
    fn hold(&mut self, held: bool, now: Instant) {
        match (held, self.held_since) {
            (true, None) => self.held_since = Some(now),
            (false, Some(since)) => {
                self.born += now.duration_since(since);
                self.held_since = None;
            }
            (true, Some(_)) | (false, None) => {}
        }
    }

    fn expired(&self, lifetime: Duration, now: Instant) -> bool {
        self.held_since.is_none() && now.duration_since(self.born) >= lifetime
    }
}

const fn kind_icon(kind: ToastKind, pal: &Palette) -> (IconName, u32) {
    match kind {
        ToastKind::Error => (IconName::AlertCircle, pal.error_fg),
        ToastKind::Warning => (IconName::Warning, pal.warning_fg),
        ToastKind::Success => (IconName::CheckCircle, pal.success_fg),
        ToastKind::Info => (IconName::InfoCircle, pal.info_fg),
    }
}

impl EnvironmentShell {
    fn toast_frame(&self, width: f32, height: f32) -> Frame {
        Frame {
            width,
            height,
            sidebar: self.sidebar_width(),
        }
    }

    pub(super) fn toast_stack(
        &self,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.toasts.shown.is_empty() || self.toasts.placing.is_some() {
            return div().into_any_element();
        }
        let frame = self.toast_frame(width, height);
        let anchor = frame.anchor_of(self.settings.toasts.placement);
        let side = frame.side_of(anchor);
        let cards = self
            .toasts
            .shown
            .iter()
            .map(|toast| Self::toast_card(toast, side, cx));
        toast_column(frame, anchor, cards.collect())
    }

    fn toast_card(toast: &Toast, side: WindowSide, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let seq = toast.seq;
        let (glyph, tint) = kind_icon(toast.kind, pal);
        let leading = div()
            .flex_none()
            .child(icon(glyph, ICON_SM, tint))
            .into_any_element();
        let text = div()
            .flex_1()
            .min_w_0()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .child(toast.text.clone())
            .into_any_element();
        let action = toast.action.clone().map(|action| {
            Button::new(SharedString::from(format!("{ACTION_PREFIX}-{seq}")))
                .label(action.label.clone())
                .small()
                .on_click(
                    cx.listener(move |this: &mut Self, _: &ClickEvent, window, cx| {
                        this.dismiss_toast(seq);
                        (action.run)(this, window, cx);
                        cx.notify();
                    }),
                )
                .into_any_element()
        });
        let close = div()
            .flex_none()
            .invisible()
            .group_hover(HOVER_GROUP, Styled::visible)
            .child(close_button(
                SharedString::from(format!("{CLOSE_PREFIX}-{seq}")),
                cx.listener(move |this: &mut Self, _, _, cx| {
                    this.dismiss_toast(seq);
                    cx.notify();
                }),
            ))
            .into_any_element();
        let mut parts = vec![leading, text];
        parts.extend(action);
        parts.push(close);
        if side == WindowSide::Left {
            parts.reverse();
        }
        floating_surface(pal, cx)
            .id(SharedString::from(format!("{TOAST_PREFIX}-{seq}")))
            .group(HOVER_GROUP)
            .max_w(px(TOAST_MAX_W))
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_3))
            .py(px(SPACE_1))
            .min_h(px(LINE_SM + SPACE_3))
            .when(side == WindowSide::Left, |card| card.pl(px(SPACE_1)))
            .when(side == WindowSide::Right, |card| card.pr(px(SPACE_1)))
            .children(parts)
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                this.hold_toast(seq, *hovered, Instant::now());
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn push_toast(&mut self, kind: ToastKind, text: String) {
        self.add_toast(kind, text, None);
    }

    pub(super) fn toast(&mut self, kind: ToastKind, text: &str, cx: &mut Context<Self>) {
        self.push_toast(kind, text.to_owned());
        cx.notify();
    }

    pub(super) fn toast_with_action(
        &mut self,
        kind: ToastKind,
        text: &str,
        action: ToastAction,
        cx: &mut Context<Self>,
    ) {
        self.add_toast(kind, text.to_owned(), Some(action));
        cx.notify();
    }

    fn add_toast(&mut self, kind: ToastKind, text: String, action: Option<ToastAction>) {
        self.activity_log.record(text.clone());
        self.toasts.seq += 1;
        self.toasts.shown.push(Toast {
            seq: self.toasts.seq,
            kind,
            text,
            action,
            born: Instant::now(),
            held_since: None,
        });
        while self.toasts.shown.len() > TOAST_CAP {
            self.toasts.shown.remove(0);
        }
    }

    fn dismiss_toast(&mut self, seq: u64) {
        self.toasts.shown.retain(|toast| toast.seq != seq);
    }

    fn hold_toast(&mut self, seq: u64, held: bool, now: Instant) {
        if let Some(toast) = self.toasts.shown.iter_mut().find(|toast| toast.seq == seq) {
            toast.hold(held, now);
        }
    }

    pub(super) fn expire_toasts(&mut self, now: Instant) -> bool {
        let Some(lifetime) = self.settings.toasts.lifetime.duration() else {
            return false;
        };
        let before = self.toasts.shown.len();
        self.toasts
            .shown
            .retain(|toast| !toast.expired(lifetime, now));
        self.toasts.shown.len() != before
    }
}

fn toast_column(frame: Frame, anchor: Anchor, cards: Vec<AnyElement>) -> AnyElement {
    let side = frame.side_of(anchor);
    let direction = frame.direction_of(anchor);
    div()
        .absolute()
        .flex()
        .gap(px(SPACE_2))
        .map(|column| match direction {
            StackDirection::Down => column.flex_col().top(px(anchor.y)),
            StackDirection::Up => column
                .flex_col_reverse()
                .bottom(px(frame.height - anchor.y)),
        })
        .map(|column| match side {
            WindowSide::Left => column.items_start().left(px(anchor.x)),
            WindowSide::Right => column.items_end().right(px(frame.width - anchor.x)),
        })
        .children(cards)
        .into_any_element()
}

fn close_button(
    id: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id)
        .icon(IconName::Close)
        .small()
        .on_click(on_click)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIFETIME: Duration = Duration::from_secs(6);

    fn toast(born: Instant) -> Toast {
        Toast {
            seq: 1,
            kind: ToastKind::Info,
            text: String::new(),
            action: None,
            born,
            held_since: None,
        }
    }

    #[test]
    fn a_toast_goes_once_its_lifetime_has_passed() {
        let born = Instant::now();
        let toast = toast(born);
        assert!(!toast.expired(LIFETIME, born + Duration::from_secs(5)));
        assert!(toast.expired(LIFETIME, born + LIFETIME));
    }

    #[test]
    fn pointing_at_a_toast_holds_it_and_the_time_held_is_not_counted() {
        let born = Instant::now();
        let mut toast = toast(born);
        toast.hold(true, born + Duration::from_secs(4));
        assert!(!toast.expired(LIFETIME, born + Duration::from_secs(60)));
        toast.hold(false, born + Duration::from_secs(64));
        assert!(!toast.expired(LIFETIME, born + Duration::from_secs(65)));
        assert!(toast.expired(LIFETIME, born + Duration::from_secs(66)));
    }
}
