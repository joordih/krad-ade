use super::AgentPreset;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

const ACP_PREFIX: &str = "Acp:";
const SHELL: &str = "Shell";
const CLAUDE_CODE: &str = "ClaudeCode";
const CODEX: &str = "Codex";
const OPENCODE: &str = "Opencode";
const GATEWAY: &str = "Gateway";
const EXPECTED: &str = "a harness name";

const NATIVE_NAMES: [(AgentPreset, &str); 5] = [
    (AgentPreset::Shell, SHELL),
    (AgentPreset::ClaudeCode, CLAUDE_CODE),
    (AgentPreset::Codex, CODEX),
    (AgentPreset::Opencode, OPENCODE),
    (AgentPreset::Gateway, GATEWAY),
];

impl Serialize for AgentPreset {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let name = match self {
            Self::Acp(id) => return serializer.collect_str(&format_args!("{ACP_PREFIX}{id}")),
            Self::Shell => SHELL,
            Self::ClaudeCode => CLAUDE_CODE,
            Self::Codex => CODEX,
            Self::Opencode => OPENCODE,
            Self::Gateway => GATEWAY,
        };
        serializer.serialize_str(name)
    }
}

struct PresetVisitor;

impl Visitor<'_> for PresetVisitor {
    type Value = AgentPreset;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(EXPECTED)
    }

    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        if let Some(id) = text.strip_prefix(ACP_PREFIX) {
            return id.parse().map(AgentPreset::Acp).map_err(E::custom);
        }
        NATIVE_NAMES
            .into_iter()
            .find_map(|(preset, name)| (name == text).then_some(preset))
            .ok_or_else(|| {
                E::unknown_variant(text, &[SHELL, CLAUDE_CODE, CODEX, OPENCODE, GATEWAY])
            })
    }
}

impl<'de> Deserialize<'de> for AgentPreset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(PresetVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn native_harnesses_keep_their_stored_names_and_acp_agents_carry_their_id(
    ) -> serde_json::Result<()> {
        for (preset, stored) in [
            (AgentPreset::Shell, r#""Shell""#),
            (AgentPreset::ClaudeCode, r#""ClaudeCode""#),
            (AgentPreset::Codex, r#""Codex""#),
            (AgentPreset::Opencode, r#""Opencode""#),
            (AgentPreset::Gateway, r#""Gateway""#),
        ] {
            assert_eq!(serde_json::to_string(&preset)?, stored);
            assert_eq!(serde_json::from_str::<AgentPreset>(stored)?, preset);
        }
        let acp = AgentPreset::Acp(
            "codex-acp"
                .parse()
                .unwrap_or_else(|_| unreachable!("a valid id")),
        );
        assert_eq!(serde_json::to_string(&acp)?, r#""Acp:codex-acp""#);
        assert_eq!(
            serde_json::from_str::<AgentPreset>(r#""Acp:codex-acp""#)?,
            acp
        );
        assert!(serde_json::from_str::<AgentPreset>(r#""Acp:Codex ACP""#).is_err());
        assert!(serde_json::from_str::<AgentPreset>(r#""Claude""#).is_err());
        Ok(())
    }

    #[test]
    fn acp_agents_key_maps_next_to_the_native_harnesses() -> serde_json::Result<()> {
        let stored = r#"{"ClaudeCode":"claude --verbose","Acp:opencode":"opencode acp"}"#;
        let read: HashMap<AgentPreset, String> = serde_json::from_str(stored)?;
        assert_eq!(
            read.get(&AgentPreset::ClaudeCode).map(String::as_str),
            Some("claude --verbose")
        );
        let back: HashMap<AgentPreset, String> =
            serde_json::from_str(&serde_json::to_string(&read)?)?;
        assert_eq!(back, read);
        Ok(())
    }
}
