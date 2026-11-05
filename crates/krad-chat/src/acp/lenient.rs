use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tracing::debug;

pub(super) fn readable_items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let items = Option::<Vec<Value>>::deserialize(deserializer)?;
    Ok(items.map_or_default(keep_readable))
}

pub(super) fn optional_readable_items<'de, D, T>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let items = Option::<Vec<Value>>::deserialize(deserializer)?;
    Ok(items.map(keep_readable))
}

fn keep_readable<T: DeserializeOwned>(items: Vec<Value>) -> Vec<T> {
    items
        .into_iter()
        .filter_map(|item| {
            serde_json::from_value(item)
                .inspect_err(|error| debug!(category = ?error.classify(), "skipped an ACP item the app cannot read"))
                .ok()
        })
        .collect()
}
