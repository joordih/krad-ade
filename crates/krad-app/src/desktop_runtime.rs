use krad_agents::CliPlace;
use krad_runtime::{
    BusSetup, Clock, EnvironmentsSetup, EventsSetup, HostTools, KraddiesSetup, Runtime,
    RuntimeHandle, RuntimeSetup, SettingsSetup,
};
use krad_store::{
    config_dir, open_layout, read_settings, read_user_themes, ChatImages, LayoutOpening,
    OpenedLayout, Secrets,
};
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

const LOCK_FILE: &str = "runtime.lock";
const HELD: &str = "another runtime of this computer already runs; close it or stop its service";
const NO_CONFIG: &str = "there is no config folder";
#[cfg(unix)]
const BAD_TOKEN: &str = "the app's pairing with this computer's runtime is not valid";

pub(crate) struct RuntimeLock {
    _held: File,
}

pub(crate) fn config_folder() -> io::Result<PathBuf> {
    config_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NO_CONFIG))
}

/// # Errors
///
/// Fails when the lock cannot be made or another runtime of this computer holds it.
pub(crate) fn lock_runtime(config: &Path) -> io::Result<RuntimeLock> {
    std::fs::create_dir_all(config)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(config.join(LOCK_FILE))?;
    match file.try_lock() {
        Ok(()) => Ok(RuntimeLock { _held: file }),
        Err(TryLockError::WouldBlock) => Err(io::Error::new(io::ErrorKind::WouldBlock, HELD)),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn opened_layout() -> (krad_domain::Layout, EnvironmentsSetup) {
    let OpenedLayout { layout, opening } = open_layout();
    let state = layout.state.clone();
    let setup = match &opening {
        LayoutOpening::StateUnreadable { .. } => EnvironmentsSetup { state, file: None },
        _ => EnvironmentsSetup::in_config_folder(state),
    };
    match opening {
        LayoutOpening::Empty | LayoutOpening::Read => {}
        LayoutOpening::Migrated { from, .. } => {
            tracing::info!(
                from,
                "split the layout into state and view files, kept the old one"
            );
        }
        LayoutOpening::MigratedUnsaved { from, error } => {
            tracing::warn!(from, %error, "could not save the split layout; the old file stays");
        }
        LayoutOpening::SingleFileFromNewerApp { version } => {
            tracing::warn!(version, "the layout is from a newer app; left untouched");
        }
        LayoutOpening::StateSetAside { .. } => {
            tracing::warn!("the state file could not be read; kept it aside and started empty");
        }
        LayoutOpening::StateUnreadable { error } => {
            tracing::error!(%error, "could not read the state file; it will not be written");
        }
    }
    (layout, setup)
}

pub(crate) struct Started {
    pub(crate) layout: krad_domain::Layout,
    pub(crate) settings: krad_domain::Settings,
    pub(crate) user_themes: Vec<krad_domain::Theme>,
    pub(crate) host_tools: HostTools,
    pub(crate) runtime: RuntimeHandle,
}

/// # Errors
///
/// Fails when the runtime cannot start.
pub(crate) fn start_desktop_runtime(clock: Clock) -> io::Result<Started> {
    for (name, value) in krad_ui::login_shell_env() {
        std::env::set_var(name, value);
    }
    let (layout, environments) = opened_layout();
    let mut settings = read_settings();
    settings.migrate_themes();
    let user_themes = read_user_themes();
    let host_tools = HostTools::discover();
    let bus = CliPlace::find().map(|cli| BusSetup {
        cli_dir: cli.dir(),
        config_dir: config_dir(),
    });
    let runtime = Runtime::start(RuntimeSetup {
        bus,
        events: Some(EventsSetup::in_config_folder()),
        environments,
        settings: SettingsSetup::in_config_folder(settings.clone()),
        kraddies: KraddiesSetup::in_config_folder(clock),
        secrets: Secrets::system(),
        chat_images: ChatImages::in_config_folder(),
        host_tools: host_tools.clone(),
        own_runner: None,
    })?;
    Ok(Started {
        layout,
        settings,
        user_themes,
        host_tools,
        runtime,
    })
}

pub(crate) struct Seen {
    pub(crate) layout: krad_domain::Layout,
    pub(crate) settings: krad_domain::Settings,
    pub(crate) user_themes: Vec<krad_domain::Theme>,
    pub(crate) host_tools: HostTools,
}

pub(crate) fn seen_from_files() -> Seen {
    for (name, value) in krad_ui::login_shell_env() {
        std::env::set_var(name, value);
    }
    let (layout, _) = opened_layout();
    let mut settings = read_settings();
    settings.migrate_themes();
    Seen {
        layout,
        settings,
        user_themes: read_user_themes(),
        host_tools: HostTools::discover(),
    }
}

#[cfg(unix)]
/// # Errors
///
/// Fails when the service's socket or the app's pairing cannot be reached.
pub(crate) fn connect_service(config: &Path) -> io::Result<krad_client::Runner> {
    use krad_domain::Pairing;
    use krad_protocol::Hello;
    use std::os::unix::net::UnixStream;

    let token = std::fs::read_to_string(crate::service::desktop_token_file(config))?
        .trim()
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, BAD_TOKEN))?;
    let pairing = Pairing {
        install: crate::service::desktop_install()?,
        token,
    };
    let socket = krad_host::runner::runtime_socket(&crate::service::runtime_folder(config));
    let stream = UnixStream::connect(socket)?;
    let reading = stream.try_clone()?;
    krad_client::Runner::connect(
        reading,
        stream,
        Hello::paired(env!("CARGO_PKG_VERSION"), pairing),
    )
    .map_err(|error| io::Error::other(error.to_string()))
}
