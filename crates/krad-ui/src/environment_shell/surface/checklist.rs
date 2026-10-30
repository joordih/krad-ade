use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::{Checklist, ChecklistItem, ChecklistStatus, TerminalId};

use super::super::ask::tray;
use super::super::chat::ChatMessage;
use super::super::EnvironmentShell;
use super::SurfaceChat;
use crate::components::{icon, IconName};
use crate::theme::{palette, Palette, ICON_SM, LINE_XS, SPACE_1, SPACE_1_5, SPACE_2, TEXT_XS};

const CHECKLIST_TITLE: &str = "Checklist";
const COUNT_SEPARATOR: &str = "/";
const SHOW_ALL_LABEL: &str = "Show all";
const SHOW_LESS_LABEL: &str = "Show less";
const TOGGLE_PREFIX: &str = "surface-checklist-toggle";
const COLLAPSE_AFTER: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum ChecklistView {
    #[default]
    Collapsed,
    Expanded,
}

impl ChecklistView {
    const fn toggled(self) -> Self {
        match self {
            Self::Collapsed => Self::Expanded,
            Self::Expanded => Self::Collapsed,
        }
    }

    const fn toggle_label(self) -> &'static str {
        match self {
            Self::Collapsed => SHOW_ALL_LABEL,
            Self::Expanded => SHOW_LESS_LABEL,
        }
    }
}

fn shown_checklist(messages: &[ChatMessage]) -> Option<&Checklist> {
    let list = messages
        .iter()
        .rev()
        .find_map(|message| message.checklist.as_ref())?;
    let turn_running = messages.last().is_some_and(|last| last.running);
    (turn_running || !list.is_finished()).then_some(list)
}

fn shown_items(list: &Checklist, view: ChecklistView) -> Vec<&ChecklistItem> {
    let folded = view == ChecklistView::Collapsed && list.items.len() > COLLAPSE_AFTER;
    if !folded {
        return list.items.iter().collect();
    }
    list.current()
        .or_else(|| list.items.last())
        .into_iter()
        .collect()
}

const fn mark(status: ChecklistStatus, pal: &Palette) -> (IconName, u32) {
    match status {
        ChecklistStatus::Done => (IconName::CheckCircle, pal.success_fg),
        ChecklistStatus::InProgress => (IconName::Loading, pal.accent),
        ChecklistStatus::Pending => (IconName::RadioButton, pal.text_muted),
    }
}

fn item_row(item: &ChecklistItem, pal: &Palette) -> AnyElement {
    let (mark, mark_color) = mark(item.status, pal);
    let text_color = match item.status {
        ChecklistStatus::Done => pal.text_muted,
        ChecklistStatus::InProgress => pal.text_strong,
        ChecklistStatus::Pending => pal.text_default,
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_1_5))
        .min_w_0()
        .child(icon(mark, ICON_SM, mark_color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(text_color))
                .when(item.status == ChecklistStatus::InProgress, |text| {
                    text.font_weight(FontWeight::MEDIUM)
                })
                .child(item.text.clone()),
        )
        .into_any_element()
}

impl EnvironmentShell {
    pub(super) fn checklist_tray(
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let list = shown_checklist(&chat.messages)?;
        let pal = palette(cx);
        let view = chat.checklist_view;
        let foldable = list.items.len() > COLLAPSE_AFTER;
        let count = format!("{}{COUNT_SEPARATOR}{}", list.done(), list.items.len());
        let toggle = foldable.then(|| {
            div()
                .id((TOGGLE_PREFIX, id.as_u64()))
                .flex_none()
                .cursor_pointer()
                .text_size(px(TEXT_XS))
                .line_height(px(LINE_XS))
                .text_color(rgb(pal.text_muted))
                .hover(|style| style.text_color(rgb(pal.text_strong)))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.toggle_checklist(id);
                    cx.notify();
                }))
                .child(view.toggle_label())
        });
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(icon(IconName::Tasks, ICON_SM, pal.text_muted))
            .child(div().font_weight(FontWeight::MEDIUM).child(CHECKLIST_TITLE))
            .child(
                div()
                    .flex_1()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(count),
            )
            .children(toggle);
        let rows = shown_items(list, view)
            .into_iter()
            .map(|item| item_row(item, pal));
        Some(
            tray(pal)
                .child(header)
                .child(div().flex().flex_col().gap(px(SPACE_1)).children(rows))
                .into_any_element(),
        )
    }

    fn toggle_checklist(&mut self, id: TerminalId) {
        let env = self.active.clone();
        if let Some(chat) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
        {
            chat.checklist_view = chat.checklist_view.toggled();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_chat::ChatEvent;
    use krad_domain::{AgentPreset, ChatEntry};

    use super::super::super::chat::fold_entry;

    fn list(statuses: &[ChecklistStatus]) -> Checklist {
        statuses
            .iter()
            .enumerate()
            .map(|(index, status)| ChecklistItem {
                text: format!("Step {index}"),
                status: *status,
            })
            .collect()
    }

    fn turn(messages: &mut Vec<ChatMessage>, text: &str, events: Vec<ChatEvent>) {
        fold_entry(
            messages,
            &ChatEntry::User {
                text: text.to_owned(),
                preset: AgentPreset::Gateway,
                model: None,
                images: Vec::new(),
            },
        );
        for event in events {
            fold_entry(messages, &ChatEntry::Event(event));
        }
    }

    #[test]
    fn the_latest_checklist_stays_until_it_is_done_and_its_turn_ends() {
        use ChecklistStatus::{Done, InProgress, Pending};
        let mut messages = Vec::new();
        turn(
            &mut messages,
            "fix it",
            vec![ChatEvent::Checklist(list(&[InProgress, Pending]))],
        );
        assert_eq!(shown_checklist(&messages).map(Checklist::done), Some(0));
        fold_entry(&mut messages, &ChatEntry::Event(ChatEvent::Done));
        turn(&mut messages, "go on", Vec::new());
        assert_eq!(
            shown_checklist(&messages).map(Checklist::done),
            Some(0),
            "a new turn keeps the list until the agent sends another"
        );
        fold_entry(
            &mut messages,
            &ChatEntry::Event(ChatEvent::Checklist(list(&[Done, Done]))),
        );
        assert_eq!(shown_checklist(&messages).map(Checklist::done), Some(2));
        fold_entry(&mut messages, &ChatEntry::Event(ChatEvent::Done));
        assert_eq!(shown_checklist(&messages), None);
    }

    #[test]
    fn a_long_checklist_folds_to_the_item_in_progress() {
        use ChecklistStatus::{Done, InProgress, Pending};
        let long = list(&[Done, Done, InProgress, Pending, Pending]);
        let folded: Vec<&str> = shown_items(&long, ChecklistView::Collapsed)
            .into_iter()
            .map(|item| item.text.as_str())
            .collect();
        assert_eq!(folded, ["Step 2"]);
        assert_eq!(shown_items(&long, ChecklistView::Expanded).len(), 5);
        assert_eq!(
            shown_items(&list(&[Done, Pending]), ChecklistView::Collapsed).len(),
            2
        );
        assert_eq!(ChecklistView::Collapsed.toggled(), ChecklistView::Expanded);
    }
}
