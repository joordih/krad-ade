use krad_domain::ToolchainVersion;

use super::manifest::CommandOutput;

pub(crate) const MAX_OUTPUT_ITEMS: usize = 256;
const DEFAULT_MARKER: char = '*';
const VERSION_QUOTES: [char; 2] = ['"', '\''];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) value: String,
    pub(crate) version: Option<String>,
    pub(crate) name: Option<String>,
}

fn json_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn lines(text: &str, skip_prefix: &[String]) -> Vec<Found> {
    text.lines()
        .map(|line| line.trim().trim_start_matches(DEFAULT_MARKER).trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            !skip_prefix
                .iter()
                .any(|prefix| line.starts_with(prefix.as_str()))
        })
        .filter_map(|line| line.split_whitespace().next())
        .take(MAX_OUTPUT_ITEMS)
        .map(|value| Found {
            value: value.to_owned(),
            version: None,
            name: None,
        })
        .collect()
}

fn json(
    text: &str,
    items: &str,
    path: &str,
    version: Option<&str>,
    name: Option<&str>,
) -> Vec<Found> {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(entries)) = document.pointer(items) else {
        return Vec::new();
    };
    entries
        .iter()
        .take(MAX_OUTPUT_ITEMS)
        .filter_map(|entry| {
            Some(Found {
                value: json_text(entry.pointer(path)?)?,
                version: version
                    .and_then(|pointer| entry.pointer(pointer))
                    .and_then(json_text),
                name: name
                    .and_then(|pointer| entry.pointer(pointer))
                    .and_then(json_text),
            })
        })
        .collect()
}

pub(crate) fn parse_output(text: &str, output: &CommandOutput) -> Vec<Found> {
    match output {
        CommandOutput::Lines { skip_prefix } => lines(text, skip_prefix),
        CommandOutput::Json {
            items,
            path,
            version,
            name,
        } => json(text, items, path, version.as_deref(), name.as_deref()),
    }
}

pub(crate) fn version_after(text: &str, after: Option<&str>) -> Option<ToolchainVersion> {
    let start = match after {
        Some(marker) => text.find(marker)? + marker.len(),
        None => 0,
    };
    text[start..]
        .split_whitespace()
        .find_map(|word| word.trim_matches(VERSION_QUOTES).parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_output_takes_the_first_word_of_each_line() {
        let rustup = "stable-x86_64-unknown-linux-gnu (default)\nnightly-2024-11-01-x86_64-unknown-linux-gnu\n\n";
        let found = parse_output(
            rustup,
            &CommandOutput::Lines {
                skip_prefix: Vec::new(),
            },
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].value, "stable-x86_64-unknown-linux-gnu");
        let asdf = "  18.19.0\n *20.11.1\nNo versions installed\n";
        let skip = CommandOutput::Lines {
            skip_prefix: vec!["No ".to_owned()],
        };
        let values: Vec<String> = parse_output(asdf, &skip)
            .into_iter()
            .map(|found| found.value)
            .collect();
        assert_eq!(values, ["18.19.0", "20.11.1"]);
    }

    #[test]
    fn json_output_maps_fields_by_pointer() {
        let mise = r#"[{"version":"20.11.1","install_path":"/home/me/.local/share/mise/installs/node/20.11.1"},{"version":"18.19.0"}]"#;
        let output = CommandOutput::Json {
            items: String::new(),
            path: "/install_path".to_owned(),
            version: Some("/version".to_owned()),
            name: None,
        };
        let found = parse_output(mise, &output);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].version.as_deref(), Some("20.11.1"));
        assert_eq!(parse_output("not json", &output), Vec::new());
    }

    #[test]
    fn versions_are_read_after_their_marker() {
        assert_eq!(
            version_after("Python 3.12.4\n", Some("Python")).map(|v| v.to_string()),
            Some("3.12.4".to_owned())
        );
        assert_eq!(
            version_after("go version go1.22.3 linux/amd64", Some("go version go"))
                .map(|v| v.to_string()),
            Some("1.22.3".to_owned())
        );
        assert_eq!(
            version_after("v20.11.1", None).map(|v| v.to_string()),
            Some("v20.11.1".to_owned())
        );
        assert_eq!(version_after("no numbers here", None), None);
        assert_eq!(
            version_after("openjdk version \"21.0.2\" 2024-01-16", Some("version"))
                .map(|v| v.to_string()),
            Some("21.0.2".to_owned())
        );
    }
}
