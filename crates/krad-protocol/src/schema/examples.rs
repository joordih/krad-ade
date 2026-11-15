use serde_json::{json, Value};

mod editor;
mod files;

pub(super) const VERSIONING: &str = "protocol_version is the major version. Within it every change is additive: new fields carry a default, new commands, events, messages and codes read as `unknown` on an older peer, which answers a command it does not know with Refused(unsupported) instead of closing. A breaking change raises the major version, and the runtime refuses a Hello of another major version.";

pub(super) const TRANSPORT: &str = "One byte stream per client (a socket to a cloud runner, or nothing at all in process, where the client calls the runtime handle with the same types). Every frame is length-prefixed; a message frame holds one JSON message, a terminal frame holds raw terminal bytes.";

pub(super) const HANDSHAKE: &str = "The client sends Hello with its protocol version and build; the runtime answers Welcome with its own protocol version, build and run, or Refused(unsupported) when the major versions differ. A runner's runtime also asks for a pairing in Hello and names in Welcome the permissions it grants (read, drive, answer, admin) and the first request id of the connection's own range, which the client counts up from; a Hello with no pairing may only redeem a pairing code, and pairing commands are answered only to the connection that sent them. The run names this start of the runtime: a client whose remembered run differs subscribes without since.";

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
        (
            "paired_hello",
            json!({ "type": "hello", "protocol": 1, "build": "0.3.3", "pairing": { "install": "0123456789abcdef", "token": "00112233445566778899aabbccddeeff" } }),
        ),
        (
            "paired_welcome",
            json!({ "type": "welcome", "protocol": 1, "build": "0.3.3", "run": 7_340_116_812_u64, "permissions": ["read", "drive", "answer"], "first_request": 4_294_967_296_u64 }),
        ),
        (
            "ask_snapshot",
            json!({ "type": "ask_snapshot", "id": 14, "scope": "runtime" }),
        ),
        (
            "snapshot_answer",
            json!({ "type": "snapshot", "id": 14, "snapshot": { "scope": "runtime", "seq": 41 } }),
        ),
        (
            "create_pairing_code",
            json!({ "type": "pairing", "id": 15, "command": { "verb": "create_pairing_code", "permissions": ["read", "answer"] } }),
        ),
        (
            "pairing_code",
            json!({ "type": "paired", "id": 15, "answer": { "kind": "code", "code": "K7QF3MZP", "expires": "2026-11-14T10:10:00Z" } }),
        ),
        (
            "redeem_pairing_code",
            json!({ "type": "pairing", "id": 1, "command": { "verb": "redeem_pairing_code", "code": "K7QF3MZP", "install": "fedcba9876543210", "device": "phone" } }),
        ),
        (
            "revoke_pairing",
            json!({ "type": "pairing", "id": 16, "command": { "verb": "revoke_pairing", "install": "fedcba9876543210" } }),
        ),
        (
            "refused_message",
            json!({ "type": "refused", "code": "unsupported", "message": "this runtime does not know that message" }),
        ),
    ]
}

pub(super) fn commands() -> Vec<(&'static str, Value)> {
    let mut commands = state_commands();
    commands.extend(conversation_commands());
    commands.extend(workspace_commands());
    commands.extend(repo_commands());
    commands.extend(files::file_commands());
    commands.extend(editor::editor_commands());
    commands
}

fn state_commands() -> Vec<(&'static str, Value)> {
    vec![
        ("runtime_ping", json!({ "area": "runtime", "verb": "ping" })),
        (
            "runtime_settle",
            json!({ "area": "runtime", "verb": "settle" }),
        ),
        (
            "bus_observe_agent",
            json!({ "area": "bus", "verb": "observe_agent", "agent": chat(), "hint": "permission_pending" }),
        ),
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
            "usage_place",
            json!({ "area": "usage", "verb": "place", "spot": { "environment": ENVIRONMENT } }),
        ),
        (
            "models_list",
            json!({ "area": "models", "verb": "list", "spot": { "environment": ENVIRONMENT }, "harness": "Codex" }),
        ),
        (
            "health_check",
            json!({ "area": "health", "verb": "check", "spot": {} }),
        ),
        (
            "opencode_start",
            json!({ "area": "opencode", "verb": "start", "spot": { "environment": ENVIRONMENT, "root": "/home/dev/krad" } }),
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
        (
            "host_close_all",
            json!({ "area": "host", "verb": "close_all" }),
        ),
        (
            "host_ssh_hosts",
            json!({ "area": "host", "verb": "ssh_hosts" }),
        ),
        (
            "host_cancel_prompt",
            json!({ "area": "host", "verb": "answer_prompt", "prompt": 2, "answer": null }),
        ),
        (
            "host_install_acp_package",
            json!({ "area": "host", "verb": "install_acp_package", "spot": { "environment": ENVIRONMENT }, "agent": "codex-acp" }),
        ),
    ]
}

