use krad_domain::{AcpAgent, AcpAgentId, AcpSecretKey, AcpSettings, McpSecret};

use super::draft::{Draft, ReadyAgent};

pub(super) struct Admitted {
    pub(super) acp: AcpSettings,
    pub(super) writes: Vec<(AcpSecretKey, McpSecret)>,
    pub(super) deletes: Vec<AcpSecretKey>,
}

pub(super) fn admit(
    kept: &AcpSettings,
    editing: Option<AcpAgentId>,
    draft: &Draft,
) -> Result<Admitted, String> {
    let ReadyAgent { agent, secrets } = draft.ready().map_err(|error| error.to_string())?;
    let mut acp = kept.clone();
    let replaced: Option<AcpAgent> = editing.and_then(|id| acp.remove_agent(id));
    let new_keys = agent.secret_keys();
    let writes = secrets
        .into_iter()
        .map(|(variable, secret)| {
            let key = AcpSecretKey {
                agent: agent.id,
                variable,
            };
            (key, secret)
        })
        .collect();
    acp.add_agent(agent)
        .map_err(|rejected| rejected.to_string())?;
    let deletes = replaced
        .map_or_default(|old| old.secret_keys())
        .into_iter()
        .filter(|key| !new_keys.contains(key))
        .collect();
    Ok(Admitted {
        acp,
        writes,
        deletes,
    })
}

#[cfg(test)]
mod tests {
    use super::super::draft::{DraftVariable, StoredSecret, ValueKind};
    use super::*;
    use krad_domain::{McpValue, Settings};

    fn draft(id: &str, variables: Vec<DraftVariable>) -> Draft {
        Draft {
            id: id.to_owned(),
            name: "Gemini".to_owned(),
            program: "gemini".to_owned(),
            args: "--experimental-acp".to_owned(),
            terminal: "gemini".to_owned(),
            variables,
        }
    }

    fn secret_row(name: &str, value: &str, stored: StoredSecret) -> DraftVariable {
        DraftVariable {
            name: name.to_owned(),
            value: value.to_owned(),
            kind: ValueKind::Secret,
            stored,
        }
    }

    #[test]
    fn an_added_agent_sends_its_secret_to_the_keyring_and_not_to_the_settings() {
        let draft = draft(
            "gemini",
            vec![secret_row(
                "GEMINI_API_KEY",
                "g-secret-1",
                StoredSecret::None,
            )],
        );
        let admitted = admit(&AcpSettings::default(), None, &draft)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let keys: Vec<String> = admitted
            .writes
            .iter()
            .map(|(key, _)| key.to_string())
            .collect();
        assert_eq!(keys, ["gemini/GEMINI_API_KEY"]);
        assert_eq!(admitted.agent_env_values(), [McpValue::Secret]);
        let settings = Settings {
            acp: admitted.acp,
            ..Settings::default()
        };
        let written = format!("{settings:?}");
        assert!(written.contains("GEMINI_API_KEY"), "{written}");
        assert!(!written.contains("g-secret-1"), "{written}");
    }

    #[test]
    fn edits_keep_stored_secrets_drop_removed_ones_and_rejections_explain_themselves() {
        let first = draft(
            "gemini",
            vec![
                secret_row("A", "a-1", StoredSecret::None),
                secret_row("B", "b-1", StoredSecret::None),
            ],
        );
        let added = admit(&AcpSettings::default(), None, &first)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let id = added.acp.agents[0].id;
        let edit = draft("gemini", vec![secret_row("A", "", StoredSecret::Kept)]);
        let edited =
            admit(&added.acp, Some(id), &edit).unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(edited.writes.len(), 0);
        let deleted: Vec<String> = edited.deletes.iter().map(ToString::to_string).collect();
        assert_eq!(deleted, ["gemini/B"]);
        assert_eq!(edited.acp.agents.len(), 1);
        assert_eq!(
            admit(&added.acp, None, &first).err().as_deref(),
            Some("There is already an agent with the id gemini")
        );
        assert_eq!(
            admit(
                &AcpSettings::default(),
                None,
                &draft("codex-acp", Vec::new())
            )
            .err()
            .as_deref(),
            Some("codex-acp is the id of a built-in agent")
        );
    }

    impl Admitted {
        fn agent_env_values(&self) -> Vec<McpValue> {
            self.acp.agents[0]
                .env
                .iter()
                .map(|variable| variable.value.clone())
                .collect()
        }
    }
}
