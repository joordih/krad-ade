use super::*;
use serde_json::json;

const LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

fn catalog(models: &serde_json::Value) -> Catalog {
    serde_json::from_value(json!({ "models": models }))
        .unwrap_or_else(|_| unreachable!("a catalog reads from any object"))
}

fn names(catalog: &Catalog) -> Vec<(&str, &str)> {
    catalog
        .models
        .iter()
        .map(|offer| (offer.value.as_str(), offer.name.as_str()))
        .collect()
}

fn reported_by_claude_code() -> serde_json::Value {
    json!([
        {"value": "default", "resolvedModel": "claude-opus-5-5", "displayName": "Default (recommended)",
         "description": "Opus 5.5 · Best for everyday, complex tasks", "supportedEffortLevels": LEVELS},
        {"value": "opus", "resolvedModel": "claude-opus-5-5", "displayName": "Opus",
         "description": "Opus 5.5 · Best for everyday, complex tasks", "supportedEffortLevels": LEVELS},
        {"value": "claude-fable-5-1[1m]", "resolvedModel": "claude-fable-5-1", "displayName": "Fable",
         "description": "Fable 5.1 · Most capable for your hardest and longest-running tasks",
         "supportedEffortLevels": LEVELS},
        {"value": "sonnet", "resolvedModel": "claude-sonnet-5-5", "displayName": "Sonnet",
         "description": "Sonnet 5.5 · Efficient for routine tasks", "supportedEffortLevels": LEVELS},
        {"value": "haiku", "resolvedModel": "claude-haiku-5-5", "displayName": "Haiku",
         "description": "Haiku 5.5 · Fastest for quick answers", "supportedEffortLevels": ["low", "high"]}
    ])
}

#[test]
fn claude_code_models_show_their_real_names_and_versions() {
    let read = catalog(&reported_by_claude_code());
    assert_eq!(
        names(&read),
        [
            ("opus", "Claude Opus 5.5"),
            ("claude-fable-5-1[1m]", "Claude Fable 5.1"),
            ("sonnet", "Claude Sonnet 5.5"),
            ("haiku", "Claude Haiku 5.5"),
        ]
    );
    let opus = &read.models[0];
    assert!(
        opus.recommended,
        "the default folds into the model it picks"
    );
    assert!(opus.answers_to("default"));
    assert_eq!(
        opus.description.as_deref(),
        Some("Best for everyday, complex tasks")
    );
    assert_eq!(opus.efforts, Effort::ALL);
    assert_eq!(
        opus.source,
        Some(ModelSource::Provider(ANTHROPIC.to_owned()))
    );
    assert_eq!(read.models[1].context, Some(ContextSize::OneMillion));
    assert_eq!(read.models[3].efforts, [Effort::Low, Effort::High]);
}

#[test]
fn a_default_picking_a_model_not_listed_stays_as_its_own_row() {
    let read = catalog(&json!([
        {"value": "default", "resolvedModel": "claude-sonnet-5-5", "description": "Sonnet 5.5 · Efficient"},
        {"value": "opus", "resolvedModel": "claude-opus-5-5"}
    ]));
    assert_eq!(
        names(&read),
        [
            ("default", "Claude Sonnet 5.5"),
            ("opus", "Claude Opus 5.5")
        ]
    );
    assert!(read.models[0].recommended);
    assert!(!read.models[1].recommended);
}

#[test]
fn older_lines_without_ids_fall_back_to_the_description_then_the_alias_table() {
    let read = catalog(&json!([
        {"value": "sonnet", "displayName": "Sonnet", "description": "Sonnet 4.5 · Daily work"},
        {"value": "haiku", "displayName": "Haiku"},
        {"value": "custom-model", "displayName": "Custom"},
        {"value": "claude-opus-4-1-20250805"},
        {"value": "mystery"}
    ]));
    assert_eq!(
        names(&read),
        [
            ("sonnet", "Claude Sonnet 4.5"),
            ("haiku", "Claude Haiku"),
            ("custom-model", "Custom"),
            ("claude-opus-4-1-20250805", "Claude Opus 4.1"),
            ("mystery", "mystery"),
        ]
    );
    assert!(read.models.iter().all(|offer| offer.efforts.is_empty()));
}

#[test]
fn unknown_effort_levels_are_left_out_and_the_rest_ordered() {
    let read = catalog(&json!([
        {"value": "opus", "resolvedModel": "claude-opus-5-5",
         "supportedEffortLevels": ["max", "turbo", "low", "low"]}
    ]));
    assert_eq!(read.models[0].efforts, [Effort::Low, Effort::Max]);
}
