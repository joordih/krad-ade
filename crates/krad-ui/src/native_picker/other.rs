use gpui_kit::{Pixels, Point, Window};

use super::{Picker, PickerChoice};

pub(crate) enum PickerHost {}

pub(crate) const fn picker_host(_: &Window) -> Option<PickerHost> {
    None
}

pub(crate) const fn show_picker(
    _: &Picker,
    _: Point<Pixels>,
    host: &PickerHost,
) -> Option<PickerChoice> {
    match *host {}
}
