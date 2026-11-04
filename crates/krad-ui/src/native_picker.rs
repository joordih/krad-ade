#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
pub(crate) use macos::{picker_host, show_picker};
#[cfg(not(target_os = "macos"))]
pub(crate) use other::{picker_host, show_picker};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerRow {
    pub(crate) label: String,
    pub(crate) checked: bool,
    pub(crate) shortcut: Option<char>,
    pub(crate) symbol: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Picker {
    pub(crate) placeholder: &'static str,
    pub(crate) rows: Vec<PickerRow>,
    pub(crate) footer: Vec<PickerRow>,
    pub(crate) create_label: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickerChoice {
    Row(usize),
    Footer(usize),
    Create(String),
}
