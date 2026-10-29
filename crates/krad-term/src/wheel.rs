use libghostty_vt::mouse::{self, Button, EncoderSize, Position};
use libghostty_vt::screen::Screen;
use libghostty_vt::terminal::Mode;
use libghostty_vt::{Error, Terminal};

use crate::CellPoint;

const ARROW_UP: &[u8] = b"\x1bOA";
const ARROW_DOWN: &[u8] = b"\x1bOB";
const CELL_PX: u32 = 1;
const CELL_CENTRE: f32 = 0.5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WheelInput {
    History(i32),
    Program(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Drag,
    Release,
}

pub(crate) struct MouseInput {
    encoder: mouse::Encoder<'static>,
    event: mouse::Event<'static>,
}

fn cell_position(at: CellPoint) -> Position {
    let coordinate = |index: usize| u16::try_from(index).map_or(f32::from(u16::MAX), f32::from);
    Position {
        x: coordinate(at.col) + CELL_CENTRE,
        y: coordinate(at.row) + CELL_CENTRE,
    }
}

impl MouseInput {
    pub(crate) fn new() -> Result<Self, Error> {
        Ok(Self {
            encoder: mouse::Encoder::new()?,
            event: mouse::Event::new()?,
        })
    }

    fn encode(&mut self, term: &Terminal<'_, '_>, held: bool) -> Option<Vec<u8>> {
        let cols = term.cols().ok()?;
        let rows = term.rows().ok()?;
        self.encoder
            .set_options_from_terminal(term)
            .set_size(EncoderSize {
                screen_width: u32::from(cols) * CELL_PX,
                screen_height: u32::from(rows) * CELL_PX,
                cell_width: CELL_PX,
                cell_height: CELL_PX,
                padding_top: 0,
                padding_bottom: 0,
                padding_right: 0,
                padding_left: 0,
            })
            .set_any_button_pressed(held);
        let mut bytes = Vec::new();
        self.encoder.encode_to_vec(&self.event, &mut bytes).ok()?;
        (!bytes.is_empty()).then_some(bytes)
    }

    pub(crate) fn click(
        &mut self,
        term: &Terminal<'_, '_>,
        action: MouseAction,
        at: CellPoint,
    ) -> Option<Vec<u8>> {
        if !term.is_mouse_tracking().ok()? {
            return None;
        }
        let (event, held) = match action {
            MouseAction::Press => (mouse::Action::Press, true),
            MouseAction::Drag => (mouse::Action::Motion, true),
            MouseAction::Release => (mouse::Action::Release, false),
        };
        self.event
            .set_action(event)
            .set_button(Some(Button::Left))
            .set_position(cell_position(at));
        self.encode(term, held)
    }

    pub(crate) fn wheel(
        &mut self,
        term: &Terminal<'_, '_>,
        lines: i32,
        at: CellPoint,
    ) -> WheelInput {
        let count = lines.unsigned_abs() as usize;
        let up = lines > 0;
        if term.is_mouse_tracking().unwrap_or(false) {
            let button = if up { Button::Four } else { Button::Five };
            let mut reports = Vec::new();
            for _ in 0..count {
                for action in [mouse::Action::Press, mouse::Action::Release] {
                    self.event
                        .set_action(action)
                        .set_button(Some(button))
                        .set_position(cell_position(at));
                    reports.extend(self.encode(term, false).unwrap_or_default());
                }
            }
            return WheelInput::Program(reports);
        }
        let full_screen = term
            .active_screen()
            .is_ok_and(|screen| screen == Screen::Alternate);
        if full_screen && term.mode(Mode::ALT_SCROLL).unwrap_or(false) {
            let arrow = if up { ARROW_UP } else { ARROW_DOWN };
            return WheelInput::Program(arrow.repeat(count));
        }
        WheelInput::History(lines)
    }
}
