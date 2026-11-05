use krad_domain::{built_in_acp_agents, AgentPreset};
use std::borrow::Cow;

const NO_PROGRAM: &str = "this harness has no program to name it on the bus";
const ACP_BUS_PREFIX: &str = "acp:";

#[must_use]
pub(crate) fn bus_name(preset: AgentPreset) -> Option<Cow<'static, str>> {
    match preset {
        AgentPreset::Acp(id) => Some(Cow::Owned(format!("{ACP_BUS_PREFIX}{id}"))),
        native => native.program().map(Cow::Borrowed),
    }
}

#[must_use]
pub(crate) fn preset_by_bus_name(name: &str) -> Option<AgentPreset> {
    match name.strip_prefix(ACP_BUS_PREFIX) {
        Some(id) => id.parse().ok().map(AgentPreset::Acp),
        None => name.parse().ok(),
    }
}

#[must_use]
pub(crate) fn preset_by_display_name(name: &str) -> Option<AgentPreset> {
    AgentPreset::NATIVE
        .into_iter()
        .find(|preset| preset.display_name().eq_ignore_ascii_case(name))
        .or_else(|| {
            built_in_acp_agents()
                .into_iter()
                .find(|agent| agent.display_name().eq_ignore_ascii_case(name))
                .map(|agent| AgentPreset::Acp(agent.id))
        })
}

pub(crate) mod program {
    use super::{bus_name, preset_by_bus_name, NO_PROGRAM};
    use krad_domain::AgentPreset;
    use serde::{de, ser, Deserialize as _, Deserializer, Serializer};

    const UNKNOWN_BUS_NAME: &str = "unknown harness on the bus";

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn serialize<S: Serializer>(
        preset: &AgentPreset,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let name = bus_name(*preset).ok_or_else(|| <S::Error as ser::Error>::custom(NO_PROGRAM))?;
        serializer.serialize_str(&name)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<AgentPreset, D::Error> {
        let name = String::deserialize(deserializer)?;
        preset_by_bus_name(&name).ok_or_else(|| <D::Error as de::Error>::custom(UNKNOWN_BUS_NAME))
    }
}

pub(crate) mod display_name {
    use super::{bus_name, preset_by_bus_name, preset_by_display_name};
    use krad_domain::AgentPreset;
    use serde::{de, Deserialize as _, Deserializer, Serializer};

    const UNKNOWN_DISPLAY_NAME: &str = "unknown harness name";

    #[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn serialize<S: Serializer>(
        preset: &Option<AgentPreset>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match preset {
            Some(preset @ AgentPreset::Acp(_)) => {
                serializer.serialize_some(&bus_name(*preset).unwrap_or_default())
            }
            Some(preset) => serializer.serialize_some(preset.display_name()),
            None => serializer.serialize_none(),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<AgentPreset>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|name| {
                preset_by_display_name(&name)
                    .or_else(|| {
                        preset_by_bus_name(&name).filter(|preset| preset.acp_agent().is_some())
                    })
                    .ok_or_else(|| <D::Error as de::Error>::custom(UNKNOWN_DISPLAY_NAME))
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acp(id: &str) -> AgentPreset {
        AgentPreset::Acp(id.parse().unwrap_or_else(|_| unreachable!("{id} is an id")))
    }

    #[test]
    fn every_agent_round_trips_through_its_bus_name() {
        let presets = [
            AgentPreset::ClaudeCode,
            AgentPreset::Codex,
            AgentPreset::Opencode,
            acp("opencode"),
            acp("gemini"),
        ];
        let names: Vec<String> = presets
            .iter()
            .filter_map(|preset| bus_name(*preset).map(Cow::into_owned))
            .collect();
        assert_eq!(
            names,
            ["claude", "codex", "opencode", "acp:opencode", "acp:gemini"]
        );
        for (preset, name) in presets.iter().zip(&names) {
            assert_eq!(preset_by_bus_name(name), Some(*preset), "{name}");
        }
        assert_eq!(bus_name(AgentPreset::Shell), None);
        assert_eq!(preset_by_bus_name("acp:Not An Id"), None);
        assert_eq!(
            preset_by_display_name("Codex (ACP)"),
            Some(acp("codex-acp"))
        );
        assert_eq!(preset_by_display_name("codex"), Some(AgentPreset::Codex));
    }
}
