use serde_json::Value;
use std::sync::Arc;

use super::Ran;
use crate::engine::mcp::{McpError, McpToolbox};

const SUMMARY_SEPARATOR: &str = ", ";
const PAIR_SEPARATOR: &str = ": ";
const MAX_SUMMARY_CHARS: usize = 120;
const ELLIPSIS: char = '…';

fn shown_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(super) fn summary(arguments: &Value) -> String {
    let full = match arguments {
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| format!("{key}{PAIR_SEPARATOR}{}", shown_value(value)))
            .collect::<Vec<_>>()
            .join(SUMMARY_SEPARATOR),
        Value::Null => String::new(),
        other => shown_value(other),
    };
    let line = full.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(MAX_SUMMARY_CHARS) {
        Some((end, _)) => format!("{}{ELLIPSIS}", &line[..end]),
        None => line,
    }
}

pub(super) fn run(toolbox: &Arc<McpToolbox>, name: &str, arguments: &Value) -> Ran {
    match toolbox.call(name, arguments.clone()) {
        Ok(content) => Ran::done(content),
        Err(McpError::ToolFailed(content)) => Ran::failed(content),
        Err(error) => Ran::failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_summary_shows_the_arguments_on_one_short_line() {
        assert_eq!(summary(&json!({"path": "src/lib.rs"})), "path: src/lib.rs");
        assert_eq!(summary(&json!({"depth": 2})), "depth: 2");
        assert_eq!(summary(&json!({"a": 1, "b": 2})).len(), "a: 1, b: 2".len());
        assert_eq!(summary(&json!({"query": "two\nlines"})), "query: two lines");
        assert_eq!(summary(&Value::Null), "");
        let long = summary(&json!({"text": "x".repeat(300)}));
        assert_eq!(long.chars().count(), MAX_SUMMARY_CHARS + 1);
        assert!(long.ends_with(ELLIPSIS));
    }
}
