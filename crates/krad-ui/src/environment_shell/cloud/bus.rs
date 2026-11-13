use krad_protocol::CloudAskReplayed;

use super::super::EnvironmentShell;

impl EnvironmentShell {

    pub(in crate::environment_shell) fn cloud_ask_replayed(&mut self, asked: CloudAskReplayed) {
        let CloudAskReplayed {
            agent,
            kind,
            delivered,
        } = asked;
        self.with_webhooks_delivered(delivered, |shell| {
            shell.raise_attention(&agent.environment, agent.terminal, kind);
        });
    }
}
