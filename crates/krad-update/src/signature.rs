use std::fmt;
use std::str::FromStr;

use minisign_verify::{Error, PublicKey, Signature, StreamVerifier};
use serde::Deserialize;

const PUBLIC_KEY: &str = include_str!("../../../packaging/update/minisign.pub");
const BLANK_COMMENT_LINE: &str = "\n";
const REDACTED_SIGNATURE: &str = "ReleaseSignature";

pub(crate) struct ReleaseKey(PublicKey);

#[derive(Clone, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct ReleaseSignature(Signature);

impl ReleaseKey {
    pub(crate) fn embedded() -> Result<Self, Error> {
        PublicKey::from_base64(PUBLIC_KEY.trim()).map(Self)
    }

    pub(crate) fn verify(&self, bytes: &[u8], signature: &ReleaseSignature) -> Result<(), Error> {
        self.0.verify(bytes, &signature.0, false)
    }

    pub(crate) fn stream<'a>(
        &'a self,
        signature: &'a ReleaseSignature,
    ) -> Result<StreamVerifier<'a>, Error> {
        self.0.verify_stream(&signature.0)
    }
}

impl FromStr for ReleaseSignature {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Signature::decode(&format!("{BLANK_COMMENT_LINE}{}", text.trim())).map(Self)
    }
}

impl TryFrom<String> for ReleaseSignature {
    type Error = Error;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl fmt::Debug for ReleaseSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED_SIGNATURE)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    pub(crate) const SIGNED_TEXT: &[u8] = b"hola\n";
    pub(crate) const SIGNATURE: &str = "RUQKeDKorDutRt1mv7SUyXWW9HBtMha81v+I+nmox2ZHF+8Y/PK2kzFj1jbS61ftBFTzmE89hDf8uA6sqS2/750iC6XmbNTQAAA=
trusted comment: krad-ade
lTXrSvQedTATcNp8YlYOvF149zJpox2O92KNKRkVDYn+M1Q2P9FRN4qGwGQpWhHP7B7Li8Xh4Na/M540wG0zAA==";

    #[test]
    fn verifies_what_the_release_key_signed() -> TestResult {
        let key = ReleaseKey::embedded()?;
        let signature: ReleaseSignature = SIGNATURE.parse()?;
        assert!(key.verify(SIGNED_TEXT, &signature).is_ok());
        Ok(())
    }

    #[test]
    fn rejects_changed_bytes() -> TestResult {
        let key = ReleaseKey::embedded()?;
        let signature: ReleaseSignature = SIGNATURE.parse()?;
        assert!(key.verify(b"adios\n", &signature).is_err());
        Ok(())
    }

    #[test]
    fn streams_the_same_check() -> TestResult {
        let key = ReleaseKey::embedded()?;
        let signature: ReleaseSignature = SIGNATURE.parse()?;
        let mut stream = key.stream(&signature)?;
        let (head, tail) = SIGNED_TEXT.split_at(2);
        stream.update(head);
        stream.update(tail);
        assert!(stream.finalize().is_ok());
        Ok(())
    }
}
