use krad_domain::EnvironmentName;

use super::EnvironmentShell;

const NEW_ENV_PREFIX: &str = "environment-";

impl EnvironmentShell {
    pub(super) fn unused_env_name(&self) -> EnvironmentName {
        let mut position = self.order.len() + 1;
        loop {
            let name = EnvironmentName::new(format!("{NEW_ENV_PREFIX}{position}"));
            if !self.envs.contains_key(&name) {
                return name;
            }
            position += 1;
        }
    }
}
