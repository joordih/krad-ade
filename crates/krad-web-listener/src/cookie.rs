use krad_domain::{InstallId, Pairing, PairingToken};

use crate::http::Request;

const COOKIE_NAME: &str = "krad-pairing";
const SEPARATOR: char = '.';
const LIFETIME_SECONDS: u64 = 400 * 24 * 60 * 60;

pub(crate) fn pairing_of(request: &Request) -> Option<Pairing> {
    let cookies = request.header("cookie")?;
    let value = cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == COOKIE_NAME).then_some(value)
    })?;
    let (install, token) = value.split_once(SEPARATOR)?;
    Some(Pairing {
        install: install.parse::<InstallId>().ok()?,
        token: token.parse::<PairingToken>().ok()?,
    })
}

fn attributes(secure: bool) -> &'static str {
    if secure {
        "Path=/; HttpOnly; SameSite=Strict; Secure"
    } else {
        "Path=/; HttpOnly; SameSite=Strict"
    }
}

pub(crate) fn set_pairing(pairing: &Pairing, secure: bool) -> String {
    format!(
        "Set-Cookie: {COOKIE_NAME}={}{SEPARATOR}{}; Max-Age={LIFETIME_SECONDS}; {}",
        pairing.install.as_str(),
        pairing.token.expose(),
        attributes(secure)
    )
}

pub(crate) fn clear_pairing(secure: bool) -> String {
    format!(
        "Set-Cookie: {COOKIE_NAME}=; Max-Age=0; {}",
        attributes(secure)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::read_request;

    #[test]
    fn a_set_pairing_cookie_reads_back_as_the_same_pairing(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pairing = Pairing {
            install: "web-0123456789".parse()?,
            token: "00112233445566778899aabbccddeeff".parse()?,
        };
        let header = set_pairing(&pairing, true);
        assert!(header.contains("HttpOnly") && header.contains("Secure"));
        let value = header
            .trim_start_matches("Set-Cookie: ")
            .split(';')
            .next()
            .ok_or("no cookie value")?;
        let raw = format!("GET / HTTP/1.1\r\nCookie: theme=dark; {value}\r\n\r\n");
        let mut pending = Vec::new();
        let request = read_request(&mut raw.as_bytes(), &mut pending)?.ok_or("no request")?;
        assert_eq!(pairing_of(&request), Some(pairing));
        Ok(())
    }
}
