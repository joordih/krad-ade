use std::time::Duration;

use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, FontWeight, InteractiveElement,
    IntoElement, ParentElement, StatefulInteractiveElement, Styled, Task, Window,
};
use krad_domain::AppUpdateCheck;
use krad_update::{
    latest_update, AfterInstall, AppVersion, AvailableUpdate, Installation, Reach, StagedUpdate,
    UpdateError,
};

use super::EnvironmentShell;
use crate::components::{icon, IconName};
use crate::theme::{palette, ICON_SM, LINE_SM, RADIUS_MD, SPACE_1, SPACE_2, TEXT_SM};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
const CHECK_INTERVAL: Duration = Duration::from_hours(6);
const PILL_ID: &str = "app-update-pill";
const RESTART_LABEL: &str = "Restart to update";
const AVAILABLE_LABEL: &str = "Update available";
const UP_TO_DATE_TOAST: &str = "Krad ADE is up to date";
const NOT_INSTALLED_TOAST: &str = "Only installed releases of Krad ADE update themselves";
const CHECK_FAILED_TOAST: &str = "Couldn't check for updates";
const DOWNLOAD_FAILED_TOAST: &str = "Couldn't download the update";
const INSTALL_FAILED_TOAST: &str = "Couldn't install the update";

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckOrigin {
    Schedule,
    Asked,
}

#[derive(Default)]
pub(super) enum AppUpdateState {
    #[default]
    Idle,
    Checking,
    Downloading(AppVersion),
    Ready(StagedUpdate),
    Announced {
        version: AppVersion,
        page: String,
    },
    Installing,
}

#[derive(Default)]
pub(super) struct AppUpdates {
    state: AppUpdateState,
    schedule: Option<Task<()>>,
}

enum CheckOutcome {
    NotInstalled,
    UpToDate,
    ReadOnly(AvailableUpdate),
    Downloadable(AvailableUpdate),
}

fn check_release() -> Result<CheckOutcome, UpdateError> {
    let Some(installation) = Installation::detect()? else {
        return Ok(CheckOutcome::NotInstalled);
    };
    let Some(update) = latest_update(&installation)? else {
        return Ok(CheckOutcome::UpToDate);
    };
    Ok(match installation.reach()? {
        Reach::Replaceable => CheckOutcome::Downloadable(update),
        Reach::ReadOnly => CheckOutcome::ReadOnly(update),
    })
}

fn available_toast(version: AppVersion) -> String {
    format!("Krad ADE {version} is available")
}

fn downloading_toast(version: AppVersion) -> String {
    format!("Krad ADE {version} is already downloading")
}

fn ready_toast(version: AppVersion) -> String {
    format!("Krad ADE {version} is ready: restart to update")
}

fn check_notice(
    outcome: &Result<CheckOutcome, UpdateError>,
    origin: CheckOrigin,
    announced: Option<AppVersion>,
) -> Option<String> {
    let asked = origin == CheckOrigin::Asked;
    match outcome {
        Ok(CheckOutcome::ReadOnly(update)) if asked || announced != Some(update.version) => {
            Some(available_toast(update.version))
        }
        Ok(CheckOutcome::NotInstalled) if asked => Some(NOT_INSTALLED_TOAST.to_owned()),
        Ok(CheckOutcome::UpToDate) if asked => Some(UP_TO_DATE_TOAST.to_owned()),
        Err(error) if asked => Some(format!("{CHECK_FAILED_TOAST}: {error}")),
        Ok(_) | Err(_) => None,
    }
}

