use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled,
};
use krad_domain::{PairingPermission, PairingPermissions};
use std::io;
use std::sync::Arc;

use super::super::super::dialog_parts::sheet_padded;
use super::super::sections::settings_section;
use super::super::{form_row, hint, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, Palette, SPACE_2, TEXT_SM};
use crate::web_access::WebAccessHost;

const WEB_TITLE: &str = "Web access";
const WEB_HINT: &str = "Use this computer's sessions, chats, terminals, Kraddies and automations from a browser on your phone or another computer in your tailnet, cloud sessions included.";
const SWITCH_ID: &str = "web-access-switch";
const SWITCH_LABEL: &str = "Serve this computer to your browsers";
const OFF_DETAIL: &str = "Turning it on asks Tailscale for this computer's HTTPS certificate, then the app serves this computer to browsers in your tailnet while it is open.";
const ON_DETAIL: &str = "The app serves this computer to browsers in your tailnet while it is open, and keeps it awake while an agent works, or for two hours while one waits for you; a Mac with its lid closed and no display still sleeps.";
const PAIR_ID: &str = "web-access-pair";
const PAIR_LABEL: &str = "Pair a browser";
const STARTING: &str = "Asking Tailscale for a certificate…";
const STOPPING: &str = "Stopping web access…";
const CODE_LABEL: &str = "Code";
const URL_LABEL: &str = "Address";
const CODE_HINT: &str = "Scan the code with the phone's camera, or open the address and type the code. It works once, for ten minutes.";
const PAIR_FRAGMENT: &str = "#pair=";
const QR_MODULE_PX: f32 = 4.0;
const QR_QUIET_MODULES: f32 = 4.0;
const QR_DARK: u32 = 0x0000_0000;
const QR_LIGHT: u32 = 0x00ff_ffff;
const PAIR_FAILED: &str = "Could not make a pairing code";
const ADDRESS_LINK_ID: &str = "web-access-address-link";
const PAIR_LINK_ID: &str = "web-access-pair-link";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Switching {
    #[default]
    Settled,
    TurningOn,
    TurningOff,
}

#[derive(Default)]
pub(in crate::environment_shell) struct WebAccessUi {
    switching: Switching,
    problem: Option<String>,
    shown: Option<ShownCode>,
}

struct ShownCode {
    code: String,
    link: String,
    modules: Vec<Vec<bool>>,
}

fn browser_permissions() -> PairingPermissions {
    [
        PairingPermission::Read,
        PairingPermission::Answer,
        PairingPermission::Drive,
    ]
    .into_iter()
    .collect()
}

fn modules_of(link: &str) -> Vec<Vec<bool>> {
    let Ok(code) = qrcode::QrCode::new(link.as_bytes()) else {
        return Vec::new();
    };
    let width = code.width();
    code.to_colors()
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|color| *color == qrcode::Color::Dark)
                .collect()
        })
        .collect()
}

fn qr(modules: &[Vec<bool>]) -> AnyElement {
    let rows = modules.iter().map(|row| {
        div().flex().flex_row().children(row.iter().map(|dark| {
            div()
                .size(px(QR_MODULE_PX))
                .bg(rgb(if *dark { QR_DARK } else { QR_LIGHT }))
        }))
    });
    div()
        .p(px(QR_MODULE_PX * QR_QUIET_MODULES))
        .bg(rgb(QR_LIGHT))
        .flex()
        .flex_col()
        .children(rows)
        .into_any_element()
}

fn link(id: &'static str, url: &str, pal: &Palette) -> AnyElement {
    let opened = url.to_owned();
    let hover = pal.accent_hover;
    div()
        .id(id)
        .text_size(px(TEXT_SM))
        .text_color(rgb(pal.accent))
        .cursor_pointer()
        .hover(move |style| style.text_color(rgb(hover)).text_decoration_1())
        .on_click(move |_, _, cx| cx.open_url(&opened))
        .child(url.to_owned())
        .into_any_element()
}

fn shown_code(address: &str, code: &str) -> ShownCode {
    let link = format!("{address}/{PAIR_FRAGMENT}{code}");
    let modules = modules_of(&link);
    ShownCode {
        code: code.to_owned(),
        link,
        modules,
    }
}

impl EnvironmentShell {
    fn web_access_ui(&mut self) -> Option<&mut WebAccessUi> {
        self.settings_ui
            .as_mut()
            .map(|ui| &mut ui.cloud_runners.web)
    }

