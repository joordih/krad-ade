use serde_json::{json, Value};

pub(super) const VERSIONING: &str = "protocol_version is the major version. Within it every change is additive: new fields carry a default, new commands, events, messages and codes read as `unknown` on an older peer, which answers a command it does not know with Refused(unsupported) instead of closing. A breaking change raises the major version, and the runtime refuses a Hello of another major version.";

pub(super) const TRANSPORT: &str = "One byte stream per client (a socket to a cloud runner, or nothing at all in process, where the client calls the runtime handle with the same types). Every frame is length-prefixed; a message frame holds one JSON message, a terminal frame holds raw terminal bytes.";

pub(super) const HANDSHAKE: &str = "The client sends Hello with its protocol version and build; the runtime answers Welcome with its own protocol version, build and run, or Refused(unsupported) when the major versions differ. The run names this start of the runtime: a client whose remembered run differs subscribes without since.";

pub(super) const RESUME: &str = "Subscribe without since starts from a Snapshot followed by every later Event. Subscribe with since gets exactly the Events after that Seq when the runtime still keeps them in its ring of recent events, otherwise a Snapshot followed by the later Events. A subscriber that falls behind also gets a fresh Snapshot. Events folded into a Snapshot are never sent again.";

pub(super) const IDEMPOTENCY: &str = "Answers name their object by a stable id (an ask, a plan, a variant run, a session's hunks). The first answer wins; a repeated one gets Refused(already_answered).";

pub(super) const PATHS: &str = "Paths in commands and events are paths on the environment's host, written with forward slashes; never paths of the client's machine.";

const ENVIRONMENT: &str = "main";

fn chat() -> Value {
    json!({ "environment": ENVIRONMENT, "terminal": 4 })
}

pub(super) fn messages() -> Vec<(&'static str, Value)> {
    vec![
        (
            "hello",
            json!({ "type": "hello", "protocol": 1, "build": "0.3.3" }),
        ),
        (
            "welcome",
            json!({ "type": "welcome", "protocol": 1, "build": "0.3.3", "run": 7_340_116_812_u64 }),
        ),
        ("subscribe", json!({ "type": "subscribe" })),
        ("resume", json!({ "type": "subscribe", "since": 41 })),
        (
            "command",
            json!({ "type": "command", "id": 12, "command": { "area": "runtime", "verb": "ping" } }),
        ),
        (
            "accepted",
            json!({ "type": "reply", "id": 12, "reply": { "outcome": "accepted" } }),
        ),
        (
            "refused",
            json!({ "type": "reply", "id": 13, "reply": { "outcome": "refused", "code": "already_answered", "message": "this ask was already answered" } }),
        ),
        (
            "event",
            json!({ "type": "feed", "feed": "event", "seq": 42, "kind": { "type": "pong", "request": 12 } }),
        ),
        (
            "snapshot",
            json!({ "type": "feed", "feed": "snapshot", "scope": "runtime", "seq": 41 }),
        ),
    ]
}

pub(super) fn commands() -> Vec<(&'static str, Value)> {
    let mut commands = state_commands();
    commands.extend(conversation_commands());
    commands.extend(workspace_commands());
    commands
}

fn state_commands() -> Vec<(&'static str, Value)> {
    vec![
        ("runtime_ping", json!({ "area": "runtime", "verb": "ping" })),
        (
            "bus_answer_ask",
            json!({ "area": "bus", "verb": "answer_ask", "ask": 3, "decision": "Pass" }),
        ),
        (
            "events_watch_code_hosts",
            json!({ "area": "events", "verb": "watch_code_hosts", "watch": "on" }),
        ),
        (
            "attention_dismiss",
            json!({ "area": "attention", "verb": "dismiss", "target": chat() }),
        ),
        (
            "usage_refresh",
            json!({ "area": "usage", "verb": "refresh" }),
        ),
        (
            "models_keep",
            json!({ "area": "models", "verb": "keep", "host": { "kind": "local" }, "harness": "ClaudeCode", "models": [] }),
        ),
        (
            "environments_order",
            json!({ "area": "environments", "verb": "order", "order": [ENVIRONMENT, "staging"] }),
        ),
        (
            "session_rename",
            json!({ "area": "session", "environment": ENVIRONMENT, "edit": { "verb": "rename", "session": 2, "name": "Fix the webhook retries" } }),
        ),
        (
            "session_close",
            json!({ "area": "session", "environment": ENVIRONMENT, "edit": { "verb": "close", "session": 2 } }),
        ),
        (
            "group_remove",
            json!({ "area": "group", "environment": ENVIRONMENT, "edit": { "verb": "remove", "group": 1 } }),
        ),
        (
            "node_close",
            json!({ "area": "node", "environment": ENVIRONMENT, "edit": { "verb": "close", "terminal": 4 } }),
        ),
        (
            "terminal_resize",
            json!({ "area": "terminal", "verb": "resize", "terminal": chat(), "size": { "cols": 120, "rows": 40, "cell_width_px": 8, "cell_height_px": 16 } }),
        ),
        (
            "terminal_scroll",
            json!({ "area": "terminal", "verb": "scroll", "terminal": chat(), "scroll": { "to": "bottom" } }),
        ),
        (
            "host_reconnect",
            json!({ "area": "host", "verb": "reconnect", "environment": ENVIRONMENT }),
        ),
    ]
}

