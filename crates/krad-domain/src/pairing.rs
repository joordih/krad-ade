use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::{InstallId, Timestamp};

const REDACTED: &str = "[REDACTED]";
const MAX_DEVICE_CHARS: usize = 64;
pub const PAIRING_CODE_LEN: usize = 8;
pub const PAIRING_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingPermission {
    Read,
    Drive,
    Answer,
    Admin,
}

impl PairingPermission {
    pub const ALL: [Self; 4] = [Self::Read, Self::Drive, Self::Answer, Self::Admin];
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PairingPermissions(BTreeSet<PairingPermission>);

impl PairingPermissions {
    #[must_use]
    pub fn all() -> Self {
        PairingPermission::ALL.into_iter().collect()
    }

    #[must_use]
    pub fn allows(&self, permission: PairingPermission) -> bool {
        self.0.contains(&permission)
    }

    #[must_use]
    pub fn covers(&self, other: &Self) -> bool {
        other.0.is_subset(&self.0)
    }
}

impl FromIterator<PairingPermission> for PairingPermissions {
    fn from_iter<I: IntoIterator<Item = PairingPermission>>(permissions: I) -> Self {
        Self(permissions.into_iter().collect())
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DeviceName(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDeviceName;

impl fmt::Display for InvalidDeviceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a device name is 1 to {MAX_DEVICE_CHARS} printable characters"
        )
    }
}

impl std::error::Error for InvalidDeviceName {}

impl DeviceName {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for DeviceName {
    type Err = InvalidDeviceName;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        let chars = raw.chars().count();
        let valid = (1..=MAX_DEVICE_CHARS).contains(&chars) && !raw.chars().any(char::is_control);
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(InvalidDeviceName)
        }
    }
}

impl TryFrom<String> for DeviceName {
    type Error = InvalidDeviceName;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<DeviceName> for String {
    fn from(name: DeviceName) -> Self {
        name.0
    }
}

impl fmt::Display for DeviceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PairingCode(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidPairingCode;

impl fmt::Display for InvalidPairingCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a pairing code is {PAIRING_CODE_LEN} letters and digits"
        )
    }
}

impl std::error::Error for InvalidPairingCode {}

impl PairingCode {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl FromStr for PairingCode {
    type Err = InvalidPairingCode;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let code = raw.trim().to_ascii_uppercase();
        let valid = code.len() == PAIRING_CODE_LEN
            && code
                .bytes()
                .all(|byte| PAIRING_CODE_ALPHABET.contains(&byte));
        if valid {
            Ok(Self(code))
        } else {
            Err(InvalidPairingCode)
        }
    }
}

impl TryFrom<String> for PairingCode {
    type Error = InvalidPairingCode;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        raw.parse()
    }
}

impl From<PairingCode> for String {
    fn from(code: PairingCode) -> Self {
        code.0
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PairedDevice {
    pub install: InstallId,
    pub device: DeviceName,
    pub paired_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<Timestamp>,
    pub permissions: PairingPermissions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_code_reads_in_any_case_and_never_prints() -> Result<(), InvalidPairingCode> {
        let code: PairingCode = " abcd2345 ".parse()?;
        assert_eq!(code.expose(), "ABCD2345");
        assert_eq!(format!("{code:?}"), REDACTED);
        assert!("ABCD234".parse::<PairingCode>().is_err());
        assert!("ABCD2340".parse::<PairingCode>().is_err());
        Ok(())
    }

    #[test]
    fn permissions_cover_only_what_they_hold() {
        let read: PairingPermissions = [PairingPermission::Read].into_iter().collect();
        assert!(PairingPermissions::all().covers(&read));
        assert!(!read.covers(&PairingPermissions::all()));
        assert!(read.allows(PairingPermission::Read));
        assert!(!read.allows(PairingPermission::Answer));
    }

    #[test]
    fn a_device_name_is_short_and_printable() {
        assert!("Jordi's phone".parse::<DeviceName>().is_ok());
        assert!("".parse::<DeviceName>().is_err());
        assert!("a\nb".parse::<DeviceName>().is_err());
        assert!("x"
            .repeat(MAX_DEVICE_CHARS + 1)
            .parse::<DeviceName>()
            .is_err());
    }
}
