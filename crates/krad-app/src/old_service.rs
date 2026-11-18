use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_LABEL: &str = "com.joordih.krad-ade.runtime";
const LAUNCH_AGENTS: &str = "Library/LaunchAgents";
const PLIST_EXTENSION: &str = "plist";
const SYSTEMD_USER_UNITS: &str = "systemd/user";
const SYSTEMD_UNIT: &str = "krad-ade-runtime.service";
const DOT_CONFIG: &str = ".config";
const LEFTOVERS: [&str; 2] = ["desktop.token", "runtime.sock"];

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn launch_agent_file(home: &Path) -> PathBuf {
    home.join(LAUNCH_AGENTS)
        .join(format!("{SERVICE_LABEL}.{PLIST_EXTENSION}"))
}

fn systemd_unit_file(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map_or_else(|| home.join(DOT_CONFIG), PathBuf::from)
        .join(SYSTEMD_USER_UNITS)
        .join(SYSTEMD_UNIT)
}

fn run(program: &str, args: &[&str]) {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => {}
        Ok(output) => tracing::debug!(program, status = %output.status, "a service step failed"),
        Err(error) => tracing::debug!(program, %error, "a service step did not run"),
    }
}

fn user_domain() -> io::Result<String> {
    let output = Command::new("id").arg("-u").output()?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(format!("gui/{uid}"))
}

fn retire_launch_agent(file: &Path) -> io::Result<()> {
    let domain = user_domain()?;
    run("launchctl", &["bootout", &domain, &file.to_string_lossy()]);
    fs::remove_file(file)
}

fn retire_systemd_unit(file: &Path) -> io::Result<()> {
    run("systemctl", &["--user", "disable", "--now", SYSTEMD_UNIT]);
    fs::remove_file(file)?;
    run("systemctl", &["--user", "daemon-reload"]);
    Ok(())
}

fn remove_leftovers(runtime_folder: &Path) {
    for leftover in LEFTOVERS {
        match fs::remove_file(runtime_folder.join(leftover)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => tracing::debug!(%error, leftover, "an old service file stayed"),
        }
    }
}

#[must_use]
pub(crate) fn retire_old_service(runtime_folder: &Path) -> bool {
    let Some(home) = home() else {
        return false;
    };
    let (file, retired) = if cfg!(target_os = "macos") {
        let file = launch_agent_file(&home);
        let retired = file.exists().then(|| retire_launch_agent(&file));
        (file, retired)
    } else {
        let file = systemd_unit_file(&home);
        let retired = file.exists().then(|| retire_systemd_unit(&file));
        (file, retired)
    };
    match retired {
        None => false,
        Some(Ok(())) => {
            remove_leftovers(runtime_folder);
            tracing::info!(file = %file.display(), "retired the old runtime service");
            true
        }
        Some(Err(error)) => {
            tracing::warn!(%error, file = %file.display(), "the old runtime service could not be retired");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_service_files_are_where_the_service_installed_them() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            launch_agent_file(home),
            Path::new("/Users/someone/Library/LaunchAgents/com.joordih.krad-ade.runtime.plist")
        );
        assert!(systemd_unit_file(home).ends_with("systemd/user/krad-ade-runtime.service"));
    }
}