    fn switch_web_access(&mut self, on: bool, cx: &mut Context<Self>) {
        let Some(host) = self.web_access.clone() else {
            return;
        };
        let Some(ui) = self.web_access_ui() else {
            return;
        };
        ui.switching = if on {
            Switching::TurningOn
        } else {
            Switching::TurningOff
        };
        ui.problem = None;
        ui.shown = None;
        cx.notify();
        let switching = cx.background_spawn(async move {
            if on {
                host.turn_on()
            } else {
                host.turn_off()
            }
        });
        cx.spawn(async move |this, cx| {
            let outcome = switching.await;
            this.update(cx, |shell, cx| {
                if let Some(ui) = shell.web_access_ui() {
                    ui.switching = Switching::Settled;
                    ui.problem = outcome.err().map(|error| error.to_string());
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn pair_a_browser(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.web_access.clone() else {
            return;
        };
        let asking = cx.background_spawn(async move { paired_code(&host) });
        cx.spawn(async move |this, cx| {
            let answered = asking.await;
            this.update(cx, |shell, cx| {
                let Some(ui) = shell.web_access_ui() else {
                    return;
                };
                match answered {
                    Ok(shown) => {
                        ui.shown = Some(shown);
                        ui.problem = None;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "{PAIR_FAILED}");
                        ui.problem = Some(PAIR_FAILED.to_owned());
                    }
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn shown_code(shown: &ShownCode, pal: &Palette) -> Vec<AnyElement> {
        let line = |label: &str, value: &str| {
            form_row(
                label,
                None,
                div()
                    .text_size(px(TEXT_SM))
                    .text_color(rgb(pal.text_strong))
                    .child(value.to_owned())
                    .into_any_element(),
                pal,
            )
        };
        vec![
            line(CODE_LABEL, &shown.code),
            form_row(URL_LABEL, None, link(PAIR_LINK_ID, &shown.link, pal), pal),
            sheet_padded(
                div()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .child(qr(&shown.modules)),
            )
            .into_any_element(),
            sheet_padded(hint(CODE_HINT, pal)).into_any_element(),
        ]
    }

    pub(super) fn web_access_section(
        &self,
        ui: &WebAccessUi,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let address = self.web_access.as_ref()?.address();
        let pal = palette(cx);
        let serving = address.is_some();
        let checked = match ui.switching {
            Switching::Settled => serving,
            Switching::TurningOn => true,
            Switching::TurningOff => false,
        };
        let switch = Switch::new(SWITCH_ID)
            .checked(checked)
            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                if this
                    .web_access_ui()
                    .is_some_and(|ui| ui.switching != Switching::Settled)
                {
                    return;
                }
                this.switch_web_access(!serving, cx);
            }))
            .into_any_element();
        let detail = match (ui.switching, serving) {
            (Switching::TurningOn, _) => STARTING,
            (Switching::TurningOff, _) => STOPPING,
            (Switching::Settled, true) => ON_DETAIL,
            (Switching::Settled, false) => OFF_DETAIL,
        };
        let mut rows = vec![form_row(SWITCH_LABEL, Some(detail), switch, pal)];
        if let Some(problem) = &ui.problem {
            rows.push(
                sheet_padded(
                    div()
                        .text_color(rgb(pal.error_fg))
                        .child(hint(problem, pal)),
                )
                .into_any_element(),
            );
        }
        if let Some(address) = &address {
            let pair = Button::new(PAIR_ID)
                .label(PAIR_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.pair_a_browser(cx)))
                .into_any_element();
            let served = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(SPACE_2))
                .child(link(ADDRESS_LINK_ID, address, pal))
                .child(pair)
                .into_any_element();
            rows.push(form_row(URL_LABEL, None, served, pal));
            if let Some(shown) = &ui.shown {
                rows.extend(Self::shown_code(shown, pal));
            }
        }
        Some(settings_section(
            WEB_TITLE,
            Some(WEB_HINT),
            [div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .child(panel_card(rows, pal))
                .into_any_element()],
            pal,
        ))
    }
}

fn paired_code(host: &Arc<dyn WebAccessHost>) -> io::Result<ShownCode> {
    let address = host
        .address()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, PAIR_FAILED))?;
    let code = host.pairing_code(browser_permissions())?;
    Ok(shown_code(&address, code.expose()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_link_becomes_a_square_qr_code() {
        let modules = modules_of("https://laptop.tailnet.ts.net:7421/#pair=AFWNFSPV");
        assert_ne!(modules.len(), 0);
        assert!(modules.iter().all(|row| row.len() == modules.len()));
    }

    #[test]
    fn the_pairing_link_opens_the_served_address_with_the_code() {
        let shown = shown_code("https://laptop.tailnet.ts.net:7421", "AFWNFSPV");
        assert_eq!(
            shown.link,
            "https://laptop.tailnet.ts.net:7421/#pair=AFWNFSPV"
        );
        assert_eq!(shown.code, "AFWNFSPV");
        assert_ne!(shown.modules.len(), 0);
    }
}
