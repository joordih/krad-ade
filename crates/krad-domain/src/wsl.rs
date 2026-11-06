use std::path::{Path, PathBuf};

const WSL_HOSTS: [&str; 2] = ["wsl.localhost", "wsl$"];
const WSL_HOST: &str = "wsl.localhost";
const UNC_PREFIX: &str = r"\\";
const WINDOWS_SEPARATOR: char = '\\';
const LINUX_SEPARATOR: char = '/';
const DRIVE_SEPARATOR: char = ':';
const DRIVE_MOUNT: &str = "/mnt";
const DISTRO_FLAG: &str = "--distribution";
const CD_FLAG: &str = "--cd";
const EXEC_FLAG: &str = "--exec";
pub const POSIX_SHELL: &str = "sh";
pub const POSIX_SHELL_COMMAND_FLAG: &str = "-c";
const EXEC_ARGUMENTS: &str = r#"exec "$@""#;

pub const WSL_PROGRAM: &str = "wsl.exe";
pub const WSL_LOGIN_SHELL_SCRIPT: &str =
    r#"script="$1"; shift; exec "${SHELL:-sh}" -lic "$script" sh "$@""#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WslPath {
    pub distro: String,
    pub linux: String,
}

#[must_use]
pub fn parse_wsl_path(path: &Path) -> Option<WslPath> {
    let text = path
        .to_string_lossy()
        .replace(LINUX_SEPARATOR, &WINDOWS_SEPARATOR.to_string());
    let rest = text.strip_prefix(UNC_PREFIX)?;
    let mut parts = rest
        .split(WINDOWS_SEPARATOR)
        .filter(|part| !part.is_empty());
    let host = parts.next()?;
    if !WSL_HOSTS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(host))
    {
        return None;
    }
    let distro = parts.next()?.to_owned();
    let linux = parts.fold(String::new(), |mut linux, part| {
        linux.push(LINUX_SEPARATOR);
        linux.push_str(part);
        linux
    });
    Some(WslPath {
        distro,
        linux: if linux.is_empty() {
            LINUX_SEPARATOR.to_string()
        } else {
            linux
        },
    })
}

#[must_use]
pub fn wsl_unc_path(distro: &str, linux: &str) -> PathBuf {
    let mut unc = format!("{UNC_PREFIX}{WSL_HOST}{WINDOWS_SEPARATOR}{distro}");
    for part in linux.split(LINUX_SEPARATOR).filter(|part| !part.is_empty()) {
        unc.push(WINDOWS_SEPARATOR);
        unc.push_str(part);
    }
    PathBuf::from(unc)
}

#[must_use]
pub fn wsl_linux_path(distro: &str, path: &Path) -> Option<String> {
    if let Some(inside) = parse_wsl_path(path) {
        return (inside.distro.eq_ignore_ascii_case(distro)).then_some(inside.linux);
    }
    let text = path.to_string_lossy();
    let mut chars = text.chars();
    let drive = chars.next().filter(char::is_ascii_alphabetic)?;
    if chars.next() != Some(DRIVE_SEPARATOR) {
        return None;
    }
    let mut linux = format!(
        "{DRIVE_MOUNT}{LINUX_SEPARATOR}{}",
        drive.to_ascii_lowercase()
    );
    for part in chars
        .as_str()
        .split([WINDOWS_SEPARATOR, LINUX_SEPARATOR])
        .filter(|part| !part.is_empty())
    {
        linux.push(LINUX_SEPARATOR);
        linux.push_str(part);
    }
    Some(linux)
}

