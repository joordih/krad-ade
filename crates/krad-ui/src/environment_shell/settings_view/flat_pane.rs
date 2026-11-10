use gpui_kit::{div, px, AnyElement, IntoElement, ParentElement, Styled};

use super::gitlab::page::section_heading;
use super::sections::{HEADING_GAP, PANE_GAP};
use super::SWATCH_GAP;
use crate::theme::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Section,
    Heading,
    Item,
}

fn gap_before(previous: Option<Entry>, next: Entry) -> Option<f32> {
    let previous = previous?;
    Some(match (previous, next) {
        (_, Entry::Section) => PANE_GAP,
        (Entry::Section | Entry::Heading, _) => HEADING_GAP,
        (Entry::Item, _) => SWATCH_GAP,
    })
}

#[derive(Default)]
pub(super) struct FlatPane {
    children: Vec<AnyElement>,
    last: Option<Entry>,
}

impl FlatPane {
    fn push(&mut self, entry: Entry, element: AnyElement) {
        if let Some(gap) = gap_before(self.last, entry) {
            self.children
                .push(div().flex_none().h(px(gap)).into_any_element());
        }
        self.children.push(element);
        self.last = Some(entry);
    }

    pub(super) fn section(&mut self, title: &str, detail: Option<&str>, pal: &Palette) {
        self.push(Entry::Section, section_heading(title, detail, pal));
    }

    pub(super) fn heading(&mut self, title: &str, pal: &Palette) {
        self.push(Entry::Heading, section_heading(title, None, pal));
    }

    pub(super) fn item(&mut self, element: AnyElement) {
        self.push(Entry::Item, element);
    }

    pub(super) fn into_element(self) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .children(self.children)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaps(entries: &[Entry]) -> Vec<Option<f32>> {
        let mut last = None;
        entries
            .iter()
            .map(|entry| {
                let gap = gap_before(last, *entry);
                last = Some(*entry);
                gap
            })
            .collect()
    }

    #[test]
    fn a_flat_pane_spaces_its_entries_as_nested_sections_do() {
        let spaced = gaps(&[
            Entry::Section,
            Entry::Item,
            Entry::Item,
            Entry::Section,
            Entry::Heading,
            Entry::Item,
            Entry::Heading,
            Entry::Item,
            Entry::Section,
            Entry::Item,
        ]);
        assert_eq!(
            spaced,
            [
                None,
                Some(HEADING_GAP),
                Some(SWATCH_GAP),
                Some(PANE_GAP),
                Some(HEADING_GAP),
                Some(HEADING_GAP),
                Some(SWATCH_GAP),
                Some(HEADING_GAP),
                Some(PANE_GAP),
                Some(HEADING_GAP),
            ]
        );
    }
}
