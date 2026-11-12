use crate::model::ModelId;

const SCORE_MAX: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Score(u8);

impl Score {
    pub const MAX: u8 = SCORE_MAX;

    #[must_use]
    pub const fn new(value: u8) -> Self {
        if value > SCORE_MAX {
            Self(SCORE_MAX)
        } else {
            Self(value)
        }
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ModelTraits {
    pub smart: Score,
    pub speed: Score,
    pub cost: Score,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelFamily {
    ClaudeOpus,
    ClaudeSonnet,
    ClaudeHaiku,
    GptCodexMini,
    GptCodex,
    GptMini,
    Gpt,
    GeminiPro,
    GeminiFlash,
}

const FAMILY_MARKERS: [(&[&str], ModelFamily); 9] = [
    (&["opus"], ModelFamily::ClaudeOpus),
    (&["sonnet"], ModelFamily::ClaudeSonnet),
    (&["haiku"], ModelFamily::ClaudeHaiku),
    (&["gpt", "codex", "mini"], ModelFamily::GptCodexMini),
    (&["gpt", "codex"], ModelFamily::GptCodex),
    (&["gpt", "mini"], ModelFamily::GptMini),
    (&["gpt"], ModelFamily::Gpt),
    (&["gemini", "pro"], ModelFamily::GeminiPro),
    (&["gemini", "flash"], ModelFamily::GeminiFlash),
];

impl ModelFamily {
    fn of(id: &ModelId) -> Option<Self> {
        let name = id.as_str().to_ascii_lowercase();
        FAMILY_MARKERS
            .iter()
            .find(|(markers, _)| markers.iter().all(|marker| name.contains(marker)))
            .map(|(_, family)| *family)
    }

    const fn traits(self) -> ModelTraits {
        let (smart, speed, cost) = match self {
            Self::ClaudeOpus => (5, 2, 5),
            Self::ClaudeSonnet | Self::GptCodex => (4, 4, 3),
            Self::ClaudeHaiku | Self::GptCodexMini | Self::GptMini | Self::GeminiFlash => (3, 5, 1),
            Self::Gpt => (4, 3, 3),
            Self::GeminiPro => (5, 3, 4),
        };
        ModelTraits {
            smart: Score::new(smart),
            speed: Score::new(speed),
            cost: Score::new(cost),
        }
    }
}

#[must_use]
pub fn model_traits(id: &ModelId) -> Option<ModelTraits> {
    ModelFamily::of(id).map(ModelFamily::traits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn traits_of(name: &str) -> Option<ModelTraits> {
        ModelId::parse(name).as_ref().and_then(model_traits)
    }

    #[test]
    fn known_families_have_traits_and_unknown_models_have_none() {
        let opus = traits_of("claude-opus-4-5").map(|traits| traits.smart.value());
        assert_eq!(opus, Some(Score::MAX));
        let mini = traits_of("gpt-5.1-codex-mini").map(|traits| traits.cost.value());
        let codex = traits_of("gpt-5.1-codex").map(|traits| traits.cost.value());
        assert!(mini < codex);
        assert_eq!(traits_of("kimi-k2"), None);
    }

    #[test]
    fn a_score_never_passes_the_top() {
        assert_eq!(Score::new(9).value(), Score::MAX);
    }
}
