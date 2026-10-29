use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::GroupId;

use super::groups::GroupGit;
use super::EnvironmentShell;
use crate::components::{icon, Button, IconName};
use crate::theme::{
    palette, Palette, ICON_SM, LINE_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_1_5, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const GROUP_HEADER_PREFIX: &str = "group-header";
const UNGROUPED_TITLE: &str = "Terminals";
const NEW_SESSION_PREFIX: &str = "new-session-in-group";
const NEW_SESSION_TOOLTIP: &str = "New session";
const NEEDS_YOU_SUFFIX: &str = "needs you";
const DETACHED_LABEL: &str = "detached";
const AHEAD_MARK: &str = "↑";
const BEHIND_MARK: &str = "↓";
const BRANCH_ICON: f32 = 12.0;
const BRANCH_GAP: f32 = 3.0;

fn branch_caption(git: &GroupGit) -> String {
    let status = &git.status;
    let mut parts = vec![status
        .branch
        .clone()
        .unwrap_or_else(|| DETACHED_LABEL.to_owned())];
    if status.ahead > 0 {
        parts.push(format!("{AHEAD_MARK}{}", status.ahead));
    }
    if status.behind > 0 {
        parts.push(format!("{BEHIND_MARK}{}", status.behind));
    }
    parts.join(" ")
}

struct HeaderInfo {
    id: Option<GroupId>,
    name: String,
    count: Option<usize>,
    branch: Option<String>,
    branch_control: Option<AnyElement>,
    trailing: Option<(String, Trailing)>,
    selected: bool,
    adds_session: bool,
}

#[derive(Clone, Copy)]
enum Trailing {
    Attention,
}

fn branch_label(caption: String, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(BRANCH_GAP))
        .font_weight(FontWeight::NORMAL)
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_muted))
        .child(icon(IconName::GitBranch, BRANCH_ICON, pal.text_muted))
        .child(div().truncate().child(caption))
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn terminal_groups(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(live) = self.envs.get(&self.active) else {
            return Vec::new();
        };
        let in_sessions = true;
        let mut blocks = Vec::new();
        for group in &live.data.groups {
            let git = self.git.get(&group.root);
            let tally = live.data.tally_of_group(Some(group.id));
            let info = HeaderInfo {
                id: Some(group.id),
                name: group.name.clone(),
                count: (!in_sessions).then_some(tally.total()),
                branch: git.map(branch_caption),
                branch_control: None,
                trailing: (tally.waiting > 0).then(|| {
                    (
                        format!("{} {NEEDS_YOU_SUFFIX}", tally.waiting),
                        Trailing::Attention,
                    )
                }),
                selected: live.selected_group == Some(group.id),
                adds_session: in_sessions,
            };
            let mut rows = vec![Self::group_header(info, blocks.is_empty(), cx)];
            if let Some(draft) = self.worktree_draft_label(group.id) {
                rows.push(Self::worktree_draft_row(draft, palette(cx)));
            }
            rows.extend(self.group_rows(Some(group.id), cx));
            blocks.push(Self::group_block(rows));
        }
        let ungrouped = live.data.tally_of_group(None);
        if ungrouped.total() > 0 || live.data.sessions_of(None).next().is_some() {
            let info = HeaderInfo {
                id: None,
                name: UNGROUPED_TITLE.to_owned(),
                count: (!in_sessions).then_some(ungrouped.total()),
                branch: None,
                branch_control: None,
                trailing: (ungrouped.waiting > 0).then(|| {
                    (
                        format!("{} {NEEDS_YOU_SUFFIX}", ungrouped.waiting),
                        Trailing::Attention,
                    )
                }),
                selected: false,
                adds_session: in_sessions,
            };
            let mut rows = vec![Self::group_header(info, blocks.is_empty(), cx)];
            rows.extend(self.group_rows(None, cx));
            blocks.push(Self::group_block(rows));
        }
        blocks
    }

    fn group_block(rows: Vec<AnyElement>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(SPACE_0_5))
            .py(px(SPACE_1))
            .children(rows)
            .into_any_element()
    }

    fn group_header(info: HeaderInfo, first: bool, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let id = info.id;
        let raw = id.map_or(0, GroupId::as_u64);
        let has_trailing = info.trailing.is_some();
        let add_session = info.adds_session.then(|| {
            Button::new((NEW_SESSION_PREFIX, raw))
                .icon(IconName::Add)
                .small()
                .tooltip(NEW_SESSION_TOOLTIP)
                .on_click(cx.listener(move |this: &mut Self, _, _window, cx| {
                    cx.stop_propagation();
                    this.start_chat_session(id, cx);
                }))
        });
        div()
            .id((GROUP_HEADER_PREFIX, raw))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .when(!first, |header| header.mt(px(SPACE_3)))
            .rounded(px(RADIUS_XS))
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(if info.selected {
                pal.text_strong
            } else {
                pal.text_subtle
            }))
            .when_some(id, |header, id| {
                header
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.select_group(id, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this: &mut Self, event: &MouseDownEvent, _, cx| {
                            let entries = this.group_menu(id);
                            this.open_menu_at(event, entries, cx);
                        }),
                    )
            })
            .child(
                div()
                    .flex_shrink_0()
                    .max_w_1_2()
                    .truncate()
                    .child(info.name),
            )
            .children(info.count.map(|count| {
                div()
                    .flex_none()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgb(pal.text_muted))
                    .child(count.to_string())
            }))
            .children(info.branch.map(|caption| branch_label(caption, pal)))
            .children(info.branch_control)
            .children(info.trailing.map(|(text, tone)| {
                div()
                    .ml_auto()
                    .flex_none()
                    .font_weight(FontWeight::NORMAL)
                    .when(matches!(tone, Trailing::Attention), |note| {
                        note.text_color(rgb(pal.attention_text))
                    })
                    .child(text)
            }))
            .children(add_session.map(|button| {
                div()
                    .flex_none()
                    .when(!has_trailing, Styled::ml_auto)
                    .child(button)
            }))
            .into_any_element()
    }

    fn group_rows(&self, group: Option<GroupId>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.session_rows(group, cx)
    }

    fn worktree_draft_row(draft: String, pal: &Palette) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .bg(pal.bg_selected)
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_strong))
            .child(icon(IconName::GitBranch, ICON_SM, pal.accent))
            .child(div().flex_1().min_w_0().truncate().child(draft))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_repo::GitStatus;
    use std::path::PathBuf;

    #[test]
    fn caption_shows_branch_and_only_nonzero_divergence() {
        let mut git = GroupGit {
            repo_root: PathBuf::from("/r"),
            status: GitStatus {
                branch: Some("main".to_owned()),
                ..GitStatus::default()
            },
            lines: std::collections::HashMap::new(),
        };
        assert_eq!(branch_caption(&git), "main");
        git.status.ahead = 2;
        git.status.branch = None;
        assert_eq!(branch_caption(&git), "detached ↑2");
    }
}
