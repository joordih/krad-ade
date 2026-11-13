use krad_chat::{BusVars, ChatBackend, EngineConfig, LiveConfig};
use krad_runtime::ChatLaunch;

use super::DIGEST_MODE;

fn quiet_live(config: &mut LiveConfig) {
    config.system_prompt = None;
    config.bus_vars = BusVars(Vec::new());
    config.permission_mode = Some(DIGEST_MODE);
}

fn quiet_engine(config: &mut EngineConfig) {
    config.system_prompt = None;
    config.bus_vars = BusVars(Vec::new());
    config.permission_mode = Some(DIGEST_MODE);
    config.mcp = Vec::new();
}

pub(super) fn quiet_launch(launch: ChatLaunch) -> Option<ChatLaunch> {
    match launch {
        ChatLaunch::Gateway(mut config) => {
            quiet_engine(&mut config);
            Some(ChatLaunch::Gateway(config))
        }
        ChatLaunch::Opencode(mut config) => {
            quiet_live(&mut config);
            Some(ChatLaunch::Opencode(config))
        }
        ChatLaunch::Backend(backend) => quiet_backend(*backend).map(ChatLaunch::from),
        ChatLaunch::Acp(_) => None,
    }
}

fn quiet_backend(backend: ChatBackend) -> Option<ChatBackend> {
    match backend {
        ChatBackend::Claude(mut config) => {
            quiet_live(&mut config);
            Some(ChatBackend::Claude(config))
        }
        ChatBackend::Codex(mut config) => {
            quiet_live(&mut config);
            Some(ChatBackend::Codex(config))
        }
        ChatBackend::Opencode(server, mut config) => {
            quiet_live(&mut config);
            Some(ChatBackend::Opencode(server, config))
        }
        ChatBackend::Gateway(access, mut config) => {
            quiet_engine(&mut config);
            Some(ChatBackend::Gateway(access, config))
        }
        ChatBackend::Acp(_) => None,
    }
}