#[must_use]
pub fn wsl_windows_path(distro: &str, linux: &str) -> PathBuf {
    let drive_path = linux
        .strip_prefix(DRIVE_MOUNT)
        .and_then(|rest| rest.strip_prefix(LINUX_SEPARATOR))
        .and_then(|rest| {
            let (drive, inside) = rest.split_once(LINUX_SEPARATOR).unwrap_or((rest, ""));
            let mut letters = drive.chars();
            let letter = letters.next().filter(char::is_ascii_alphabetic)?;
            letters.next().is_none().then_some((letter, inside))
        });
    let Some((drive, inside)) = drive_path else {
        return wsl_unc_path(distro, linux);
    };
    let mut windows = format!("{}{DRIVE_SEPARATOR}", drive.to_ascii_uppercase());
    for part in inside
        .split(LINUX_SEPARATOR)
        .filter(|part| !part.is_empty())
    {
        windows.push(WINDOWS_SEPARATOR);
        windows.push_str(part);
    }
    if windows.ends_with(DRIVE_SEPARATOR) {
        windows.push(WINDOWS_SEPARATOR);
    }
    PathBuf::from(windows)
}

#[must_use]
pub fn wsl_login_exec_args(distro: &str, linux_dir: &str) -> Vec<String> {
    [
        DISTRO_FLAG,
        distro,
        CD_FLAG,
        linux_dir,
        EXEC_FLAG,
        POSIX_SHELL,
        POSIX_SHELL_COMMAND_FLAG,
        WSL_LOGIN_SHELL_SCRIPT,
        POSIX_SHELL,
        EXEC_ARGUMENTS,
    ]
    .map(str::to_owned)
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unc_paths_map_to_distro_and_linux_path_and_back() {
        let parsed = parse_wsl_path(Path::new(r"\\wsl.localhost\Ubuntu\home\dev\api"));
        assert_eq!(
            parsed,
            Some(WslPath {
                distro: "Ubuntu".to_owned(),
                linux: "/home/dev/api".to_owned(),
            })
        );
        assert_eq!(
            parse_wsl_path(Path::new("//wsl$/Debian/")).map(|path| path.linux),
            Some("/".to_owned())
        );
        assert_eq!(parse_wsl_path(Path::new(r"C:\Users\dev")), None);
        assert_eq!(parse_wsl_path(Path::new(r"\\server\share\x")), None);
        assert_eq!(
            wsl_unc_path("Ubuntu", "/home/dev/api"),
            PathBuf::from(r"\\wsl.localhost\Ubuntu\home\dev\api")
        );
    }

    #[test]
    fn windows_paths_are_written_as_the_distro_sees_them() {
        let share = Path::new(r"\\wsl.localhost\Ubuntu\home\dev\notes.md");
        assert_eq!(
            wsl_linux_path("Ubuntu", share),
            Some("/home/dev/notes.md".to_owned())
        );
        assert_eq!(wsl_linux_path("Debian", share), None);
        assert_eq!(
            wsl_linux_path("Ubuntu", Path::new(r"C:\Users\dev\spec.pdf")),
            Some("/mnt/c/Users/dev/spec.pdf".to_owned())
        );
        assert_eq!(
            wsl_linux_path("Ubuntu", Path::new(r"\\server\share\x")),
            None
        );
    }

    #[test]
    fn paths_the_distro_names_are_opened_as_windows_sees_them() {
        assert_eq!(
            wsl_windows_path("Ubuntu", "/home/dev/api/src/lib.rs"),
            PathBuf::from(r"\\wsl.localhost\Ubuntu\home\dev\api\src\lib.rs")
        );
        assert_eq!(
            wsl_windows_path("Ubuntu", "/mnt/c/Users/dev/spec.pdf"),
            PathBuf::from(r"C:\Users\dev\spec.pdf")
        );
        assert_eq!(wsl_windows_path("Ubuntu", "/mnt/d"), PathBuf::from(r"D:\"));
        assert_eq!(
            wsl_windows_path("Ubuntu", "/mnt/wsl/shared"),
            PathBuf::from(r"\\wsl.localhost\Ubuntu\mnt\wsl\shared")
        );
        let windows = Path::new(r"C:\Users\dev\spec.pdf");
        assert_eq!(
            wsl_linux_path(
                "Ubuntu",
                &wsl_windows_path("Ubuntu", "/mnt/c/Users/dev/spec.pdf")
            ),
            wsl_linux_path("Ubuntu", windows)
        );
    }
}
