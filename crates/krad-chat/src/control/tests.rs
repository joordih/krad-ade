use super::*;
use krad_domain::{Effort, PermissionMode};
use serde_json::json;

const INIT_ID: &str = "krad_0";

fn routed(value: &Value) -> Option<Routed> {
    route(&value.to_string(), INIT_ID)
}

fn ask(input: &Value, tool: &str) -> ToolAsk {
    suggested_ask(input, tool, &json!([]))
}

fn suggested_ask(input: &Value, tool: &str, suggestions: &Value) -> ToolAsk {
    let request = json!({
        "type": "control_request",
        "request_id": "req_7",
        "request": {
            "subtype": "can_use_tool",
            "tool_name": tool,
            "input": input,
            "title": "Run the tests",
            "permission_suggestions": suggestions,
        },
    });
    match routed(&request) {
        Some(Routed::Ask(ask)) => ask,
        other => unreachable!("a can_use_tool request, not {other:?}"),
    }
}

fn parsed(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or_default()
}

fn questions(asked: &ToolAsk) -> &[Question] {
    match &asked.kind {
        AskKind::Questions(questions) => questions,
        other => unreachable!("questions, not {other:?}"),
    }
}

#[test]
fn a_permission_request_names_its_tool_and_what_it_touches() {
    let asked = ask(&json!({ "command": "npm test" }), "Bash");
    assert_eq!(
        asked.kind,
        AskKind::Permission {
            tool: "Bash".to_owned(),
            summary: "npm test".to_owned(),
            title: Some("Run the tests".to_owned()),
            edit: None,
        }
    );
    let interrupt = json!({ "type": "control_request", "request_id": "1", "request": { "subtype": "interrupt" } });
    assert_eq!(routed(&interrupt), Some(Routed::Control));
}

#[test]
fn a_permission_without_a_title_reads_its_description() {
    let request = json!({
        "type": "control_request",
        "request_id": "req_8",
        "request": {
            "subtype": "can_use_tool",
            "tool_name": "Write",
            "input": { "file_path": "/tmp/a" },
            "description": "Write a file",
        },
    });
    let Some(Routed::Ask(asked)) = routed(&request) else {
        unreachable!("a can_use_tool request");
    };
    assert!(matches!(
        asked.kind,
        AskKind::Permission { title: Some(title), .. } if title == "Write a file"
    ));
}

#[test]
fn allowing_echoes_the_input_and_denying_says_why() {
    let input = json!({ "command": "npm test" });
    let asked = ask(&input, "Bash");
    assert_eq!(
        parsed(&answer_line(&asked, &AskAnswer::Allow)),
        json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": "req_7",
                "response": { "behavior": "allow", "updatedInput": input },
            },
        })
    );
    let denied = answer_line(
        &asked,
        &AskAnswer::Deny {
            message: "not now".to_owned(),
        },
    );
    assert_eq!(
        parsed(&denied),
        json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": "req_7",
                "response": { "behavior": "deny", "message": "not now" },
            },
        })
    );
}

#[test]
fn questions_are_answered_with_the_picked_labels() {
    let input = json!({ "questions": [{
        "question": "Which database?",
        "header": "Database",
        "options": [{ "label": "Postgres", "description": "Relational" }, { "label": "SQLite" }],
        "multiSelect": false,
    }]});
    let asked = ask(&input, "AskUserQuestion");
    let asked_questions = questions(&asked);
    assert_eq!(asked_questions[0].text, "Which database?");
    assert_eq!(asked_questions[0].header.as_deref(), Some("Database"));
    assert_eq!(asked_questions[0].options[1].label, "SQLite");
    assert!(!asked_questions[0].multi_select);
    let answered = parsed(&answer_line(
        &asked,
        &AskAnswer::Answers(vec![PickedAnswer {
            question: "Which database?".to_owned(),
            labels: vec!["SQLite".to_owned()],
        }]),
    ));
    let updated = &answered["response"]["response"]["updatedInput"];
    assert_eq!(answered["response"]["response"]["behavior"], "allow");
    assert_eq!(updated["answers"], json!({ "Which database?": "SQLite" }));
    assert_eq!(updated["questions"], input["questions"]);
}

