use gpui_kit::{
    div, AnyElement, ClickEvent, Context, FontWeight, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::TerminalId;

use super::super::super::cloud::{CloudMenu, RunOn, RunOnChoice};
use super::super::super::context_menu::{MenuAction, MenuEntry};
use super::control_pill;
use super::EnvironmentShell;
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, ICON_XS};

const RUN_ON_PREFIX: &str = "surface-run-on";
const RUN_ON_HEADER: &str = "Run on";
const TOOLTIP_RUN_ON: &str =
    "Where this session runs: here, or as a cloud session on one of your cloud runners";
const CLOSED_HINT: &str = "Reconnect it in Settings to use it";
const DETAIL_SEPARATOR: &str = " · ";

fn choice_entry(choice: &RunOnChoice, chosen: RunOn) -> MenuEntry {
    let entry = MenuEntry::checked(
        choice.label.clone(),
        choice.run_on == chosen,
        MenuAction::Cloud(CloudMenu::RunOn(choice.run_on)),
    )
    .disabled_when(!choice.open);
    match (choice.state, choice.open) {
        (Some(state), true) => entry.with_detail(state),
        (Some(state), false) => {
            entry.with_detail(format!("{state}{DETAIL_SEPARATOR}{CLOSED_HINT}"))
        }
        (None, _) => entry,
    }
}

impl EnvironmentShell {
    pub(super) fn run_on_control(
        &self,
        id: TerminalId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let offer = self.run_on_offer(id)?;
        let pal = palette(cx);
        let choices = self.run_on_choices(cx);
        let label = choices
            .iter()
            .find(|choice| choice.run_on == offer.run_on)
            .map_or_else(|| self.run_on_here_label(), |choice| choice.label.clone());
        let glyph = match offer.run_on {
            RunOn::Here => IconName::Home,
            RunOn::Runner(_) => IconName::Cloud,
        };
        let entries: Vec<MenuEntry> = std::iter::once(MenuEntry::Header(RUN_ON_HEADER.to_owned()))
            .chain(
                choices
                    .iter()
                    .map(|choice| choice_entry(choice, offer.run_on)),
            )
            .collect();
        let entries = std::rc::Rc::new(entries);
        Some(
            control_pill((RUN_ON_PREFIX, id.as_u64()), pal)
                .tooltip(|_, cx| tooltip_view(TOOLTIP_RUN_ON.into(), cx))
                .on_click(
                    cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                        this.show_cloud_runners(cx);
                        this.open_menu_above(event, entries.to_vec(), cx);
                    }),
                )
                .child(icon(glyph, ICON_XS, pal.text_muted))
                .child(div().font_weight(FontWeight::MEDIUM).child(label))
                .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
                .into_any_element(),
        )
    }
}
