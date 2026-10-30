use std::path::Path;

const PATHEXT_ENV: &str = "PATHEXT";
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";
const PATHEXT_SEPARATOR: char = ';';
const POWERSHELL_PROGRAM: &str = "powershell.exe";
const NO_LOGO_FLAG: &str = "-NoLogo";
const POWERSHELL_CLEAR: &str = "Clear-Host; ";
const POSIX_CLEAR: &str = "printf '\\033[H\\033[2J\\033[3J'; ";

#[must_use]
pub fn default_shell() -> (String, Vec<String>) {
    (POWERSHELL_PROGRAM.to_owned(), vec![NO_LOGO_FLAG.to_owned()])
}

pub const SHELL_QUOTING: krad_domain::ShellQuoting = krad_domain::ShellQuoting::PowerShell;

pub(crate) fn executable_names(name: &str) -> Vec<String> {
    let extensions = std::env::var(PATHEXT_ENV).unwrap_or_else(|_| DEFAULT_PATHEXT.to_owned());
    extensions
        .split(PATHEXT_SEPARATOR)
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{name}{}", extension.to_ascii_lowercase()))
        .collect()
}

pub(crate) fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[must_use]
pub fn login_shell_env() -> Vec<(String, String)> {
    Vec::new()
}

#[must_use]
pub const fn clear_screen_prefix(in_wsl: bool) -> &'static str {
    if in_wsl {
        POSIX_CLEAR
    } else {
        POWERSHELL_CLEAR
    }
}
