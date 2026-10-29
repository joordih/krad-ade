use super::*;

fn engine(cols: usize, rows: usize) -> VtEngine {
    VtEngine::new(cols, rows).unwrap_or_else(|error| unreachable!("{error}"))
}

fn row_text(row: &TermRow) -> String {
    row.spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

fn grid_contains(engine: &VtEngine, needle: &str) -> bool {
    engine
        .screen(&XTERM_PALETTE)
        .rows
        .iter()
        .any(|row| row_text(row).contains(needle))
}

#[test]
fn plain_bytes_land_on_grid() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"hello-term");
    assert!(grid_contains(&engine, "hello-term"));
}

#[test]
fn the_cell_under_the_cursor_is_a_span_of_its_own() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"a=>b\x1b[2D");
    let screen = engine.screen(&XTERM_PALETTE);
    let texts: Vec<&str> = screen.rows[0]
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect();
    assert_eq!(
        texts,
        ["a=", ">", "b"],
        "a ligature cannot join across the cursor"
    );
}

#[test]
fn sgr_colors_become_spans() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"plain \x1b[31mRED");
    let red = engine.screen(&XTERM_PALETTE).rows.iter().any(|row| {
        row.spans
            .iter()
            .any(|span| span.text.contains("RED") && span.fg == Some(XTERM_PALETTE.0[1]))
    });
    assert!(red, "red bytes keep their color");
}

#[test]
fn theme_palette_recolors_base_colors() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b[31mRED");
    let mut light = XTERM_PALETTE;
    light.0[1] = 0x12_34_56;
    let recolored = engine.screen(&light).rows.iter().any(|row| {
        row.spans
            .iter()
            .any(|span| span.text.contains("RED") && span.fg == Some(0x12_34_56))
    });
    assert!(recolored, "base colors come from the theme palette");
}

#[test]
fn scrollback_keeps_history() {
    let mut engine = engine(ENGINE_COLS, 4);
    for index in 0..10 {
        engine.push(format!("line-{index}\r\n").as_bytes());
    }
    let back = engine.scrollback(100);
    assert!(back.len() >= 6, "overflowed lines persist");
    assert!(back.first().is_some_and(|line| line.contains("line-0")));
}

#[test]
fn resize_clamps_and_reports() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    assert!(engine.resize(100, 40, DEFAULT_CELL));
    assert!(!engine.resize(100, 40, DEFAULT_CELL), "nothing changed");
    assert_eq!(engine.visible_size(), (100, 40));
    engine.resize(0, 0, DEFAULT_CELL);
    assert_eq!(engine.visible_size(), (1, 1));
}

#[test]
fn cursor_queries_produce_replies_for_the_child() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b[6n");
    assert_eq!(engine.take_replies(), b"\x1b[1;1R".to_vec());
    assert!(engine.take_replies().is_empty(), "replies are taken once");
}

#[test]
fn mouse_selection_copies_the_selected_cells() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"hello world");
    engine.start_selection(CellPoint { row: 0, col: 6 });
    engine.extend_selection(CellPoint { row: 0, col: 10 });
    assert_eq!(engine.selection_text().as_deref(), Some("world"));
    engine.clear_selection();
    assert_eq!(engine.selection_text(), None);
}

#[test]
fn scrolling_shows_history_and_hides_the_cursor() {
    let mut engine = engine(ENGINE_COLS, 4);
    for index in 0..10 {
        engine.push(format!("line-{index}\r\n").as_bytes());
    }
    assert!(engine.screen(&XTERM_PALETTE).cursor.is_some());
    engine.scroll_lines(3);
    let screen = engine.screen(&XTERM_PALETTE);
    assert!(
        row_text(&screen.rows[0]).contains("line-4"),
        "viewport moved back"
    );
    assert!(
        screen.cursor.is_none(),
        "the live cursor is below the viewport"
    );
    engine.scroll_to_bottom();
    assert!(engine.screen(&XTERM_PALETTE).cursor.is_some());
}

#[test]
fn recent_lines_span_history_and_screen_without_trailing_blanks() {
    let mut engine = engine(ENGINE_COLS, 4);
    for index in 0..10 {
        engine.push(format!("line-{index}\r\n").as_bytes());
    }
    let recent: Vec<String> = engine
        .recent_lines(6)
        .into_iter()
        .map(|line| line.trim_end().to_owned())
        .collect();
    assert_eq!(
        recent,
        ["line-4", "line-5", "line-6", "line-7", "line-8", "line-9"],
        "the oldest three come from scrollback"
    );
}

