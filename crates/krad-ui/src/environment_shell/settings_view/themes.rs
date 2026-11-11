use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    Pixels, Point, StatefulInteractiveElement, Styled,
};
use krad_domain::{Theme, ThemeAppearance, ThemeId, ThemeSelection};

use super::super::context_menu::{MenuAction, MenuEntry, MenuSearch};
use super::theme_editor::theme_preview;
use super::tuning::{reset_button, tuning_section};
use super::{form_row, panel_card, EnvironmentShell, FontZone, SWATCH_GAP};
use crate::components::{icon, Button, ButtonVariant, IconName};
use crate::theme::{
    apply_environment_theme, palette, theme_library, themed_for, ICON_SM, LINE_SM, LINE_XS,
    RADIUS_SM, SPACE_2, TEXT_SM, TEXT_XS,
};

const LIGHT_LABEL: &str = "Light theme";
const DARK_LABEL: &str = "Dark theme";
const LIGHT_HINT: &str = "Colors the interface, terminals and editor while the app looks light.";
const DARK_HINT: &str = "Colors the interface, terminals and editor while the app looks dark.";
const FILTER_PLACEHOLDER: &str = "Filter themes…";
const FOLLOW_APP_LABEL: &str = "App theme";
const ENVIRONMENT_SECTION: &str = "Environment theme";
const ENVIRONMENT_SECTION_HINT: &str =
    "Give this environment its own colors; the app theme fills in the rest. Environment: ";
const ENVIRONMENT_LIGHT_LABEL: &str = "Light";
const ENVIRONMENT_DARK_LABEL: &str = "Dark";
const ENVIRONMENT_LIGHT_HINT: &str = "Used here while the app looks light.";
const ENVIRONMENT_DARK_HINT: &str = "Used here while the app looks dark.";
const ENVIRONMENT_RESET_ID: &str = "environment-theme-reset";
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerTarget {
    App(ThemeAppearance),
    Environment(ThemeAppearance),
}

impl PickerTarget {
    const fn key(self) -> u64 {
        match self {
            Self::App(ThemeAppearance::Light) => 0,
            Self::App(ThemeAppearance::Dark) => 1,
            Self::Environment(ThemeAppearance::Light) => 2,
            Self::Environment(ThemeAppearance::Dark) => 3,
        }
    }

    const fn appearance(self) -> ThemeAppearance {
        match self {
            Self::App(appearance) | Self::Environment(appearance) => appearance,
        }
    }
}

impl EnvironmentShell {
    fn selection(&self) -> ThemeSelection {
        self.settings.themes.clone().unwrap_or_default()
    }

    fn environment_choice(&self, appearance: ThemeAppearance) -> Option<&ThemeId> {
        self.settings
            .environment_themes
            .of(&self.active)
            .and_then(|theme| theme.for_appearance(appearance))
    }

    pub(in crate::environment_shell) fn apply_active_theme(&self, cx: &mut gpui_kit::App) {
        apply_environment_theme(&self.settings, Some(&self.active), cx);
    }

    pub(in crate::environment_shell) fn sync_environment_theme(&self, cx: &mut gpui_kit::App) {
        if !themed_for(&self.active, cx) {
            self.apply_active_theme(cx);
        }
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
        self.apply_active_theme(cx);
        self.save_settings(cx);
        cx.notify();
    }

    pub(in crate::environment_shell) fn choose_environment_theme(
        &mut self,
        appearance: ThemeAppearance,
        id: Option<ThemeId>,
        cx: &mut Context<Self>,
    ) {
        let active = self.active.clone();
        self.settings
            .environment_themes
            .set(&active, appearance, id);
        self.apply_active_theme(cx);
        self.save_settings(cx);
        cx.notify();
    }

    fn reset_environment_theme(&mut self, cx: &mut Context<Self>) {
        let active = self.active.clone();
        for appearance in [ThemeAppearance::Light, ThemeAppearance::Dark] {
            self.settings
                .environment_themes
                .set(&active, appearance, None);
        }
        self.apply_active_theme(cx);
        self.save_settings(cx);
        cx.notify();
    }

