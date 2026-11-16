use krad_web_listener::{write_web_access, TlsFiles, WebAccess};
use serde::Deserialize;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::desktop_runtime::config_folder;
use crate::service::{runtime_folder, start_service, stop_service};

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
const WAIT_STEP: Duration = Duration::from_millis(250);
const APP_GONE_LIMIT: Duration = Duration::from_secs(60);
const SERVICE_READY_LIMIT: Duration = Duration::from_secs(60);
const APP_BUNDLE_SUFFIX: &str = ".app";
const PID_ALIVE_SIGNAL: &str = "-0";

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

fn certificate(tailnet: &Tailnet, root: &Path) -> io::Result<TlsFiles> {
    let files = TlsFiles {
        certificate: root.join(CERTIFICATE_FILE),
        key: root.join(KEY_FILE),
    };
    let output = Command::new(&tailnet.program)
        .arg("cert")
        .arg("--cert-file")
        .arg(&files.certificate)
        .arg("--key-file")
        .arg(&files.key)
        .arg(&tailnet.name)
        .output()?;
    if output.status.success() {
        return Ok(files);
    }
    Err(io::Error::other(
        String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    ))
}

/// # Errors
///
/// Fails when Tailscale cannot give this computer a name, an address or a certificate, or the
/// settings cannot be written.
pub(crate) fn enable() -> io::Result<String> {
    let root = runtime_folder(&config_folder()?);
    krad_host::runner::ensure_private_dir(&root)?;
    let tailnet = tailnet()?;
    let tls = certificate(&tailnet, &root)?;
    let access = WebAccess {
        bind: SocketAddr::new(tailnet.address, WEB_PORT),
        tls: Some(tls),
        names: vec![tailnet.name.clone()],
    };
    write_web_access(&root, &access)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    Ok(format!("https://{}:{WEB_PORT}", tailnet.name))
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args([PID_ALIVE_SIGNAL, pid])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::sleep(WAIT_STEP);
    }
    false
}

fn app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|path| path.to_string_lossy().ends_with(APP_BUNDLE_SUFFIX))
        .map(Path::to_path_buf)
}

fn reopen_app() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    match app_bundle(&exe).filter(|_| cfg!(target_os = "macos")) {
        Some(bundle) => Command::new("open").arg("-n").arg(bundle).spawn()?,
        None => Command::new(exe).spawn()?,
    };
    Ok(())
}

/// # Errors
///
/// Fails when the window does not close in time, the service does not start or the app cannot
/// be opened again.
pub(crate) fn hand_over(pid: &str) -> io::Result<()> {
    if !wait_until(APP_GONE_LIMIT, || !alive(pid)) {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the window did not close",
        ));
    }
    start_service()?;
    let socket = krad_host::runner::runtime_socket(&runtime_folder(&config_folder()?));
    if !wait_until(SERVICE_READY_LIMIT, || UnixStream::connect(&socket).is_ok()) {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the service did not start",
        ));
    }
    reopen_app()
}

/// # Errors
///
/// Fails when the window does not close in time, the service cannot be stopped or the app
/// cannot be opened again.
pub(crate) fn hand_back(pid: &str) -> io::Result<()> {
    if !wait_until(APP_GONE_LIMIT, || !alive(pid)) {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the window did not close",
        ));
    }
    stop_service()?;
    reopen_app()
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
    fn the_app_bundle_is_found_above_its_binary() {
        let exe = Path::new("/Applications/Krad ADE.app/Contents/MacOS/krad-ade");
        assert_eq!(
            app_bundle(exe),
            Some(PathBuf::from("/Applications/Krad ADE.app"))
        );
        assert_eq!(app_bundle(Path::new("/usr/bin/krad-ade")), None);
    }
}