#[test]
fn search_selects_the_next_older_match() {
    let mut engine = engine(ENGINE_COLS, 4);
    for index in 0..10 {
        engine.push(format!("item-{index}\r\n").as_bytes());
    }
    assert!(engine.search("item-2", SearchDirection::Older));
    assert_eq!(engine.selection_text().as_deref(), Some("item-2"));
    assert!(!engine.search("missing (", SearchDirection::Older));
}

#[test]
fn osc8_links_are_reported_per_cell() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b]8;;https://example.com\x1b\\docs\x1b]8;;\x1b\\ plain");
    assert_eq!(
        engine.hyperlink_at(CellPoint { row: 0, col: 1 }).as_deref(),
        Some("https://example.com")
    );
    assert_eq!(engine.hyperlink_at(CellPoint { row: 0, col: 6 }), None);
    let linked = engine.screen(&XTERM_PALETTE).rows[0]
        .spans
        .iter()
        .any(|span| span.link.is_some() && span.style.underline);
    assert!(linked, "linked spans render underlined");
}

#[test]
fn spans_cover_the_cells_the_cursor_counts() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push("❯ Añ界x".as_bytes());
    let screen = engine.screen(&XTERM_PALETTE);
    let spans = &screen.rows[0].spans;
    let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(texts, ["❯", " A", "ñ界", "x"], "non-ASCII runs stand apart");
    let covered: usize = spans.iter().map(|span| span.cells).sum();
    assert_eq!(Some(covered), screen.cursor.map(|cursor| cursor.col));
}

#[test]
fn bell_rings_once() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x07");
    assert!(engine.take_bell(), "bell latches");
    assert!(!engine.take_bell(), "bell clears after taking");
}

#[test]
fn restored_text_replays_without_control() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push_text_lines(&["restored \x1b[31m line".to_owned()]);
    assert!(grid_contains(&engine, "restored"));
    assert!(!engine.take_bell(), "restored text stays quiet");
}

const NONE: KeyModifiers = KeyModifiers {
    ctrl: false,
    alt: false,
    shift: false,
};
const CTRL: KeyModifiers = KeyModifiers { ctrl: true, ..NONE };
const ALT: KeyModifiers = KeyModifiers { alt: true, ..NONE };
const SHIFT: KeyModifiers = KeyModifiers {
    shift: true,
    ..NONE
};
const CELL: CellPoint = CellPoint { row: 4, col: 9 };

#[test]
fn keystrokes_reach_the_program_as_native_terminals_send_them() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    let mut key = |key: &str, text: Option<&str>, held| engine.encode_key(key, text, held);
    assert_eq!(key("a", Some("a"), NONE).as_deref(), Some(&b"a"[..]));
    assert_eq!(key("enter", None, NONE).as_deref(), Some(&b"\r"[..]));
    assert_eq!(key("c", None, CTRL).as_deref(), Some(&[0x03][..]));
    assert_eq!(key("a", Some("a"), ALT).as_deref(), Some(&b"\x1ba"[..]));
    assert_eq!(key("up", None, NONE).as_deref(), Some(&b"\x1b[A"[..]));
    assert_eq!(
        key("left", None, CTRL).as_deref(),
        Some(&b"\x1b[1;5D"[..]),
        "Ctrl+Left moves a word back in readline"
    );
    assert_eq!(key("backspace", None, CTRL), Some(vec![0x17]));
    assert_eq!(
        key("enter", None, SHIFT).as_deref(),
        Some(&b"\x1b\r"[..]),
        "Shift+Enter starts a new line in agent CLIs instead of submitting"
    );
    let alt_gr = KeyModifiers { alt: true, ..CTRL };
    assert_eq!(
        key("2", Some("@"), alt_gr).as_deref(),
        Some(&b"@"[..]),
        "AltGr text arrives without an Escape"
    );
    assert_eq!(key("caps_lock", None, NONE), None);
}

#[test]
fn keys_follow_the_modes_the_program_turns_on() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b[?1h");
    assert_eq!(
        engine.encode_key("up", None, NONE).as_deref(),
        Some(&b"\x1bOA"[..]),
        "application cursor keys"
    );
    engine.push(b"\x1b[>1u");
    assert_eq!(
        engine.encode_key("enter", None, SHIFT).as_deref(),
        Some(&b"\x1b[13;2u"[..]),
        "the kitty keyboard protocol tells Shift+Enter apart by itself"
    );
}

