mod blocks;
mod color;
mod images;
mod input;
mod snapshot;
mod wheel;

pub use blocks::{block, is_blocks, Block, CellRect, Shade, CELL_STEPS};
pub use color::{AnsiPalette, ANSI_COLORS, XTERM_PALETTE};
pub use images::{ImageDepth, ImagePixels, SourceRect, TermImage};
pub use input::KeyModifiers;
pub use libghostty_vt::Error as EngineError;
pub use snapshot::{CursorShape, SpanStyle, TermCursor, TermRow, TermScreen, TermSpan};
pub use wheel::{MouseAction, WheelInput};

use input::KeyInput;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::kitty::graphics::{set_png_decoder, RustPngDecoder};
use libghostty_vt::screen::TrackedGridRef;
use libghostty_vt::search::Search;
use libghostty_vt::selection::{FormatOptions, Selection};
use libghostty_vt::terminal::{Mode, Point, PointCoordinate, ScrollViewport};
use libghostty_vt::{Error, Terminal};
use snapshot::Renderer;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Once;
use wheel::MouseInput;

pub const SCROLLBACK_LINES: usize = 50_000;
const ESC: char = '\u{1b}';
const BEL: char = '\u{7}';
const LINE_BREAK: char = '\n';
const IMAGE_STORAGE_BYTES: u64 = 64 * 1024 * 1024;
static PNG_DECODER: Once = Once::new();
const DEFAULT_CELL: CellPixels = CellPixels {
    width: 8,
    height: 16,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellPixels {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellPoint {
    pub row: usize,
    pub col: usize,
}

impl From<CellPoint> for Point {
    fn from(cell: CellPoint) -> Self {
        Self::Viewport(PointCoordinate {
            x: u16::try_from(cell.col).unwrap_or(u16::MAX),
            y: u32::try_from(cell.row).unwrap_or(u32::MAX),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchDirection {
    Older,
    Newer,
}

struct OpenSearch {
    query: String,
    search: Search<'static, 'static>,
}

fn select(term: &Terminal<'_, '_>, selection: Option<&Selection<'_>>) {
    if term.set_selection(selection).is_err() {
        debug_assert!(false, "a selection of this terminal");
    }
}

fn grid_size(value: usize) -> u16 {
    u16::try_from(value.max(1)).unwrap_or(u16::MAX)
}

pub struct VtEngine {
    term: RefCell<Terminal<'static, 'static>>,
    renderer: RefCell<Renderer>,
    keys: KeyInput,
    mouse: MouseInput,
    palette: Cell<Option<AnsiPalette>>,
    replies: Rc<RefCell<Vec<u8>>>,
    bell: Rc<Cell<bool>>,
    anchor: Option<TrackedGridRef>,
    search: Option<OpenSearch>,
    cell: CellPixels,
    sized_cell: CellPixels,
}

impl VtEngine {
    /// # Errors
    ///
    /// Fails when libghostty-vt cannot allocate the terminal or its render
    /// state.
    pub fn new(cols: usize, rows: usize) -> Result<Self, EngineError> {
        PNG_DECODER.call_once(|| {
            // SAFETY: `call_once` runs this before any engine exists, and
            let decoder = unsafe { set_png_decoder(Some(Box::new(RustPngDecoder::new()))) };
            debug_assert!(decoder.is_ok(), "the PNG decoder is set");
        });
        let (cols, rows) = (grid_size(cols), grid_size(rows));
        let mut term = Terminal::new(cols, rows)?;
        term.resize(cols, rows, DEFAULT_CELL.width, DEFAULT_CELL.height)?;
        term.set_scrollback_max_lines(Some(SCROLLBACK_LINES))?
            .set_kitty_image_storage_limit(IMAGE_STORAGE_BYTES)?
            .set_kitty_image_from_file_allowed(false)?
            .set_kitty_image_from_shared_mem_allowed(false)?
            .set_kitty_image_temp_file_dir(Some(&std::env::temp_dir()))?;
        let replies = Rc::new(RefCell::new(Vec::new()));
        let bell = Rc::new(Cell::new(false));
        let reply_sink = Rc::clone(&replies);
        let bell_sink = Rc::clone(&bell);
        term.on_pty_write(move |_, data| reply_sink.borrow_mut().extend_from_slice(data))?
            .on_bell(move |_| bell_sink.set(true))?;
        Ok(Self {
            term: RefCell::new(term),
            renderer: RefCell::new(Renderer::new()?),
            keys: KeyInput::new()?,
            mouse: MouseInput::new()?,
            palette: Cell::new(None),
            replies,
            bell,
            anchor: None,
            search: None,
            cell: DEFAULT_CELL,
            sized_cell: DEFAULT_CELL,
        })
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.term.get_mut().vt_write(bytes);
    }

    pub fn resize(&mut self, cols: usize, rows: usize, cell: CellPixels) -> bool {
        if cell.width > 0 && cell.height > 0 {
            self.cell = cell;
        }
        let (cols, rows) = (grid_size(cols), grid_size(rows));
        let term = self.term.get_mut();
        let same_grid = term.cols().ok() == Some(cols) && term.rows().ok() == Some(rows);
        if same_grid && self.cell == self.sized_cell {
            return false;
        }
        self.sized_cell = self.cell;
        term.resize(cols, rows, self.cell.width, self.cell.height)
            .is_ok()
    }

    #[must_use]
    pub fn take_replies(&self) -> Vec<u8> {
        std::mem::take(&mut *self.replies.borrow_mut())
    }

    #[must_use]
    pub fn take_bell(&self) -> bool {
        self.bell.replace(false)
    }

    #[must_use]
    pub fn screen(&self, palette: &AnsiPalette) -> TermScreen {
        let mut term = self.term.borrow_mut();
        let recolored = self.palette.get() != Some(*palette);
        if recolored {
            let applied = term
                .set_default_color_palette(Some(color::full_palette(palette)))
                .is_ok();
            self.palette.set(applied.then_some(*palette));
        }
        snapshot::screen(&term, &mut self.renderer.borrow_mut(), recolored).unwrap_or_default()
    }

    #[must_use]
    pub const fn cell_pixels(&self) -> CellPixels {
        self.sized_cell
    }

    #[must_use]
    pub fn images(&self) -> Vec<TermImage> {
        let term = self.term.borrow();
        snapshot::images(&term, &mut self.renderer.borrow_mut()).unwrap_or_default()
    }

    #[must_use]
    pub fn image_pixels(&self, id: u32) -> Option<ImagePixels> {
        images::pixels(&self.term.borrow(), id)
    }

    pub fn encode_key(
        &mut self,
        key: &str,
        key_char: Option<&str>,
        held: KeyModifiers,
    ) -> Option<Vec<u8>> {
        self.keys.encode(self.term.get_mut(), key, key_char, held)
    }

    pub fn start_selection(&mut self, cell: CellPoint) {
        let term = self.term.get_mut();
        select(term, None);
        self.anchor = term.track_grid_ref(cell.into()).ok();
    }

    pub fn extend_selection(&mut self, cell: CellPoint) {
        let term = self.term.get_mut();
        let Some(start) = self
            .anchor
            .as_ref()
            .and_then(|anchor| anchor.snapshot(term).ok().flatten())
        else {
            return;
        };
        let Ok(end) = term.grid_ref(cell.into()) else {
            return;
        };
        let selection = Selection::new(start, end, false);
        select(term, Some(&selection));
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
        select(self.term.get_mut(), None);
    }

    #[must_use]
    pub fn selection_text(&self) -> Option<String> {
        let term = self.term.borrow();
        let bytes = term
            .format_selection_alloc(None, FormatOptions::new())
            .ok()??;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (!text.is_empty()).then_some(text)
    }

    pub fn scroll_lines(&mut self, lines: i32) {
        let delta = isize::try_from(lines).unwrap_or_default();
        self.term
            .get_mut()
            .scroll_viewport(ScrollViewport::Delta(-delta));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.get_mut().scroll_viewport(ScrollViewport::Bottom);
    }

    pub fn wheel(&mut self, lines: i32, at: CellPoint) -> WheelInput {
        self.mouse.wheel(self.term.get_mut(), lines, at)
    }

    pub fn click_report(&mut self, action: MouseAction, at: CellPoint) -> Option<Vec<u8>> {
        self.mouse.click(self.term.get_mut(), action, at)
    }

    #[must_use]
    pub fn bracketed_paste(&self) -> bool {
        self.term
            .borrow()
            .mode(Mode::BRACKETED_PASTE)
            .unwrap_or(false)
    }

    #[must_use]
    pub fn hyperlink_at(&self, cell: CellPoint) -> Option<String> {
        snapshot::link_at(&self.term.borrow(), cell.into())
    }

    pub fn search(&mut self, query: &str, direction: SearchDirection) -> bool {
        self.find(query, direction).unwrap_or(false)
    }

    fn find(&mut self, query: &str, direction: SearchDirection) -> Result<bool, Error> {
        let term = self.term.get_mut();
        if self.search.as_ref().is_none_or(|open| open.query != query) {
            let mut search = Search::new(term)?;
            search.set_needle(term, query.as_bytes())?;
            self.search = Some(OpenSearch {
                query: query.to_owned(),
                search,
            });
        }
        let Some(open) = self.search.as_mut() else {
            return Ok(false);
        };
        let found = match direction {
            SearchDirection::Older => open.search.select_next(term)?,
            SearchDirection::Newer => open.search.select_prev(term)?,
        };
        if !found {
            return Ok(false);
        }
        let snapshot = open.search.snapshot(term)?;
        let selected = snapshot.selected_match()?;
        snapshot.terminal().set_selection(selected.as_ref())?;
        Ok(true)
    }

    fn plain_lines(&self) -> Vec<String> {
        let term = self.term.borrow();
        let options = FormatterOptions::new()
            .with_format(Format::Plain)
            .with_trim(true);
        let text = Formatter::new(&term, options)
            .and_then(|mut formatter| formatter.format_alloc(None))
            .map_or_default(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        text.split(LINE_BREAK).map(str::to_owned).collect()
    }

    #[must_use]
    pub fn scrollback(&self, max_rows: usize) -> Vec<String> {
        let history = self.term.borrow().scrollback_rows().unwrap_or_default();
        let mut lines = self.plain_lines();
        lines.truncate(history);
        let start = lines.len().saturating_sub(max_rows);
        lines.split_off(start)
    }

    #[must_use]
    pub fn recent_lines(&self, max_lines: usize) -> Vec<String> {
        let mut lines = self.plain_lines();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        let start = lines.len().saturating_sub(max_lines);
        lines.split_off(start)
    }

    pub fn push_text_lines(&mut self, lines: &[String]) {
        for line in lines {
            let clean: String = line.chars().filter(|c| *c != ESC && *c != BEL).collect();
            self.push(clean.as_bytes());
            self.push(b"\r\n");
        }
    }
}

#[cfg(test)]
mod tests;
