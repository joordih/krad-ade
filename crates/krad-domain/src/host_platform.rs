use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const LINUX_KERNEL: &str = "Linux";
const X86_64_MACHINE: &str = "x86_64";
const AARCH64_MACHINES: [&str; 2] = ["aarch64", "arm64"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostPlatform {
    LinuxX86_64,
    LinuxAarch64,
}

impl HostPlatform {
    pub const ALL: [Self; 2] = [Self::LinuxX86_64, Self::LinuxAarch64];

    #[must_use]
    pub const fn musl_target(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "x86_64-unknown-linux-musl",
            Self::LinuxAarch64 => "aarch64-unknown-linux-musl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedPlatform(pub String);

impl fmt::Display for UnsupportedPlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} hosts are not supported; SSH environments need Linux on x86_64 or aarch64",
            self.0
        )
    }
}

impl std::error::Error for UnsupportedPlatform {}

impl FromStr for HostPlatform {
    type Err = UnsupportedPlatform;

    fn from_str(uname: &str) -> Result<Self, Self::Err> {
        let uname = uname.trim();
        let mut words = uname.split_whitespace();
        let (Some(kernel), Some(machine)) = (words.next(), words.next()) else {
            return Err(UnsupportedPlatform(uname.to_owned()));
        };
        match (kernel, machine) {
            (LINUX_KERNEL, X86_64_MACHINE) => Ok(Self::LinuxX86_64),
            (LINUX_KERNEL, machine) if AARCH64_MACHINES.contains(&machine) => {
                Ok(Self::LinuxAarch64)
            }
            _ => Err(UnsupportedPlatform(uname.to_owned())),
        }
    }
}

impl fmt::Display for HostPlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_on_both_architectures_is_supported() {
        assert_eq!("Linux x86_64\n".parse(), Ok(HostPlatform::LinuxX86_64));
        assert_eq!("Linux aarch64".parse(), Ok(HostPlatform::LinuxAarch64));
    }

    #[test]
    fn other_systems_are_refused_with_what_they_are() {
        assert_eq!(
            "Darwin arm64".parse::<HostPlatform>(),
            Err(UnsupportedPlatform("Darwin arm64".to_owned()))
        );
        assert!("".parse::<HostPlatform>().is_err());
    }

    #[test]
    fn a_platform_round_trips_through_json() -> serde_json::Result<()> {
        let json = serde_json::to_string(&HostPlatform::LinuxAarch64)?;
        assert_eq!(json, r#""linux_aarch64""#);
        assert_eq!(
            serde_json::from_str::<HostPlatform>(&json)?,
            HostPlatform::LinuxAarch64
        );
        Ok(())
    }
}
