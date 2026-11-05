use krad_domain::CodeTools;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const EXTENSION_TOOL_PREFIX: &str = "ext_";
const NAME_SEPARATOR: char = '_';
const NAME_FILLER: char = '-';
const MAX_TOOL_NAME: usize = 64;
const NO_EXTENSION_TOOLS: &str = "No extension tools";
const OBJECT_TYPE: &str = "object";
pub(crate) const TOOL_USAGE: &str = "Usage: krad-ade-agents tool NAME [JSON_ARGUMENTS]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionToolEffect {
    #[default]
    ReadOnly,
    Destructive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionToolInfo {
    pub name: String,
    pub extension: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: ExtensionToolEffect,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCatalog {
    pub code_tools: CodeTools,
    pub extension_tools: Vec<ExtensionToolInfo>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionToolOutcome {
    #[default]
    Done,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionToolAnswer {
    pub text: String,
    pub outcome: ExtensionToolOutcome,
}

fn mcp_safe(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == NAME_FILLER {
                character
            } else {
                NAME_FILLER
            }
        })
        .collect()
}

#[must_use]
pub fn extension_tool_name(extension: &str, tool: &str) -> Option<String> {
    let name = format!(
        "{EXTENSION_TOOL_PREFIX}{}{NAME_SEPARATOR}{}",
        mcp_safe(extension),
        mcp_safe(tool)
    );
    (name.len() <= MAX_TOOL_NAME).then_some(name)
}

#[must_use]
pub fn is_extension_tool(name: &str) -> bool {
    name.starts_with(EXTENSION_TOOL_PREFIX)
}

fn object_schema(schema: &Value) -> Value {
    if schema.get("type").and_then(Value::as_str) == Some(OBJECT_TYPE) {
        schema.clone()
    } else {
        json!({ "type": OBJECT_TYPE })
    }
}

pub(crate) fn mcp_schema(tool: &ExtensionToolInfo) -> Value {
    let destructive = tool.effect == ExtensionToolEffect::Destructive;
    json!({
        "name": tool.name,
        "description": format!("{}: {}", tool.extension, tool.description),
        "inputSchema": object_schema(&tool.input_schema),
        "annotations": {"readOnlyHint": !destructive, "destructiveHint": destructive},
    })
}

pub(crate) fn render_catalog(catalog: &ToolCatalog) -> String {
    if catalog.extension_tools.is_empty() {
        return NO_EXTENSION_TOOLS.to_owned();
    }
    catalog
        .extension_tools
        .iter()
        .map(|tool| format!("{}  {}: {}", tool.name, tool.extension, tool.description))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn tool_call_request(positional: &[String]) -> Result<(String, Value), String> {
    let (name, rest) = positional
        .split_first()
        .ok_or_else(|| TOOL_USAGE.to_owned())?;
    if !is_extension_tool(name) {
        return Err(format!(
            "{name} is not an extension tool; see `krad-ade-agents tools`"
        ));
    }
    let arguments = if rest.is_empty() {
        json!({})
    } else {
        serde_json::from_str(&rest.join(" ")).map_err(|error| error.to_string())?
    };
    Ok((name.clone(), arguments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_namespaced_and_mcp_safe() {
        assert_eq!(
            extension_tool_name("docker", "list_containers").as_deref(),
            Some("ext_docker_list-containers")
        );
        assert_eq!(
            extension_tool_name("acme.tools", "a.b").as_deref(),
            Some("ext_acme-tools_a-b")
        );
        assert_eq!(extension_tool_name(&"x".repeat(60), "tool"), None);
        assert!(is_extension_tool("ext_docker_ps"));
        assert!(!is_extension_tool("code_hover"));
    }

    #[test]
    fn destructive_tools_are_annotated_and_schemas_stay_objects() {
        let tool = ExtensionToolInfo {
            name: "ext_docker_restart".to_owned(),
            extension: "Docker".to_owned(),
            description: "Restart a container".to_owned(),
            input_schema: json!({"type": "string"}),
            effect: ExtensionToolEffect::Destructive,
        };
        let schema = mcp_schema(&tool);
        assert_eq!(schema["annotations"]["destructiveHint"], json!(true));
        assert_eq!(schema["annotations"]["readOnlyHint"], json!(false));
        assert_eq!(schema["inputSchema"], json!({"type": "object"}));
        assert_eq!(schema["description"], json!("Docker: Restart a container"));
    }

    #[test]
    fn a_cli_call_takes_a_name_and_json_arguments() {
        let words = |text: &str| text.split(' ').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            tool_call_request(&words(r#"ext_docker_logs {"id": "web"}"#)),
            Ok(("ext_docker_logs".to_owned(), json!({"id": "web"})))
        );
        assert_eq!(
            tool_call_request(&words("ext_docker_ps")),
            Ok(("ext_docker_ps".to_owned(), json!({})))
        );
        assert!(tool_call_request(&words("code_hover")).is_err());
        assert!(tool_call_request(&[]).is_err());
    }
}
