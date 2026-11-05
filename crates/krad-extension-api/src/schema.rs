use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde_json::{json, Map, Value};

use crate::manifest::ExtensionManifest;
use crate::protocol::{
    CallTool, CloseView, Confirm, ConnectMachine, ExecuteCommand, Exit, GetSecret, GetSettings,
    Initialize, Initialized, InputBox, ListMachines, Log, Notification, OpenFile, OpenTerminal,
    OpenView, PatchView, Pick, Progress, RenderView, Request, Rows, RunCommand, SendTerminal,
    SetSecret, SettingsChanged, Shutdown, Toast, ViewClosed, ViewEventNotification,
    WorkspaceContext, WorkspaceEventNotification,
};
use crate::version::CURRENT_API;

struct Collector {
    generator: SchemaGenerator,
    requests: Map<String, Value>,
    notifications: Map<String, Value>,
}

impl Collector {
    fn request<R: Request>(&mut self) -> &mut Self {
        let entry = json!({
            "answered_by": R::ANSWERED_BY,
            "params": self.generator.subschema_for::<R::Params>(),
            "result": self.generator.subschema_for::<R::Result>(),
        });
        self.requests.insert(R::METHOD.to_owned(), entry);
        self
    }

    fn notification<N: Notification>(&mut self) -> &mut Self {
        let entry = json!({
            "received_by": N::RECEIVED_BY,
            "params": self.generator.subschema_for::<N::Params>(),
        });
        self.notifications.insert(N::METHOD.to_owned(), entry);
        self
    }
}

#[must_use]
pub fn api_schema() -> Value {
    let mut collector = Collector {
        generator: SchemaSettings::draft2020_12().into_generator(),
        requests: Map::new(),
        notifications: Map::new(),
    };
    collector
        .request::<Initialize>()
        .request::<Shutdown>()
        .request::<ExecuteCommand>()
        .request::<RunCommand>()
        .request::<RenderView>()
        .request::<Rows>()
        .request::<OpenView>()
        .request::<CloseView>()
        .request::<Confirm>()
        .request::<Pick>()
        .request::<InputBox>()
        .request::<OpenTerminal>()
        .request::<WorkspaceContext>()
        .request::<OpenFile>()
        .request::<GetSettings>()
        .request::<GetSecret>()
        .request::<SetSecret>()
        .request::<CallTool>()
        .request::<ListMachines>()
        .request::<ConnectMachine>()
        .notification::<Initialized>()
        .notification::<Exit>()
        .notification::<Log>()
        .notification::<ViewEventNotification>()
        .notification::<ViewClosed>()
        .notification::<PatchView>()
        .notification::<Toast>()
        .notification::<Progress>()
        .notification::<SendTerminal>()
        .notification::<WorkspaceEventNotification>()
        .notification::<SettingsChanged>();
    let manifest = collector.generator.subschema_for::<ExtensionManifest>();
    let definitions = collector.generator.take_definitions(true);
    sorted(json!({
        "api": CURRENT_API,
        "manifest": manifest,
        "requests": collector.requests,
        "notifications": collector.notifications,
        "$defs": definitions,
    }))
}

fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SCHEMA_FILE: &str = "../../sdk/schema/krad-extension-api.json";
    const WRITE_VAR: &str = "KRAD_EXTENSION_SCHEMA";
    const WRITE_VALUE: &str = "write";

    #[test]
    fn the_published_schema_matches_the_types() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_FILE);
        let generated = format!("{}\n", serde_json::to_string_pretty(&api_schema())?);
        if std::env::var(WRITE_VAR).as_deref() == Ok(WRITE_VALUE) {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder)?;
            }
            std::fs::write(&path, &generated)?;
        }
        let published = std::fs::read_to_string(&path).unwrap_or_default();
        let published: Option<Value> = serde_json::from_str(&published).ok();
        assert!(
            published == Some(api_schema()),
            "the schema is stale: run `{WRITE_VAR}={WRITE_VALUE} cargo test -p krad-extension-api`"
        );
        Ok(())
    }

    #[test]
    fn every_method_appears_once_with_its_direction() {
        let schema = api_schema();
        assert_eq!(schema["requests"]["view/render"]["answered_by"], "program");
        assert_eq!(schema["requests"]["window/pick"]["answered_by"], "app");
        assert_eq!(schema["notifications"]["view/patch"]["received_by"], "app");
        assert!(schema["$defs"]["Node"].is_object());
    }
}
