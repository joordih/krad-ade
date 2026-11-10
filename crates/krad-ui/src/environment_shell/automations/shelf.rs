use gpui_kit::{
    div, px, rgb, AnyElement, Context, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{AutomationTemplate, TemplateCategory, TemplateIcon};
use std::rc::Rc;

use super::super::EnvironmentShell;
use super::grid::{grid_rows, CellRect, GridFit};
use super::layout::{BoardFit, SHELF_PAD};
use super::wording::cues_summary;
use crate::components::motion::{reveal, rise_in};
use crate::components::{glide_track, icon, GlideOption, GlidePick, GlideSizing, IconName};
use crate::theme::{
    tinted, Palette, ICON_SM, LINE_SM, LINE_XS, RADIUS_LG, RADIUS_MD, SPACE_1, SPACE_2, SPACE_3,
    TEXT_SM, TEXT_XS,
};

const SHELF_REVEAL: &str = "automation-shelf";
const TRACK_ID: &str = "automation-shelf-category";
const TILE_PREFIX: &str = "automation-blueprint";
const TILE_ENTER_PREFIX: &str = "automation-blueprint-enter";
const BLANK_ID: &str = "automation-blank";
const SHELF_HEADER_H: f32 = 120.0;
const GLYPH_BOX: f32 = 28.0;
const TILE_PAD: f32 = SPACE_3 + SPACE_1;
const DESCRIPTION_LINES: usize = 3;
const CHIP_MIN_W: f32 = 112.0;
const GLYPH_ALPHA: u32 = 0x22;
const TILE_PRESSED: f32 = 0.85;
const TILE_HOVER_ALPHA: u32 = GLYPH_ALPHA * 2;
const TILES_PER_CATEGORY: u64 = 64;
const SHELF_TITLE: &str = "Blueprints";
const SHELF_TEXT: &str = "Borrow a ready-made brief and adjust it, or begin on a blank canvas.";
const BLANK_TITLE: &str = "Blank canvas";
const BLANK_TEXT: &str = "Every step is yours to shape, from the cue to the workspace.";
const BLANK_HINT: &str = "Any cue you like";

#[must_use]
pub(super) const fn blueprint_glyph(template: TemplateIcon) -> IconName {
    match template {
        TemplateIcon::Bug => IconName::Target,
        TemplateIcon::Shield => IconName::Lock,
        TemplateIcon::Docs => IconName::Book,
        TemplateIcon::Tests => IconName::CheckCircle,
        TemplateIcon::Review => IconName::Merge,
        TemplateIcon::Dependencies => IconName::Layers,
        TemplateIcon::Incident => IconName::Bell,
        TemplateIcon::Flag => IconName::Flag,
        TemplateIcon::Research => IconName::Search,
        TemplateIcon::Environment => IconName::Settings,
    }
}

#[must_use]
pub(super) fn blueprint_hint(template: &AutomationTemplate) -> String {
    cues_summary(&template.triggers)
}

#[must_use]
pub(super) fn shelf_height(grid: GridFit) -> f32 {
    SHELF_HEADER_H + grid.height()
}

struct Tile<'a> {
    id: ElementId,
    glyph: IconName,
    title: &'a str,
    description: &'a str,
    hint: String,
    template: Option<String>,
}

