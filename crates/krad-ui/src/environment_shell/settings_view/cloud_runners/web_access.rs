use gpui_kit::component::switch::Switch;
use gpui_kit::{
    div, px, rgb, AnyElement, AppContext as _, Context, IntoElement, ParentElement, Styled,
};
use krad_domain::{PairingCode, PairingPermission, PairingPermissions};
use krad_protocol::{PairingAnswer, PairingCommand};
use std::path::PathBuf;
use std::process::Command;

use super::super::super::dialog_parts::sheet_padded;
use super::super::sections::settings_section;
use super::super::{form_row, hint, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant};
use crate::theme::{palette, Palette, SPACE_2, TEXT_SM};

const WEB_TITLE: &str = "Web access";
const WEB_HINT: &str = "Use this computer's sessions, chats, terminals, Kraddies and automations from a browser on your phone or another computer in your tailnet, cloud sessions included.";
const SWITCH_ID: &str = "web-access-switch";
const SWITCH_LABEL: &str = "Serve this computer to your browsers";
const OFF_DETAIL: &str = "Turning it on keeps this computer's runtime running as a service even with the app closed, asks Tailscale for this computer's HTTPS certificate and opens the app again as a client of the service.";
const ON_DETAIL: &str = "This computer's runtime runs as a service and stays awake while an agent works, or for two hours while one waits for you; a Mac with its lid closed and no display still sleeps. Turning it off stops the service and opens the app again with the runtime inside it.";
const PAIR_ID: &str = "web-access-pair";
const PAIR_LABEL: &str = "Pair a browser";
const STARTING: &str = "Asking Tailscale for a certificate…";
const CODE_LABEL: &str = "Code";
const URL_LABEL: &str = "Address";
const CODE_HINT: &str = "Scan the code with the phone's camera, or open the address and type the code. It works once, for ten minutes.";
const SERVICE_VERB: &str = "service";
const ENABLE: &str = "enable";
const HANDOVER: &str = "handover";
const HANDBACK: &str = "handback";
const RUNTIME_FOLDER: &str = "runtime";
const PAIR_FRAGMENT: &str = "#pair=";
const QR_MODULE_PX: f32 = 4.0;
const QR_QUIET_MODULES: f32 = 4.0;
const QR_DARK: u32 = 0x0000_0000;
const QR_LIGHT: u32 = 0x00ff_ffff;
const NO_CODE: &str = "The service did not make a pairing code";
const PAIR_FAILED: &str = "Could not ask the service for a pairing code";

#[derive(Default)]
pub(in crate::environment_shell) struct WebAccessUi {
    busy: bool,
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

fn served_address() -> Option<String> {
    let root = krad_store::config_dir()?.join(RUNTIME_FOLDER);
    let access = krad_web_listener::read_web_access(&root).ok()??;
    let port = access.bind.port();
    match (access.names.first(), access.secure()) {
        (Some(name), true) => Some(format!("https://{name}:{port}")),
        _ => Some(format!("http://{}", access.bind)),
    }
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

fn this_program() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn hand_off(verb: &str) -> bool {
    let Some(program) = this_program() else {
        return false;
    };
    Command::new(program)
        .args([SERVICE_VERB, verb, &std::process::id().to_string()])
        .spawn()
        .inspect_err(|error| tracing::warn!(%error, "the hand-off helper did not start"))
        .is_ok()
}

fn enabled() -> Result<(), String> {
    let program =
        this_program().ok_or_else(|| "the app does not know its own program".to_owned())?;
    let output = Command::new(program)
        .args([SERVICE_VERB, ENABLE])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(said.split_once(": ").map_or_else(
        || said.trim().to_owned(),
        |(_, message)| message.trim().to_owned(),
    ))
}

impl EnvironmentShell {
    fn web_access_ui(&mut self) -> Option<&mut WebAccessUi> {
        self.settings_ui
            .as_mut()
            .map(|ui| &mut ui.cloud_runners.web)
    }

    fn turn_web_access_on(&mut self, cx: &mut Context<Self>) {
        let Some(ui) = self.web_access_ui() else {
            return;
        };
        ui.busy = true;
        ui.problem = None;
        cx.notify();
        let enabling = cx.background_spawn(async { enabled() });
        cx.spawn(async move |this, cx| {
            let outcome = enabling.await;
            this.update(cx, |shell, cx| {
                if let Some(ui) = shell.web_access_ui() {
                    ui.busy = false;
                    ui.problem = outcome.as_ref().err().cloned();
                }
                if outcome.is_ok() && hand_off(HANDOVER) {
                    cx.quit();
                }
                cx.notify();
            })
            .unwrap_or_default();
        })
        .detach();
    }

    fn turn_web_access_off(cx: &mut Context<Self>) {
        if hand_off(HANDBACK) {
            cx.quit();
        }
    }

    fn pair_a_browser(&mut self, cx: &mut Context<Self>) {
        let Some(asking) = self
            .runtime
            .local
            .browser_code(PairingCommand::CreatePairingCode {
                permissions: browser_permissions(),
            })
        else {
            return;
        };
        let address = served_address();
        cx.spawn(async move |this, cx| {
            let answered = asking.await;
            this.update(cx, |shell, cx| {
                let Some(ui) = shell.web_access_ui() else {
                    return;
                };
                match answered {
                    Ok(PairingAnswer::Code { code, .. }) => {
                        let code = PairingCode::expose(&code).to_owned();
                        let link = format!("{}/{PAIR_FRAGMENT}{code}", address.unwrap_or_default());
                        let modules = modules_of(&link);
                        ui.shown = Some(ShownCode {
                            code,
                            link,
                            modules,
                        });
                        ui.problem = None;
                    }
                    Ok(_) => ui.problem = Some(NO_CODE.to_owned()),
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
            line(URL_LABEL, &shown.link),
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
    ) -> AnyElement {
        let pal = palette(cx);
        let serving = self.runtime.local.handle().is_none();
        let switch = Switch::new(SWITCH_ID)
            .checked(serving || ui.busy)
            .on_click(cx.listener(move |this: &mut Self, _: &bool, _, cx| {
                if this.web_access_ui().is_some_and(|ui| ui.busy) {
                    return;
                }
                if serving {
                    Self::turn_web_access_off(cx);
                } else {
                    this.turn_web_access_on(cx);
                }
            }))
            .into_any_element();
        let detail = if ui.busy {
            STARTING
        } else if serving {
            ON_DETAIL
        } else {
            OFF_DETAIL
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
        if serving {
            let address = served_address().unwrap_or_default();
            let pair = Button::new(PAIR_ID)
                .label(PAIR_LABEL)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.pair_a_browser(cx)))
                .into_any_element();
            rows.push(form_row(URL_LABEL, Some(&address), pair, pal));
            if let Some(shown) = &ui.shown {
                rows.extend(Self::shown_code(shown, pal));
            }
        }
        settings_section(
            WEB_TITLE,
            Some(WEB_HINT),
            [div()
                .flex()
                .flex_col()
                .gap(px(SPACE_2))
                .child(panel_card(rows, pal))
                .into_any_element()],
            pal,
        )
    }
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
}
