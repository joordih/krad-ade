use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, ClipboardItem, Context, FontWeight, IntoElement,
    ParentElement, Styled,
};
use krad_github::{DeviceLogin, GithubClient, GithubError, GithubToken};
use krad_store::{delete_github_login, find_github_login, write_github_login, LoginPlace};

use super::sections::{account_row, AccountRow};
use super::EnvironmentShell;
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::palette;

const GITHUB_NAME: &str = "GitHub";
const CHECKING_TITLE: &str = "Checking the GitHub login…";
const SIGNED_OUT_TITLE: &str = "Not signed in";
const SIGNED_OUT_DETAIL: &str = "Sign in to list pull requests and review them here.";
const UNREACHABLE_TITLE: &str = "Signed in";
const UNREACHABLE_DETAIL: &str = "GitHub could not be reached to confirm the login";
const CODE_TITLE: &str = "Paste this code on GitHub";
const KEYRING_PLACE: &str = "Token in the system keyring";
const CONFIG_FOLDER_PLACE: &str = "Token in the config folder";
const DETAIL_SEPARATOR: &str = " · ";
const SIGN_IN_LABEL: &str = "Sign in to GitHub";
const DISCONNECT_LABEL: &str = "Disconnect";
const CANCEL_LABEL: &str = "Cancel";
const OPEN_PAGE_LABEL: &str = "Open GitHub";
const COPY_CODE_LABEL: &str = "Copy code";
const SIGNED_OUT_TOAST: &str = "Signed out of GitHub";
const CODE_COPIED_TOAST: &str = "Code copied. Paste it on the GitHub page that just opened.";
const BTN_SIGN_IN: &str = "github-sign-in";
const BTN_DISCONNECT: &str = "github-disconnect";
const BTN_CANCEL: &str = "github-cancel-sign-in";
const BTN_OPEN_PAGE: &str = "github-open-device-page";
const BTN_COPY_CODE: &str = "github-copy-code";
const CODE_TEXT: f32 = 24.0;
const CODE_LINE: f32 = 30.0;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum GithubStatus {
    Checking,
    SignedOut,
    SignedIn(Option<String>, LoginPlace),
    WaitingForCode { user_code: String, page: String },
}

pub(super) struct GithubUi {
    pub(super) status: GithubStatus,
    attempt: u64,
}

const fn place_text(place: LoginPlace) -> &'static str {
    match place {
        LoginPlace::Keyring => KEYRING_PLACE,
        LoginPlace::ConfigFolder => CONFIG_FOLDER_PLACE,
    }
}

fn check_login() -> Result<GithubStatus, String> {
    let Some((token, place)) = find_github_login().map_err(|error| error.to_string())? else {
        return Ok(GithubStatus::SignedOut);
    };
    match GithubClient::new(&token).current_user() {
        Ok(user) => Ok(GithubStatus::SignedIn(Some(user.username), place)),
        Err(GithubError::SignedOut) => Ok(GithubStatus::SignedOut),
        Err(_) => Ok(GithubStatus::SignedIn(None, place)),
    }
}

fn keep_login(token: &GithubToken) -> Result<String, String> {
    write_github_login(token).map_err(|error| error.to_string())?;
    GithubClient::new(token)
        .current_user()
        .map(|user| user.username)
        .map_err(|error| error.to_string())
}

impl EnvironmentShell {
    pub(super) const fn github_ui() -> GithubUi {
        GithubUi {
            status: GithubStatus::Checking,
            attempt: 0,
        }
    }

    fn github_ui_mut(&mut self) -> Option<&mut GithubUi> {
        self.settings_ui.as_mut().map(|ui| &mut ui.github)
    }

    fn set_github_status(&mut self, status: GithubStatus, cx: &mut Context<Self>) {
        if let Some(github) = self.github_ui_mut() {
            github.status = status;
            cx.notify();
        }
    }

    fn is_github_attempt(&mut self, attempt: u64) -> bool {
        self.github_ui_mut()
            .is_some_and(|github| github.attempt == attempt)
    }

