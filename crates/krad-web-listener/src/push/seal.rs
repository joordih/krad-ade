use ring::rand::{SecureRandom, SystemRandom};
use ring::{aead, agreement, hkdf};

const SALT_BYTES: usize = 16;
const RECORD_SIZE: u32 = 4096;
const KEY_INFO: &[u8] = b"WebPush: info\0";
const CONTENT_KEY_INFO: &[u8] = b"Content-Encoding: aes128gcm\0";
const NONCE_INFO: &[u8] = b"Content-Encoding: nonce\0";
const INPUT_KEY_BYTES: usize = 32;
const CONTENT_KEY_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const LAST_RECORD: u8 = 2;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SealError {
    #[error("the browser's push keys are not valid")]
    Keys,
    #[error("the notification could not be sealed")]
    Crypto,
}

impl From<ring::error::Unspecified> for SealError {
    fn from(_: ring::error::Unspecified) -> Self {
        Self::Crypto
    }
}

struct Length(usize);

impl hkdf::KeyType for Length {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(key: &hkdf::Prk, info: &[u8], len: usize) -> Result<Vec<u8>, SealError> {
    let mut out = vec![0; len];
    key.expand(&[info], Length(len))?.fill(&mut out)?;
    Ok(out)
}

pub(crate) struct Keys {
    pub(crate) content: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
}

pub(crate) fn derive(
    shared: &[u8],
    auth: &[u8],
    receiver: &[u8],
    sender: &[u8],
    salt: &[u8],
) -> Result<Keys, SealError> {
    let mut info = KEY_INFO.to_vec();
    info.extend_from_slice(receiver);
    info.extend_from_slice(sender);
    let auth_key = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(shared);
    let input = expand(&auth_key, &info, INPUT_KEY_BYTES)?;
    let key = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&input);
    Ok(Keys {
        content: expand(&key, CONTENT_KEY_INFO, CONTENT_KEY_BYTES)?,
        nonce: expand(&key, NONCE_INFO, NONCE_BYTES)?,
    })
}

pub(crate) fn seal(
    receiver: &[u8],
    auth: &[u8],
    plain: &[u8],
    random: &SystemRandom,
) -> Result<Vec<u8>, SealError> {
    let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, random)?;
    let sender = private.compute_public_key()?;
    let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, receiver);
    let shared =
        agreement::agree_ephemeral(private, &peer, <[u8]>::to_vec).map_err(|_| SealError::Keys)?;
    let mut salt = [0; SALT_BYTES];
    random.fill(&mut salt)?;
    let keys = derive(&shared, auth, receiver, sender.as_ref(), &salt)?;
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &keys.content)?);
    let nonce = aead::Nonce::try_assume_unique_for_key(&keys.nonce)?;
    let mut body = plain.to_vec();
    body.push(LAST_RECORD);
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut body)?;
    let id_len = u8::try_from(sender.as_ref().len()).map_err(|_| SealError::Crypto)?;
    let mut sealed = Vec::with_capacity(SALT_BYTES + 5 + sender.as_ref().len() + body.len());
    sealed.extend_from_slice(&salt);
    sealed.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    sealed.push(id_len);
    sealed.extend_from_slice(sender.as_ref());
    sealed.extend_from_slice(&body);
    Ok(sealed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_opens_what_the_runtime_sealed() -> Result<(), Box<dyn std::error::Error>> {
        let random = SystemRandom::new();
        let browser = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &random)
            .map_err(|_| "key")?;
        let receiver = browser.compute_public_key().map_err(|_| "public")?;
        let auth = [7; 16];
        let sealed = seal(receiver.as_ref(), &auth, b"{\"title\":\"Done\"}", &random)?;
        let (salt, rest) = sealed.split_at(SALT_BYTES);
        let (size, rest) = rest.split_at(4);
        assert_eq!(size, RECORD_SIZE.to_be_bytes());
        let (id_len, rest) = rest.split_at(1);
        let (sender, cipher) = rest.split_at(usize::from(id_len[0]));
        let peer = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, sender);
        let shared =
            agreement::agree_ephemeral(browser, &peer, <[u8]>::to_vec).map_err(|_| "agree")?;
        let keys = derive(&shared, &auth, receiver.as_ref(), sender, salt)?;
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_128_GCM, &keys.content).map_err(|_| "aes")?,
        );
        let nonce = aead::Nonce::try_assume_unique_for_key(&keys.nonce).map_err(|_| "nonce")?;
        let mut body = cipher.to_vec();
        let plain = key
            .open_in_place(nonce, aead::Aad::empty(), &mut body)
            .map_err(|_| "open")?;
        assert_eq!(plain, b"{\"title\":\"Done\"}\x02");
        Ok(())
    }
}
