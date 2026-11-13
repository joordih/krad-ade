#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
#[cfg(windows)]
mod rounded_corners;

use gpui_kit::component::command::CommandState;
use gpui_kit::component::TitleBar;
use gpui_kit::{application, init, open_window, AppContext, WindowDecorations, WindowOptions};
use krad_agents::CliPlace;
use krad_runtime::{
    BusSetup, Clock, EnvironmentsSetup, EventsSetup, KraddiesSetup, Runtime, RuntimeSetup,
    SettingsSetup,
};
use krad_store::{
    config_dir, open_layout, read_settings, read_user_themes, LayoutOpening, OpenedLayout, APP_NAME,
};
use krad_ui::{
    active_appearance, apply_editor_font, apply_interface_font, apply_settings_theme,
    apply_window_material, apply_window_tint, http_client, initial_window_bounds,
    install_app_menus, install_theme_library, key_bindings, local_moment, login_shell_env,
    match_window_frame, register_fonts, title_bar_options, window_background, AppAssets,
    Appearance, EnvironmentShell, HostTools,
};

const APP_TITLE: &str = "Krad ADE";
const APP_IDENTIFIER: &str = "com.joordih.krad-ade";

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

fn main() {
    let log_guard = logging::start(config_dir().as_deref());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "krad-ade starting");
    for (name, value) in login_shell_env() {
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
    let runtime = match Runtime::start(RuntimeSetup {
        bus,
        events: Some(EventsSetup::in_config_folder()),
        environments,
        settings: SettingsSetup::in_config_folder(settings.clone()),
        kraddies: KraddiesSetup::in_config_folder(Clock::new(local_moment)),
    }) {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start the runtime");
            std::process::exit(1);
        }
    };
    application()
        .with_assets(AppAssets)
        .with_http_client(http_client())
        .run(move |cx| {
            init(cx);
            cx.set_app_identity(APP_IDENTIFIER, APP_TITLE);
            drop(register_fonts(cx));
            install_theme_library(user_themes, cx);
            apply_settings_theme(&settings, cx);
            apply_window_material(settings.window_material, cx);
            apply_window_tint(settings.window_tint, cx);
            apply_editor_font(&settings.editor_font, cx);
            apply_interface_font(&settings.interface_font, cx);
            let dark_frame = active_appearance(cx) == Appearance::Dark;
            cx.bind_keys(key_bindings());
            install_app_menus(cx);
            let mut titlebar = title_bar_options();
            titlebar.title = Some(APP_TITLE.into());
            let options = WindowOptions {
                titlebar: Some(titlebar),
                app_id: Some(APP_NAME.to_owned()),
                window_decorations: Some(WindowDecorations::Client),
                window_background: window_background(settings.window_material, dark_frame),
                window_bounds: Some(initial_window_bounds(cx)),
                ..TitleBar::window_options()
            };
            match open_window(options, cx, |window, cx| {
                let view = cx.new(|cx| {
                    EnvironmentShell::with_seed(cx, layout, settings, host_tools, runtime)
                });
                let palette = cx.new(|cx| CommandState::new(window, cx));
                let weak = view.downgrade();
                let veto = view.downgrade();
                view.update(cx, |shell, cx| {
                    shell.palette = Some(palette);
                    shell.self_ref = Some(weak);
                    shell.focus_canvas(window, cx);
                    shell.follow_window_activation(window, cx);
                    shell.apply_performance_mode(cx);
                });
                window.on_window_should_close(cx, move |window, cx| {
                    veto.update(cx, |shell, cx| shell.should_close(window, cx))
                        .unwrap_or(true)
                });
                view
            }) {
                Ok((_handle, _view)) => {
                    match_window_frame(dark_frame);
                    #[cfg(windows)]
                    rounded_corners::round_window_corners();
                }
                Err(error) => {
                    tracing::error!(%error, "could not open the main window");
                    std::process::exit(1);
                }
            }
        });
    drop(log_guard);
}
