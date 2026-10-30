use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, ease_out_quint, px, rgb, Animation, AnimationExt as _, AnyElement, Context, ElementId,
    FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled,
};
use krad_chat::ToolCall;
use krad_domain::{TerminalId, ToolKind};
use std::path::Path;
use std::time::Duration;

use super::super::EnvironmentShell;
use super::edit_diff::{edit_counts, edit_diff};
use crate::components::{icon, shimmer_text, IconName};
use crate::theme::{
    palette, Palette, CHIP_H, ICON_SM, ICON_XS, LINE_SM, RADIUS_XS, SPACE_1, SPACE_1_5, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const HEADER_PREFIX: &str = "surface-tools";
const ROW_PREFIX: &str = "surface-tool-row";
const SUMMARY_SEPARATOR: &str = ", ";
const RAIL_INSET: f32 = ICON_SM / 2.0;
const RAIL_W: f32 = 1.0;
const ELBOW_W: f32 = 10.0;
const ELBOW_RADIUS: f32 = 4.0;
const ROW_H: f32 = 22.0;
const ROW_GAP: f32 = 2.0;
const ROWS_GAP_TOP: f32 = 4.0;
const ROW_ENTER: Duration = Duration::from_millis(260);
const ROW_ENTER_RISE: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolLook {
    Shell,
    Read,
    Edit,
    Search,
    Agent,
    Plan,
    Web,
    Other,
}

struct Wording {
    row: &'static str,
    summary: &'static str,
    one: &'static str,
    many: &'static str,
    doing: &'static str,
}

impl From<ToolKind> for ToolLook {
    fn from(kind: ToolKind) -> Self {
        match kind {
            ToolKind::Shell => Self::Shell,
            ToolKind::Read => Self::Read,
            ToolKind::Edit => Self::Edit,
            ToolKind::Search => Self::Search,
            ToolKind::Subagent => Self::Agent,
            ToolKind::Checklist => Self::Plan,
            ToolKind::Fetch => Self::Web,
            ToolKind::Question | ToolKind::Plan | ToolKind::Other => Self::Other,
        }
    }
}

impl ToolLook {
    pub(super) const fn icon(self) -> IconName {
        match self {
            Self::Shell => IconName::Terminal,
            Self::Read | Self::Edit => IconName::File,
            Self::Search => IconName::Search,
            Self::Agent => IconName::Workflow,
            Self::Plan => IconName::Tasks,
            Self::Web => IconName::OpenExternal,
            Self::Other => IconName::Plug,
        }
    }

    const fn wording(self) -> Wording {
        let (row, summary, one, many, doing) = match self {
            Self::Shell => ("Ran", "Ran", "command", "commands", "Running commands"),
            Self::Read => ("Read", "Read", "file", "files", "Reading files"),
            Self::Edit => ("Edited", "Edited", "file", "files", "Editing files"),
            Self::Search => ("Searched", "Ran", "search", "searches", "Searching"),
            Self::Agent => ("Started", "Started", "agent", "agents", "Starting an agent"),
            Self::Plan => ("Updated", "Updated", "plan", "plans", "Planning"),
            Self::Web => ("Fetched", "Fetched", "page", "pages", "Fetching the web"),
            Self::Other => ("Used", "Used", "tool", "tools", "Using tools"),
        };
        Wording {
            row,
            summary,
            one,
            many,
            doing,
        }
    }

    const fn names_file(self) -> bool {
        matches!(self, Self::Read | Self::Edit)
    }
}

fn done_summary(calls: &[(usize, &ToolCall)]) -> String {
    let mut counts: Vec<(ToolLook, usize)> = Vec::new();
    for (_, call) in calls {
        let kind = ToolLook::from(call.kind);
        match counts.iter_mut().find(|(counted, _)| *counted == kind) {
            Some((_, count)) => *count += 1,
            None => counts.push((kind, 1)),
        }
    }
    let summary = counts
        .iter()
        .map(|(kind, count)| {
            let wording = kind.wording();
            let noun = if *count == 1 {
                wording.one
            } else {
                wording.many
            };
            format!("{} {count} {noun}", wording.summary.to_ascii_lowercase())
        })
        .collect::<Vec<_>>()
        .join(SUMMARY_SEPARATOR);
    let mut letters = summary.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

fn file_chip(path: &str, pal: &Palette) -> AnyElement {
    let name = Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(CHIP_H))
        .px(px(SPACE_1_5))
        .rounded(px(RADIUS_XS))
        .bg(rgb(pal.bg_panel))
        .border_1()
        .border_color(pal.divider)
        .text_color(rgb(pal.text_strong))
        .child(icon(IconName::File, ICON_XS, pal.text_muted))
        .child(name)
        .into_any_element()
}

fn rail(last: bool, pal: &Palette) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(ELBOW_W))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .w(px(ELBOW_W))
                .h(px(ROW_H / 2.0))
                .border_l(px(RAIL_W))
                .border_b(px(RAIL_W))
                .rounded_bl(px(ELBOW_RADIUS))
                .border_color(pal.divider),
        )
        .when(!last, |rail| {
            rail.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom(px(-ROW_GAP))
                    .w(px(RAIL_W))
                    .bg(pal.divider),
            )
        })
}

