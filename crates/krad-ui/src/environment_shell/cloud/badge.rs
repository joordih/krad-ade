use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::CloudRunnerId;

use super::super::EnvironmentShell;
use super::stops::REMOVED_RUNNER;
use super::RunnerState;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, Palette, ICON_SM};

const MARKER_PREFIX: &str = "cloud-session-";
const BADGE_PREFIX: &str = "Cloud session on";
const BADGE_SEPARATOR: &str = " · ";
const DOT: f32 = 6.0;
const DOT_OUTSET: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum RunnerLink {
    Connected,
    Detached,
    WaitingForYou,
}

impl RunnerLink {
    const fn of(state: &RunnerState, waiting: bool) -> Self {
        match (state.is_ready(), waiting) {
            (true, true) => Self::WaitingForYou,
            (true, false) => Self::Connected,
            (false, _) => Self::Detached,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Detached => "detached",
            Self::WaitingForYou => "waiting for you",
        }
    }

    const fn color(self, pal: &Palette) -> u32 {
        match self {
            Self::Connected => pal.success_fg,
            Self::Detached => pal.text_muted,
            Self::WaitingForYou => pal.warning_fg,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::environment_shell) struct CloudBadge {
    tooltip: SharedString,
    link: RunnerLink,
}

fn badge_text(runner: &str, link: RunnerLink, state: &RunnerState) -> String {
    let mut text = format!("{BADGE_PREFIX} {runner}{BADGE_SEPARATOR}{}", link.label());
    if link == RunnerLink::Detached {
        text.push_str(BADGE_SEPARATOR);
        text.push_str(&state.label());
    }
    text
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_badge(
        &self,
        runner: CloudRunnerId,
        waiting: bool,
    ) -> CloudBadge {
        if self.runner_removed_from_settings(runner) {
            return CloudBadge {
                tooltip: REMOVED_RUNNER.into(),
                link: RunnerLink::Detached,
            };
        }
        let state = self.runner_state(runner);
        let link = RunnerLink::of(&state, waiting);
        CloudBadge {
            tooltip: badge_text(&self.runner_name(runner), link, &state).into(),
            link,
        }
    }
}

pub(in crate::environment_shell) fn cloud_marker(
    session: u64,
    badge: CloudBadge,
    cx: &mut Context<EnvironmentShell>,
) -> AnyElement {
    let pal = palette(cx);
    let CloudBadge { tooltip, link } = badge;
    div()
        .id(SharedString::from(format!("{MARKER_PREFIX}{session}")))
        .relative()
        .flex_none()
        .child(icon(IconName::Cloud, ICON_SM, pal.text_muted))
        .child(
            div()
                .absolute()
                .bottom(px(-DOT_OUTSET))
                .right(px(-DOT_OUTSET))
                .size(px(DOT))
                .rounded_full()
                .bg(rgb(link.color(pal))),
        )
        .tooltip(move |_, cx| tooltip_view(tooltip.clone(), cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_host::runner::Linger;

    #[test]
    fn the_badge_names_the_runner_and_how_the_session_is_reached() {
        let ready = RunnerState::Ready {
            linger: Linger::Enabled,
        };
        assert_eq!(RunnerLink::of(&ready, false), RunnerLink::Connected);
        assert_eq!(RunnerLink::of(&ready, true), RunnerLink::WaitingForYou);
        assert_eq!(
            RunnerLink::of(&RunnerState::Connecting, true),
            RunnerLink::Detached
        );
        assert_eq!(
            badge_text("gpu", RunnerLink::Connected, &ready),
            "Cloud session on gpu · connected"
        );
        assert_eq!(
            badge_text("gpu", RunnerLink::Detached, &RunnerState::Connecting),
            "Cloud session on gpu · detached · Connecting"
        );
    }
}
