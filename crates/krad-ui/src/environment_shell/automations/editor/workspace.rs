use gpui_kit::{
    div, px, AnyElement, App, Context, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window,
};
use krad_domain::{CatchUp, Conversation, WorkingCopy};
use std::path::PathBuf;
use std::rc::Rc;

use super::super::super::dialog_parts::{segment, SegmentSize};
use super::super::super::EnvironmentShell;
use super::super::AutomationMenu;
use super::parts::{chips, field, field_group, hint, panel};
use super::AutomationEditor;
use crate::components::motion::reveal;
use crate::components::{glide_track, Button, GlideOption, GlidePick, GlideSizing, IconName};
use crate::theme::{Palette, SPACE_2, SPACE_3};

const COPY_TRACK: &str = "automation-working-copy";
const THREAD_TRACK: &str = "automation-thread";
const MISSED_TRACK: &str = "automation-missed";
const GROUP_PREFIX: &str = "automation-group";
const FINE_PRINT_REVEAL: &str = "automation-fine-print";
const BTN_FINE_PRINT: &str = "automation-fine-print-toggle";
const FINE_PRINT_H: f32 = 180.0;
const CHECKOUT_LABEL: &str = "Checkout";
const THREAD_LABEL: &str = "Thread";
const LANDS_LABEL: &str = "Shows up in";
const OWN_GROUP: &str = "A group of its own";
const LANDS_HINT: &str = "A sidebar group only works with the checkout right here.";
const MISSED_SUMMARY: &str = "Clock cues that fall while the app is closed are skipped.";
const FINE_PRINT_OPEN: &str = "Hide the fine print";
const FINE_PRINT_CLOSED: &str = "Make up for missed beats";
const MISSED_LABEL: &str = "Make up for one missed beat";
const MISSED_DETAIL: &str =
    "When the app starts, a single beat skipped within this span runs right away.";

fn options<T: Copy>(all: &[T], label: fn(T) -> &'static str) -> Vec<GlideOption> {
    all.iter()
        .map(|item| GlideOption {
            label: label(*item).into(),
            glyph: None,
        })
        .collect()
}

fn picker(cx: &Context<EnvironmentShell>, change: fn(&mut AutomationEditor, usize)) -> GlidePick {
    let shell = cx.entity().downgrade();
    Rc::new(move |index: usize, _: &mut Window, cx: &mut App| {
        shell
            .update(cx, |shell, cx| {
                shell.edit_automation_draft(cx, |editor| change(editor, index));
            })
            .unwrap_or_default();
    })
}

fn pick_copy(editor: &mut AutomationEditor, index: usize) {
    let Some(copy) = WorkingCopy::ALL.get(index).copied() else {
        return;
    };
    editor.draft.session.working_copy = copy;
    if copy == WorkingCopy::FreshWorktree {
        editor.draft.session.group = None;
    }
}

fn pick_thread(editor: &mut AutomationEditor, index: usize) {
    if let Some(conversation) = Conversation::ALL.get(index).copied() {
        editor.draft.session.conversation = conversation;
    }
}

fn pick_missed(editor: &mut AutomationEditor, index: usize) {
    if let Some(window) = CatchUp::ALL.get(index).copied() {
        editor.draft.catch_up = window;
    }
}

impl EnvironmentShell {
    pub(super) fn workspace_step(
        &self,
        editor: &AutomationEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let session = &editor.draft.session;
        let copy_at = WorkingCopy::ALL
            .iter()
            .position(|copy| *copy == session.working_copy);
        let thread_at = Conversation::ALL
            .iter()
            .position(|conversation| *conversation == session.conversation);
        let copies = glide_track(
            COPY_TRACK,
            options(&WorkingCopy::ALL, WorkingCopy::label),
            copy_at,
            &picker(cx, pick_copy),
            GlideSizing::Fill,
            pal,
            cx,
        );
        let threads = glide_track(
            THREAD_TRACK,
            options(&Conversation::ALL, Conversation::label),
            thread_at,
            &picker(cx, pick_thread),
            GlideSizing::Fill,
            pal,
            cx,
        );
        let mut groups: Vec<(SharedString, Option<PathBuf>)> = vec![(OWN_GROUP.into(), None)];
        groups.extend(self.envs.get(&self.active).into_iter().flat_map(|live| {
            live.data.groups.iter().map(|group| {
                (
                    SharedString::from(group.name.clone()),
                    Some(group.root.clone()),
                )
            })
        }));
        let groups: Vec<AnyElement> = groups
            .into_iter()
            .enumerate()
            .map(|(index, (name, root))| {
                let selected = session.group == root;
                segment(
                    (GROUP_PREFIX, index),
                    name,
                    Some(IconName::Folder),
                    selected,
                    SegmentSize::Compact,
                    pal,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                    this.run_automation_menu(AutomationMenu::Group(root.clone()), window, cx);
                }))
                .into_any_element()
            })
            .collect();
        panel(pal)
            .gap(px(SPACE_3))
            .child(
                field_group()
                    .child(field(CHECKOUT_LABEL, pal))
                    .child(copies),
            )
            .child(field_group().child(field(THREAD_LABEL, pal)).child(threads))
            .child(
                field_group()
                    .child(field(LANDS_LABEL, pal))
                    .child(chips().children(groups))
                    .child(hint(LANDS_HINT, pal)),
            )
            .into_any_element()
    }

    pub(super) fn missed_step(
        editor: &AutomationEditor,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = editor.fine_print_open;
        let toggle = Button::new(BTN_FINE_PRINT)
            .label(if open {
                FINE_PRINT_OPEN
            } else {
                FINE_PRINT_CLOSED
            })
            .icon(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .small()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.edit_automation_draft(cx, |editor| {
                    editor.fine_print_open = !editor.fine_print_open;
                });
            }));
        let missed_at = CatchUp::ALL
            .iter()
            .position(|window| *window == editor.draft.catch_up);
        let track = glide_track(
            MISSED_TRACK,
            options(&CatchUp::ALL, CatchUp::label),
            missed_at,
            &picker(cx, pick_missed),
            GlideSizing::Fill,
            pal,
            cx,
        );
        let fine_print = panel(pal)
            .child(field(MISSED_LABEL, pal))
            .child(hint(MISSED_DETAIL, pal))
            .child(div().w_full().child(track));
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(hint(MISSED_SUMMARY, pal))
            .child(div().flex().flex_row().child(toggle))
            .child(reveal(
                FINE_PRINT_REVEAL,
                open,
                FINE_PRINT_H,
                div().flex().flex_col().child(fine_print),
            ))
            .into_any_element()
    }
}
