#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    Visible,
    Hidden,
}

impl Visibility {
    #[must_use]
    pub const fn toggle(self) -> Self {
        match self {
            Visibility::Visible => Visibility::Hidden,
            Visibility::Hidden => Visibility::Visible,
        }
    }

    #[must_use]
    pub const fn is_visible(self) -> bool {
        matches!(self, Visibility::Visible)
    }
}
