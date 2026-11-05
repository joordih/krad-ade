use gpui_kit::component::input::Input;
use gpui_kit::{div, px, AnyElement, Context, IntoElement, ParentElement, Styled};
use krad_domain::ExtensionSource;
use krad_extensions::index::Compatibility;

use super::super::super::sections::{card_field, empty_row, settings_section};
use super::super::super::{form_row, hint, panel_card};
use super::rows::{failure_text, index_rows, origin_text, problems_text, IndexRow, RowAction};
use super::{EnvironmentShell, IndexState};
use crate::components::{chip, Button, ButtonVariant};
use crate::theme::{palette, Palette, SPACE_2};

const BROWSE_TITLE: &str = "Browse";
const BROWSE_HINT: &str =
    "Extensions listed in the index, each pinned to a reviewed commit. Nothing updates on its own.";
const UPDATES_TITLE: &str = "Updates";
const UPDATES_HINT: &str = "Newer commits the index lists for extensions you installed from it.";
const INDEX_LABEL: &str = "Index repository";
const SEARCH_LABEL: &str = "Search";
const LOADING_TEXT: &str = "Loading the extension index…";
const NO_RESULTS_TEXT: &str = "No extension matches.";
const USE_LABEL: &str = "Use";
const REFRESH_LABEL: &str = "Refresh";
const INSTALL_LABEL: &str = "Install";
const UPDATE_LABEL: &str = "Update";
const INSTALLED_LABEL: &str = "Installed";
const WORKING_LABEL: &str = "Working…";
const NEWER_APP_LABEL: &str = "Needs a newer app";
const COMMIT_SHOWN: usize = 7;
const USE_ID: &str = "extension-index-use";
const REFRESH_ID: &str = "extension-index-refresh";
const INSTALL_PREFIX: &str = "extension-index-install";
const UPDATE_PREFIX: &str = "extension-index-update";
const FIELD_W: f32 = 240.0;

fn short(commit: &str) -> &str {
    commit.get(..COMMIT_SHOWN).unwrap_or(commit)
}

impl EnvironmentShell {
    fn index_row_element(
        index: usize,
        row: &IndexRow,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id.clone();
        let control = match row.action {
            RowAction::Install => Button::new((INSTALL_PREFIX, index))
                .label(INSTALL_LABEL)
                .small()
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.install_from_index(&id, cx);
                }))
                .into_any_element(),
            RowAction::Update => Button::new((INSTALL_PREFIX, index))
                .label(UPDATE_LABEL)
                .small()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.update_from_index_by_id(&id, cx);
                }))
                .into_any_element(),
            RowAction::Installed => chip(INSTALLED_LABEL, pal).into_any_element(),
            RowAction::Working => chip(WORKING_LABEL, pal).into_any_element(),
            RowAction::NeedsNewerApp => chip(NEWER_APP_LABEL, pal).into_any_element(),
        };
        form_row(&row.name, Some(&row.detail), control, pal)
    }

    fn update_from_index_by_id(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .settings
            .extensions
            .iter()
            .find(|entry| entry.id == id && matches!(entry.source, ExtensionSource::Pinned { .. }))
            .cloned()
        else {
            return;
        };
        self.update_pinned_extension(entry, cx);
    }

    fn index_source_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let Some(browser) = self.browser() else {
            return div().into_any_element();
        };
        let field = browser.repository.as_ref().map_or_else(
            || div().into_any_element(),
            |field| {
                div()
                    .w(px(FIELD_W))
                    .child(Input::new(field))
                    .into_any_element()
            },
        );
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(field)
            .child(
                Button::new(USE_ID).label(USE_LABEL).small().on_click(
                    cx.listener(|this: &mut Self, _, _, cx| this.apply_index_repository(cx)),
                ),
            )
            .child(
                Button::new(REFRESH_ID)
                    .label(REFRESH_LABEL)
                    .small()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.load_extension_index(krad_extensions::index::Freshness::Refresh, cx);
                    })),
            )
            .into_any_element();
        let repository = &self.settings.extension_index;
        let status = match &browser.state {
            IndexState::NotLoaded | IndexState::Loading => LOADING_TEXT.to_owned(),
            IndexState::Loaded(loaded) => {
                let origin = origin_text(loaded.origin, loaded.age);
                match problems_text(loaded.index.problems.len()) {
                    Some(problems) => format!("{origin}. {problems}."),
                    None => origin,
                }
            }
            IndexState::Failed(failure) => failure_text(repository, failure),
        };
        form_row(INDEX_LABEL, Some(&status), controls, pal)
    }

    fn update_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = palette(cx);
        let Some(loaded) = self.browser().and_then(super::IndexBrowser::loaded) else {
            return Vec::new();
        };
        let working = self
            .browser()
            .map_or_else(Default::default, |browser| browser.working.clone());
        loaded
            .index
            .updates(&self.settings.extensions)
            .into_iter()
            .enumerate()
            .map(|(index, update)| {
                let detail = format!(
                    "{} → {}",
                    short(update.installed.as_ref()),
                    short(update.available.as_ref())
                );
                let id = update.extension.clone();
                let control = if working.contains(&id) {
                    chip(WORKING_LABEL, pal).into_any_element()
                } else if update.compatibility == Compatibility::NeedsNewerApp {
                    chip(NEWER_APP_LABEL, pal).into_any_element()
                } else {
                    Button::new((UPDATE_PREFIX, index))
                        .label(UPDATE_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.update_from_index_by_id(&id, cx);
                        }))
                        .into_any_element()
                };
                form_row(&update.extension, Some(&detail), control, pal)
            })
            .collect()
    }

    pub(in crate::environment_shell) fn index_sections(
        &self,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let pal = palette(cx);
        let Some(browser) = self.browser() else {
            return Vec::new();
        };
        let mut sections = Vec::new();
        let updates = self.update_rows(cx);
        if !updates.is_empty() {
            sections.push(settings_section(
                UPDATES_TITLE,
                Some(UPDATES_HINT),
                [panel_card(updates, pal)],
                pal,
            ));
        }
        let mut cards = vec![panel_card([self.index_source_card(cx)], pal)];
        if let Some(query) = &browser.query {
            cards.push(panel_card(
                [card_field(
                    SEARCH_LABEL,
                    Input::new(query).into_any_element(),
                    pal,
                )],
                pal,
            ));
        }
        if let Some(loaded) = browser.loaded() {
            let text = browser
                .query
                .as_ref()
                .map_or_else(String::new, |query| query.read(cx).value().to_string());
            let rows = index_rows(
                &loaded.index,
                &text,
                &self.settings.extensions,
                &browser.working,
            );
            let results: Vec<AnyElement> = if rows.is_empty() {
                vec![empty_row(NO_RESULTS_TEXT, pal)]
            } else {
                rows.into_iter()
                    .enumerate()
                    .map(|(index, row)| Self::index_row_element(index, &row, pal, cx))
                    .collect()
            };
            cards.push(panel_card(results, pal));
        }
        cards.push(hint(BROWSE_HINT, pal));
        sections.push(settings_section(BROWSE_TITLE, None, cards, pal));
        sections
    }
}