fn call_row(call: &ToolCall, last: bool, mono: &SharedString, pal: &Palette) -> gpui_kit::Div {
    let kind = ToolLook::from(call.kind);
    let verb = match kind {
        ToolLook::Other => call.name.clone(),
        _ => kind.wording().row.to_owned(),
    };
    let target = if kind.names_file() && !call.summary.is_empty() {
        file_chip(&call.summary, pal)
    } else {
        div()
            .min_w_0()
            .truncate()
            .font_family(mono.clone())
            .text_size(px(TEXT_XS))
            .text_color(rgb(pal.text_muted))
            .child(call.summary.clone())
            .into_any_element()
    };
    let line = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .min_w_0()
        .h(px(ROW_H))
        .child(
            div()
                .flex_none()
                .text_color(rgb(pal.text_default))
                .child(verb),
        )
        .child(target)
        .children(call.edit.as_ref().map(|edit| {
            div()
                .flex_none()
                .font_family(mono.clone())
                .text_size(px(TEXT_XS))
                .text_color(rgb(pal.text_muted))
                .child(edit_counts(edit))
        }));
    div()
        .relative()
        .flex()
        .flex_col()
        .min_w_0()
        .pl(px(ELBOW_W + SPACE_1_5))
        .child(rail(last, pal))
        .child(line)
        .children(call.edit.as_ref().map(|edit| edit_diff(edit, mono, pal)))
}

impl EnvironmentShell {
    pub(super) fn tool_group(
        id: TerminalId,
        key: (usize, usize),
        calls: &[(usize, &ToolCall)],
        running: bool,
        open: Option<bool>,
        mono: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let open = open.unwrap_or(running);
        let raw = id.as_u64();
        let kind = calls
            .last()
            .map_or(ToolLook::Other, |(_, call)| ToolLook::from(call.kind));
        let header_id = ElementId::Name(SharedString::from(format!(
            "{HEADER_PREFIX}-{raw}-{}-{}",
            key.0, key.1
        )));
        let label: AnyElement = if running {
            shimmer_text(
                ElementId::Name(SharedString::from(format!(
                    "{HEADER_PREFIX}-label-{raw}-{}-{}",
                    key.0, key.1
                ))),
                kind.wording().doing.into(),
                pal.text_strong,
            )
            .into_any_element()
        } else {
            div()
                .min_w_0()
                .truncate()
                .text_color(rgb(pal.text_default))
                .child(done_summary(calls))
                .into_any_element()
        };
        let header = div()
            .id(header_id)
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .min_w_0()
            .cursor_pointer()
            .font_weight(FontWeight::MEDIUM)
            .hover(|style| style.text_color(rgb(pal.text_strong)))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.toggle_tool_group(id, key, open, cx);
            }))
            .child(icon(kind.icon(), ICON_SM, pal.text_muted))
            .child(label)
            .child(div().flex_1())
            .child(icon(
                if open {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                },
                ICON_SM,
                pal.text_muted,
            ));
        let last = calls.len().saturating_sub(1);
        let rows = calls.iter().enumerate().map(|(position, (part, call))| {
            let row = call_row(call, position == last, mono, pal);
            if !running {
                return row.into_any_element();
            }
            let enter = Animation::new(ROW_ENTER).with_easing(ease_out_quint());
            let row_id = ElementId::Name(SharedString::from(format!(
                "{ROW_PREFIX}-{raw}-{}-{part}",
                key.0
            )));
            row.with_animation(row_id, enter, |row, shown| {
                row.opacity(shown).top(px((1.0 - shown) * ROW_ENTER_RISE))
            })
            .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .text_color(rgb(pal.text_default))
            .child(header)
            .when(open, |group| {
                group.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(ROW_GAP))
                        .pt(px(ROWS_GAP_TOP))
                        .pl(px(RAIL_INSET))
                        .children(rows),
                )
            })
            .into_any_element()
    }

    fn toggle_tool_group(
        &mut self,
        id: TerminalId,
        key: (usize, usize),
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.tool_groups.insert(key, !open);
            chat.thread.remeasure();
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, kind: ToolKind) -> ToolCall {
        ToolCall {
            id: None,
            name: name.to_owned(),
            kind,
            summary: String::new(),
            edit: None,
        }
    }

    #[test]
    fn questions_plans_and_unknown_tools_look_alike() {
        assert_eq!(ToolLook::from(ToolKind::Subagent), ToolLook::Agent);
        assert_eq!(ToolLook::from(ToolKind::Checklist), ToolLook::Plan);
        assert_eq!(ToolLook::from(ToolKind::Question), ToolLook::Other);
        assert_eq!(ToolLook::from(ToolKind::Plan), ToolLook::Other);
    }

    #[test]
    fn a_finished_step_counts_its_calls_by_kind_in_the_order_they_came() {
        let (bash, read, edit) = (
            call("Bash", ToolKind::Shell),
            call("Read", ToolKind::Read),
            call("Edit", ToolKind::Edit),
        );
        let calls = [(0, &bash), (1, &read), (2, &bash), (3, &edit)];
        assert_eq!(
            done_summary(&calls),
            "Ran 2 commands, read 1 file, edited 1 file"
        );
        assert_eq!(done_summary(&[]), "");
    }
}
