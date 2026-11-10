use super::name::{KraddieId, KraddieName};

pub const KRADDIE_NAME_IDEAS: [&str; 24] = [
    "Pip", "Mochi", "Bramble", "Nimbus", "Juniper", "Biscuit", "Sprocket", "Wobble", "Tansy",
    "Pebble", "Clover", "Fennel", "Ziggy", "Marlow", "Tofu", "Quill", "Rumi", "Basil", "Kumo",
    "Pickle", "Noodle", "Waffle", "Fig", "Dumpling",
];
pub const NAME_IDEAS_SHOWN: usize = 4;
const STRIDES: [usize; 7] = [5, 7, 11, 13, 17, 19, 23];

#[must_use]
pub fn name_ideas(seed: u64, count: usize, taken: &[KraddieId]) -> Vec<&'static str> {
    let len = KRADDIE_NAME_IDEAS.len();
    let wide = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
    let narrow = |value: u64| usize::try_from(value).unwrap_or_default();
    let start = narrow(seed % wide(len));
    let stride = STRIDES[narrow(seed / wide(len) % wide(STRIDES.len()))];
    (0..len)
        .map(|step| KRADDIE_NAME_IDEAS[(start + step * stride) % len])
        .filter(|idea| {
            KraddieName::try_from((*idea).to_owned()).is_ok_and(|name| !taken.contains(&name.id()))
        })
        .take(count)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ideas_are_distinct_valid_names_that_change_with_the_seed() {
        let first = name_ideas(3, NAME_IDEAS_SHOWN, &[]);
        assert_eq!(first.len(), NAME_IDEAS_SHOWN);
        let mut unique = first.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), first.len());
        assert!(first
            .iter()
            .all(|idea| KraddieName::try_from((*idea).to_owned()).is_ok()));
        assert_ne!(name_ideas(4, NAME_IDEAS_SHOWN, &[]), first);
        assert_eq!(name_ideas(3, NAME_IDEAS_SHOWN, &[]), first);
    }

    #[test]
    fn every_seed_visits_every_idea_once() {
        for seed in [0, 1, 23, 24, 25, 1_000_003, u64::MAX] {
            let mut all = name_ideas(seed, KRADDIE_NAME_IDEAS.len(), &[]);
            all.sort_unstable();
            let mut expected = KRADDIE_NAME_IDEAS.to_vec();
            expected.sort_unstable();
            assert_eq!(all, expected, "seed {seed}");
        }
    }

    #[test]
    fn names_already_taken_are_not_offered() {
        let taken: Vec<KraddieId> = KRADDIE_NAME_IDEAS[..20]
            .iter()
            .filter_map(|idea| KraddieName::try_from((*idea).to_owned()).ok())
            .map(|name| name.id())
            .collect();
        let offered = name_ideas(7, NAME_IDEAS_SHOWN, &taken);
        assert_eq!(offered.len(), NAME_IDEAS_SHOWN);
        assert!(offered
            .iter()
            .all(|idea| KRADDIE_NAME_IDEAS[20..].contains(idea)));
    }
}
