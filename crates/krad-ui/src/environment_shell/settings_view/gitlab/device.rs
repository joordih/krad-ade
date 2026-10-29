use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Styled,
};
use krad_domain::GitlabSignIn;
use std::time::{Duration, Instant};

use super::page::field_box;
use super::{DeviceCode, GitlabStatus};
use crate::components::{icon, solid_card, Button, ButtonVariant, IconName};
use crate::environment_shell::EnvironmentShell;
use crate::theme::{
    floating_shadow, palette, scrim_fill, tinted, Palette, ICON, ICON_SM, LINE_LG, LINE_SM,
    LINE_XS, RADIUS_LG, RADIUS_SM, SPACE_1_5, SPACE_2, SPACE_3, SPACE_5, TEXT_2XS, TEXT_LG,
    TEXT_SM, TEXT_XS,
};

const TITLE: &str = "Sign in with a code";
const STEP_OPEN: &str = "Open this page in any browser";
const STEP_CODE: &str = "Enter this code and approve krad-ade";
const WAITING: &str = "Waiting for approval…";
const EXPIRED: &str = "expired";
const NOTE: &str = "Use this when the browser cannot reach the app, as under WSL. Needs GitLab 17.9 or newer; tokens go to the system keyring once you approve.";
const USE_BROWSER_LABEL: &str = "Use the browser instead";
const CANCEL_LABEL: &str = "Cancel";
const COPY_LABEL: &str = "Copy";
const COPY_PAGE_TOOLTIP: &str = "Copy address";
const OPEN_PAGE_TOOLTIP: &str = "Open in browser";
const SCHEME_SEPARATOR: &str = "://";
const BTN_CLOSE: &str = "gitlab-device-close";
const BTN_COPY_PAGE: &str = "gitlab-device-copy-page";
const BTN_OPEN_PAGE: &str = "gitlab-device-open-page";
const BTN_COPY_CODE: &str = "gitlab-device-copy-code";
const BTN_USE_BROWSER: &str = "gitlab-device-use-browser";
const BTN_CANCEL: &str = "gitlab-device-cancel";
const DIALOG_W: f32 = 460.0;
const ICON_BOX: f32 = 30.0;
const STEP_BADGE: f32 = 20.0;
const STEP_GAP: f32 = 10.0;
const CODE_BOX_H: f32 = 64.0;
const CODE_TEXT: f32 = 28.0;
const CODE_LINE: f32 = 32.0;
const WAITING_TINT_ALPHA: u32 = 0x1f;
const SECS_PER_MINUTE: u64 = 60;

fn expiry_text(left: Duration) -> String {
    if left.is_zero() {
        return EXPIRED.to_owned();
    }
    let secs = left.as_secs();
    format!(
        "expires in {}:{:02}",
        secs / SECS_PER_MINUTE,
        secs % SECS_PER_MINUTE
    )
}

