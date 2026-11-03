use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

const LOG_DIR: &str = "logs";
const LOG_PREFIX: &str = "krad-ade";
const LOG_SUFFIX: &str = "log";
const KEPT_LOG_FILES: usize = 7;
const FILTER_VAR: &str = "KRAD_ADE_LOG";
const DEFAULT_FILTER: &str = "info";

#[must_use]
pub fn start(config_dir: Option<&Path>) -> Option<WorkerGuard> {
    let dir = config_dir?.join(LOG_DIR);
    std::fs::create_dir_all(&dir).ok()?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .filename_suffix(LOG_SUFFIX)
        .max_log_files(KEPT_LOG_FILES)
        .build(dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter =
        EnvFilter::try_from_env(FILTER_VAR).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .ok()?;
    Some(guard)
}
