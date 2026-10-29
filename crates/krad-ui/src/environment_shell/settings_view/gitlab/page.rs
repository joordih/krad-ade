use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, ClipboardItem, Context, Entity, FontWeight, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled,
};
use krad_domain::{relative_age, GitlabSignIn};
use krad_gitlab::{REDIRECT_URI, SCOPE};
use krad_store::LoginPlace;
use std::time::Duration;

use super::super::{choice_card, hint, CHOICE_GAP, CHOICE_PAD};
use super::remotes::RemoteMatch;
use super::{Connected, GitlabStatus, GitlabUi};
use crate::components::{chip, icon, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    palette, row_shadow, tinted, Palette, ICON, ICON_SM, INPUT_H, LINE_SM, LINE_XS, RADIUS_SM,
    RADIUS_XL, SPACE_0_5, SPACE_1, SPACE_2, SPACE_3, TEXT_SM, TEXT_XS,
};

const CHECKING_TITLE: &str = "Checking the GitLab login…";
const SIGNED_OUT_TITLE: &str = "Not signed in";
const SIGNED_OUT_DETAIL: &str = "Sign in to list merge requests and review them here.";
const UNREACHABLE_TITLE: &str = "Signed in";
const UNREACHABLE_DETAIL: &str = "GitLab could not be reached to confirm the login";
const BROWSER_TITLE: &str = "Finish signing in in the browser";
const BROWSER_DETAIL: &str = "GitLab opened in your browser and returns here once you approve.";
const CODE_TITLE: &str = "Waiting for the code";
const CODE_DETAIL: &str = "Enter the code on GitLab to finish signing in.";
const KEYRING_PLACE: &str = "Tokens in the system keyring";
const CONFIG_FOLDER_PLACE: &str = "Tokens in the config folder";
const REFRESHED_NOW: &str = "refreshed just now";
const DETAIL_SEPARATOR: &str = " · ";
const JUST_NOW: Duration = Duration::from_secs(60);
const SIGN_IN_LABEL: &str = "Sign in to GitLab";
const DISCONNECT_LABEL: &str = "Disconnect";
const CANCEL_LABEL: &str = "Cancel";
const OAUTH_TITLE: &str = "OAuth application";
const OAUTH_HINT: &str = "Register a non-confidential application on your instance, with the redirect URI and scope below, and paste its Application ID.";
const HOST_LABEL: &str = "Host";
const CLIENT_ID_LABEL: &str = "Application ID";
const REDIRECT_LABEL: &str = "Redirect URI";
const SCOPE_LABEL: &str = "Scope";
const SCOPE_NOTE: &str = "GitLab has no narrower scope that can comment, approve and merge.";
const METHOD_TITLE: &str = "Sign-in method";
const BROWSER_METHOD: &str = "Browser (PKCE)";
const BROWSER_METHOD_DETAIL: &str =
    "Opens GitLab in your browser and returns to the app through the loopback redirect.";
const DEVICE_METHOD: &str = "Device code";
const DEVICE_METHOD_DETAIL: &str = "Type a short code on GitLab from any browser. For WSL or when the browser cannot reach the app. GitLab 17.9 or newer.";
const PROJECTS_TITLE: &str = "Projects found from git remote";
const PROJECTS_LOADING: &str = "Reading the groups' remotes…";
const PROJECTS_EMPTY: &str = "No groups in this environment.";
pub(in crate::environment_shell::settings_view) const KEYRING_NOTE: &str = "Without a keyring, as under WSL with no Secret Service, tokens are kept in the config folder, readable only by you.";
const COPIED_TOAST: &str = "Copied";
const COPY_TOOLTIP: &str = "Copy";
const BTN_SIGN_IN: &str = "gitlab-sign-in";
const BTN_DISCONNECT: &str = "gitlab-disconnect";
const BTN_CANCEL: &str = "gitlab-cancel-sign-in";
const BTN_COPY_REDIRECT: &str = "gitlab-copy-redirect";
const METHOD_PREFIX: &str = "gitlab-method";
const STATUS_ICON_BOX: f32 = 32.0;
const STATUS_TINT_ALPHA: u32 = 0x24;
const FIELD_LABEL_W: f32 = 134.0;
const PROJECT_NAME_W: f32 = 118.0;
const PROJECT_ROW_H: f32 = 32.0;
const PROJECT_GAP: f32 = 10.0;
const FIELD_GAP: f32 = 10.0;

const fn method_title(method: GitlabSignIn) -> &'static str {
    match method {
        GitlabSignIn::Browser => BROWSER_METHOD,
        GitlabSignIn::DeviceCode => DEVICE_METHOD,
    }
}

const fn method_detail(method: GitlabSignIn) -> &'static str {
    match method {
        GitlabSignIn::Browser => BROWSER_METHOD_DETAIL,
        GitlabSignIn::DeviceCode => DEVICE_METHOD_DETAIL,
    }
}

