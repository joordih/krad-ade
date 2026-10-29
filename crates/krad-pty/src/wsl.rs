const WSL_PROGRAM: &str = "wsl.exe";
const DISTRIBUTION_FLAG: &str = "--distribution";
const CD_FLAG: &str = "--cd";
const HOME_DIR: &str = "~";
const EXEC_FLAG: &str = "-e";
const BASH_PROGRAM: &str = "bash";

#[must_use]
pub fn wsl_shell_in(distribution: &str, linux_dir: &str) -> (String, Vec<String>) {
    (
        WSL_PROGRAM.to_owned(),
        vec![
            DISTRIBUTION_FLAG.to_owned(),
            distribution.to_owned(),
            CD_FLAG.to_owned(),
            linux_dir.to_owned(),
        ],
    )
}

#[must_use]
pub fn wsl_shell(distribution: Option<&str>) -> (String, Vec<String>) {
    let mut args = Vec::new();
    if let Some(distro) = distribution {
        args.push(DISTRIBUTION_FLAG.to_owned());
        args.push(distro.to_owned());
    }
    args.extend([HOME_DIR, EXEC_FLAG, BASH_PROGRAM].map(str::to_owned));
    (WSL_PROGRAM.to_owned(), args)
}