fn conversation_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "chat_attach_image",
            json!({ "area": "chat", "verb": "attach_image", "chat": chat(), "image": { "format": "png", "bytes": [137, 80, 78, 71] } }),
        ),
        (
            "chat_send",
            json!({ "area": "chat", "verb": "send", "chat": chat(), "prompt": { "text": "Run the tests", "mentions": [] } }),
        ),
        (
            "chat_interrupt",
            json!({ "area": "chat", "verb": "interrupt", "chat": chat() }),
        ),
        (
            "chat_approve_plan",
            json!({ "area": "chat", "verb": "approve_plan", "chat": chat(), "ask": 9, "mode": "acceptEdits" }),
        ),
        (
            "agent_start_worker",
            json!({ "area": "agent", "environment": ENVIRONMENT, "lead": 2, "act": { "verb": "start_worker", "agent": "ClaudeCode", "task": "Find why the rotation races" } }),
        ),
        (
            "agent_close_worker",
            json!({ "area": "agent", "environment": ENVIRONMENT, "lead": 2, "act": { "verb": "close_worker", "worker": 7, "when_working": "cancel_turn", "worktree": "remove" } }),
        ),
    ]
}

fn workspace_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "gateway_check",
            json!({ "area": "gateway", "verb": "check", "url": null, "key": null }),
        ),
        (
            "settings_save",
            json!({ "area": "settings", "verb": "save", "settings": {} }),
        ),
        (
            "git_stage",
            json!({ "area": "git", "environment": ENVIRONMENT, "group": 1, "act": { "verb": "stage", "paths": ["src/main.rs"] } }),
        ),
        (
            "git_commit",
            json!({ "area": "git", "environment": ENVIRONMENT, "group": 1, "act": { "verb": "commit", "message": "Retry webhooks with backoff", "mode": "new", "push": false } }),
        ),
        (
            "changes_accept",
            json!({ "area": "changes", "environment": ENVIRONMENT, "session": 2, "act": { "verb": "accept", "hunks": [9_187_331_004_u64, 42] } }),
        ),
        (
            "kraddie_run_kraddie",
            json!({ "area": "kraddie", "verb": "run_kraddie", "kraddie": "night-owl", "prompt": "Check the nightly build" }),
        ),
        (
            "kraddie_share_memory",
            json!({ "area": "kraddie", "verb": "share_memory", "reader": "night-owl", "from": "early-bird", "switch": "on" }),
        ),
        (
            "variant_pick",
            json!({ "area": "variant", "environment": ENVIRONMENT, "act": { "verb": "pick", "run": 2, "winner": 1, "outcome": "keep_worktree", "removal": "forced" } }),
        ),
        (
            "review_probe",
            json!({ "area": "review", "environment": ENVIRONMENT, "root": "/home/me/code/api", "act": { "verb": "probe" } }),
        ),
        (
            "code_host_check",
            json!({ "area": "code_host", "verb": "check", "host": "Github" }),
        ),
        (
            "runner_reconnect",
            json!({ "area": "runner", "verb": "reconnect", "runner": 1 }),
        ),
        (
            "cloud_send",
            json!({ "area": "cloud", "verb": "send", "environment": ENVIRONMENT, "session": 2, "runner": 1 }),
        ),
    ]
}

