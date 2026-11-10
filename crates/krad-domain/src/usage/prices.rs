use super::cost::{InvalidCost, UsdCost};
use super::tokens::TokenUsage;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

const BUNDLED_PRICES: &str = include_str!("model_prices.json");
const TOKENS_PER_PRICE: u128 = 1_000_000;
const PROVIDER_SEPARATOR: char = '/';
const VARIANT_START: char = '[';

static BUNDLED: LazyLock<Vec<ModelPrice>> =
    LazyLock::new(|| serde_json::from_str(BUNDLED_PRICES).unwrap_or_default());

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct PricePerMillion(UsdCost);

impl TryFrom<f64> for PricePerMillion {
    type Error = InvalidCost;

    fn try_from(dollars: f64) -> Result<Self, Self::Error> {
        UsdCost::try_from(dollars).map(Self)
    }
}

impl From<PricePerMillion> for f64 {
    fn from(price: PricePerMillion) -> Self {
        price.0.dollars()
    }
}

impl PricePerMillion {
    fn of(self, tokens: u64) -> UsdCost {
        let nanos = u128::from(self.0.nanos()) * u128::from(tokens) / TOKENS_PER_PRICE;
        UsdCost::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPrice {
    pub model: String,
    #[serde(default)]
    pub input: PricePerMillion,
    #[serde(default)]
    pub output: PricePerMillion,
    #[serde(default)]
    pub cache_read: PricePerMillion,
    #[serde(default)]
    pub cache_write: PricePerMillion,
}

impl ModelPrice {
    #[must_use]
    pub fn cost(&self, tokens: TokenUsage) -> UsdCost {
        self.input.of(tokens.input)
            + self.output.of(tokens.output)
            + self.cache_read.of(tokens.cache_read)
            + self.cache_write.of(tokens.cache_write)
    }

    fn matches(&self, model: &str) -> bool {
        !self.model.is_empty() && model.starts_with(&self.model.to_ascii_lowercase())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelPrices(Vec<ModelPrice>);

fn bare_model(model: &str) -> String {
    let named = model.rsplit(PROVIDER_SEPARATOR).next().unwrap_or(model);
    let named = named.split(VARIANT_START).next().unwrap_or(named);
    named.trim().to_ascii_lowercase()
}

impl ModelPrices {
    #[must_use]
    pub fn new(prices: Vec<ModelPrice>) -> Self {
        Self(prices)
    }

    #[must_use]
    pub fn bundled() -> Self {
        Self(BUNDLED.clone())
    }

    #[must_use]
    pub fn with_overrides(overrides: &[ModelPrice]) -> Self {
        Self(overrides.iter().chain(BUNDLED.iter()).cloned().collect())
    }

    #[must_use]
    pub fn price_of(&self, model: &str) -> Option<&ModelPrice> {
        let model = bare_model(model);
        self.0
            .iter()
            .filter(|price| price.matches(&model))
            .rev()
            .max_by_key(|price| price.model.len())
    }

    #[must_use]
    pub fn cost(&self, model: &str, tokens: TokenUsage) -> Option<UsdCost> {
        self.price_of(model).map(|price| price.cost(tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: u64, output: u64, cache_read: u64, cache_write: u64) -> TokenUsage {
        TokenUsage {
            input,
            output,
            cache_read,
            cache_write,
        }
    }

    #[test]
    fn the_bundled_prices_all_load() {
        let bundled = ModelPrices::bundled();
        let listed = BUNDLED_PRICES.matches("\"model\"").count();
        assert!(listed > 0);
        assert_eq!(bundled.0.len(), listed);
    }

    #[test]
    fn a_model_costs_what_its_price_says_for_each_kind_of_token() {
        let prices = ModelPrices::bundled();
        let cost = prices.cost("claude-haiku-5-5", tokens(2, 4, 33_692, 0));
        assert_eq!(cost, Some(UsdCost::from_nanos(339_120)));
    }

    #[test]
    fn the_longest_matching_name_wins_whatever_the_provider_or_variant() {
        let prices = ModelPrices::bundled();
        let named = |model: &str| prices.price_of(model).map(|price| price.model.as_str());
        assert_eq!(named("claude-opus-5-5"), Some("claude-opus-5-5"));
        assert_eq!(
            named("anthropic/claude-opus-5-5[1m]"),
            Some("claude-opus-5-5")
        );
        assert_eq!(named("claude-opus-5"), Some("claude-opus-5"));
        assert_eq!(named("gpt-5-codex"), Some("gpt-5-codex"));
        assert_eq!(named("llama-4"), None);
    }

    #[test]
    fn a_price_the_user_sets_wins_over_the_bundled_one() {
        let own = ModelPrice {
            model: "claude-opus-5-5".to_owned(),
            input: PricePerMillion::try_from(1.0).unwrap_or_default(),
            output: PricePerMillion::default(),
            cache_read: PricePerMillion::default(),
            cache_write: PricePerMillion::default(),
        };
        let prices = ModelPrices::with_overrides(&[own]);
        assert_eq!(
            prices.cost("claude-opus-5-5", tokens(1_000_000, 1_000, 0, 0)),
            UsdCost::try_from(1.0).ok()
        );
    }
}