#[test]
fn several_labels_of_a_multi_select_question_are_joined() {
    let input = json!({ "questions": [{
        "question": "Which checks?",
        "options": [{ "label": "Lint" }, { "label": "Test" }, { "label": "Build" }],
        "multiSelect": true,
    }]});
    let asked = ask(&input, "AskUserQuestion");
    let asked_questions = questions(&asked);
    assert!(asked_questions[0].multi_select);
    assert_eq!(asked_questions[0].header, None);
    assert_eq!(asked_questions[0].options.len(), 3);
    let answered = parsed(&answer_line(
        &asked,
        &AskAnswer::Answers(vec![PickedAnswer {
            question: "Which checks?".to_owned(),
            labels: vec!["Lint".to_owned(), "Build".to_owned()],
        }]),
    ));
    assert_eq!(
        answered["response"]["response"]["updatedInput"]["answers"],
        json!({ "Which checks?": "Lint, Build" })
    );
}

#[test]
fn the_initialize_answer_lists_models_commands_and_the_plan() {
    let answer = json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": INIT_ID,
            "response": {
                "models": [{
                    "value": "opus",
                    "displayName": "Opus",
                    "description": "Opus 5.5",
                    "supportedEffortLevels": ["low", "turbo", "xhigh"],
                    "supportsThinkingOff": false,
                }, {
                    "value": "haiku",
                    "supportsThinkingOff": true,
                }],
                "commands": [{ "name": "review", "description": "Review a diff" }, { "name": "init" }],
                "account": { "subscriptionType": "Claude Max" },
            },
        },
    });
    let Some(Routed::Ready(found)) = routed(&answer) else {
        unreachable!("the initialize answer");
    };
    assert_eq!(found.models[0].value, "opus");
    assert_eq!(found.models[0].name, "Opus");
    assert_eq!(
        found.models[0].efforts,
        [Effort::Low, Effort::ExtraHigh],
        "an unknown level is skipped"
    );
    assert!(!found.models[0].thinking_off);
    assert_eq!(found.models[1].name, "haiku", "named by its value");
    assert_eq!(found.models[1].efforts, []);
    assert!(found.models[1].thinking_off);
    assert_eq!(found.commands[0].name, "review");
    assert_eq!(found.commands[1].description, "");
    assert_eq!(found.subscription.as_deref(), Some("Claude Max"));
    let mut other = answer;
    other["response"]["request_id"] = json!("krad_9");
    assert_eq!(
        routed(&other),
        Some(Routed::Control),
        "another request's answer"
    );
}

#[test]
fn conversation_lines_pass_through() {
    let line = json!({ "type": "assistant", "message": { "content": [] } });
    assert_eq!(routed(&line), Some(Routed::Conversation(line)));
    let untyped = json!({ "text": "hi" });
    assert_eq!(routed(&untyped), Some(Routed::Conversation(untyped)));
    assert_eq!(route("not json", INIT_ID), None);
}

#[test]
fn control_requests_carry_their_subtype() {
    assert_eq!(
        parsed(&initialize_line("req_0")),
        json!({ "type": "control_request", "request_id": "req_0", "request": { "subtype": "initialize" } })
    );
    assert_eq!(
        parsed(&interrupt_line("req_1")),
        json!({ "type": "control_request", "request_id": "req_1", "request": { "subtype": "interrupt" } })
    );
    let Some(model) = ModelId::parse("sonnet") else {
        unreachable!("a valid model id");
    };
    assert_eq!(
        parsed(&set_model_line("req_2", &model)),
        json!({
            "type": "control_request",
            "request_id": "req_2",
            "request": { "subtype": "set_model", "model": "sonnet" },
        })
    );
    assert_eq!(
        parsed(&set_permission_mode_line("req_4", PermissionMode::Manual)),
        json!({
            "type": "control_request",
            "request_id": "req_4",
            "request": { "subtype": "set_permission_mode", "mode": "manual" },
        })
    );
    assert!(interrupt_line("req_3").ends_with('\n'));
}

