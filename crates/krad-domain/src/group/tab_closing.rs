use crate::ids::TerminalId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabClosing {
    This,
    Others,
    ToTheRight,
    ToTheLeft,
    All,
}

impl TabClosing {
    #[must_use]
    pub fn closed(self, tabs: &[TerminalId], tab: TerminalId) -> Vec<TerminalId> {
        let Some(at) = tabs.iter().position(|candidate| *candidate == tab) else {
            return Vec::new();
        };
        let (left, rest) = tabs.split_at(at);
        let right = rest.get(1..).unwrap_or_default();
        match self {
            Self::This => vec![tab],
            Self::Others => left.iter().chain(right).copied().collect(),
            Self::ToTheRight => right.to_vec(),
            Self::ToTheLeft => left.to_vec(),
            Self::All => tabs.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs() -> Vec<TerminalId> {
        (1..=4).map(TerminalId).collect()
    }

    #[test]
    fn each_closing_takes_the_tabs_around_the_one_clicked() {
        let tabs = tabs();
        let ids = |closing: TabClosing| -> Vec<u64> {
            closing
                .closed(&tabs, TerminalId(2))
                .into_iter()
                .map(TerminalId::as_u64)
                .collect()
        };
        assert_eq!(ids(TabClosing::This), [2]);
        assert_eq!(ids(TabClosing::Others), [1, 3, 4]);
        assert_eq!(ids(TabClosing::ToTheRight), [3, 4]);
        assert_eq!(ids(TabClosing::ToTheLeft), [1]);
        assert_eq!(ids(TabClosing::All), [1, 2, 3, 4]);
    }

    #[test]
    fn a_tab_outside_the_strip_closes_nothing() {
        assert_eq!(TabClosing::All.closed(&tabs(), TerminalId(9)), Vec::new());
    }
}
