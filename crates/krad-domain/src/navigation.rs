use std::path::PathBuf;

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationPlace {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationStep {
    Back,
    Forward,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationHistory {
    back: Vec<NavigationPlace>,
    forward: Vec<NavigationPlace>,
}

impl NavigationHistory {
    pub fn record(&mut self, from: NavigationPlace) {
        self.forward.clear();
        if self.back.last() == Some(&from) {
            return;
        }
        self.back.push(from);
        if self.back.len() > HISTORY_LIMIT {
            self.back.remove(0);
        }
    }

    pub fn step(
        &mut self,
        step: NavigationStep,
        current: Option<NavigationPlace>,
    ) -> Option<NavigationPlace> {
        let (from, to) = match step {
            NavigationStep::Back => (&mut self.back, &mut self.forward),
            NavigationStep::Forward => (&mut self.forward, &mut self.back),
        };
        let place = from.pop()?;
        to.extend(current);
        Some(place)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(path: &str, line: u32) -> NavigationPlace {
        NavigationPlace {
            path: PathBuf::from(path),
            line,
            column: 0,
        }
    }

    #[test]
    fn going_back_and_forward_retraces_the_jumps() {
        let mut history = NavigationHistory::default();
        history.record(place("a.rs", 1));
        history.record(place("b.rs", 5));
        let back = history.step(NavigationStep::Back, Some(place("c.rs", 9)));
        assert_eq!(back, Some(place("b.rs", 5)));
        let forward = history.step(NavigationStep::Forward, Some(place("b.rs", 5)));
        assert_eq!(forward, Some(place("c.rs", 9)));
        assert_eq!(history.step(NavigationStep::Forward, None), None);
    }

    #[test]
    fn a_new_jump_drops_the_forward_places_and_repeats_are_kept_once() {
        let mut history = NavigationHistory::default();
        history.record(place("a.rs", 1));
        history.record(place("a.rs", 1));
        history.step(NavigationStep::Back, Some(place("b.rs", 2)));
        history.record(place("a.rs", 3));
        assert_eq!(history.step(NavigationStep::Forward, None), None);
        assert_eq!(
            history.step(NavigationStep::Back, None),
            Some(place("a.rs", 3))
        );
        assert_eq!(history.step(NavigationStep::Back, None), None);
    }
}
