use gpui_kit::{
    div, px, rgb, uniform_list, AnyElement, Context, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, Window,
};
use krad_extensions::api::view::{Column, Node, Sort, SortOrder, Sortable, ViewEvent};
use krad_extensions::api::NodeKey;
use std::ops::Range;

use super::{
    loading_row, row_id, visible, COLLECTION_MIN_H, HEADER_PREFIX, ROW_PREFIX, TABLE_PREFIX,
};
use crate::components::{icon, IconName};
use crate::extension_view::form::element_id;
use crate::extension_view::model::CachedRow;
use crate::extension_view::style::fixed;
use crate::extension_view::ExtensionView;
use crate::theme::{
    palette, Palette, ICON_SM, LINE_XS, NAV_ROW_H, RADIUS_XS, SPACE_1, SPACE_2, TEXT_XS,
};

fn next_sort(current: Option<&Sort>, column: &NodeKey) -> Sort {
    let order = match current {
        Some(sort) if sort.column == *column && sort.order == SortOrder::Ascending => {
            SortOrder::Descending
        }
        _ => SortOrder::Ascending,
    };
    Sort {
        column: column.clone(),
        order,
    }
}

impl ExtensionView {
    pub(in crate::extension_view) fn table(
        &self,
        node: &Node,
        columns: &[Column],
        count: u32,
        sort: Option<&Sort>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = node.key.clone();
        let current = self.sorts.get(&key).or(sort).cloned();
        let header = div()
            .flex()
            .flex_row()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .h(px(NAV_ROW_H))
            .items_center()
            .border_b_1()
            .border_color(pal.divider)
            .children(columns.iter().map(|column| {
                let glyph = current.as_ref().and_then(|sort| {
                    (sort.column == column.key).then_some(match sort.order {
                        SortOrder::Ascending => IconName::ChevronUp,
                        SortOrder::Descending => IconName::ChevronDown,
                    })
                });
                let cell = column_cell(column)
                    .id(row_id(HEADER_PREFIX, &key, &column.key))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_1))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_muted))
                    .child(column.title.clone())
                    .children(glyph.map(|glyph| icon(glyph, ICON_SM, pal.text_muted)));
                if column.sortable == Sortable::No {
                    return cell.into_any_element();
                }
                let table = key.clone();
                let column_key = column.key.clone();
                cell.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let sort = next_sort(this.sorts.get(&table), &column_key);
                        this.sorts.insert(table.clone(), sort.clone());
                        this.emit(ViewEvent::Sort {
                            node: table.clone(),
                            sort,
                        });
                        cx.notify();
                    }))
                    .into_any_element()
            }));
        let body_key = key.clone();
        let widths: Vec<Column> = columns.to_vec();
        let body = uniform_list(
            element_id(TABLE_PREFIX, &key),
            usize::try_from(count).unwrap_or(usize::MAX),
            cx.processor(
                move |this: &mut Self, range: Range<usize>, _: &mut Window, cx| {
                    this.table_rows(&body_key, &widths, &range, cx)
                },
            ),
        )
        .flex_1()
        .min_h(px(COLLECTION_MIN_H));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn table_rows(
        &mut self,
        key: &NodeKey,
        columns: &[Column],
        range: &Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        self.request_rows(key, range);
        let pal = palette(cx);
        visible(range)
            .map(|index| match self.model.row(key, index) {
                Some(CachedRow::Table(row)) => {
                    let cells = row.cells.iter().zip(columns).map(|(cell, column)| {
                        column_cell(column)
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.element(cell, pal, cx))
                    });
                    let cells: Vec<_> = cells.collect();
                    let menu = Self::item_actions(key, &row.key, &row.menu, pal, cx);
                    div()
                        .id(row_id(ROW_PREFIX, key, &row.key))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(SPACE_2))
                        .px(px(SPACE_2))
                        .min_h(px(NAV_ROW_H))
                        .rounded(px(RADIUS_XS))
                        .hover(|style| style.bg(pal.bg_hover))
                        .children(cells)
                        .children(menu)
                        .into_any_element()
                }
                _ => loading_row(pal),
            })
            .collect()
    }
}

fn column_cell(column: &Column) -> gpui_kit::Div {
    match fixed(column.width) {
        Some(width) => div().flex_none().w(px(width)),
        None => div().flex_1().flex_basis(px(0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorting_a_column_again_flips_its_order_and_a_new_column_starts_ascending(
    ) -> Result<(), krad_extensions::api::IdError> {
        let (name, state) = ("name".parse::<NodeKey>()?, "state".parse::<NodeKey>()?);
        let first = next_sort(None, &name);
        assert_eq!(first.order, SortOrder::Ascending);
        assert_eq!(next_sort(Some(&first), &name).order, SortOrder::Descending);
        let other = next_sort(Some(&first), &state);
        assert_eq!((other.column, other.order), (state, SortOrder::Ascending));
        Ok(())
    }
}
