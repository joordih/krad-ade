use krad_domain::{DeviceName, InstallId, PairingPermission, PairingPermissions, PairingToken};
use krad_host::runner::{ensure_private_dir, runtime_socket};
use krad_protocol::{PairingAnswer, PairingCommand};
use krad_runner::{bind_runtime_socket, pair_over_ssh, RuntimeServer};
use krad_runtime::Clock;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::desktop_runtime::{config_folder, lock_runtime, start_desktop_runtime};

pub(crate) const SERVICE_VERB: &str = "service";
const INSTALL: &str = "install";
const UNINSTALL: &str = "uninstall";
const WEB_ACCESS: &str = "web-access";
const PAIR_BROWSER: &str = "pair-browser";
const BROWSER_PERMISSIONS: [PairingPermission; 3] = [
    PairingPermission::Read,
    PairingPermission::Answer,
    PairingPermission::Drive,
];
const RUNTIME_FOLDER: &str = "runtime";
const DESKTOP_INSTALL: &str = "krad-ade-desktop";
const DESKTOP_DEVICE: &str = "This computer's app";
const DESKTOP_TOKEN: &str = "desktop.token";
const PRIVATE_FILE: u32 = 0o600;
const SERVICE_LABEL: &str = "com.joordih.krad-ade.runtime";
const SYSTEMD_UNIT: &str = "krad-ade-runtime.service";
const USAGE: &str = "usage: krad-ade service [install | uninstall | web-access <on|off|status> ... | pair-browser [--admin]]";
const ADMIN_FLAG: &str = "--admin";
const NO_CODE: &str = "the service did not make a pairing code";
const UPDATE_CHECK: Duration = Duration::from_secs(60);
const RESTART_FOR_UPDATE: i32 = 75;
const UPDATE_THREAD: &str = "krad-ade-service-update";
const BAD_NAME: &str = "the desktop pairing has an invalid name";
const UNSUPPORTED: &str = "this computer's runtime runs as a service on macOS and Linux only";

pub(crate) fn runtime_folder(config: &Path) -> PathBuf {
    config.join(RUNTIME_FOLDER)
}

pub(crate) fn desktop_token_file(config: &Path) -> PathBuf {
    runtime_folder(config).join(DESKTOP_TOKEN)
}

pub(crate) fn desktop_install() -> io::Result<InstallId> {
    DESKTOP_INSTALL
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, BAD_NAME))
}

fn pair_desktop(root: &Path, config: &Path) -> io::Result<()> {
    let device: DeviceName = DESKTOP_DEVICE
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, BAD_NAME))?;
    let token: PairingToken = pair_over_ssh(root, &desktop_install()?, device)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(PRIVATE_FILE)
        .open(desktop_token_file(config))?;
    file.write_all(token.expose().as_bytes())
}

fn serve(clock: Clock) -> io::Result<i32> {
    let config = config_folder()?;
    let _lock = lock_runtime(&config)?;
    let root = runtime_folder(&config);
    ensure_private_dir(&root)?;
    let started = start_desktop_runtime(clock)?;
    let listener = bind_runtime_socket(&root)?;
    pair_desktop(&root, &config)?;
    watch_for_update(started.runtime.clone())?;
    let server = RuntimeServer::new(started.runtime, &root);
    if let Err(error) = krad_web_listener::start_web_access(&root, &server) {
        tracing::warn!(%error, "web access did not start");
    }
    tracing::info!(socket = %runtime_socket(&root).display(), "this computer's runtime serves");
    server.serve(&listener);
    Ok(0)
}

fn binary_changed_at() -> Option<SystemTime> {
    std::env::current_exe()
        .and_then(std::fs::metadata)
        .and_then(|meta| meta.modified())
        .ok()
}

fn idle(snapshot: &krad_protocol::Snapshot) -> bool {
    snapshot
        .environments
        .iter()
        .flat_map(|environment| &environment.nodes)
        .all(|node| node.state != krad_domain::AgentState::Working)
}

fn watch_for_update(runtime: krad_runtime::RuntimeHandle) -> io::Result<()> {
    let started = binary_changed_at();
    std::thread::Builder::new()
        .name(UPDATE_THREAD.to_owned())
        .spawn(move || loop {
            std::thread::sleep(UPDATE_CHECK);
            if binary_changed_at() == started {
                continue;
            }
            let Ok(snapshot) =
                futures::executor::block_on(runtime.snapshot(krad_protocol::Scope::Runtime))
            else {
                return;
            };
            if idle(&snapshot) {
                tracing::info!("the app was updated and no agent works: the service restarts");
                if let Err(error) = futures::executor::block_on(runtime.settle()) {
                    tracing::debug!(%error, "the runtime stopped before it settled");
                }
                std::process::exit(RESTART_FOR_UPDATE);
            }
        })
        .map(drop)
}

fn launch_agent_file() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist")))
}

fn systemd_unit_file() -> io::Result<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    Ok(config.join("systemd/user").join(SYSTEMD_UNIT))
}

fn launch_agent(exe: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key><string>{SERVICE_LABEL}</string>\n\
         \x20 <key>ProgramArguments</key><array><string>{}</string><string>{SERVICE_VERB}</string></array>\n\
         \x20 <key>RunAtLoad</key><true/>\n\
         \x20 <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n\
         \x20 <key>ProcessType</key><string>Interactive</string>\n\
         </dict>\n</plist>\n",
        exe.display()
    )
}

