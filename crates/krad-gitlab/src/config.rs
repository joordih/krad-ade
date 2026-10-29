use krad_domain::GitlabSettings;

const SCHEME_SEPARATOR: &str = "://";
const DEFAULT_SCHEME: &str = "https://";
const API_PATH: &str = "/api/v4";

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("Set the GitLab host in Settings")]
    MissingHost,
    #[error("Set the client ID of your GitLab OAuth application in Settings")]
    MissingClientId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitlabConfig {
    pub host: String,
    pub client_id: String,
}

impl GitlabConfig {
    pub(crate) fn api_url(&self, path: &str) -> String {
        format!("{}{API_PATH}{path}", self.host)
    }

    pub(crate) fn oauth_url(&self, path: &str) -> String {
        format!("{}{path}", self.host)
    }
}

impl TryFrom<&GitlabSettings> for GitlabConfig {
    type Error = ConfigError;

    fn try_from(settings: &GitlabSettings) -> Result<Self, ConfigError> {
        let host = settings.host.trim().trim_end_matches('/');
        if host.is_empty() {
            return Err(ConfigError::MissingHost);
        }
        let client_id = settings.client_id.trim();
        if client_id.is_empty() {
            return Err(ConfigError::MissingClientId);
        }
        let host = if host.contains(SCHEME_SEPARATOR) {
            host.to_owned()
        } else {
            format!("{DEFAULT_SCHEME}{host}")
        };
        Ok(Self {
            host,
            client_id: client_id.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(host: &str, client_id: &str) -> GitlabSettings {
        GitlabSettings {
            host: host.to_owned(),
            client_id: client_id.to_owned(),
            ..GitlabSettings::default()
        }
    }

    #[test]
    fn a_bare_host_gets_https_and_loses_its_slash() -> Result<(), ConfigError> {
        let config = GitlabConfig::try_from(&settings(" gitlab.example.test/ ", " app "))?;
        assert_eq!(config.host, "https://gitlab.example.test");
        assert_eq!(config.client_id, "app");
        assert_eq!(
            config.api_url("/user"),
            "https://gitlab.example.test/api/v4/user"
        );
        Ok(())
    }

    #[test]
    fn an_explicit_scheme_is_kept() -> Result<(), ConfigError> {
        let config = GitlabConfig::try_from(&settings("http://127.0.0.1:8080", "app"))?;
        assert_eq!(config.host, "http://127.0.0.1:8080");
        Ok(())
    }

    #[test]
    fn blank_values_are_named() {
        assert_eq!(
            GitlabConfig::try_from(&settings(" ", "app")),
            Err(ConfigError::MissingHost)
        );
        assert_eq!(
            GitlabConfig::try_from(&settings("gitlab.com", "")),
            Err(ConfigError::MissingClientId)
        );
    }
}
