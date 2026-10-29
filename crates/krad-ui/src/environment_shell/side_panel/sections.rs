use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use krad_domain::SidePanelSection;

use super::super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{
    palette, row_fill, row_shadow, ICON_SM, LINE_SM, LINE_XS, RADIUS_XS, SPACE_0_5, SPACE_1,
    SPACE_1_5, SPACE_2, TEXT_SM, TEXT_XS,
};

const SECTION_TAB_PREFIX: &str = "side-section";
const HEADER_H: f32 = 36.0;
const TAB_H: f32 = 26.0;
const RULE_H: f32 = 1.0;
const EXPLORER_LABEL: &str = "Explorer";
const CHANGES_LABEL: &str = "Changes";
const ADDED_SIGN: &str = "+";
const REMOVED_SIGN: &str = "−";

const SECTIONS: [SidePanelSection; 2] = [SidePanelSection::Explorer, SidePanelSection::Changes];

const fn section_label(section: SidePanelSection) -> &'static str {
    match section {
        SidePanelSection::Explorer => EXPLORER_LABEL,
        SidePanelSection::Changes => CHANGES_LABEL,
    }
}

const fn section_glyph(section: SidePanelSection) -> IconName {
    match section {
        SidePanelSection::Explorer => IconName::File,
        SidePanelSection::Changes => IconName::GitBranch,
    }
}

impl EnvironmentShell {
    fn side_section_shown(&self) -> SidePanelSection {
        self.side_layout()
            .map_or_else(SidePanelSection::default, |layout| layout.section)
    }

    pub(super) fn side_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let shown = self.side_section_shown();
        let body = match shown {
            SidePanelSection::Explorer => self.side_files(cx),
            SidePanelSection::Changes => self.side_changes(cx),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(self.section_header(shown, cx))
            .child(body)
            .into_any_element()
    }

    fn section_header(&self, shown: SidePanelSection, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let totals = self
            .group_git()
            .map(super::super::groups::GroupGit::total_lines);
        let tabs: Vec<AnyElement> = SECTIONS
            .iter()
            .enumerate()
            .map(|(index, section)| self.section_tab(index, *section, shown, cx))
            .collect();
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_0_5))
            .h(px(HEADER_H))
            .px(px(SPACE_1_5))
            .border_b(px(RULE_H))
            .border_color(pal.divider)
            .children(tabs)
            .child(div().flex_1())
            .when_some(totals, |header, totals| {
                header.child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_none()
                        .gap(px(SPACE_1))
                        .pr(px(SPACE_1))
                        .text_size(px(TEXT_XS))
                        .line_height(px(LINE_XS))
                        .font_weight(FontWeight::MEDIUM)
                        .child(
                            div()
                                .text_color(rgb(pal.success_fg))
                                .child(format!("{ADDED_SIGN}{}", totals.added)),
                        )
                        .child(
                            div()
                                .text_color(rgb(pal.error_fg))
                                .child(format!("{REMOVED_SIGN}{}", totals.removed)),
                        ),
                )
            })
            .into_any_element()
    }

    fn section_tab(
        &self,
        index: usize,
        section: SidePanelSection,
        shown: SidePanelSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let active = section == shown;
        let changed = (section == SidePanelSection::Changes)
            .then(|| self.group_git().map(|git| git.status.changes.len()))
            .flatten()
            .filter(|count| *count > 0);
        div()
            .id((SECTION_TAB_PREFIX, index))
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap(px(SPACE_1_5))
            .h(px(TAB_H))
            .px(px(SPACE_2))
            .rounded(px(RADIUS_XS))
            .cursor_pointer()
            .text_size(px(TEXT_SM))
            .line_height(px(LINE_SM))
            .when(active, |tab| {
                tab.bg(row_fill(pal, cx))
                    .shadow(row_shadow(pal, cx))
                    .text_color(rgb(pal.text_strong))
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!active, |tab| {
                tab.text_color(rgb(pal.text_muted))
                    .hover(|style| style.bg(pal.bg_hover))
            })
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if section != shown {
                    this.cancel_new_entry();
                }
                this.edit_side_layout(cx, |layout| layout.section = section);
            }))
            .child(icon(
                section_glyph(section),
                ICON_SM,
                if active {
                    pal.text_strong
                } else {
                    pal.text_muted
                },
            ))
            .child(section_label(section))
            .when_some(changed, |tab, count| {
                tab.child(
                    div()
                        .text_size(px(TEXT_XS))
                        .text_color(rgb(pal.text_subtle))
                        .child(count.to_string()),
                )
            })
            .into_any_element()
    }
}
