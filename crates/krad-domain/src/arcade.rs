use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArcadeGame {
    ByteRun,
    BugHunt,
    LineBreak,
    TokenTrail,
}

impl ArcadeGame {
    pub const ALL: [Self; 4] = [
        Self::ByteRun,
        Self::BugHunt,
        Self::LineBreak,
        Self::TokenTrail,
    ];

    #[must_use]
    pub fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|game| *game == self)
            .unwrap_or_default();
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn drawn(previous: Option<Self>, roll: u64) -> Self {
        let choices: Vec<Self> = Self::ALL
            .into_iter()
            .filter(|game| Some(*game) != previous)
            .collect();
        let count = u64::try_from(choices.len()).unwrap_or(1).max(1);
        let index = usize::try_from(roll % count).unwrap_or_default();
        choices.get(index).copied().unwrap_or(Self::ByteRun)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArcadeScores {
    #[serde(default)]
    best: BTreeMap<ArcadeGame, u32>,
}

impl ArcadeScores {
    #[must_use]
    pub fn best(&self, game: ArcadeGame) -> u32 {
        self.best.get(&game).copied().unwrap_or_default()
    }

    pub fn record(&mut self, game: ArcadeGame, points: u32) -> bool {
        if points <= self.best(game) {
            return false;
        }
        self.best.insert(game, points);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draw_never_repeats_the_previous_game_and_reaches_every_other() {
        for previous in ArcadeGame::ALL {
            let drawn: Vec<ArcadeGame> = (0..ArcadeGame::ALL.len() as u64)
                .map(|roll| ArcadeGame::drawn(Some(previous), roll))
                .collect();
            assert!(!drawn.contains(&previous), "{previous:?}");
            for other in ArcadeGame::ALL.into_iter().filter(|game| *game != previous) {
                assert!(drawn.contains(&other), "{previous:?} never draws {other:?}");
            }
        }
    }

    #[test]
    fn the_next_game_cycles_through_all_four() {
        let mut game = ArcadeGame::ByteRun;
        let mut seen = Vec::new();
        for _ in ArcadeGame::ALL {
            seen.push(game);
            game = game.next();
        }
        assert_eq!(seen, ArcadeGame::ALL);
        assert_eq!(game, ArcadeGame::ByteRun);
    }

    #[test]
    fn only_a_higher_score_becomes_the_best_and_it_is_kept_per_game() {
        let mut scores = ArcadeScores::default();
        assert!(scores.record(ArcadeGame::BugHunt, 40));
        assert!(!scores.record(ArcadeGame::BugHunt, 40));
        assert!(!scores.record(ArcadeGame::BugHunt, 12));
        assert!(scores.record(ArcadeGame::LineBreak, 3));
        assert_eq!(
            (
                scores.best(ArcadeGame::BugHunt),
                scores.best(ArcadeGame::LineBreak),
                scores.best(ArcadeGame::TokenTrail)
            ),
            (40, 3, 0)
        );
    }

    #[test]
    fn scores_are_saved_under_kebab_case_names() {
        let mut scores = ArcadeScores::default();
        scores.record(ArcadeGame::TokenTrail, 9);
        let saved = serde_json::to_value(&scores).unwrap_or_default();
        assert_eq!(saved, serde_json::json!({"best": {"token-trail": 9}}));
        let read: ArcadeScores = serde_json::from_value(serde_json::json!({})).unwrap_or_default();
        assert_eq!(read, ArcadeScores::default());
    }
}
