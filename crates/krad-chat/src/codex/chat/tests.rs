use super::*;
use serde_json::json;

#[test]
fn permission_modes_become_approval_policies_and_sandboxes() {
    assert_eq!(
        policy(PermissionMode::Manual, true),
        Policy {
            approval: "untrusted",
            sandbox: Sandbox::WorkspaceWrite {
                network_access: true
            },
            mode: Mode::Default,
        }
    );
    assert_eq!(policy(PermissionMode::Plan, false).mode, Mode::Plan);
    assert_eq!(
        policy(PermissionMode::BypassPermissions, false).sandbox,
        Sandbox::DangerFullAccess
    );
    assert_eq!(
        serde_json::to_value(Sandbox::WorkspaceWrite {
            network_access: false
        })
        .unwrap_or_default(),
        json!({"type": "workspaceWrite", "networkAccess": false})
    );
}

#[test]
fn a_skill_call_becomes_a_skill_input_followed_by_its_arguments() {
    let skills = HashMap::from([("release".to_owned(), "/s/release/SKILL.md".to_owned())]);
    let called = serde_json::to_value(inputs("/release v2", &skills, &[])).unwrap_or_default();
    assert_eq!(
        called,
        json!([
            {"type": "text", "text": "$release v2", "text_elements": []},
            {"type": "skill", "name": "release", "path": "/s/release/SKILL.md"}
        ])
    );
    let plain = serde_json::to_value(inputs("/unknown thing", &skills, &[])).unwrap_or_default();
    assert_eq!(
        plain,
        json!([{"type": "text", "text": "/unknown thing", "text_elements": []}])
    );
}

#[test]
fn an_image_goes_as_a_local_image_the_app_server_reads() {
    let skills = HashMap::new();
    let images = ["/shots/1.png".to_owned()];
    assert_eq!(
        serde_json::to_value(inputs("What is it?", &skills, &images)).unwrap_or_default(),
        json!([
            {"type": "text", "text": "What is it?", "text_elements": []},
            {"type": "localImage", "path": "/shots/1.png"}
        ])
    );
    assert_eq!(
        serde_json::to_value(inputs("", &skills, &images)).unwrap_or_default(),
        json!([{"type": "localImage", "path": "/shots/1.png"}])
    );
}

#[test]
fn a_missing_thread_starts_a_new_conversation() {
    assert!(missing_thread(&CodexError::Refused(
        "thread not found: 01a1".to_owned()
    )));
    assert!(!missing_thread(&CodexError::Gone));
}
