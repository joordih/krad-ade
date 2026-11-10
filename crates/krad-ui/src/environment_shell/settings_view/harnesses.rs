use gpui_kit::{AnyElement, Context, IntoElement};

use super::super::harness_health::{health_rows, HealthPlace};
use super::sections::{pane_column, settings_section};
use super::{panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::palette;

const SECTION_TITLE: &str = "Agent CLIs";
const THIS_COMPUTER: &str = "this computer";
const IN_WSL: &str = "Each agent CLI in WSL";
const ON_HOST: &str = "Each agent CLI on";
const CHECKS_HINT: &str =
    "whether it is installed, which version it runs and whether you are signed in.";
const CHECK_AGAIN: &str = "Check again";
const BTN_CHECK_AGAIN: &str = "harness-health-check-again";
const COPY_PREFIX: &str = "harness-health-copy";

fn place_hint(place: &HealthPlace) -> String {
    match place {
        HealthPlace::ThisComputer => format!("{ON_HOST} {THIS_COMPUTER}: {CHECKS_HINT}"),
        HealthPlace::Wsl(distro) => format!("{IN_WSL} {distro}: {CHECKS_HINT}"),
        HealthPlace::Ssh(host) => format!("{ON_HOST} {host}: {CHECKS_HINT}"),
    }
}

impl EnvironmentShell {
    pub(super) fn harnesses_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let (place, _, _) = self.health_place();
        let mut rows = health_rows(self, &place, COPY_PREFIX, cx);
        rows.push(
            Button::new(BTN_CHECK_AGAIN)
                .label(CHECK_AGAIN)
                .icon(IconName::Refresh)
                .small()
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.check_harness_health(cx);
                }))
                .into_any_element(),
        );
        pane_column([settings_section(
            SECTION_TITLE,
            Some(&place_hint(&place)),
            [panel_card(rows, pal)],
            pal,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::Host;

    #[test]
    fn the_hint_names_where_the_checks_ran() -> Result<(), krad_domain::SshDestinationError> {
        assert!(
            place_hint(&HealthPlace::ThisComputer).starts_with("Each agent CLI on this computer:")
        );
        assert!(place_hint(&HealthPlace::Wsl("Ubuntu".to_owned()))
            .starts_with("Each agent CLI in WSL Ubuntu:"));
        let host = Host::Ssh {
            destination: "devbox".parse()?,
        };
        assert!(place_hint(&HealthPlace::Ssh(host)).starts_with("Each agent CLI on devbox:"));
        Ok(())
    }
}
