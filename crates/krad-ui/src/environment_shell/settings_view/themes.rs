use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, StatefulInteractiveElement, Styled,
};
use krad_domain::{Theme, ThemeAppearance, ThemeId, ThemeSelection};

use super::super::context_menu::{MenuAction, MenuEntry, MenuSearch};
use super::theme_editor::theme_preview;
use super::{form_row, panel_card, EnvironmentShell, FontZone, SWATCH_GAP};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    apply_settings_theme, palette, theme_library, ICON_SM, LINE_SM, LINE_XS, RADIUS_SM, SPACE_2,
    TEXT_SM, TEXT_XS,
};

const LIGHT_LABEL: &str = "Light theme";
const DARK_LABEL: &str = "Dark theme";
const LIGHT_HINT: &str = "Colors the interface, terminals and editor while the app looks light.";
const DARK_HINT: &str = "Colors the interface, terminals and editor while the app looks dark.";
const FILTER_PLACEHOLDER: &str = "Filter themes…";
const CREDIT_SEPARATOR: &str = " · ";
const OWN_THEME: &str = "Krad";
const PICKER_PREFIX: &str = "theme-picker";
const BTN_CREATE_THEME: &str = "theme-create";
const CREATE_THEME_LABEL: &str = "Create theme…";
const PICKER_W: f32 = 260.0;
const PICKER_H: f32 = 40.0;

fn credit_line(theme: &Theme) -> String {
    theme.credit.as_ref().map_or_else(
        || OWN_THEME.to_owned(),
        |credit| format!("{}{CREDIT_SEPARATOR}{}", credit.author, credit.license),
    )
}

const fn slot_key(appearance: ThemeAppearance) -> u64 {
    match appearance {
        ThemeAppearance::Light => 0,
        ThemeAppearance::Dark => 1,
    }
}

impl EnvironmentShell {
    fn selection(&self) -> ThemeSelection {
        self.settings.themes.clone().unwrap_or_default()
    }

    pub(super) fn select_theme(&mut self, theme: &Theme, cx: &mut Context<Self>) {
        self.choose_theme(theme.appearance, theme.id.clone(), cx);
    }

    pub(in crate::environment_shell) fn choose_theme(
        &mut self,
        appearance: ThemeAppearance,
        id: ThemeId,
        cx: &mut Context<Self>,
    ) {
        self.settings
            .themes
            .get_or_insert_with(Default::default)
            .set(appearance, id);
        apply_settings_theme(&self.settings, cx);
        self.save_settings(cx);
        cx.notify();
    }

    fn theme_entries(&self, appearance: ThemeAppearance, cx: &gpui_kit::App) -> Vec<MenuEntry> {
        let selection = self.selection();
        let current = selection.for_appearance(appearance);
        theme_library(cx)
            .iter()
            .filter(|theme| theme.appearance == appearance)
            .map(|theme| {
                MenuEntry::checked(
                    theme.name.clone(),
                    &theme.id == current,
                    MenuAction::ChooseTheme(appearance, theme.id.clone()),
                )
                .with_detail(credit_line(theme))
            })
            .collect()
    }

    fn theme_picker(&self, appearance: ThemeAppearance, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let selection = self.selection();
        let current = selection.for_appearance(appearance);
        let (name, credit) = theme_library(cx)
            .iter()
            .find(|theme| &theme.id == current)
            .map_or_else(
                || (current.0.clone(), String::new()),
                |theme| (theme.name.clone(), credit_line(theme)),
            );
        div()
            .id((PICKER_PREFIX, slot_key(appearance)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .w(px(PICKER_W))
            .h(px(PICKER_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_SM))
            .border_1()
            .border_color(pal.border)
            .bg(pal.bg_hover)
            .cursor_pointer()
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, window, cx| {
                    let Point { x, y }: Point<Pixels> = event.position();
                    let entries = this.theme_entries(appearance, cx);
                    this.open_searchable_menu(
                        (x.as_f32(), y.as_f32()),
                        entries,
                        MenuSearch {
                            placeholder: FILTER_PLACEHOLDER,
                            create: None,
                            footer: Vec::new(),
                        },
                        window,
                        cx,
                    );
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .text_color(rgb(pal.text_strong))
                            .child(name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(credit),
                    ),
            )
            .child(icon(IconName::ChevronDown, ICON_SM, pal.text_muted))
            .into_any_element()
    }

    fn chosen_previews(&self, cx: &mut Context<Self>) -> AnyElement {
        let mono = cx.theme().mono_font_family.clone();
        let selection = self.selection();
        let previews: Vec<AnyElement> = [ThemeAppearance::Light, ThemeAppearance::Dark]
            .into_iter()
            .filter_map(|appearance| {
                let id = selection.for_appearance(appearance);
                theme_library(cx)
                    .iter()
                    .find(|theme| &theme.id == id)
                    .map(|theme| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(theme_preview(theme, mono.clone()))
                            .into_any_element()
                    })
            })
            .collect();
        div()
            .flex()
            .flex_row()
            .gap(px(SWATCH_GAP))
            .children(previews)
            .into_any_element()
    }

    pub(super) fn appearance_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mut rows = vec![
            self.appearance_mode_row(cx),
            form_row(
                LIGHT_LABEL,
                Some(LIGHT_HINT),
                self.theme_picker(ThemeAppearance::Light, cx),
                pal,
            ),
            form_row(
                DARK_LABEL,
                Some(DARK_HINT),
                self.theme_picker(ThemeAppearance::Dark, cx),
                pal,
            ),
            self.material_row(cx),
        ];
        rows.extend(self.tint_rows(cx));
        div()
            .flex()
            .flex_col()
            .gap(px(SWATCH_GAP))
            .child(panel_card(rows, pal))
            .child(self.chosen_previews(cx))
            .child(
                div().flex().flex_row().justify_end().child(
                    Button::new(BTN_CREATE_THEME)
                        .label(CREATE_THEME_LABEL)
                        .icon(IconName::Add)
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.open_theme_editor(window, cx);
                        })),
                ),
            )
            .child(self.font_card(FontZone::Interface, cx))
            .into_any_element()
    }
}