fn conversation_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "chat_open",
            json!({ "area": "chat", "verb": "open", "chat": chat(), "start": { "harness": "ClaudeCode", "model": null, "effort": null, "permission_mode": null, "mcp_servers": null, "acp": { "mode": null, "options": [] }, "session": { "kind": "resume", "from": "4f2c0b7c-a91e-4c1d-9e0f-2b7a1d3c5e8f" }, "messages": [], "notices": [] } }),
        ),
        (
            "chat_page",
            json!({ "area": "chat", "verb": "page", "chat": chat(), "before": 64, "count": 64 }),
        ),
        (
            "chat_mentions",
            json!({ "area": "chat", "verb": "mentions", "environment": ENVIRONMENT, "root": "/home/dev/app", "query": "main", "limit": 20 }),
        ),
        (
            "chat_attach_image",
            json!({ "area": "chat", "verb": "attach_image", "chat": chat(), "image": { "format": "png", "bytes": [137, 80, 78, 71] } }),
        ),
        (
            "chat_forget_transcript",
            json!({ "area": "chat", "verb": "forget_transcript", "chat": chat() }),
        ),
        (
            "chat_read_image",
            json!({ "area": "chat", "verb": "read_image", "image": { "path": "/home/dev/.config/krad-ade/chat-images/6d61696e/3/1.png", "format": "png" } }),
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
            "cloud_create",
            json!({ "area": "cloud", "verb": "create", "environment": ENVIRONMENT, "group": 1, "runner": 1, "reach": "runtime", "first": { "harness": "Codex", "prompt": { "text": "Fix the flaky login test", "mentions": [] } } }),
        ),
        (
            "cloud_send",
            json!({ "area": "cloud", "verb": "send", "environment": ENVIRONMENT, "session": 2, "runner": 1, "prompt": { "chat": 4, "prompt": { "text": "Fix the flaky login test", "mentions": [] } } }),
        ),
        (
            "cloud_host",
            json!({ "area": "cloud", "verb": "host", "session": { "name": "Cloud · api", "workspace": { "remote_url": "git@example.com:me/api.git", "upstream_branch": "main", "session_slug": "api-1a2b" }, "starts": [{ "harness": "ClaudeCode", "conversation": { "conversation": "fresh", "resume": "0d6f2b8e-4c1a-4f7e-9a51-2b6c3d4e5f60" } }] } }),
        ),
    ]
}

fn repo_commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            "repo_commit_log",
            json!({ "area": "repo", "environment": ENVIRONMENT, "ask": { "verb": "commit_log", "repo": "/home/dev/app", "revisions": ["HEAD"], "limit": 400 } }),
        ),
        (
            "repo_diff",
            json!({ "area": "repo", "environment": ENVIRONMENT, "ask": { "verb": "diff", "repo": "/home/dev/app", "of": { "kind": "working", "change": { "path": "src/main.rs", "kind": "modified", "origin": null } } } }),
        ),
        (
            "repo_remove_worktree",
            json!({ "area": "repo", "environment": ENVIRONMENT, "ask": { "verb": "remove_worktree", "repo": "/home/dev/app", "path": "/home/dev/app-fix", "removal": "discard" } }),
        ),
        (
            "repo_replace_text",
            json!({ "area": "repo", "environment": ENVIRONMENT, "ask": { "verb": "replace_text", "path": "/home/dev/app/src/main.rs", "expected": "fn main() {}\n", "text": "fn main() {\n}\n" } }),
        ),
    ]
}

pub(super) fn events() -> Vec<(&'static str, Value)> {
    let mut events = vec![
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
        (
            "answered",
            json!({ "feed": "event", "seq": 50, "kind": { "type": "answered", "request": 15, "answer": { "kind": "mentions", "entries": [{ "path": "src/main.rs", "kind": "file" }] } } }),
        ),
        (
            "answered_failed",
            json!({ "feed": "event", "seq": 51, "kind": { "type": "answered", "request": 16, "answer": { "kind": "failed", "code": "failed", "message": "`git log` failed: not a repository" } } }),
        ),
        (
            "answered_text_replaced",
            json!({ "feed": "event", "seq": 52, "kind": { "type": "answered", "request": 17, "answer": { "kind": "text_replaced", "replaced": false } } }),
        ),
        (
            "runner_signing_in",
            json!({ "feed": "event", "seq": 53, "kind": { "type": "runner_changed", "runner": 1, "state": { "state": "signing_in" } } }),
        ),
        (
            "runner_prompted",
            json!({ "feed": "event", "seq": 55, "kind": { "type": "host_prompted", "prompt": 2, "asker": { "asker": "runner", "runner": 1 }, "kind": "secret", "text": "me@gpu's password: " } }),
        ),
    ];
    events.extend(files::file_events());
    events.extend(editor::editor_events());
    events
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
            json!({ "type": "feed", "feed": "event", "seq": 54, "kind": { "type": "runner_changed", "runner": 1, "state": { "state": "migrating" } } }),
            json!({ "type": "feed", "feed": "event", "seq": 54, "kind": { "type": "runner_changed", "runner": 1, "state": { "state": "unknown" } } }),
        ),
        (
            json!({ "type": "feed", "feed": "event", "seq": 56, "kind": { "type": "host_prompted", "prompt": 3, "asker": { "asker": "extension", "extension": "vm" }, "kind": "secret", "text": "Passphrase: " } }),
            json!({ "type": "feed", "feed": "event", "seq": 56, "kind": { "type": "host_prompted", "prompt": 3, "asker": { "asker": "unknown" }, "kind": "secret", "text": "Passphrase: " } }),
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
