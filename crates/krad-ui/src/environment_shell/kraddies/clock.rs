use chrono::{DateTime, Local};
use gpui_kit::Context;
use krad_domain::{automation_since, ANIMATION_FRAME};
use std::time::SystemTime;

use super::super::EnvironmentShell;
use super::{AutomationEntry, KraddieEntry};

const NEXT_RUN_FORMAT: &str = "%a %H:%M";

fn local(at: SystemTime) -> DateTime<Local> {
    DateTime::<Local>::from(at)
}

pub(in crate::environment_shell) fn next_run(
    kraddie: &KraddieEntry,
    entry: &AutomationEntry,
    opened_at: SystemTime,
) -> Option<DateTime<Local>> {
    if !kraddie.kraddie.switch.is_on() || !entry.automation.switch.is_on() {
        return None;
    }
    let since = automation_since(entry.history.last_started(), opened_at);
    entry.automation.next_after(&local(since))
}

pub(in crate::environment_shell) fn next_run_label(at: &DateTime<Local>) -> String {
    at.format(NEXT_RUN_FORMAT).to_string()
}

impl EnvironmentShell {
    pub(super) fn start_kraddie_clock(&mut self, cx: &mut Context<Self>) {
        self.kraddies.ticker = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(ANIMATION_FRAME).await;
            let alive = this.update(cx, |shell, cx| {
                if shell.kraddies.animated.replace(false) {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        }));
    }
}
