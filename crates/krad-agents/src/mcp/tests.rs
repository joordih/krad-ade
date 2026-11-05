use super::*;
use crate::extension_tools::{ExtensionToolAnswer, ExtensionToolEffect, ExtensionToolInfo};
use std::cell::RefCell;

struct FakeBus {
    catalog: Option<ToolCatalog>,
    asked: RefCell<Vec<Request>>,
}

impl FakeBus {
    fn new(catalog: Option<ToolCatalog>) -> Self {
        Self {
            catalog,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl Bus for FakeBus {
    fn ask(&self, request: Request) -> Result<Response, String> {
        self.asked.borrow_mut().push(request.clone());
        match request {
            Request::Tools => self
                .catalog
                .clone()
                .map(Response::Tools)
                .ok_or_else(|| OUTSIDE_KRAD.to_owned()),
            Request::ExtensionTool { name, .. } => {
                Ok(Response::ExtensionTool(ExtensionToolAnswer {
                    text: format!("ran {name}"),
                    outcome: ExtensionToolOutcome::Done,
                }))
            }
            _ => Err(OUTSIDE_KRAD.to_owned()),
        }
    }
}

fn docker_catalog(code_tools: CodeTools) -> ToolCatalog {
    ToolCatalog {
        code_tools,
        extension_tools: vec![ExtensionToolInfo {
            name: "ext_docker_restart-container".to_owned(),
            extension: "Docker".to_owned(),
            description: "Restart a container".to_owned(),
            input_schema: json!({"type": "object", "properties": {"id": {"type": "string"}}}),
            effect: ExtensionToolEffect::Destructive,
        }],
    }
}

fn ask(bus: &FakeBus, method: &str, params: &Value) -> Option<Value> {
    answer(
        serde_json::from_value(
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
        )
        .ok()?,
        bus,
    )
}

fn listed_names(listed: &Value) -> Vec<String> {
    listed["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn without_the_app_every_code_tool_is_listed_with_its_schema() {
    let listed = ask(&FakeBus::new(None), "tools/list", &json!({})).unwrap_or_default();
    let tools = listed["result"]["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(tools.len(), CodeTool::ALL.len());
    let rename = tools
        .iter()
        .find(|tool| tool["name"] == "code_rename")
        .cloned()
        .unwrap_or_default();
    assert_eq!(rename["annotations"]["destructiveHint"], json!(true));
    assert!(rename["inputSchema"]["required"]
        .as_array()
        .is_some_and(|required| required.contains(&json!("newName"))));
}

#[test]
fn extension_tools_are_listed_beside_code_tools_with_their_annotations() {
    let bus = FakeBus::new(Some(docker_catalog(CodeTools::On)));
    let listed = ask(&bus, "tools/list", &json!({})).unwrap_or_default();
    let names = listed_names(&listed);
    assert_eq!(names.len(), CodeTool::ALL.len() + 1);
    let docker = listed["result"]["tools"]
        .as_array()
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool["name"] == "ext_docker_restart-container")
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(docker["annotations"]["destructiveHint"], json!(true));
    assert_eq!(docker["annotations"]["readOnlyHint"], json!(false));
    assert_eq!(
        docker["inputSchema"]["properties"]["id"]["type"],
        json!("string")
    );
}

#[test]
fn code_tools_turned_off_leave_only_extension_tools() {
    let bus = FakeBus::new(Some(docker_catalog(CodeTools::Off)));
    let listed = ask(&bus, "tools/list", &json!({})).unwrap_or_default();
    assert_eq!(listed_names(&listed), ["ext_docker_restart-container"]);
}

#[test]
fn an_extension_tool_call_is_forwarded_over_the_bus_with_its_arguments() {
    let bus = FakeBus::new(Some(docker_catalog(CodeTools::On)));
    let called = ask(
        &bus,
        "tools/call",
        &json!({"name": "ext_docker_restart-container", "arguments": {"id": "web"}}),
    )
    .unwrap_or_default();
    assert_eq!(
        called["result"]["content"][0]["text"],
        json!("ran ext_docker_restart-container")
    );
    assert_eq!(called["result"]["isError"], json!(false));
    assert!(bus.asked.borrow().contains(&Request::ExtensionTool {
        name: "ext_docker_restart-container".to_owned(),
        arguments: json!({"id": "web"}),
    }));
}

#[test]
fn a_call_without_a_position_is_refused_before_reaching_the_bus() {
    let bus = FakeBus::new(None);
    let refused = ask(
        &bus,
        "tools/call",
        &json!({"name": "code_definition", "arguments": {"path": "Main.java"}}),
    )
    .unwrap_or_default();
    assert_eq!(refused["error"]["code"], json!(INVALID_PARAMS));
    assert!(bus.asked.borrow().is_empty());
}

#[test]
fn notifications_get_no_answer() {
    let notification = serde_json::from_value::<Incoming>(
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
    assert!(notification.is_ok_and(|incoming| answer(incoming, &FakeBus::new(None)).is_none()));
}