fn bare(url: &str) -> &str {
    url.split_once(SCHEME_SEPARATOR)
        .map_or(url, |(_, rest)| rest)
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn gitlab_device_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let GitlabStatus::WaitingForCode(code) = &self.settings_ui.as_ref()?.gitlab.status else {
            return None;
        };
        let pal = palette(cx);
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(scrim_fill(pal, cx))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    solid_card(pal)
                        .w(px(DIALOG_W))
                        .shadow(floating_shadow(pal))
                        .child(self.device_header(cx))
                        .child(Self::device_body(code, cx))
                        .child(Self::device_footer(cx)),
                )
                .into_any_element(),
        )
    }

    fn device_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let host = bare(self.settings.gitlab.host.trim()).to_owned();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(STEP_GAP))
            .px(px(SPACE_5))
            .pt(px(SPACE_5))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_BOX))
                    .rounded(px(RADIUS_SM))
                    .bg(rgb(pal.bg_panel))
                    .child(icon(IconName::Key, ICON, pal.text_default)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(TEXT_LG))
                            .line_height(px(LINE_LG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_strong))
                            .child(TITLE),
                    )
                    .child(
                        div()
                            .text_size(px(TEXT_XS))
                            .line_height(px(LINE_XS))
                            .text_color(rgb(pal.text_muted))
                            .child(host),
                    ),
            )
            .child(
                Button::new(BTN_CLOSE)
                    .icon(IconName::Close)
                    .tooltip(CANCEL_LABEL)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.cancel_gitlab_sign_in(cx);
                    })),
            )
            .into_any_element()
    }

    fn device_body(code: &DeviceCode, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let mono = cx.theme().mono_font_family.clone();
        let page = code.page.clone();
        let copied_page = code.page.clone();
        let user_code = code.user_code.clone();
        let page_box = field_box(pal)
            .bg(rgb(pal.bg_panel))
            .font_family(mono.clone())
            .child(div().flex_1().truncate().child(bare(&code.page).to_owned()))
            .child(
                Button::new(BTN_COPY_PAGE)
                    .icon(IconName::Copy)
                    .small()
                    .tooltip(COPY_PAGE_TOOLTIP)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.copy_to_clipboard(&copied_page, cx);
                    })),
            )
            .child(
                Button::new(BTN_OPEN_PAGE)
                    .icon(IconName::OpenExternal)
                    .small()
                    .tooltip(OPEN_PAGE_TOOLTIP)
                    .on_click(cx.listener(move |_: &mut Self, _, _, cx| cx.open_url(&page))),
            );
        let code_box = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(SPACE_3))
            .h(px(CODE_BOX_H))
            .rounded(px(RADIUS_LG))
            .bg(rgb(pal.bg_panel))
            .child(
                div()
                    .font_family(mono)
                    .text_size(px(CODE_TEXT))
                    .line_height(px(CODE_LINE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_strong))
                    .child(code.user_code.clone()),
            )
            .child(
                Button::new(BTN_COPY_CODE)
                    .icon(IconName::Copy)
                    .label(COPY_LABEL)
                    .small()
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.copy_to_clipboard(&user_code, cx);
                    })),
            );
        let left = code.expires_at.saturating_duration_since(Instant::now());
        div()
            .flex()
            .flex_col()
            .gap(px(SPACE_5))
            .px(px(SPACE_5))
            .py(px(SPACE_5))
            .child(step(1, STEP_OPEN, page_box.into_any_element(), pal))
            .child(step(2, STEP_CODE, code_box.into_any_element(), pal))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_3))
                    .py(px(STEP_GAP))
                    .rounded(px(RADIUS_SM))
                    .bg(tinted(pal.info_fg, WAITING_TINT_ALPHA))
                    .text_size(px(TEXT_XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.info_fg))
                    .child(icon(IconName::Loading, ICON_SM, pal.info_fg))
                    .child(div().flex_1().child(WAITING))
                    .child(expiry_text(left)),
            )
            .child(
                div()
                    .text_size(px(TEXT_2XS))
                    .line_height(px(LINE_XS))
                    .text_color(rgb(pal.text_subtle))
                    .child(NOTE),
            )
            .into_any_element()
    }

    fn device_footer(cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(SPACE_3))
            .py(px(SPACE_3))
            .border_t_1()
            .border_color(pal.divider)
            .child(
                Button::new(BTN_USE_BROWSER)
                    .label(USE_BROWSER_LABEL)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.sign_in_gitlab(GitlabSignIn::Browser, cx);
                    })),
            )
            .child(
                Button::new(BTN_CANCEL)
                    .label(CANCEL_LABEL)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.cancel_gitlab_sign_in(cx);
                    })),
            )
            .into_any_element()
    }
}

fn step(number: u8, title: &'static str, content: AnyElement, pal: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap(px(STEP_GAP))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(STEP_BADGE))
                .rounded_full()
                .bg(rgb(pal.bg_panel))
                .text_size(px(TEXT_2XS))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(pal.text_strong))
                .child(number.to_string()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(SPACE_1_5))
                .text_size(px(TEXT_SM))
                .line_height(px(LINE_SM))
                .text_color(rgb(pal.text_strong))
                .child(title)
                .child(content),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_countdown_reads_minutes_and_seconds_until_it_expires() {
        assert_eq!(
            expiry_text(Duration::from_mins(14) + Duration::from_secs(32)),
            "expires in 14:32"
        );
        assert_eq!(expiry_text(Duration::from_secs(5)), "expires in 0:05");
        assert_eq!(expiry_text(Duration::ZERO), EXPIRED);
    }
}
