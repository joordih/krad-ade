use krad_domain::EnvironmentName;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HarnessSpot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
}

impl HarnessSpot {
    #[must_use]
    pub fn at(environment: EnvironmentName, root: Option<PathBuf>) -> Self {
        Self {
            environment: Some(environment),
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_is_an_empty_spot_on_the_wire() -> serde_json::Result<()> {
        assert_eq!(serde_json::to_string(&HarnessSpot::default())?, "{}");
        let spot: HarnessSpot = serde_json::from_str(r#"{"environment":"work","root":"/repo"}"#)?;
        assert_eq!(
            spot,
            HarnessSpot::at(
                EnvironmentName::new("work".to_owned()),
                Some(PathBuf::from("/repo"))
            )
        );
        Ok(())
    }
}