    pub(super) fn refresh_github_status(&mut self, cx: &mut Context<Self>) {
        self.set_github_status(GithubStatus::Checking, cx);
        let check = cx.background_spawn(async move { check_login() });
        cx.spawn(async move |this, cx| {
            let checked = check.await;
            this.update(cx, |shell, cx| {
                let status = checked.unwrap_or_else(|error| {
                    shell.toast(&error, cx);
                    GithubStatus::SignedOut
                });
                shell.set_github_status(status, cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn cancel_github_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(github) = self.github_ui_mut() {
            github.attempt += 1;
        }
        self.refresh_github_status(cx);
    }

    fn sign_in_github(&mut self, cx: &mut Context<Self>) {
        let Some(attempt) = self.github_ui_mut().map(|github| {
            github.attempt += 1;
            github.attempt
        }) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let started = background.spawn(async move { DeviceLogin::start() }).await;
            let outcome = match started {
                Ok(login) => {
                    let waiting = GithubStatus::WaitingForCode {
                        user_code: login.user_code.clone(),
                        page: login.verification_uri.clone(),
                    };
                    let page = login.verification_uri.clone();
                    let code = login.user_code.clone();
                    this.update(cx, |shell, cx| {
                        if shell.is_github_attempt(attempt) {
                            shell.set_github_status(waiting, cx);
                            cx.write_to_clipboard(ClipboardItem::new_string(code));
                            cx.open_url(&page);
                            shell.toast(CODE_COPIED_TOAST, cx);
                        }
                    })
                    .unwrap_or_default();
                    background.spawn(async move { login.finish() }).await
                }
                Err(error) => Err(error),
            };
            let current = this
                .update(cx, |shell, _| shell.is_github_attempt(attempt))
                .unwrap_or_default();
            if !current {
                return;
            }
            let kept = match outcome {
                Ok(token) => background.spawn(async move { keep_login(&token) }).await,
                Err(error) => Err(error.to_string()),
            };
            this.update(cx, |shell, cx| {
                match kept {
                    Ok(username) => shell.toast(&format!("Signed in to GitHub as @{username}"), cx),
                    Err(text) => shell.toast(&text, cx),
                }
                shell.refresh_github_status(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn sign_out_github(cx: &mut Context<Self>) {
        let delete = cx.background_spawn(async move { delete_github_login() });
        cx.spawn(async move |this, cx| {
            let deleted = delete.await;
            this.update(cx, |shell, cx| {
                match deleted {
                    Ok(()) => shell.toast(SIGNED_OUT_TOAST, cx),
                    Err(error) => shell.toast(&error.to_string(), cx),
                }
                shell.refresh_github_status(cx);
            })
            .unwrap_or_default();
        })
        .detach();
    }

    pub(super) fn github_account(status: &GithubStatus, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let connected = matches!(status, GithubStatus::SignedIn(..));
        let (state, detail) = match status {
            GithubStatus::Checking => (CHECKING_TITLE.to_owned(), String::new()),
            GithubStatus::SignedOut => (SIGNED_OUT_TITLE.to_owned(), SIGNED_OUT_DETAIL.to_owned()),
            GithubStatus::SignedIn(Some(username), place) => (
                format!("Connected as @{username}"),
                place_text(*place).to_owned(),
            ),
            GithubStatus::SignedIn(None, place) => (
                UNREACHABLE_TITLE.to_owned(),
                format!(
                    "{}{DETAIL_SEPARATOR}{UNREACHABLE_DETAIL}",
                    place_text(*place)
                ),
            ),
            GithubStatus::WaitingForCode { page, .. } => (CODE_TITLE.to_owned(), page.clone()),
        };
        let below = match status {
            GithubStatus::WaitingForCode { user_code, .. } => Some(
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(CODE_TEXT))
                    .line_height(px(CODE_LINE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(user_code.clone())
                    .into_any_element(),
            ),
            _ => None,
        };
        let mark = match status {
            GithubStatus::Checking | GithubStatus::WaitingForCode { .. } => IconName::Loading,
            GithubStatus::SignedOut | GithubStatus::SignedIn(..) => IconName::PullRequest,
        };
        account_row(
            AccountRow {
                mark,
                name: GITHUB_NAME,
                state,
                detail,
                connected,
                actions: Self::github_actions(status, cx),
                below,
            },
            pal,
            cx,
        )
    }

    fn github_actions(status: &GithubStatus, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match status {
            GithubStatus::Checking => Vec::new(),
            GithubStatus::SignedOut => vec![Button::new(BTN_SIGN_IN)
                .label(SIGN_IN_LABEL)
                .small()
                .variant(ButtonVariant::Primary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.sign_in_github(cx)))
                .into_any_element()],
            GithubStatus::SignedIn(..) => vec![Button::new(BTN_DISCONNECT)
                .label(DISCONNECT_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(|_: &mut Self, _, _, cx| Self::sign_out_github(cx)))
                .into_any_element()],
            GithubStatus::WaitingForCode { user_code, page } => {
                let code = user_code.clone();
                let page = page.clone();
                vec![
                    Button::new(BTN_COPY_CODE)
                        .label(COPY_CODE_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.copy_to_clipboard(&code, cx);
                        }))
                        .into_any_element(),
                    Button::new(BTN_OPEN_PAGE)
                        .label(OPEN_PAGE_LABEL)
                        .small()
                        .variant(ButtonVariant::Primary)
                        .on_click(move |_, _, cx| cx.open_url(&page))
                        .into_any_element(),
                    Button::new(BTN_CANCEL)
                        .label(CANCEL_LABEL)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.cancel_github_sign_in(cx);
                        }))
                        .into_any_element(),
                ]
            }
        }
    }
}
