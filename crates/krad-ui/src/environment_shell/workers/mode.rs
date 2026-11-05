use krad_domain::{AgentPreset, PermissionMode};

pub(in crate::environment_shell) const fn worker_mode(
    lead: Option<PermissionMode>,
    worker: AgentPreset,
) -> Option<PermissionMode> {
    let Some(mode) = lead else {
        return None;
    };
    match (worker, mode) {
        (AgentPreset::Shell, _) => None,
        (AgentPreset::Codex, PermissionMode::DontAsk)
        | (AgentPreset::Opencode, PermissionMode::DontAsk | PermissionMode::BypassPermissions) => {
            Some(PermissionMode::Auto)
        }
        (_, mode) => Some(mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment_shell::ask::OFFERED_MODES;

    #[test]
    fn a_worker_starts_in_the_mode_its_lead_chat_has() {
        for harness in [
            AgentPreset::ClaudeCode,
            AgentPreset::Codex,
            AgentPreset::Opencode,
            AgentPreset::Gateway,
        ] {
            for mode in OFFERED_MODES {
                assert_eq!(worker_mode(Some(mode), harness), Some(mode));
            }
        }
    }

    #[test]
    fn a_worker_without_a_known_lead_mode_keeps_its_own_default() {
        assert_eq!(worker_mode(None, AgentPreset::ClaudeCode), None);
        assert_eq!(
            worker_mode(Some(PermissionMode::Auto), AgentPreset::Shell),
            None
        );
    }

    #[test]
    fn a_mode_the_worker_harness_lacks_falls_back_to_the_one_it_runs_as() {
        let claude = worker_mode(Some(PermissionMode::DontAsk), AgentPreset::ClaudeCode);
        assert_eq!(claude, Some(PermissionMode::DontAsk));
        let codex = worker_mode(Some(PermissionMode::DontAsk), AgentPreset::Codex);
        assert_eq!(codex, Some(PermissionMode::Auto));
        let codex_bypass = worker_mode(Some(PermissionMode::BypassPermissions), AgentPreset::Codex);
        assert_eq!(codex_bypass, Some(PermissionMode::BypassPermissions));
        let opencode = worker_mode(
            Some(PermissionMode::BypassPermissions),
            AgentPreset::Opencode,
        );
        assert_eq!(opencode, Some(PermissionMode::Auto));
    }
}