    fn theme_entries(&self, target: PickerTarget, cx: &gpui_kit::App) -> Vec<MenuEntry> {
        let appearance = target.appearance();
        let selection = self.selection();
        let current = match target {
            PickerTarget::App(_) => Some(selection.for_appearance(appearance)),
            PickerTarget::Environment(_) => self.environment_choice(appearance),
        };
        let follow_app = match target {
            PickerTarget::App(_) => None,
            PickerTarget::Environment(_) => Some(MenuEntry::checked(
                FOLLOW_APP_LABEL,
                current.is_none(),
                MenuAction::ChooseEnvironmentTheme(appearance, None),
            )),
        };
        let themes = theme_library(cx)
            .iter()
            .filter(|theme| theme.appearance == appearance)
            .map(|theme| {
                let action = match target {
                    PickerTarget::App(_) => MenuAction::ChooseTheme(appearance, theme.id.clone()),
                    PickerTarget::Environment(_) => {
                        MenuAction::ChooseEnvironmentTheme(appearance, Some(theme.id.clone()))
                    }
                };
                MenuEntry::checked(theme.name.clone(), Some(&theme.id) == current, action)
                    .with_detail(credit_line(theme))
            });
        follow_app.into_iter().chain(themes).collect()
    }

    fn picked_theme_label(&self, target: PickerTarget, cx: &gpui_kit::App) -> (String, String) {
        let appearance = target.appearance();
        let selection = self.selection();
        let (current, follows_app) = match target {
            PickerTarget::App(_) => (selection.for_appearance(appearance), false),
            PickerTarget::Environment(_) => self.environment_choice(appearance).map_or_else(
                || (selection.for_appearance(appearance), true),
                |id| (id, false),
            ),
        };
        let (name, credit) = theme_library(cx)
            .iter()
            .find(|theme| &theme.id == current)
            .map_or_else(
                || (current.0.clone(), String::new()),
                |theme| (theme.name.clone(), credit_line(theme)),
            );
        if follows_app {
            return (FOLLOW_APP_LABEL.to_owned(), name);
        }
        (name, credit)
    }

    fn theme_picker(&self, target: PickerTarget, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let (name, credit) = self.picked_theme_label(target, cx);
        div()
            .id((PICKER_PREFIX, target.key()))
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
            .hover(|style| style.bg(pal.bg_selected))
            .on_click(
                cx.listener(move |this: &mut Self, event: &ClickEvent, window, cx| {
                    let Point { x, y }: Point<Pixels> = event.position();
                    let entries = this.theme_entries(target, cx);
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

    fn environment_theme_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let rows = [ThemeAppearance::Light, ThemeAppearance::Dark].map(|appearance| {
            let (label, hint) = match appearance {
                ThemeAppearance::Light => (ENVIRONMENT_LIGHT_LABEL, ENVIRONMENT_LIGHT_HINT),
                ThemeAppearance::Dark => (ENVIRONMENT_DARK_LABEL, ENVIRONMENT_DARK_HINT),
            };
            form_row(
                label,
                Some(hint),
                self.theme_picker(PickerTarget::Environment(appearance), cx),
                pal,
            )
        });
        let reset = reset_button(ENVIRONMENT_RESET_ID).on_click(cx.listener(
            |this: &mut Self, _, _, cx| {
                this.reset_environment_theme(cx);
            },
        ));
        tuning_section(
            ENVIRONMENT_SECTION,
            &format!("{ENVIRONMENT_SECTION_HINT}{}", self.active.as_str()),
            Some(reset.into_any_element()),
            rows,
            pal,
        )
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
                self.theme_picker(PickerTarget::App(ThemeAppearance::Light), cx),
                pal,
            ),
            form_row(
                DARK_LABEL,
                Some(DARK_HINT),
                self.theme_picker(PickerTarget::App(ThemeAppearance::Dark), cx),
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
            .child(self.backdrop_sections(cx))
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
            .child(self.environment_theme_section(cx))
            .children(self.tuning_sections(cx))
            .child(self.font_card(FontZone::Interface, cx))
            .into_any_element()
    }
}
