use futures::StreamExt as _;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    div, px, rgb, AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, ScrollWheelEvent, Styled, Task, Window,
};
use krad_domain::{CloudRunnerId, GridSize, RuntimeId, ToastKind};
use krad_protocol::{AgentRef, Command, Reply, TerminalCommand, WriteBy};
use krad_session::{CellPoint, KeyModifiers, XTERM_PALETTE};

use super::super::super::terminal_body::{
    body_grid, frame_screen, screen_body, BodyGeometry, BodyLook, BODY_PAD_TOP, BODY_PAD_X,
    TERMINAL_FONT_PX, TERMINAL_LINE_PX,
};
use super::super::super::EnvironmentShell;
use super::terminal::{session_terminals, HostedTerminalKey, RemoteTerminal};
use crate::components::{card, Button, ButtonVariant};
use crate::terminal_theme::terminal_colors;
use crate::theme::{mono_family, palette, RADIUS_LG, SPACE_2, SPACE_3, TEXT_XS};

const WATCH_BATCH: usize = 64;
const HEADER_H: f32 = 36.0;
const CLOSE_ID: &str = "hosted-terminal-close";
const CLOSE_LABEL: &str = "Close";
const ENDED: &str = "The runner stopped sending this terminal";
const UNWATCHED: &str = "The runner did not let this window watch the terminal";
const UNREACHED: &str = "The cloud runner is not connected";
const TITLE_PREFIX: &str = "Terminal on";
const UNZOOMED: f32 = 1.0;
const CHIP_PREFIX: &str = "hosted-terminal-chip";
const CHIP_LABEL: &str = "Terminal";

pub(in crate::environment_shell) struct HostedTerminalView {
    terminal: RemoteTerminal,
    _watch: Task<()>,
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn show_hosted_terminal(
        &mut self,
        key: &HostedTerminalKey,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.client_for(RuntimeId::Runner(key.runner), cx) else {
            return self.toast(ToastKind::Warning, UNREACHED, cx);
        };
        let Some(terminal) = RemoteTerminal::new(key.clone()) else {
            return;
        };
        let (reply, feed) = client.watch_terminal(key.terminal.clone());
        let watched = key.clone();
        let watch = cx.spawn(async move |this, cx| {
            let accepted = matches!(reply.await, Ok(Reply::Accepted));
            if !accepted {
                this.update(cx, |shell, cx| shell.hosted_terminal_refused(&watched, cx))
                    .unwrap_or_default();
                return;
            }
            let mut chunks = feed.ready_chunks(WATCH_BATCH);
            while let Some(chunks) = chunks.next().await {
                let bytes = chunks.concat();
                let heard = this.update(cx, |shell, cx| {
                    shell.hosted_terminal_heard(&watched, &bytes, cx);
                });
                if heard.is_err() {
                    return;
                }
            }
            this.update(cx, |shell, cx| shell.hosted_terminal_ended(&watched, cx))
                .unwrap_or_default();
        });
        self.leave_hosted_chat();
        self.cloud.terminal = Some(HostedTerminalView {
            terminal,
            _watch: watch,
        });
        cx.notify();
    }

    fn shown_terminal(&mut self, key: &HostedTerminalKey) -> Option<&mut RemoteTerminal> {
        self.cloud
            .terminal
            .as_mut()
            .map(|view| &mut view.terminal)
            .filter(|terminal| terminal.key == *key)
    }

    fn hosted_terminal_heard(
        &mut self,
        key: &HostedTerminalKey,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) {
        let palette = self.runtime.palette().unwrap_or(XTERM_PALETTE);
        if let Some(terminal) = self.shown_terminal(key) {
            terminal.hear(bytes, &palette);
            cx.notify();
        }
    }

    fn hosted_terminal_ended(&mut self, key: &HostedTerminalKey, cx: &mut Context<Self>) {
        if let Some(terminal) = self.shown_terminal(key) {
            terminal.ended = true;
            cx.notify();
        }
    }

    fn hosted_terminal_refused(&mut self, key: &HostedTerminalKey, cx: &mut Context<Self>) {
        if self.shown_terminal(key).is_some() {
            self.cloud.terminal = None;
            self.toast(ToastKind::Warning, UNWATCHED, cx);
        }
    }

    pub(in crate::environment_shell) fn leave_hosted_terminal(&mut self) {
        self.cloud.terminal = None;
    }

    fn write_hosted_terminal(&self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let Some(view) = &self.cloud.terminal else {
            return;
        };
        let key = &view.terminal.key;
        let Some(client) = self.client_for(RuntimeId::Runner(key.runner), cx) else {
            return;
        };
        client.post(Command::Terminal(TerminalCommand::Write {
            terminal: key.terminal.clone(),
            bytes,
            by: WriteBy::User,
        }));
    }

    pub(in crate::environment_shell) fn type_into_hosted_terminal(
        &mut self,
        (key, text): (&str, Option<&str>),
        held: KeyModifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.cloud.terminal.as_mut() else {
            return false;
        };
        if let Some(bytes) = view.terminal.key_bytes(key, text, held) {
            self.write_hosted_terminal(bytes, cx);
        }
        true
    }

    pub(in crate::environment_shell) fn send_to_hosted_terminal(
        &mut self,
        bytes: &[u8],
        cx: &mut Context<Self>,
    ) -> bool {
        if self.cloud.terminal.is_none() {
            return false;
        }
        self.write_hosted_terminal(bytes.to_vec(), cx);
        true
    }