fn systemd_unit(exe: &Path) -> String {
    format!(
        "[Unit]\nDescription=Krad ADE runtime of this computer\n\n\
         [Service]\nExecStart={} {SERVICE_VERB}\nRestart=on-failure\n\n\
         [Install]\nWantedBy=default.target\n",
        exe.display()
    )
}

fn run(program: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{program} failed: {status}")))
    }
}

fn user_domain() -> io::Result<String> {
    let output = Command::new("id").arg("-u").output()?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(format!("gui/{uid}"))
}

fn install() -> io::Result<i32> {
    let exe = std::env::current_exe()?;
    if cfg!(target_os = "macos") {
        let file = launch_agent_file()?;
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file, launch_agent(&exe))?;
        let domain = user_domain()?;
        let path = file.to_string_lossy().into_owned();
        drop(run("launchctl", &["bootout", &domain, &path]));
        run("launchctl", &["bootstrap", &domain, &path])?;
    } else if cfg!(target_os = "linux") {
        let file = systemd_unit_file()?;
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file, systemd_unit(&exe))?;
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", SYSTEMD_UNIT])?;
    } else {
        return Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED));
    }
    Ok(0)
}

fn uninstall() -> io::Result<i32> {
    if cfg!(target_os = "macos") {
        let file = launch_agent_file()?;
        let path = file.to_string_lossy().into_owned();
        drop(run("launchctl", &["bootout", &user_domain()?, &path]));
        if file.exists() {
            fs::remove_file(file)?;
        }
    } else if cfg!(target_os = "linux") {
        drop(run(
            "systemctl",
            &["--user", "disable", "--now", SYSTEMD_UNIT],
        ));
        let file = systemd_unit_file()?;
        if file.exists() {
            fs::remove_file(file)?;
        }
        run("systemctl", &["--user", "daemon-reload"])?;
    } else {
        return Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED));
    }
    Ok(0)
}

fn say(text: &str) -> io::Result<i32> {
    let mut out = io::stdout().lock();
    writeln!(out, "{text}")?;
    out.flush()?;
    Ok(0)
}

fn web_access(rest: &[String]) -> io::Result<i32> {
    let root = runtime_folder(&config_folder()?);
    ensure_private_dir(&root)?;
    let said = krad_web_listener::web_access_command(rest, &root)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.to_string()))?;
    say(&said)
}

fn pair_browser(rest: &[String]) -> io::Result<i32> {
    let mut permissions: PairingPermissions = BROWSER_PERMISSIONS.into_iter().collect();
    if rest.iter().any(|flag| flag == ADMIN_FLAG) {
        permissions = PairingPermissions::all();
    }
    let runner = crate::desktop_runtime::connect_service(&config_folder()?)?;
    let answer = futures::executor::block_on(
        runner.pairing(PairingCommand::CreatePairingCode { permissions }),
    )
    .map_err(|error| io::Error::other(error.to_string()))?;
    let PairingAnswer::Code { code, .. } = answer else {
        return Err(io::Error::other(NO_CODE));
    };
    say(code.expose())
}

pub(crate) fn run_service_verb(rest: &[String], clock: Clock) -> i32 {
    let outcome = match rest.first().map(String::as_str) {
        None => serve(clock),
        Some(INSTALL) => install(),
        Some(UNINSTALL) => uninstall(),
        Some(WEB_ACCESS) => web_access(&rest[1..]),
        Some(PAIR_BROWSER) => pair_browser(&rest[1..]),
        Some(_) => Err(io::Error::new(io::ErrorKind::InvalidInput, USAGE)),
    };
    outcome.unwrap_or_else(|error| {
        tracing::error!(%error, "this computer's runtime service failed");
        krad_host::runner::report_verb_failure(&error.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_with(
        state: krad_domain::AgentState,
    ) -> Result<krad_protocol::Snapshot, serde_json::Error> {
        let mut data = krad_domain::EnvironmentData::empty(krad_domain::EnvironmentName::new(
            "main".to_owned(),
        ));
        let node: krad_domain::TerminalNode = serde_json::from_value(serde_json::json!({
            "id": 1, "title": "agent", "state": state, "group": null, "kind": "Terminal"
        }))?;
        data.nodes.push(node);
        let mut snapshot: krad_protocol::Snapshot =
            serde_json::from_value(serde_json::json!({ "scope": "runtime", "seq": 1 }))?;
        snapshot.environments = vec![data];
        Ok(snapshot)
    }

    #[test]
    fn the_service_waits_for_working_agents_before_an_update_restart(
    ) -> Result<(), serde_json::Error> {
        assert!(idle(&snapshot_with(krad_domain::AgentState::Idle)?));
        assert!(idle(&snapshot_with(
            krad_domain::AgentState::WaitingForInput
        )?));
        assert!(!idle(&snapshot_with(krad_domain::AgentState::Working)?));
        Ok(())
    }

    #[test]
    fn the_service_files_run_this_binary_with_the_service_verb() {
        let exe = Path::new("/Applications/Krad ADE.app/Contents/MacOS/krad-ade");
        let agent = launch_agent(exe);
        assert!(agent.contains("<string>/Applications/Krad ADE.app/Contents/MacOS/krad-ade</string><string>service</string>"));
        assert!(agent.contains(SERVICE_LABEL));
        let unit = systemd_unit(Path::new("/usr/bin/krad-ade"));
        assert!(unit.contains("ExecStart=/usr/bin/krad-ade service"));
    }
}
