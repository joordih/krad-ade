use gpui_kit::{div, px, rgb, AnyElement, FontWeight, IntoElement, ParentElement, Styled, Svg};
use krad_domain::review::{CiStatus, MergeRequestState, MergeStatus, User};

use super::words::HostWords;

use super::summary::initials;
use crate::components::{icon, IconName};
use crate::theme::{tinted, Palette, CHIP_H, LINE_2XS, LINE_XS, SPACE_2, TEXT_2XS, TEXT_XS};

pub(super) const WASH_ALPHA: u32 = 0x26;
const AVATAR: f32 = 24.0;

pub(super) const fn state_label(state: MergeRequestState) -> &'static str {
    match state {
        MergeRequestState::Opened => "Open",
        MergeRequestState::Closed => "Closed",
        MergeRequestState::Locked => "Locked",
        MergeRequestState::Merged => "Merged",
        MergeRequestState::Other => "Unknown state",
    }
}

pub(super) const fn merge_status_label(
    status: Option<MergeStatus>,
    words: &HostWords,
) -> &'static str {
    match status {
        Some(MergeStatus::Mergeable) => "Ready to merge",
        Some(MergeStatus::Checking | MergeStatus::Unchecked) | None => words.checking,
        Some(MergeStatus::Conflict) => "Has conflicts",
        Some(MergeStatus::NeedRebase) => "Needs a rebase",
        Some(MergeStatus::NotApproved) => "Needs approval",
        Some(MergeStatus::DiscussionsNotResolved) => "Has unresolved threads",
        Some(MergeStatus::DraftStatus) => "Is a draft",
        Some(MergeStatus::CiMustPass) => "Waits for a passing pipeline",
        Some(MergeStatus::CiStillRunning) => "Pipeline still running",
        Some(MergeStatus::Other) => words.blocks,
    }
}

pub(super) const fn state_color(state: MergeRequestState, pal: &Palette) -> u32 {
    match state {
        MergeRequestState::Opened => pal.info_fg,
        MergeRequestState::Merged => pal.success_fg,
        MergeRequestState::Closed => pal.error_fg,
        MergeRequestState::Locked | MergeRequestState::Other => pal.text_muted,
    }
}

pub(super) fn status_pill(text: &'static str, color: u32) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(CHIP_H))
        .px(px(SPACE_2))
        .rounded_full()
        .bg(tinted(color, WASH_ALPHA))
        .text_color(rgb(color))
        .text_size(px(TEXT_2XS))
        .line_height(px(LINE_2XS))
        .font_weight(FontWeight::MEDIUM)
        .child(text)
        .into_any_element()
}

pub(super) const fn ci_icon(status: Option<CiStatus>) -> IconName {
    match status {
        Some(CiStatus::Success) => IconName::CheckCircle,
        Some(CiStatus::Failed | CiStatus::Canceled) => IconName::CancelCircle,
        Some(
            CiStatus::Running
            | CiStatus::Pending
            | CiStatus::Preparing
            | CiStatus::Created
            | CiStatus::WaitingForResource
            | CiStatus::Canceling
            | CiStatus::Scheduled,
        ) => IconName::Loading,
        Some(CiStatus::Manual | CiStatus::Skipped | CiStatus::Other) | None => IconName::Minus,
    }
}

pub(super) const fn ci_color(status: Option<CiStatus>, pal: &Palette) -> u32 {
    match status {
        Some(CiStatus::Success) => pal.success_fg,
        Some(CiStatus::Failed | CiStatus::Canceled) => pal.error_fg,
        Some(CiStatus::Manual | CiStatus::Skipped | CiStatus::Other) | None => pal.text_disabled,
        Some(_) => pal.text_muted,
    }
}

pub(super) const fn ci_label(status: CiStatus) -> &'static str {
    match status {
        CiStatus::Created => "Created",
        CiStatus::WaitingForResource => "Waiting for resource",
        CiStatus::Preparing => "Preparing",
        CiStatus::Pending => "Pending",
        CiStatus::Running => "Running",
        CiStatus::Success => "Passed",
        CiStatus::Failed => "Failed",
        CiStatus::Canceling => "Canceling",
        CiStatus::Canceled => "Canceled",
        CiStatus::Skipped => "Skipped",
        CiStatus::Manual => "Manual",
        CiStatus::Scheduled => "Scheduled",
        CiStatus::Other => "Unknown",
    }
}

pub(super) fn ci_glyph(status: Option<CiStatus>, size: f32, pal: &Palette) -> Svg {
    icon(ci_icon(status), size, ci_color(status, pal))
}

pub(super) fn avatar(user: &User, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(AVATAR))
        .rounded_full()
        .bg(rgb(pal.bg_panel))
        .text_size(px(TEXT_2XS))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(pal.text_default))
        .child(initials(&user.name))
        .into_any_element()
}

pub(super) fn meta_line(text: &str, pal: &Palette) -> AnyElement {
    div()
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(text.to_owned())
        .into_any_element()
}
