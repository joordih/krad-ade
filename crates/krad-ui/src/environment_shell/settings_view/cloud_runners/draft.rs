use krad_domain::{CloudRunnerName, CloudRunnerNameError, SshDestination, SshDestinationError};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DraftError {
    Name(CloudRunnerNameError),
    Destination(SshDestinationError),
}

impl fmt::Display for DraftError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(error) => error.fmt(formatter),
            Self::Destination(error) => error.fmt(formatter),
        }
    }
}

pub(super) struct Draft {
    pub(super) name: String,
    pub(super) destination: String,
}

impl Draft {
    pub(super) fn ready(&self) -> Result<(CloudRunnerName, SshDestination), DraftError> {
        let name = self.name.parse().map_err(DraftError::Name)?;
        let destination = self
            .destination
            .trim()
            .parse()
            .map_err(DraftError::Destination)?;
        Ok((name, destination))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(name: &str, destination: &str) -> Draft {
        Draft {
            name: name.to_owned(),
            destination: destination.to_owned(),
        }
    }

    #[test]
    fn a_draft_needs_a_name_and_a_destination_ssh_understands() {
        let ready = draft(" GPU ", " me@gpu:2200 ").ready();
        assert_eq!(
            ready.map(|(name, destination)| (String::from(name), destination.to_string())),
            Ok(("GPU".to_owned(), "me@gpu:2200".to_owned()))
        );
        assert_eq!(
            draft("", "gpu").ready().err(),
            Some(DraftError::Name(CloudRunnerNameError::Empty))
        );
        assert_eq!(
            draft("GPU", "-oProxyCommand=x").ready().err(),
            Some(DraftError::Destination(
                SshDestinationError::LooksLikeAnOption
            ))
        );
    }
}
