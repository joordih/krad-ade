use gpui_kit::{
    div, px, rgb, AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use krad_domain::SessionId;

use super::super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_SM, LINE_SM, RADIUS_XS, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM};

const ROW_PREFIX: &str = "session-cloud-action";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) enum CloudAction {
    Send,
    Push,
    OpenRequest,
    BringHome,
}

impl CloudAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Send => "Send to cloud…",
            Self::Push => "Push branch",
            Self::OpenRequest => "Open pull request",
            Self::BringHome => "Bring to local",
        }
    }

    const fn glyph(self) -> IconName {
        match self {
            Self::Send => IconName::Cloud,
            Self::Push => IconName::Send,
            Self::OpenRequest => IconName::PullRequest,
            Self::BringHome => IconName::GitBranch,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Send => 0,
            Self::Push => 1,
            Self::OpenRequest => 2,
            Self::BringHome => 3,
        }
    }
}

const RESULTS: [CloudAction; 3] = [
    CloudAction::Push,
    CloudAction::OpenRequest,
    CloudAction::BringHome,
];

impl EnvironmentShell {
    pub(in crate::environment_shell) fn session_cloud_actions(
        &self,
        session: SessionId,
    ) -> Vec<CloudAction> {
        if self.can_send_to_cloud(session) {
            return vec![CloudAction::Send];
        }
        let runner = self
            .envs
            .get(&self.active)
            .and_then(|live| live.data.session(session))
            .and_then(|found| found.cloud.as_ref())
            .map(|placement| placement.runner);
        match runner {
            Some(runner) if !self.runner_removed_from_settings(runner) => RESULTS.to_vec(),
            Some(_) | None => Vec::new(),
        }
    }

    pub(in crate::environment_shell) fn run_cloud_action(
        &mut self,
        action: CloudAction,
        session: Option<SessionId>,
        origin: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (action, session) {
            (CloudAction::Send, Some(session)) => {
                self.offer_send_to_cloud(session, origin, window, cx);
            }
            (CloudAction::Send, None) => self.send_current_to_cloud(window, cx),
            (CloudAction::Push, _) => self.push_cloud_branch(session, cx),
            (CloudAction::OpenRequest, _) => self.open_cloud_request(session, cx),
            (CloudAction::BringHome, _) => self.bring_cloud_home(session, cx),
        }
    }

    pub(in crate::environment_shell) fn session_cloud_rows(
        &self,
        session: SessionId,
        origin: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        self.session_cloud_actions(session)
            .into_iter()
            .map(|action| {
                div()
                    .id(SharedString::from(format!(
                        "{ROW_PREFIX}-{}-{}",
                        session.as_u64(),
                        action.index()
                    )))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_3))
                    .px(px(SPACE_2))
                    .py(px(SPACE_1_5))
                    .rounded(px(RADIUS_XS))
                    .cursor_pointer()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .hover(|style| style.bg(pal.bg_hover))
                    .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                        this.close_name_dialog(cx);
                        this.run_cloud_action(action, Some(session), origin, window, cx);
                    }))
                    .child(icon(action.glyph(), ICON_SM, pal.text_muted))
                    .child(SharedString::from(action.label()))
                    .into_any_element()
            })
            .collect()
    }
}
