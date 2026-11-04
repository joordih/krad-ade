use gpui_kit::{Pixels, Point, Window};

use super::Picker;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickerChoice {}

impl PickerChoice {
    pub(crate) fn resolve<T: Clone>(
        self,
        _: &[T],
        _: &[T],
        _: impl FnOnce(&str) -> Option<T>,
    ) -> Option<T> {
        match self {}
    }
}

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