const fn place_text(place: LoginPlace) -> &'static str {
    match place {
        LoginPlace::Keyring => KEYRING_PLACE,
        LoginPlace::ConfigFolder => CONFIG_FOLDER_PLACE,
    }
}

fn refreshed_text(age: Duration) -> String {
    if age < JUST_NOW {
        REFRESHED_NOW.to_owned()
    } else {
        format!("refreshed {} ago", relative_age(age))
    }
}

struct StatusCopy {
    icon: IconName,
    success: bool,
    title: String,
    detail: String,
    lock: bool,
}

fn status_copy(status: &GitlabStatus) -> StatusCopy {
    let waiting = |title: &str, detail: &str| StatusCopy {
        icon: IconName::Loading,
        success: false,
        title: title.to_owned(),
        detail: detail.to_owned(),
        lock: false,
    };
    match status {
        GitlabStatus::Checking => waiting(CHECKING_TITLE, ""),
        GitlabStatus::WaitingForBrowser => waiting(BROWSER_TITLE, BROWSER_DETAIL),
        GitlabStatus::WaitingForCode(_) => waiting(CODE_TITLE, CODE_DETAIL),
        GitlabStatus::SignedOut => StatusCopy {
            icon: IconName::Key,
            success: false,
            title: SIGNED_OUT_TITLE.to_owned(),
            detail: SIGNED_OUT_DETAIL.to_owned(),
            lock: false,
        },
        GitlabStatus::SignedIn(connected) => connected_copy(connected),
    }
}

fn connected_copy(connected: &Connected) -> StatusCopy {
    let place = place_text(connected.place);
    let (title, detail) = match &connected.username {
        Some(username) => (
            format!("Connected as @{username}"),
            format!(
                "{place}{DETAIL_SEPARATOR}{}",
                refreshed_text(connected.checked_at.elapsed())
            ),
        ),
        None => (
            UNREACHABLE_TITLE.to_owned(),
            format!("{place}{DETAIL_SEPARATOR}{UNREACHABLE_DETAIL}"),
        ),
    };
    StatusCopy {
        icon: IconName::CheckCircle,
        success: true,
        title,
        detail,
        lock: true,
    }
}

impl EnvironmentShell {
    pub(in crate::environment_shell::settings_view) fn status_card(
        status: &GitlabStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let copy = status_copy(status);
        let icon_box = status_mark(copy.icon, copy.success, pal, cx);
        let action = match status {
            GitlabStatus::Checking => None,
            GitlabStatus::SignedOut => Some(
                Button::new(BTN_SIGN_IN)
                    .label(SIGN_IN_LABEL)
                    .small()
                    .variant(ButtonVariant::Primary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.sign_in_gitlab(this.settings.gitlab.sign_in, cx);
                    })),
            ),
            GitlabStatus::SignedIn(_) => Some(
                Button::new(BTN_DISCONNECT)
                    .label(DISCONNECT_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| this.sign_out_gitlab(cx))),
            ),
            GitlabStatus::WaitingForBrowser | GitlabStatus::WaitingForCode(_) => Some(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.cancel_gitlab_sign_in(cx);
                    })),
            ),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_3))
            .p(px(CHOICE_PAD))
            .rounded(px(RADIUS_XL))
            .bg(rgb(pal.bg_panel))
            .child(icon_box)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(SPACE_0_5))
                    .child(
                        div()
                            .text_size(px(TEXT_SM))
                            .line_height(px(LINE_SM))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(copy.title),
                    )
                    .when(!copy.detail.is_empty(), |text| {
                        text.child(status_detail(&copy.detail, copy.lock, pal))
                    }),
            )
            .children(action.map(|action| div().flex_none().child(action)))
            .into_any_element()
    }

    pub(in crate::environment_shell::settings_view) fn oauth_section(
        gitlab: &GitlabUi,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let input = |field: &Entity<InputState>| Input::new(field).into_any_element();
        let redirect = field_box(pal)
            .bg(pal.bg_hover)
            .font_family(mono.clone())
            .child(div().flex_1().child(REDIRECT_URI))
            .child(
                Button::new(BTN_COPY_REDIRECT)
                    .icon(IconName::Copy)
                    .small()
                    .tooltip(COPY_TOOLTIP)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.copy_to_clipboard(REDIRECT_URI, cx);
                    })),
            )
            .into_any_element();
        let scope = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(chip(SCOPE, pal).font_family(mono))
            .child(hint(SCOPE_NOTE, pal))
            .into_any_element();
        div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(section_heading(OAUTH_TITLE, Some(OAUTH_HINT), pal))
            .child(field_row(HOST_LABEL, input(&gitlab.host), pal))
            .child(field_row(CLIENT_ID_LABEL, input(&gitlab.client_id), pal))
            .child(field_row(REDIRECT_LABEL, redirect, pal))
            .child(field_row(SCOPE_LABEL, scope, pal))
            .into_any_element()
    }

    pub(in crate::environment_shell::settings_view) fn method_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let chosen = self.settings.gitlab.sign_in;
        let cards = GitlabSignIn::ALL.map(|method| {
            choice_card(
                (METHOD_PREFIX, method as u64),
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(method_title(method)),
                method_detail(method),
                method == chosen,
                pal,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.set_gitlab_sign_in(method, cx);
            }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(FIELD_GAP))
            .child(section_heading(METHOD_TITLE, None, pal))
            .child(div().flex().flex_row().gap(px(CHOICE_GAP)).children(cards))
            .into_any_element()
    }

    pub(in crate::environment_shell::settings_view) fn projects_card(
        &self,
        gitlab: &GitlabUi,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let host = &self.settings.gitlab.host;
        let rows: Vec<AnyElement> = match &gitlab.remotes {
            None => vec![hint(PROJECTS_LOADING, pal)],
            Some(remotes) if remotes.is_empty() => vec![hint(PROJECTS_EMPTY, pal)],
            Some(remotes) => remotes
                .iter()
                .map(|remote| {
                    let found = RemoteMatch::of(&remote.origin, host);
                    let is_project = found.is_project();
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(PROJECT_GAP))
                        .min_h(px(PROJECT_ROW_H))
                        .text_size(px(TEXT_SM))
                        .line_height(px(LINE_SM))
                        .child(icon(IconName::Folder, ICON_SM, pal.text_muted))
                        .child(
                            div()
                                .w(px(PROJECT_NAME_W))
                                .flex_none()
                                .truncate()
                                .text_color(rgb(pal.text_strong))
                                .child(remote.name.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .when(is_project, |text| text.text_color(rgb(pal.text_default)))
                                .when(!is_project, |text| text.text_color(rgb(pal.text_subtle)))
                                .child(found.to_string()),
                        )
                        .into_any_element()
                })
                .collect(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_1))
            .p(px(CHOICE_PAD))
            .rounded(px(RADIUS_XL))
            .bg(rgb(pal.bg_panel))
            .child(
                div()
                    .pb(px(SPACE_1))
                    .text_size(px(TEXT_SM))
                    .line_height(px(LINE_SM))
                    .text_color(rgb(pal.text_default))
                    .child(PROJECTS_TITLE),
            )
            .children(rows)
            .into_any_element()
    }

    pub(in crate::environment_shell::settings_view) fn copy_to_clipboard(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        self.toast(COPIED_TOAST, cx);
    }
}

