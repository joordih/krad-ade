use serde::Deserialize;
use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::PoisonError;

use super::{Pushed, RpcId, Waiters};
use crate::cloud::LineOrigin;

#[derive(Debug, Deserialize)]
struct RpcFailure {
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Incoming {
    Request {
        id: RpcId,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Notification {
        method: String,
        #[serde(default)]
        params: Value,
    },
    Failure {
        id: RpcId,
        error: RpcFailure,
    },
    Success {
        id: RpcId,
        #[serde(default)]
        result: Value,
    },
}

pub(super) fn route(line: &str, origin: LineOrigin, waiters: &Waiters, pushed: &Sender<Pushed>) {
    let Ok(incoming) = serde_json::from_str::<Incoming>(line.trim()) else {
        return;
    };
    if origin.is_replayed() && !matches!(incoming, Incoming::Request { .. }) {
        return;
    }
    let settle = |id: RpcId, answer: Result<Value, String>| {
        let RpcId::Number(id) = id else {
            return;
        };
        let Ok(id) = u64::try_from(id) else {
            return;
        };
        let waiter = waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if let Some(waiter) = waiter {
            waiter.send(answer).unwrap_or_default();
        }
    };
    match incoming {
        Incoming::Request { id, method, params } => {
            pushed
                .send(Pushed::Request { id, method, params })
                .unwrap_or_default();
        }
        Incoming::Notification { method, params } => {
            pushed
                .send(Pushed::Notification { method, params })
                .unwrap_or_default();
        }
        Incoming::Failure { id, error } => settle(id, Err(error.message)),
        Incoming::Success { id, result } => settle(id, Ok(result)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::mpsc::channel;
    use std::sync::Mutex;

    #[test]
    fn requests_notifications_and_answers_go_where_they_belong() {
        let waiters: Waiters = Mutex::new(HashMap::new());
        let (answered, answer) = channel();
        waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(3, answered);
        let (pushed, received) = channel();
        route(
            r#"{"id":3,"result":{"ok":true}}"#,
            LineOrigin::Live,
            &waiters,
            &pushed,
        );
        route(
            r#"{"method":"item/commandExecution/requestApproval","id":0,"params":{"command":"ls"}}"#,
            LineOrigin::Live,
            &waiters,
            &pushed,
        );
        route(
            r#"{"method":"turn/started","params":{"threadId":"t"}}"#,
            LineOrigin::Live,
            &waiters,
            &pushed,
        );
        route("not json", LineOrigin::Live, &waiters, &pushed);
        assert_eq!(
            answer.try_recv().ok(),
            Some(Ok(serde_json::json!({"ok": true})))
        );
        let pushed: Vec<String> = received
            .try_iter()
            .map(|message| match message {
                Pushed::Request { id, method, .. } => format!("{method} {id:?}"),
                Pushed::Notification { method, .. } => method,
                Pushed::Closed => String::new(),
            })
            .collect();
        assert_eq!(
            pushed,
            [
                "item/commandExecution/requestApproval Number(0)",
                "turn/started"
            ]
        );
    }

    #[test]
    fn a_refused_call_carries_the_reason() {
        let waiters: Waiters = Mutex::new(HashMap::new());
        let (answered, answer) = channel();
        waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(1, answered);
        let (pushed, _) = channel();
        route(
            r#"{"id":1,"error":{"code":-32600,"message":"thread not found"}}"#,
            LineOrigin::Live,
            &waiters,
            &pushed,
        );
        assert_eq!(
            answer.try_recv().ok(),
            Some(Err("thread not found".to_owned()))
        );
    }

    #[test]
    fn a_replayed_journal_delivers_only_the_approvals_still_asked() {
        let waiters: Waiters = Mutex::new(HashMap::new());
        let (answered, answer) = channel();
        waiters
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(0, answered);
        let (pushed, received) = channel();
        for line in [
            r#"{"id":0,"result":{"userAgent":"old"}}"#,
            r#"{"method":"turn/started","params":{"threadId":"t"}}"#,
            r#"{"method":"item/commandExecution/requestApproval","id":4,"params":{"command":"ls"}}"#,
        ] {
            route(line, LineOrigin::Replayed, &waiters, &pushed);
        }
        assert!(
            answer.try_recv().is_err(),
            "a replayed answer settles nothing"
        );
        route(
            r#"{"method":"turn/completed","params":{}}"#,
            LineOrigin::Live,
            &waiters,
            &pushed,
        );
        let methods: Vec<String> = received
            .try_iter()
            .map(|message| match message {
                Pushed::Request { method, .. } | Pushed::Notification { method, .. } => method,
                Pushed::Closed => String::new(),
            })
            .collect();
        assert_eq!(
            methods,
            ["item/commandExecution/requestApproval", "turn/completed"]
        );
    }
}