#[test]
fn a_program_tracking_the_mouse_gets_clicks_and_the_wheel() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    assert_eq!(engine.click_report(MouseAction::Press, CELL), None);
    assert_eq!(engine.wheel(3, CELL), WheelInput::History(3));
    engine.push(b"\x1b[?1002h\x1b[?1006h");
    assert_eq!(
        engine.click_report(MouseAction::Press, CELL).as_deref(),
        Some(&b"\x1b[<0;10;5M"[..])
    );
    assert_eq!(
        engine.click_report(MouseAction::Release, CELL).as_deref(),
        Some(&b"\x1b[<0;10;5m"[..])
    );
    let WheelInput::Program(report) = engine.wheel(1, CELL) else {
        unreachable!("a tracked wheel is reported");
    };
    assert!(report.starts_with(b"\x1b[<64;10;5M"));
}

#[test]
fn a_full_screen_program_without_the_mouse_gets_arrows() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b[?1049h\x1b[?1007h");
    assert_eq!(
        engine.wheel(-2, CELL),
        WheelInput::Program(b"\x1bOB\x1bOB".to_vec())
    );
}

#[test]
fn a_frame_after_a_cached_one_shows_new_output_the_selection_and_the_cursor() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"first\r\nsecond");
    let before = engine.screen(&XTERM_PALETTE);
    assert_eq!(row_text(&before.rows[1]), "second");
    engine.push(b"\r\nthird");
    let after = engine.screen(&XTERM_PALETTE);
    assert_eq!(row_text(&after.rows[2]), "third", "new output shows");
    assert_eq!(after.cursor.map(|cursor| cursor.row), Some(2));
    assert_eq!(
        row_text(&after.rows[1]),
        "second",
        "the row the cursor left"
    );
    engine.start_selection(CellPoint { row: 0, col: 0 });
    engine.extend_selection(CellPoint { row: 0, col: 4 });
    let selected = engine.screen(&XTERM_PALETTE);
    assert!(
        selected.rows[0]
            .spans
            .iter()
            .any(|span| span.style.selected),
        "the selection shows"
    );
}

#[test]
fn an_image_sent_in_band_is_placed_with_its_pixels() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b_Gi=7,f=32,s=2,v=1,a=T;/wAA/wD/AP8=\x1b\\");
    let images = engine.images();
    let Some(image) = images.first() else {
        unreachable!("the image is placed");
    };
    assert_eq!((image.id, image.col, image.row), (7, 0, 0));
    assert_eq!(
        image.depth,
        ImageDepth::AboveText,
        "z 0 draws over the text"
    );
    assert_eq!(
        engine.image_pixels(7).map(|pixels| pixels.bgra),
        Some(vec![0, 0, 255, 255, 0, 255, 0, 255]),
        "red then green, as blue, green, red, alpha"
    );
    assert_eq!(engine.screen(&XTERM_PALETTE).images, images);
    engine.push(b"\x1b_Gi=8,f=32,s=2,v=1,z=-1,a=T;/wAA/wD/AP8=\x1b\\");
    let below = engine.images();
    assert_eq!(
        below.first().map(|image| (image.id, image.depth)),
        Some((8, ImageDepth::BelowText)),
        "a negative z draws under the text, first"
    );
}

#[test]
fn a_png_image_is_decoded() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push(b"\x1b_Gi=3,f=100,a=T;iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==\x1b\\");
    let pixels = engine.image_pixels(3);
    assert_eq!(
        pixels.map(|pixels| (pixels.width, pixels.height, pixels.bgra)),
        Some((1, 1, vec![0, 0, 255, 255]))
    );
}

#[test]
fn block_elements_are_a_span_of_their_own() {
    let mut engine = engine(ENGINE_COLS, ENGINE_ROWS);
    engine.push("ab▐▛███▜▌·c".as_bytes());
    let screen = engine.screen(&XTERM_PALETTE);
    let spans: Vec<(&str, usize)> = screen.rows[0]
        .spans
        .iter()
        .map(|span| (span.text.as_str(), span.cells))
        .collect();
    assert_eq!(
        spans,
        [("ab", 2), ("▐▛███▜▌", 7), ("·", 1), ("c", 1)],
        "the UI draws block elements as rectangles, so they never share a span with text"
    );
}

#[test]
fn clearing_the_screen_and_scrollback_leaves_no_typed_command() {
    let mut engine = engine(40, 4);
    for line in 0..12 {
        engine.push(format!("typed command {line}\r\n").as_bytes());
    }
    engine.push(b"\x1b[H\x1b[2J\x1b[3J");
    let left: Vec<String> = engine
        .scrollback(64)
        .into_iter()
        .chain(engine.recent_lines(64))
        .filter(|line| line.contains("typed command"))
        .collect();
    assert_eq!(left, Vec::<String>::new());
    assert!(!grid_contains(&engine, "typed command"));
}
