use gpui_kit::{
    div, AnyElement, ClickEvent, Context, ElementId, FontWeight, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled as _,
};
use krad_domain::TerminalId;
use std::rc::Rc;

use super::super::super::context_menu::{MenuAction, MenuEntry};
use super::super::super::EnvironmentShell;
use super::super::composer::control_pill;
use super::super::SurfaceChat;
use super::{AcpChoice, AcpOffers};
use crate::components::{icon, tooltip_view, IconName};
use crate::theme::{palette, ICON_XS};

const MODE_PREFIX: &str = "acp-mode";
const MODEL_PREFIX: &str = "acp-model";
const OPTION_PREFIX: &str = "acp-option";
const MODE_HEADER: &str = "Mode";
const MODEL_HEADER: &str = "Model";
const TOOLTIP_MODE: &str = "The agent's mode";
const TOOLTIP_MODEL: &str = "The agent's model";
const TOOLTIP_OPTION: &str = "The agent's setting";

pub(super) struct Picker {
    id: ElementId,
    glyph: Option<IconName>,
    pub(super) label: String,
    tooltip: &'static str,
    entries: Vec<MenuEntry>,
}

fn element_id(prefix: &str, id: TerminalId, index: usize) -> ElementId {
    ElementId::Name(format!("{prefix}-{}-{index}", id.as_u64()).into())
}

fn header(text: &str, entries: impl Iterator<Item = MenuEntry>) -> Vec<MenuEntry> {
    std::iter::once(MenuEntry::Header(text.to_owned()))
        .chain(entries)
        .collect()
}

impl AcpOffers {
    pub(super) fn mode_picker(&self, id: TerminalId) -> Option<Picker> {
        let modes = self.modes()?;
        let label = modes
            .current_mode()
            .map_or_else(|| modes.current.0.clone(), |mode| mode.name.clone());
        let entries = modes.available.iter().map(|mode| {
            MenuEntry::checked(
                mode.name.clone(),
                mode.id == modes.current,
                MenuAction::ChooseAcp(id, AcpChoice::Mode(mode.id.clone())),
            )
        });
        Some(Picker {
            id: element_id(MODE_PREFIX, id, 0),
            glyph: Some(IconName::Lock),
            label,
            tooltip: TOOLTIP_MODE,
            entries: header(MODE_HEADER, entries),
        })
    }

    pub(super) fn model_picker(&self, id: TerminalId) -> Option<Picker> {
        let models = self.models()?;
        let label = models
            .current_model()
            .map_or_else(|| models.current.0.clone(), |model| model.name.clone());
        let entries = models.available.iter().map(|model| {
            MenuEntry::checked(
                model.name.clone(),
                model.id == models.current,
                MenuAction::ChooseAcp(id, AcpChoice::Model(model.id.clone())),
            )
        });
        Some(Picker {
            id: element_id(MODEL_PREFIX, id, 0),
            glyph: None,
            label,
            tooltip: TOOLTIP_MODEL,
            entries: header(MODEL_HEADER, entries),
        })
    }

    pub(super) fn option_pickers(&self, id: TerminalId) -> impl Iterator<Item = Picker> + '_ {
        self.options().enumerate().map(move |(index, option)| {
            let label = option
                .current_choice()
                .map_or_else(|| option.current.0.clone(), |choice| choice.name.clone());
            let entries = option.choices.iter().map(|choice| {
                MenuEntry::checked(
                    choice.name.clone(),
                    choice.value == option.current,
                    MenuAction::ChooseAcp(
                        id,
                        AcpChoice::Option(option.id.clone(), choice.value.clone()),
                    ),
                )
            });
            Picker {
                id: element_id(OPTION_PREFIX, id, index),
                glyph: Some(IconName::Flash),
                label,
                tooltip: TOOLTIP_OPTION,
                entries: header(&option.name, entries),
            }
        })
    }
}

impl EnvironmentShell {
    fn picker_pill(picker: Picker, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Picker {
            id,
            glyph,
            label,
            tooltip,
            entries,
        } = picker;
        let entries = Rc::new(entries);
        control_pill(id, pal)
            .tooltip(move |_, cx| tooltip_view(tooltip.into(), cx))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, _, cx| {
                    this.open_menu_above(event, entries.to_vec(), cx);
                }),
            )
            .children(glyph.map(|glyph| icon(glyph, ICON_XS, pal.text_muted)))
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(icon(IconName::ChevronDown, ICON_XS, pal.text_subtle))
            .into_any_element()
    }

    pub(in crate::environment_shell) fn acp_mode_control(
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = chat.structured()?.acp.mode_picker(id)?;
        Some(Self::picker_pill(picker, cx))
    }

    pub(in crate::environment_shell) fn acp_model_control(
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = chat.structured()?.acp.model_picker(id)?;
        Some(Self::picker_pill(picker, cx))
    }

    pub(in crate::environment_shell) fn acp_option_controls(
        id: TerminalId,
        chat: &SurfaceChat,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(structured) = chat.structured() else {
            return Vec::new();
        };
        let pickers: Vec<Picker> = structured.acp.option_pickers(id).collect();
        pickers
            .into_iter()
            .map(|picker| Self::picker_pill(picker, cx))
            .collect()
    }

    pub(in crate::environment_shell) fn choose_acp(
        &mut self,
        id: TerminalId,
        choice: AcpChoice,
        cx: &mut Context<Self>,
    ) {
        let env = self.active.clone();
        let Some(structured) = self
            .envs
            .get_mut(&env)
            .and_then(|live| live.chats.get_mut(&id))
            .and_then(SurfaceChat::structured_mut)
        else {
            return;
        };
        if !structured.acp.chose(&choice) {
            return;
        }
        if let Some(live) = structured.live() {
            match choice {
                AcpChoice::Mode(mode) => live.set_acp_mode(mode),
                AcpChoice::Model(model) => live.set_acp_model(model),
                AcpChoice::Option(option, value) => live.set_acp_option(option, value),
            }
        }
        cx.notify();
    }
}