pub(super) fn events() -> Vec<(&'static str, Value)> {
    vec![
        (
            "pong",
            json!({ "feed": "event", "seq": 42, "kind": { "type": "pong", "request": 12 } }),
        ),
        (
            "chat_message_appended",
            json!({ "feed": "event", "seq": 43, "kind": { "type": "chat_message_appended", "chat": chat(), "index": 5, "text": "All tests pass." } }),
        ),
        (
            "chat_ask_settled",
            json!({ "feed": "event", "seq": 44, "kind": { "type": "chat_ask_settled", "chat": chat(), "ask": 9 } }),
        ),
        (
            "terminal_changed",
            json!({ "feed": "event", "seq": 45, "kind": { "type": "terminal_changed", "terminal": chat() } }),
        ),
        (
            "attention_left",
            json!({ "feed": "event", "seq": 46, "kind": { "type": "attention_left", "target": chat() } }),
        ),
        (
            "environment_changed",
            json!({ "feed": "event", "seq": 47, "kind": { "type": "environment_changed", "environment": ENVIRONMENT, "changes": [], "answers": 12 } }),
        ),
        (
            "session_changes_read",
            json!({ "feed": "event", "seq": 48, "kind": { "type": "session_changes_read", "request": 14, "environment": ENVIRONMENT, "session": 2, "read": { "read": "no_repository" } } }),
        ),
        (
            "problem",
            json!({ "feed": "event", "seq": 49, "kind": { "type": "problem", "scope": { "on": "chat", "chat": chat() }, "code": "unavailable", "message": "Could not save the pasted image" } }),
        ),
    ]
}

pub(super) fn forward_compatible() -> Vec<(Value, Value)> {
    vec![
        (
            json!({ "type": "pair", "code": "4821" }),
            json!({ "type": "unknown" }),
        ),
        (
            json!({ "type": "command", "id": 20, "command": { "area": "workflow", "verb": "run" } }),
            json!({ "type": "command", "id": 20, "command": { "area": "unknown" } }),
        ),
        (
            json!({ "type": "command", "id": 21, "command": { "area": "chat", "verb": "translate", "chat": chat() } }),
            json!({ "type": "command", "id": 21, "command": { "area": "chat", "verb": "unknown" } }),
        ),
        (
            json!({ "type": "feed", "feed": "event", "seq": 50, "kind": { "type": "session_opened", "session": 3 } }),
            json!({ "type": "feed", "feed": "event", "seq": 50, "kind": { "type": "unknown" } }),
        ),
        (
            json!({ "type": "reply", "id": 22, "reply": { "outcome": "refused", "code": "rate_limited", "message": "slow down" } }),
            json!({ "type": "reply", "id": 22, "reply": { "outcome": "refused", "code": "unknown", "message": "slow down" } }),
        ),
        (
            json!({ "type": "hello", "protocol": 1, "build": "0.9.0", "features": ["pairing"] }),
            json!({ "type": "hello", "protocol": 1, "build": "0.9.0" }),
        ),
    ]
}

pub(super) fn session() -> Vec<(&'static str, Value)> {
    vec![
        (
            "client",
            json!({ "type": "hello", "protocol": 1, "build": "0.3.3" }),
        ),
        (
            "runtime",
            json!({ "type": "welcome", "protocol": 1, "build": "0.3.3", "run": 7_340_116_812_u64 }),
        ),
        ("client", json!({ "type": "subscribe", "since": 41 })),
        (
            "runtime",
            json!({ "type": "feed", "feed": "event", "seq": 42, "kind": { "type": "chat_message_appended", "chat": chat(), "index": 5, "text": "All tests pass." } }),
        ),
        (
            "client",
            json!({ "type": "command", "id": 13, "command": { "area": "chat", "verb": "answer_ask", "chat": chat(), "ask": 9, "answer": "allow" } }),
        ),
        (
            "runtime",
            json!({ "type": "reply", "id": 13, "reply": { "outcome": "accepted" } }),
        ),
        (
            "runtime",
            json!({ "type": "feed", "feed": "event", "seq": 43, "kind": { "type": "chat_ask_settled", "chat": chat(), "ask": 9 } }),
        ),
        (
            "client",
            json!({ "type": "command", "id": 14, "command": { "area": "chat", "verb": "answer_ask", "chat": chat(), "ask": 9, "answer": "allow_always" } }),
        ),
        (
            "runtime",
            json!({ "type": "reply", "id": 14, "reply": { "outcome": "refused", "code": "already_answered", "message": "this ask was already answered" } }),
        ),
    ]
}
