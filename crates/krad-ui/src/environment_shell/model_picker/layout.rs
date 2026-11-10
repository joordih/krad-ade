const NARROW_W: f32 = 320.0;
const STANDARD_W: f32 = 360.0;
const WIDE_W: f32 = 400.0;
const ROOMY_W: f32 = 440.0;
const DESCRIBED_FROM: f32 = 760.0;
const WIDE_FROM: f32 = 1200.0;
const ROOMY_FROM: f32 = 1800.0;
const EDGE_ROOM: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PickerLayout {
    pub(super) width: f32,
    pub(super) descriptions: bool,
}

pub(super) fn picker_layout(container_w: f32) -> PickerLayout {
    let (width, descriptions) = if container_w < DESCRIBED_FROM {
        (NARROW_W, false)
    } else if container_w < WIDE_FROM {
        (STANDARD_W, true)
    } else if container_w < ROOMY_FROM {
        (WIDE_W, true)
    } else {
        (ROOMY_W, true)
    };
    PickerLayout {
        width: width.min((container_w - EDGE_ROOM).max(0.0)),
        descriptions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_widens_and_shows_descriptions_as_the_window_grows() {
        let laid: Vec<(f32, bool)> = [640.0, 900.0, 1280.0, 1920.0, 2560.0]
            .into_iter()
            .map(|width| {
                let layout = picker_layout(width);
                (layout.width, layout.descriptions)
            })
            .collect();
        assert_eq!(
            laid,
            [
                (NARROW_W, false),
                (STANDARD_W, true),
                (WIDE_W, true),
                (ROOMY_W, true),
                (ROOMY_W, true),
            ]
        );
    }

    #[test]
    fn a_tiny_window_never_gets_a_picker_wider_than_itself() {
        assert!((picker_layout(200.0).width - (200.0 - EDGE_ROOM)).abs() < f32::EPSILON);
    }
}