impl EnvironmentShell {
    pub(super) fn start_app_updates(&mut self, cx: &mut Context<Self>) {
        if self.settings.app_updates != AppUpdateCheck::Automatic {
            self.app_updates.schedule = None;
            return;
        }
        self.app_updates.schedule = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK_DELAY).await;
            loop {
                let checked = this.update(cx, |shell, cx| {
                    shell.check_app_update(CheckOrigin::Schedule, cx);
                });
                if checked.is_err() {
                    break;
                }
                cx.background_executor().timer(CHECK_INTERVAL).await;
            }
        }));
    }

    pub(super) fn set_app_update_check(&mut self, check: AppUpdateCheck, cx: &mut Context<Self>) {
        self.settings.app_updates = check;
        self.save_settings(cx);
        self.start_app_updates(cx);
        cx.notify();
    }

    pub(super) fn ask_for_app_update(&mut self, cx: &mut Context<Self>) {
        self.check_app_update(CheckOrigin::Asked, cx);
    }

    fn check_app_update(&mut self, origin: CheckOrigin, cx: &mut Context<Self>) {
        match &self.app_updates.state {
            AppUpdateState::Checking | AppUpdateState::Installing => return,
            AppUpdateState::Downloading(version) => {
                if origin == CheckOrigin::Asked {
                    self.toast(&downloading_toast(*version), cx);
                }
                return;
            }
            AppUpdateState::Ready(staged) => {
                if origin == CheckOrigin::Asked {
                    self.toast(&ready_toast(staged.version), cx);
                }
                return;
            }
            AppUpdateState::Idle | AppUpdateState::Announced { .. } => {}
        }
        let announced = match &self.app_updates.state {
            AppUpdateState::Announced { version, .. } => Some(*version),
            _ => None,
        };
        self.app_updates.state = AppUpdateState::Checking;
        let checking = cx.background_spawn(async move { check_release() });
        cx.spawn(async move |this, cx| {
            let outcome = checking.await;
            this.update(cx, |shell, cx| {
                shell.finish_check(outcome, origin, announced, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn finish_check(
        &mut self,
        outcome: Result<CheckOutcome, UpdateError>,
        origin: CheckOrigin,
        announced: Option<AppVersion>,
        cx: &mut Context<Self>,
    ) {
        self.app_updates.state = AppUpdateState::Idle;
        if let Some(notice) = check_notice(&outcome, origin, announced) {
            self.toast(&notice, cx);
        }
        match outcome {
            Ok(CheckOutcome::ReadOnly(update)) => {
                self.app_updates.state = AppUpdateState::Announced {
                    version: update.version,
                    page: update.page,
                };
            }
            Ok(CheckOutcome::Downloadable(update)) => self.download_app_update(update, cx),
            Ok(CheckOutcome::NotInstalled | CheckOutcome::UpToDate) | Err(_) => {}
        }
        cx.notify();
    }

    fn download_app_update(&mut self, update: AvailableUpdate, cx: &mut Context<Self>) {
        self.app_updates.state = AppUpdateState::Downloading(update.version);
        let downloading = cx.background_spawn(async move { update.download() });
        cx.spawn(async move |this, cx| {
            let staged = downloading.await;
            this.update(cx, |shell, cx| {
                match staged {
                    Ok(staged) => {
                        shell.toast(&ready_toast(staged.version), cx);
                        shell.app_updates.state = AppUpdateState::Ready(staged);
                    }
                    Err(error) => {
                        shell.app_updates.state = AppUpdateState::Idle;
                        shell.toast(&format!("{DOWNLOAD_FAILED_TOAST}: {error}"), cx);
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) const fn app_update_ready(&self) -> bool {
        matches!(self.app_updates.state, AppUpdateState::Ready(_))
    }

    pub(super) fn install_app_update_then_quit(
        &mut self,
        after: AfterInstall,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = std::mem::replace(&mut self.app_updates.state, AppUpdateState::Installing);
        let AppUpdateState::Ready(staged) = state else {
            self.app_updates.state = state;
            window.remove_window();
            cx.quit();
            return;
        };
        let handle = window.window_handle();
        let installing = cx.background_spawn(async move {
            let installed = staged.install(after);
            (staged, installed)
        });
        cx.spawn(async move |this, cx| {
            let (staged, installed) = installing.await;
            let failed = installed.err();
            let quit = this.update(cx, |shell, cx| {
                let Some(error) = failed else {
                    return true;
                };
                shell.app_updates.state = AppUpdateState::Ready(staged);
                shell.toast(&format!("{INSTALL_FAILED_TOAST}: {error}"), cx);
                let quit_anyway = after == AfterInstall::Exit;
                if !quit_anyway {
                    shell.cancel_close(cx);
                }
                quit_anyway
            });
            if !quit.unwrap_or(true) {
                return;
            }
            handle
                .update(cx, |_, window, _| window.remove_window())
                .unwrap_or_default();
            cx.update(|cx| cx.quit());
        })
        .detach();
    }

    pub(super) fn app_update_pill(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pal = palette(cx);
        let (label, glyph) = match &self.app_updates.state {
            AppUpdateState::Ready(_) => (RESTART_LABEL, IconName::Refresh),
            AppUpdateState::Announced { .. } => (AVAILABLE_LABEL, IconName::OpenExternal),
            AppUpdateState::Idle
            | AppUpdateState::Checking
            | AppUpdateState::Downloading(_)
            | AppUpdateState::Installing => return None,
        };
        Some(
            div()
                .id(PILL_ID)
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(px(SPACE_1))
                .px(px(SPACE_2))
                .rounded(px(RADIUS_MD))
                .bg(pal.bg_selected)
                .hover(|style| style.bg(pal.bg_hover))
                .cursor_pointer()
                .occlude()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(pal.text_default))
                .child(icon(glyph, ICON_SM, pal.text_muted))
                .child(label)
                .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                    this.open_app_update(window, cx);
                }))
                .into_any_element(),
        )
    }

    fn open_app_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.app_updates.state {
            AppUpdateState::Ready(_) => self.restart_to_update(window, cx),
            AppUpdateState::Announced { page, .. } => cx.open_url(page),
            AppUpdateState::Idle
            | AppUpdateState::Checking
            | AppUpdateState::Downloading(_)
            | AppUpdateState::Installing => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_checks_stay_quiet_unless_there_is_news() {
        for outcome in [
            Ok(CheckOutcome::NotInstalled),
            Ok(CheckOutcome::UpToDate),
            Err(UpdateError::NoBuild),
        ] {
            assert_eq!(check_notice(&outcome, CheckOrigin::Schedule, None), None);
        }
    }

    #[test]
    fn an_asked_check_always_answers() {
        assert_eq!(
            check_notice(&Ok(CheckOutcome::UpToDate), CheckOrigin::Asked, None).as_deref(),
            Some(UP_TO_DATE_TOAST)
        );
        assert_eq!(
            check_notice(&Ok(CheckOutcome::NotInstalled), CheckOrigin::Asked, None).as_deref(),
            Some(NOT_INSTALLED_TOAST)
        );
        let failed = check_notice(&Err(UpdateError::NoBuild), CheckOrigin::Asked, None);
        assert!(failed.is_some_and(|text| text.starts_with(CHECK_FAILED_TOAST)));
    }
}
