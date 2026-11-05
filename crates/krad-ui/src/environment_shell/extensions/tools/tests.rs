use super::*;
use krad_extensions::program::read_installed;
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const DOCKER_TOOLS: &str = r#"
[[contributes.tool]]
name = "list_containers"
description = "List containers"

[[contributes.tool]]
name = "restart_container"
description = "Restart a container"
effect = "destructive"
"#;

fn manifest(id: &str, tools: &str) -> Result<ExtensionManifest, Box<dyn std::error::Error>> {
    let folder = std::env::temp_dir().join(format!(
        "krad-ui-tools-{}-{}",
        id.replace('.', "_"),
        std::process::id()
    ));
    fs::create_dir_all(&folder)?;
    fs::write(
        folder.join("extension.toml"),
        format!(
            r#"
id = "{id}"
name = "{id} tools"
version = "0.1.0"
api = "1.0"

[program]
placement = "app"
runtime = "node"
args = ["main.js"]
{tools}
"#
        ),
    )?;
    let installed = read_installed(&folder)?.ok_or("the folder holds an extension")?;
    fs::remove_dir_all(&folder)?;
    Ok(installed.manifest)
}

#[test]
fn every_tool_is_offered_under_its_namespaced_name_and_effect() -> TestResult {
    let offered = offered_tools(&[manifest("docker", DOCKER_TOOLS)?]);
    let names: Vec<&str> = offered.iter().map(|tool| tool.info.name.as_str()).collect();
    assert_eq!(
        names,
        ["ext_docker_list-containers", "ext_docker_restart-container"]
    );
    assert_eq!(offered[0].info.effect, ExtensionToolEffect::ReadOnly);
    assert_eq!(offered[1].info.effect, ExtensionToolEffect::Destructive);
    assert_eq!(offered[1].tool.as_ref(), "restart_container");
    assert_eq!(offered[1].info.extension, "docker tools");
    Ok(())
}

#[test]
fn a_name_taken_by_an_earlier_extension_is_not_offered_twice() -> TestResult {
    let first = manifest(
        "a.b",
        "[[contributes.tool]]\nname = \"x\"\ndescription = \"one\"",
    )?;
    let second = manifest(
        "a-b",
        "[[contributes.tool]]\nname = \"x\"\ndescription = \"two\"",
    )?;
    let offered = offered_tools(&[first, second]);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].info.description, "one");
    Ok(())
}

#[test]
fn a_tool_result_becomes_the_agents_answer() {
    let answer = answer_of(CallToolResult {
        content: vec![
            ToolContent::Text {
                text: "web".to_owned(),
            },
            ToolContent::Text {
                text: "db".to_owned(),
            },
        ],
        outcome: ToolOutcome::Failed,
    });
    assert_eq!(answer.text, "web\ndb");
    assert_eq!(answer.outcome, ExtensionToolOutcome::Failed);
}
