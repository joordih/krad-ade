#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct LoadGeneration(u64);

impl LoadGeneration {
    pub(in crate::environment_shell) fn advance(&mut self) -> Self {
        self.0 = self.0.wrapping_add(1);
        *self
    }

    pub(in crate::environment_shell) fn admits(self, result: Self) -> bool {
        self == result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_latest_load_is_admitted() {
        let mut current = LoadGeneration::default();
        let first = current.advance();
        assert!(current.admits(first));
        let second = current.advance();
        assert!(!current.admits(first));
        assert!(current.admits(second));
    }
}