    pub(in crate::environment_shell) fn sync_hosted_terminal_grid(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cloud.terminal.is_none() {
            return;
        }
        let cell_width = self.cell_width(window);
        let (width, height) = self.canvas_px;
        let grid = body_grid(
            width,
            height,
            HEADER_H,
            cell_width,
            self.terminal_font_zoom(),
        );
        self.regrid_hosted_terminal(grid, cx);
    }

    fn regrid_hosted_terminal(&mut self, grid: GridSize, cx: &mut Context<Self>) {
        let palette = self.runtime.palette().unwrap_or(XTERM_PALETTE);
        let Some(view) = self.cloud.terminal.as_mut() else {
            return;
        };
        if !view.terminal.regrid(grid, &palette) {
            return;
        }
        let key = view.terminal.key.clone();
        if let Some(client) = self.client_for(RuntimeId::Runner(key.runner), cx) {
            client.post(Command::Terminal(TerminalCommand::Resize {
                terminal: key.terminal,
                size: grid,
            }));
        }
    }

    fn scroll_hosted_terminal(&mut self, lines: f32, at: CellPoint, cx: &mut Context<Self>) {
        self.scroll_remainder += lines;
        let whole = self.scroll_remainder.trunc();
        if whole == 0.0 {
            return;
        }
        self.scroll_remainder -= whole;
        let palette = self.runtime.palette().unwrap_or(XTERM_PALETTE);
        #[allow(clippy::cast_possible_truncation)]
        let steps = whole as i32;
        let program = self
            .cloud
            .terminal
            .as_mut()
            .and_then(|view| view.terminal.wheel((steps, at), &palette));
        if let Some(bytes) = program {
            self.write_hosted_terminal(bytes, cx);
        }
        cx.notify();
    }

    pub(in crate::environment_shell) fn hosted_terminal_chips(
        &self,
        runner: CloudRunnerId,
        beside: &AgentRef,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let terminals = self
            .runtime
            .view
            .read(cx)
            .remotes
            .get(runner)
            .and_then(|remote| {
                remote
                    .environments
                    .all()
                    .iter()
                    .find(|data| data.name == beside.environment)
            })
            .map_or_else(Vec::new, |data| session_terminals(data, beside.terminal));
        terminals
            .into_iter()
            .map(|terminal| {
                let key = HostedTerminalKey {
                    runner,
                    terminal: AgentRef {
                        environment: beside.environment.clone(),
                        terminal,
                    },
                };
                Button::new((CHIP_PREFIX, terminal.as_u64()))
                    .label(format!("{CHIP_LABEL} {}", terminal.as_u64()))
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.show_hosted_terminal(&key, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    pub(in crate::environment_shell) fn hosted_terminal_pane(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let view = self.cloud.terminal.as_ref()?;
        let pal = palette(cx);
        let mono = mono_family(&self.settings.terminal_font);
        let colors = terminal_colors(cx);
        let text_zoom = UNZOOMED * self.terminal_font_zoom();
        let (origin_x, origin_y) = self.window_origin();
        let geometry = BodyGeometry {
            origin: (origin_x + BODY_PAD_X, origin_y + HEADER_H + BODY_PAD_TOP),
            cell_w: self.cell_width_px.unwrap_or(TERMINAL_FONT_PX) * text_zoom,
            line_h: TERMINAL_LINE_PX * text_zoom,
        };
        let look = BodyLook {
            geometry,
            ligatures: self.settings.terminal_font.ligatures,
            zoom: UNZOOMED,
            text_zoom,
            focused: true,
            corner: RADIUS_LG,
        };
        let title = format!(
            "{TITLE_PREFIX} {}",
            self.runner_name(view.terminal.key.runner)
        );
        let body = screen_body(look, &mono, &colors, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut Self, _: &MouseDownEvent, window, cx| {
                    this.focus_pane(window, cx);
                }),
            )
            .on_scroll_wheel(cx.listener(
                move |this: &mut Self, event: &ScrollWheelEvent, _, cx| {
                    cx.stop_propagation();
                    let delta = event.delta.pixel_delta(px(geometry.line_h));
                    let at = geometry.cell_at(event.position.x.as_f32(), event.position.y.as_f32());
                    this.scroll_hosted_terminal(delta.y.as_f32() / geometry.line_h, at, cx);
                },
            ));
        let screen = frame_screen(body, view.terminal.frame(), None, look, (&mono, &colors));
        Some(
            card(pal, cx)
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_none()
                        .items_center()
                        .gap(px(SPACE_2))
                        .h(px(HEADER_H))
                        .px(px(SPACE_3))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(pal.text_strong))
                                .child(title),
                        )
                        .when(view.terminal.ended, |header| {
                            header.child(
                                div()
                                    .text_size(px(TEXT_XS))
                                    .text_color(rgb(pal.text_muted))
                                    .child(ENDED),
                            )
                        })
                        .child(
                            Button::new(CLOSE_ID)
                                .label(CLOSE_LABEL)
                                .variant(ButtonVariant::Ghost)
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.leave_hosted_terminal();
                                    cx.notify();
                                })),
                        ),
                )
                .child(screen)
                .into_any_element(),
        )
    }
}
