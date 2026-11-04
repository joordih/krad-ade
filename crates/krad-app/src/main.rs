#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logging;
#[cfg(windows)]
mod rounded_corners;

use gpui_kit::component::command::CommandState;
use gpui_kit::component::TitleBar;
use gpui_kit::{application, init, open_window, AppContext, WindowDecorations, WindowOptions};
use krad_store::{config_dir, read_layout_snapshot, read_settings, read_user_themes, APP_NAME};
use krad_ui::{
    active_appearance, apply_editor_font, apply_interface_font, apply_settings_theme,
    apply_window_material, apply_window_tint, http_client, initial_window_bounds,
    install_app_menus, install_theme_library, key_bindings, login_shell_env, match_window_frame,
    register_fonts, title_bar_options, window_background, AppAssets, Appearance, EnvironmentShell,
    HostTools,
};

const APP_TITLE: &str = "Krad ADE";

fn main() {
    let log_guard = logging::start(config_dir().as_deref());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "krad-ade starting");
    for (name, value) in login_shell_env() {
        std::env::set_var(name, value);
    }
    let seed = read_layout_snapshot();
    let mut settings = read_settings();
    settings.migrate_themes();
    let user_themes = read_user_themes();
    let host_tools = HostTools::discover();
    application()
        .with_assets(AppAssets)
        .with_http_client(http_client())
        .run(move |cx| {
            init(cx);
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
                let view = cx.new(|cx| EnvironmentShell::with_seed(cx, seed, settings, host_tools));
                let palette = cx.new(|cx| CommandState::new(window, cx));
                let weak = view.downgrade();
                let veto = view.downgrade();
                view.update(cx, |shell, cx| {
                    shell.palette = Some(palette);
                    shell.self_ref = Some(weak);
                    shell.focus_canvas(window, cx);
                    shell.follow_system_material(window, cx);
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