impl EnvironmentShell {
    pub(super) fn blueprint_shelf(
        &self,
        fit: BoardFit,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.automations.shelf;
        let category_at = TemplateCategory::ALL
            .iter()
            .position(|category| *category == state.category);
        let options = TemplateCategory::ALL
            .into_iter()
            .map(|category| GlideOption {
                label: category.label().into(),
                glyph: None,
            })
            .collect();
        let shell = cx.entity().downgrade();
        let pick: GlidePick = Rc::new(
            move |index: usize, _: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
                let Some(category) = TemplateCategory::ALL.get(index).copied() else {
                    return;
                };
                shell
                    .update(cx, |shell, cx| shell.pick_blueprint_category(category, cx))
                    .unwrap_or_default();
            },
        );
        let track = glide_track(
            TRACK_ID,
            options,
            category_at,
            &pick,
            GlideSizing::Scroll(CHIP_MIN_W),
            pal,
            cx,
        );
        let count = 1 + self.automations.catalog.in_category(state.category).count();
        let grid = fit.tiles(count);
        let tiles = self.blueprint_tiles(state.category, category_at, grid, pal, cx);
        let heading = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .child(
                div()
                    .text_size(px(TEXT_SM))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(SHELF_TITLE),
            )
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(SHELF_TEXT),
            );
        let body = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(SPACE_3))
            .p(px(SHELF_PAD))
            .rounded(px(RADIUS_LG))
            .border_1()
            .border_color(pal.divider)
            .bg(rgb(pal.bg_panel))
            .child(heading)
            .child(
                div()
                    .id(TRACK_ID)
                    .w(px(fit.shelf_inner()))
                    .overflow_x_scroll()
                    .child(track),
            )
            .child(grid_rows(grid, tiles));
        reveal(
            SHELF_REVEAL,
            state.open,
            shelf_height(grid),
            div().flex().flex_col().flex_none().w_full().child(body),
        )
    }

    fn blueprint_tiles(
        &self,
        category: TemplateCategory,
        category_at: Option<usize>,
        grid: GridFit,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let blank = Tile {
            id: ElementId::from(BLANK_ID),
            glyph: IconName::Add,
            title: BLANK_TITLE,
            description: BLANK_TEXT,
            hint: BLANK_HINT.to_owned(),
            template: None,
        };
        let base =
            u64::try_from(category_at.unwrap_or_default()).unwrap_or_default() * TILES_PER_CATEGORY;
        std::iter::once(blank)
            .chain(
                self.automations
                    .catalog
                    .in_category(category)
                    .enumerate()
                    .map(|(index, template)| Tile {
                        id: ElementId::NamedInteger(
                            TILE_PREFIX.into(),
                            u64::try_from(index).unwrap_or_default(),
                        ),
                        glyph: blueprint_glyph(template.icon),
                        title: &template.title,
                        description: &template.description,
                        hint: blueprint_hint(template),
                        template: Some(template.id.clone()),
                    }),
            )
            .enumerate()
            .map(|(index, tile)| {
                let cell = grid.cell(index);
                rise_in(
                    ElementId::NamedInteger(
                        TILE_ENTER_PREFIX.into(),
                        base + u64::try_from(index).unwrap_or_default(),
                    ),
                    index,
                    div()
                        .relative()
                        .flex_none()
                        .w(px(cell.w))
                        .h(px(cell.h))
                        .child(Self::blueprint_tile(tile, cell, pal, cx)),
                )
            })
            .collect()
    }

    fn blueprint_tile(
        tile: Tile<'_>,
        cell: CellRect,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = pal.accent;
        let template = tile.template;
        let headline = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(GLYPH_BOX))
                    .rounded(px(RADIUS_MD))
                    .bg(tinted(accent, GLYPH_ALPHA))
                    .child(icon(tile.glyph, ICON_SM, accent)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(pal.text_strong))
                    .child(SharedString::from(tile.title.to_owned())),
            );
        div()
            .id(tile.id)
            .flex()
            .flex_col()
            .w(px(cell.w))
            .h(px(cell.h))
            .overflow_hidden()
            .gap(px(SPACE_2))
            .p(px(TILE_PAD))
            .rounded(px(RADIUS_MD))
            .border_1()
            .border_color(pal.divider)
            .bg(rgb(pal.bg_surface))
            .cursor_pointer()
            .hover(move |style| style.border_color(tinted(accent, TILE_HOVER_ALPHA)))
            .active(|style| style.opacity(TILE_PRESSED))
            .on_click(cx.listener(move |this: &mut Self, _, window, cx| {
                this.start_automation(template.as_deref(), window, cx);
            }))
            .child(headline)
            .child(
                div()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .line_clamp(DESCRIPTION_LINES)
                    .text_ellipsis()
                    .child(SharedString::from(tile.description.to_owned())),
            )
            .child(div().flex_1())
            .child(
                div()
                    .truncate()
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(tile.hint),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::TemplateCatalog;

    #[test]
    fn every_blueprint_reads_its_cue_in_plain_words() {
        let catalog = TemplateCatalog::built_in().unwrap_or_else(|error| unreachable!("{error}"));
        let sweep = catalog
            .find("morning-risk-sweep")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(blueprint_hint(sweep), "Weekdays at 08:30");
        let recap = catalog
            .find("friday-recap")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(blueprint_hint(recap), "Fridays at 16:30");
        let responder = catalog
            .find("red-build-responder")
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            blueprint_hint(responder),
            "When GitHub checks turn red +1 more"
        );
        assert!(catalog
            .templates
            .iter()
            .all(|template| !blueprint_hint(template).is_empty()));
    }

    #[test]
    fn the_shelf_grows_with_rows_not_with_width() {
        let fit = super::super::layout::board_fit(900.0, super::super::layout::BoardRole::Alone);
        assert!(shelf_height(fit.tiles(9)) > shelf_height(fit.tiles(3)));
        assert!(shelf_height(fit.tiles(1)) > SHELF_HEADER_H);
    }
}
