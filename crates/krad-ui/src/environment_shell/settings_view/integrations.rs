use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use super::gitlab::page::KEYRING_NOTE;
use super::{hint, EnvironmentShell, SettingsUi};
use crate::components::{icon, IconName};
use crate::theme::{
    palette, Palette, ICON, ICON_SM, LINE_SM, SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM,
};

const HOSTS_HINT: &str =
    "Review shows the merge requests of a group whose origin is on a host you are signed in to.";
const GITLAB_TITLE: &str = "GitLab";
const GITHUB_TITLE: &str = "GitHub";
const GITLAB_SETTINGS_LABEL: &str = "GitLab instance and OAuth application";
const FOLD_ID: &str = "integrations-gitlab-fold";
const SECTION_GAP: f32 = 20.0;

fn provider_heading(mark: IconName, title: &'static str, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .text_size(px(TEXT_SM))
        .line_height(px(LINE_SM))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(pal.text_strong))
        .child(icon(mark, ICON, pal.text_default))
        .child(title)
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn integrations_pane(&self, ui: &SettingsUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let gitlab = &ui.gitlab;
        let unfolded = gitlab.unfolded;
        let fold = div()
            .id(FOLD_ID)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_1_5))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                if let Some(ui) = this.settings_ui.as_mut() {
                    ui.gitlab.unfolded = !ui.gitlab.unfolded;
                }
                cx.notify();
            }))
            .child(icon(
                if unfolded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                ICON_SM,
                pal.text_muted,
            ))
            .child(GITLAB_SETTINGS_LABEL);
        div()
            .flex()
            .flex_col()
            .gap(px(SECTION_GAP))
            .child(hint(HOSTS_HINT, pal))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .child(provider_heading(IconName::Merge, GITLAB_TITLE, pal))
                    .child(Self::status_card(&gitlab.status, cx))
                    .child(fold)
                    .when(unfolded, |section| {
                        section
                            .child(Self::oauth_section(gitlab, cx))
                            .child(self.method_section(cx))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .child(provider_heading(IconName::PullRequest, GITHUB_TITLE, pal))
                    .child(Self::github_status_card(&ui.github.status, cx)),
            )
            .child(self.projects_card(gitlab, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(SPACE_3))
                    .child(icon(IconName::Lock, ICON_SM, pal.text_muted))
                    .child(div().flex_1().min_w_0().child(hint(KEYRING_NOTE, pal))),
            )
            .into_any_element()
    }
}