#[test]
fn a_malformed_model_is_skipped_and_the_rest_of_the_catalog_kept() {
    let answer = json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": INIT_ID,
            "response": {
                "models": [{ "value": 7 }, { "value": "sonnet" }],
                "commands": [{ "name": "review" }, { "description": "no name" }],
            },
        },
    });
    let Some(Routed::Ready(found)) = routed(&answer) else {
        unreachable!("the initialize answer");
    };
    let values: Vec<&str> = found
        .models
        .iter()
        .map(|offer| offer.value.as_str())
        .collect();
    assert_eq!(values, ["sonnet"]);
    assert_eq!(found.commands.len(), 1);
}

#[test]
fn a_malformed_question_or_option_is_skipped_and_the_others_kept() {
    let input = json!({ "questions": [
        { "header": "no question text" },
        {
            "question": "Which database?",
            "options": [{ "label": "Postgres" }, { "label": 3 }, { "label": "SQLite" }],
        },
    ]});
    let asked = ask(&input, "AskUserQuestion");
    let asked_questions = questions(&asked);
    assert_eq!(asked_questions.len(), 1);
    let labels: Vec<&str> = asked_questions[0]
        .options
        .iter()
        .map(|option| option.label.as_str())
        .collect();
    assert_eq!(labels, ["Postgres", "SQLite"]);
}

#[test]
fn allowing_always_sends_back_the_harness_suggestions() {
    let rule = json!({
        "type": "addRules",
        "rules": [{ "toolName": "Bash", "ruleContent": "npm test" }],
        "behavior": "allow",
        "destination": "localSettings",
    });
    let asked = suggested_ask(&json!({ "command": "npm test" }), "Bash", &json!([rule]));
    assert!(asked.can_allow_always());
    let answered = parsed(&answer_line(&asked, &AskAnswer::AllowAlways));
    assert_eq!(
        answered["response"]["response"],
        json!({
            "behavior": "allow",
            "updatedInput": { "command": "npm test" },
            "updatedPermissions": [rule],
        })
    );
    let once = parsed(&answer_line(&asked, &AskAnswer::Allow));
    assert_eq!(once["response"]["response"].get("updatedPermissions"), None);
    assert!(!ask(&json!({ "command": "ls" }), "Bash").can_allow_always());
}

#[test]
fn a_plan_is_approved_with_the_mode_to_go_on_in() {
    let input = json!({ "plan": "# Plan\n- Add notes.txt", "planFilePath": "/plans/p.md" });
    let asked = ask(&input, "ExitPlanMode");
    assert_eq!(
        asked.kind,
        AskKind::Plan {
            plan: "# Plan\n- Add notes.txt".to_owned()
        }
    );
    let answered = parsed(&answer_line(
        &asked,
        &AskAnswer::ApprovePlan {
            mode: PermissionMode::AcceptEdits,
        },
    ));
    assert_eq!(
        answered["response"],
        json!({
            "subtype": "success",
            "request_id": "req_7",
            "response": {
                "behavior": "allow",
                "updatedInput": input,
                "updatedPermissions": [
                    { "type": "setMode", "mode": "acceptEdits", "destination": "session" },
                ],
            },
        })
    );
}

#[test]
fn a_permission_to_edit_a_file_carries_the_change() {
    let input = json!({ "file_path": "notes.txt", "content": "ok\n" });
    let AskKind::Permission {
        edit: Some(edit), ..
    } = ask(&input, "Write").kind
    else {
        unreachable!("a permission with its change");
    };
    assert_eq!(edit.path, "notes.txt");
    assert_ne!(edit.lines.len(), 0);
}
