use super::KraddieDesign;

pub const MAX_UNDO: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesignHistory {
    undo: Vec<KraddieDesign>,
    redo: Vec<KraddieDesign>,
}

impl DesignHistory {
    pub fn record(&mut self, before: &KraddieDesign) {
        if self.undo.last() == Some(before) {
            return;
        }
        self.undo.push(before.clone());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut KraddieDesign) -> bool {
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(current, before));
        true
    }

    pub fn redo(&mut self, current: &mut KraddieDesign) -> bool {
        let Some(after) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(current, after));
        true
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
