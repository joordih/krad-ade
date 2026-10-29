use libghostty_vt::render::{
    CellIterator, CursorVisualStyle, Dirty, RenderState, RowIteration, RowIterator,
};
use libghostty_vt::screen::{CellContentTag, CellWide};
use libghostty_vt::style::{Style, Underline};
use libghostty_vt::terminal::{Point, PointCoordinate};
use libghostty_vt::{Error, Terminal};

use crate::blocks::is_blocks;
use crate::color::pack;
use crate::images::{ImageReader, TermImage};

const LINK_CAPACITY: usize = 2048;
const BLANK: char = ' ';

#[derive(Default)]
pub struct TermScreen {
    pub rows: Vec<TermRow>,
    pub cursor: Option<TermCursor>,
    pub images: Vec<TermImage>,
}

#[derive(Clone)]
pub struct TermRow {
    pub spans: Vec<TermSpan>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TermSpan {
    pub text: String,
    pub cells: usize,
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub style: SpanStyle,
    pub link: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub selected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TermCursor {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
}

impl From<CursorVisualStyle> for CursorShape {
    fn from(style: CursorVisualStyle) -> Self {
        match style {
            CursorVisualStyle::Bar => Self::Beam,
            CursorVisualStyle::Underline => Self::Underline,
            _ => Self::Block,
        }
    }
}

pub(crate) struct Renderer {
    state: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    read: Vec<TermRow>,
    cursor_row: Option<u16>,
    images: ImageReader,
}

impl Renderer {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            state: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            read: Vec::new(),
            cursor_row: None,
            images: ImageReader::new()?,
        })
    }
}

#[derive(PartialEq)]
struct CellStyle {
    fg: Option<u32>,
    bg: Option<u32>,
    style: SpanStyle,
    link: Option<String>,
    under_cursor: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextClass {
    Ascii,
    Blocks,
    Other,
}

impl TextClass {
    fn of(text: &str) -> Self {
        if text.is_ascii() {
            Self::Ascii
        } else if is_blocks(text) {
            Self::Blocks
        } else {
            Self::Other
        }
    }
}

struct OpenSpan {
    style: CellStyle,
    class: TextClass,
    text: String,
    cells: usize,
}

impl OpenSpan {
    fn continues(&self, style: &CellStyle, class: TextClass) -> bool {
        self.style == *style && self.class == class
    }