pub(in crate::environment_shell::settings_view) fn status_mark(
    mark: IconName,
    success: bool,
    pal: &Palette,
    cx: &gpui_kit::App,
) -> gpui_kit::Div {
    let icon_box = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(STATUS_ICON_BOX))
        .rounded(px(RADIUS_SM));
    if success {
        return icon_box
            .bg(tinted(pal.success_fg, STATUS_TINT_ALPHA))
            .child(icon(mark, ICON, pal.success_fg));
    }
    icon_box
        .bg(rgb(pal.bg_surface))
        .shadow(row_shadow(pal, cx))
        .child(icon(mark, ICON, pal.text_default))
}

pub(in crate::environment_shell::settings_view) fn status_detail(
    detail: &str,
    lock: bool,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(px(SPACE_1))
        .min_w_0()
        .when(lock, |line| {
            line.child(icon(IconName::Lock, ICON_SM, pal.text_muted))
        })
        .child(div().flex_1().min_w_0().child(hint(detail, pal)))
        .into_any_element()
}

pub(in crate::environment_shell::settings_view) fn section_heading(
    title: &str,
    detail: Option<&str>,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_0_5))
        .child(
            div()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_strong))
                .child(SharedString::from(title.to_owned())),
        )
        .children(detail.map(|detail| hint(detail, pal)))
        .into_any_element()
}

pub(super) fn field_box(pal: &Palette) -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(SPACE_2))
        .h(px(INPUT_H))
        .pl(px(FIELD_GAP))
        .pr(px(SPACE_1))
        .rounded(px(RADIUS_SM))
        .text_size(px(TEXT_XS))
        .line_height(px(LINE_XS))
        .text_color(rgb(pal.text_default))
}

pub(in crate::environment_shell::settings_view) fn field_row(
    label: &str,
    field: AnyElement,
    pal: &Palette,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .child(
            div()
                .w(px(FIELD_LABEL_W))
                .flex_none()
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_default))
                .child(SharedString::from(label.to_owned())),
        )
        .child(div().flex_1().min_w_0().child(field))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_connected_login_names_the_user_and_where_its_tokens_are() {
        let copy = connected_copy(&Connected {
            username: Some("ana".to_owned()),
            place: LoginPlace::Keyring,
            checked_at: Instant::now(),
        });
        assert_eq!(copy.title, "Connected as @ana");
        assert_eq!(
            copy.detail,
            "Tokens in the system keyring · refreshed just now"
        );
        assert_eq!(refreshed_text(Duration::from_mins(12)), "refreshed 12m ago");
    }
}
