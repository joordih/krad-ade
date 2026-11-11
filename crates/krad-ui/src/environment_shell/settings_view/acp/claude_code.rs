use gpui_kit::{
    div, AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::OfferedTwin;

use super::super::sections::settings_section;
use super::super::{form_row, panel_card, EnvironmentShell};
use crate::components::toggle_switch;
use crate::theme::palette;

const SECTION_TITLE: &str = "Claude Code";
const SECTION_HINT: &str = "New Claude Code chats run on the Claude Code CLI. Switch on its ACP adapter to run them over the Agent Client Protocol instead; chats you already have keep the way they started.";
const OVER_ACP_LABEL: &str = "Run over ACP";
const OVER_ACP_DETAIL: &str = "Needs the Claude Code ACP adapter installed";
const OVER_ACP_SWITCH: &str = "acp-claude-code-over-acp";

impl EnvironmentShell {
    pub(super) fn acp_claude_code_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let on = self.settings.offered_twin == OfferedTwin::Acp;
        let control = div()
            .id(ElementId::Name(OVER_ACP_SWITCH.into()))
            .cursor_pointer()
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.settings.offered_twin = this.settings.offered_twin.flipped();
                this.save_settings(cx);
                cx.notify();
            }))
            .child(toggle_switch(
                ElementId::Name(OVER_ACP_SWITCH.into()),
                on,
                pal,
            ))
            .into_any_element();
        settings_section(
            SECTION_TITLE,
            Some(SECTION_HINT),
            [panel_card(
                vec![form_row(
                    OVER_ACP_LABEL,
                    Some(OVER_ACP_DETAIL),
                    control,
                    pal,
                )],
                pal,
            )],
            pal,
        )
    }
}