    fn close(self) -> TermSpan {
        TermSpan {
            text: self.text,
            cells: self.cells,
            fg: self.style.fg,
            bg: self.style.bg,
            style: self.style.style,
            link: self.style.link,
        }
    }
}

pub(crate) fn link_at(term: &Terminal<'_, '_>, point: Point) -> Option<String> {
    let mut buffer = [0; LINK_CAPACITY];
    let length = term.grid_ref(point).ok()?.hyperlink_uri(&mut buffer).ok()?;
    let uri = std::str::from_utf8(buffer.get(..length)?).ok()?;
    (!uri.is_empty()).then(|| uri.to_owned())
}

fn read_row(
    term: &Terminal<'_, '_>,
    row: &RowIteration<'static, '_>,
    cells: &mut CellIterator<'static>,
    cursor_at: Option<(u16, u16)>,
    y: u16,
) -> Result<TermRow, Error> {
    let mut spans = Vec::new();
    let mut open: Option<OpenSpan> = None;
    let mut text = String::new();
    let selected = row
        .selection()?
        .map(|selection| selection.start_x..=selection.end_x);
    let mut cell_iteration = cells.update(row)?;
    let mut x: u16 = 0;
    while let Some(cell) = cell_iteration.next() {
        let column = x;
        x = x.saturating_add(1);
        let raw = cell.raw_cell()?;
        let wide = raw.wide()?;
        if matches!(wide, CellWide::SpacerTail | CellWide::SpacerHead) {
            continue;
        }
        cell.graphemes_utf8(&mut text)?;
        if text.is_empty() {
            text.push(BLANK);
        }
        let colored = !matches!(
            raw.content_tag()?,
            CellContentTag::Codepoint | CellContentTag::CodepointGrapheme
        );
        let (style, fg, bg) = if raw.has_styling()? || colored {
            (cell.style()?, cell.fg_color()?, cell.bg_color()?)
        } else {
            (Style::default(), None, None)
        };
        let link = if raw.has_hyperlink()? {
            link_at(
                term,
                Point::Viewport(PointCoordinate {
                    x: column,
                    y: u32::from(y),
                }),
            )
        } else {
            None
        };
        let cell_style = CellStyle {
            fg: fg.map(pack),
            bg: bg.map(pack),
            style: SpanStyle {
                bold: style.bold,
                italic: style.italic,
                underline: style.underline != Underline::None || link.is_some(),
                inverse: style.inverse,
                selected: selected
                    .as_ref()
                    .is_some_and(|range| range.contains(&column)),
            },
            link,
            under_cursor: cursor_at == Some((column, y)),
        };
        let cells = if wide == CellWide::Wide { 2 } else { 1 };
        let class = TextClass::of(&text);
        match &mut open {
            Some(span) if span.continues(&cell_style, class) => {
                span.text.push_str(&text);
                span.cells += cells;
            }
            _ => {
                spans.extend(open.take().map(OpenSpan::close));
                open = Some(OpenSpan {
                    style: cell_style,
                    class,
                    text: text.clone(),
                    cells,
                });
            }
        }
    }
    spans.extend(open.map(OpenSpan::close));
    trim_trailing_blank(&mut spans);
    Ok(TermRow { spans })
}

pub(crate) fn screen(
    term: &Terminal<'static, '_>,
    renderer: &mut Renderer,
    repaint: bool,
) -> Result<TermScreen, Error> {
    let Renderer {
        state,
        rows: row_iterator,
        cells: cell_iterator,
        read,
        cursor_row,
        images,
    } = renderer;
    let snapshot = state.update(term)?;
    let cursor = snapshot.cursor()?;
    let cursor_at = cursor
        .viewport
        .filter(|_| cursor.visible)
        .map(|viewport| (viewport.x, viewport.y));
    let cursor_now = cursor_at.map(|(_, row)| row);
    let all = repaint || snapshot.dirty()? == Dirty::Full;
    let mut rows = Vec::with_capacity(read.len());
    let mut row_iteration = row_iterator.update(&snapshot)?;
    let mut y: u16 = 0;
    while let Some(row) = row_iteration.next() {
        let index = usize::from(y);
        let cursor_moved =
            cursor_now != *cursor_row && (cursor_now == Some(y) || *cursor_row == Some(y));
        let kept = read
            .get(index)
            .filter(|_| !all && !cursor_moved && !row.dirty().unwrap_or(true));
        let read_now = match kept {
            Some(kept) => kept.clone(),
            None => read_row(term, row, cell_iterator, cursor_at, y)?,
        };
        rows.push(read_now);
        y = y.saturating_add(1);
    }
    let images = images.placements(&snapshot)?;
    snapshot.clean()?;
    read.clone_from(&rows);
    *cursor_row = cursor_now;
    let cursor = cursor_at.map(|(col, row)| TermCursor {
        row: usize::from(row),
        col: usize::from(col),
        shape: cursor.visual_style.into(),
    });
    Ok(TermScreen {
        rows,
        cursor,
        images,
    })
}

pub(crate) fn images(
    term: &Terminal<'static, '_>,
    renderer: &mut Renderer,
) -> Result<Vec<TermImage>, Error> {
    let snapshot = renderer.state.update(term)?;
    renderer.images.placements(&snapshot)
}

fn trim_trailing_blank(spans: &mut Vec<TermSpan>) {
    while let Some(last) = spans.last_mut() {
        if last.bg.is_some() || last.style.selected || last.style.inverse {
            return;
        }
        let trimmed = last
            .text
            .trim_end_matches(|c: char| c.is_ascii_whitespace())
            .len();
        if trimmed > 0 {
            last.cells -= last.text.len() - trimmed;
            last.text.truncate(trimmed);
            return;
        }
        spans.pop();
    }
}
