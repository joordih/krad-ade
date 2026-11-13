use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, FontWeight, IntoElement, ParentElement,
    Styled,
};
use krad_domain::{CodeHost, LoginPlace, ToastKind};
use krad_protocol::{CodeHostCommand, LoginState, SignInWith};

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
}

const fn place_text(place: LoginPlace) -> &'static str {
    match place {
        LoginPlace::Keyring => KEYRING_PLACE,
        LoginPlace::ConfigFolder => CONFIG_FOLDER_PLACE,
    }
}

impl GithubStatus {
    pub(super) fn heard(state: LoginState) -> Option<Self> {
        Some(match state {
            LoginState::Checking => Self::Checking,
            LoginState::SignedOut => Self::SignedOut,
            LoginState::SignedIn { username, place } => Self::SignedIn(username, place),
            LoginState::WaitingForCode {
                user_code, page, ..
            } => Self::WaitingForCode { user_code, page },
            LoginState::WaitingForBrowser { .. } | LoginState::Unknown => return None,
        })
    }
}

impl EnvironmentShell {
    pub(super) const fn github_ui() -> GithubUi {
        GithubUi {
            status: GithubStatus::Checking,
        }
    }

    pub(super) fn github_heard(&mut self, state: LoginState, cx: &mut Context<Self>) {
        let Some(status) = GithubStatus::heard(state) else {
            return;
        };
        if let GithubStatus::WaitingForCode { user_code, page } = &status {
            cx.write_to_clipboard(ClipboardItem::new_string(user_code.clone()));
            cx.open_url(page);
            self.toast(ToastKind::Success, CODE_COPIED_TOAST, cx);
        }
        if let Some(github) = self.settings_ui.as_mut().map(|ui| &mut ui.github) {
            github.status = status;
            cx.notify();
        }
    }

    pub(super) fn refresh_github_status(&self) {
        let host = CodeHost::Github;
        self.post_code_host(CodeHostCommand::Check { host });
    }

    fn cancel_github_sign_in(&self) {
        let host = CodeHost::Github;
        self.post_code_host(CodeHostCommand::Cancel { host });
    }

    fn sign_in_github(&self) {
        let with = SignInWith::GithubDevice;
        self.post_code_host(CodeHostCommand::SignIn { with });
    }

    fn sign_out_github(&self) {
        let host = CodeHost::Github;
        self.post_code_host(CodeHostCommand::SignOut { host });
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
                .on_click(cx.listener(|this: &mut Self, _, _, _| this.sign_in_github()))
                .into_any_element()],
            GithubStatus::SignedIn(..) => vec![Button::new(BTN_DISCONNECT)
                .label(DISCONNECT_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(|this: &mut Self, _, _, _| this.sign_out_github()))
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
                        .on_click(cx.listener(|this: &mut Self, _, _, _| {
                            this.cancel_github_sign_in();
                        }))
                        .into_any_element(),
                ]
            }
        }
    }
}
