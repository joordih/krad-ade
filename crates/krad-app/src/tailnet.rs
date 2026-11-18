use krad_web_listener::{write_web_access, TlsFiles, WebAccess};
use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::process::Command;

const WEB_PORT: u16 = 7421;
const CERTIFICATE_FILE: &str = "web.crt";
const KEY_FILE: &str = "web.key";
const TAILSCALE_CANDIDATES: [&str; 4] = [
    "tailscale",
    "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    "/opt/homebrew/bin/tailscale",
    "/usr/local/bin/tailscale",
];
const NO_TAILSCALE: &str = "Tailscale is not installed or not running on this computer";
const NO_TAILNET_NAME: &str = "Tailscale gave this computer no name or address on the tailnet; turn on MagicDNS and HTTPS certificates in the Tailscale admin console";
const NO_CERTIFICATE: &str = "Tailscale returned no certificate and key";
const PEM_BEGIN: &str = "-----BEGIN ";
const PEM_CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----";
const PEM_PRIVATE_KEY: &str = "PRIVATE KEY";
const STDOUT: &str = "-";
const PRIVATE_FILE: u32 = 0o600;

#[derive(Deserialize)]
struct TailscaleStatus {
    #[serde(rename = "Self")]
    me: TailscalePeer,
}

#[derive(Deserialize)]
struct TailscalePeer {
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    addresses: Vec<IpAddr>,
}

struct Tailnet {
    program: String,
    name: String,
    address: IpAddr,
}

fn tailnet_of(status: &[u8]) -> Option<(String, IpAddr)> {
    let status: TailscaleStatus = serde_json::from_slice(status).ok()?;
    let name = status
        .me
        .dns_name
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let address = status.me.addresses.into_iter().find(IpAddr::is_ipv4)?;
    (!name.is_empty()).then_some((name, address))
}

fn tailnet() -> io::Result<Tailnet> {
    for program in TAILSCALE_CANDIDATES {
        let Ok(output) = Command::new(program).args(["status", "--json"]).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let (name, address) = tailnet_of(&output.stdout)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NO_TAILNET_NAME))?;
        return Ok(Tailnet {
            program: program.to_owned(),
            name,
            address,
        });
    }
    Err(io::Error::new(io::ErrorKind::NotFound, NO_TAILSCALE))
}

fn split_pem(output: &str) -> Option<(&str, &str)> {
    let key_start = output
        .match_indices(PEM_BEGIN)
        .map(|(at, _)| at)
        .find(|&at| {
            output[at..]
                .lines()
                .next()
                .is_some_and(|line| line.contains(PEM_PRIVATE_KEY))
        })?;
    let (chain, key) = output.split_at(key_start);
    chain.contains(PEM_CERTIFICATE).then_some((chain, key))
}

fn write_private(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(PRIVATE_FILE)
        .open(path)?;
    file.write_all(contents.as_bytes())
}

fn certificate(tailnet: &Tailnet, root: &Path) -> io::Result<TlsFiles> {
    let files = TlsFiles {
        certificate: root.join(CERTIFICATE_FILE),
        key: root.join(KEY_FILE),
    };
    let output = Command::new(&tailnet.program)
        .args(["cert", "--cert-file", STDOUT, "--key-file", STDOUT])
        .arg(&tailnet.name)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let pem = String::from_utf8_lossy(&output.stdout);
    let (chain, key) = split_pem(&pem)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, NO_CERTIFICATE))?;
    write_private(&files.certificate, chain)?;
    write_private(&files.key, key)?;
    Ok(files)
}

/// # Errors
///
/// Fails when Tailscale cannot give this computer a name, an address or a certificate, or the
/// settings cannot be written.
pub(crate) fn tailnet_web_access(root: &Path) -> io::Result<WebAccess> {
    let tailnet = tailnet()?;
    let tls = certificate(&tailnet, root)?;
    let access = WebAccess {
        bind: SocketAddr::new(tailnet.address, WEB_PORT),
        tls: Some(tls),
        names: vec![tailnet.name],
    };
    write_web_access(root, &access)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    Ok(access)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tailnet_name_and_ipv4_come_from_tailscale_status() {
        let status = br#"{"Self":{"DNSName":"Laptop.Tailnet-ab12.ts.net.","TailscaleIPs":["fd7a:115c:a1e0::1","100.101.102.103"]}}"#;
        let (name, address) =
            tailnet_of(status).unwrap_or_else(|| (String::new(), IpAddr::from([0, 0, 0, 0])));
        assert_eq!(name, "laptop.tailnet-ab12.ts.net");
        assert_eq!(address, IpAddr::from([100, 101, 102, 103]));
        assert!(tailnet_of(br#"{"Self":{"DNSName":"","TailscaleIPs":[]}}"#).is_none());
    }

    #[test]
    fn the_certificate_chain_and_key_are_split_from_tailscale_output() {
        let output = "-----BEGIN CERTIFICATE-----\nA\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nB\n-----END CERTIFICATE-----\n-----BEGIN EC PRIVATE KEY-----\nK\n-----END EC PRIVATE KEY-----\n";
        let (chain, key) = split_pem(output).unwrap_or_default();
        assert_eq!(chain.matches("BEGIN CERTIFICATE").count(), 2);
        assert!(!chain.contains("PRIVATE KEY"));
        assert!(key.starts_with("-----BEGIN EC PRIVATE KEY-----"));
        assert!(split_pem("-----BEGIN CERTIFICATE-----\nA\n").is_none());
    }
}
