mod draft;
mod form;
mod logins;
mod token;

use gpui_kit::{div, px, AnyElement, Context, IntoElement, ParentElement, Styled, Window};
use krad_domain::CloudRunner;

use super::super::cloud::RunnerState;
use super::super::dialog_parts::sheet_padded;
use super::sections::{empty_row, footnote, pane_column, settings_section};
use super::{form_row, hint, panel_card, EnvironmentShell};
use crate::components::{Button, ButtonVariant, IconName};
use crate::theme::{palette, Palette, SPACE_2};
use form::RunnerForm;
use token::TokenForm;

const RUNNERS_TITLE: &str = "Cloud runners";
const RUNNERS_HINT: &str = "Linux hosts, x86_64 or aarch64, reached over SSH. Cloud sessions run there and keep going while this computer sleeps or is closed.";
const RUNNERS_EMPTY: &str = "No cloud runners yet.";
const ADD_LABEL: &str = "Add cloud runner";
const RECONNECT_LABEL: &str = "Reconnect";
const REMOVE_LABEL: &str = "Remove";
const DETAIL_SEPARATOR: &str = " · ";
const BTN_OPEN_FORM: &str = "cloud-runner-open-form";
const RECONNECT_PREFIX: &str = "cloud-runner-reconnect";
const REMOVE_PREFIX: &str = "cloud-runner-remove";
const RUNNERS_NOTE: &str = "The app copies its host program to ~/.krad-ade on the runner, installs a user service named krad-cloud-runner and pairs with it. The pairing token is kept in the system keyring, or in the config folder readable only by you, and reaches the runner only through the SSH connection, never on a command line. Removing a runner stops and removes that service and its pairing; the clones on the host stay.";

pub(in crate::environment_shell) struct CloudRunnersUi {
    form: RunnerForm,
    token: TokenForm,
}

fn runner_detail(runner: &CloudRunner, state: &RunnerState) -> String {
    format!("{}{DETAIL_SEPARATOR}{}", runner.destination, state.label())
}

impl EnvironmentShell {
    pub(in crate::environment_shell) fn cloud_runners_ui(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CloudRunnersUi {
        CloudRunnersUi {
            form: RunnerForm::new(window, cx),
            token: TokenForm::new(window, cx),
        }
    }

    fn cloud_runner_rows(
        &self,
        runner: &CloudRunner,
        token: &TokenForm,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let id = runner.id;
        let state = self.runner_state(id);
        let button = |prefix: &'static str, label: &'static str| {
            Button::new((prefix, id.0))
                .label(label)
                .small()
                .variant(ButtonVariant::Secondary)
        };
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(SPACE_2))
            .child(button(RECONNECT_PREFIX, RECONNECT_LABEL).on_click(
                cx.listener(move |this: &mut Self, _, _, cx| this.reconnect_runner(id, cx)),
            ))
            .child(button(REMOVE_PREFIX, REMOVE_LABEL).on_click(
                cx.listener(move |this: &mut Self, _, _, cx| this.remove_cloud_runner(id, cx)),
            ))
            .into_any_element();
        let row = form_row(
            runner.name.as_str(),
            Some(&runner_detail(runner, &state)),
            controls,
            pal,
        );
        let warning = state
            .linger_warning()
            .map(|warning| sheet_padded(hint(warning, pal)).into_any_element());
        let logins = self.runner_logins_row(id, pal, cx);
        std::iter::once(row)
            .chain(warning)
            .chain(logins)
            .chain(self.runner_token_rows(id, token, pal, cx))
            .collect()
    }

    fn cloud_runners_card(&self, ui: &CloudRunnersUi, cx: &mut Context<Self>) -> AnyElement {
        let form_open = ui.form.open;
        let pal = palette(cx);
        let rows: Vec<AnyElement> = if self.settings.cloud.runners.is_empty() {
            vec![empty_row(RUNNERS_EMPTY, pal)]
        } else {
            self.settings
                .cloud
                .runners
                .iter()
                .flat_map(|runner| self.cloud_runner_rows(runner, &ui.token, pal, cx))
                .collect()
        };
        let add = (!form_open).then(|| {
            div()
                .flex()
                .flex_row()
                .justify_end()
                .child(
                    Button::new(BTN_OPEN_FORM)
                        .label(ADD_LABEL)
                        .icon(IconName::Add)
                        .small()
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                            this.open_runner_form(window, cx);
                        })),
                )
                .into_any_element()
        });
        settings_section(
            RUNNERS_TITLE,
            Some(RUNNERS_HINT),
            std::iter::once(panel_card(rows, pal)).chain(add),
            pal,
        )
    }

    pub(super) fn cloud_runners_pane(
        &self,
        ui: &CloudRunnersUi,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = palette(cx);
        let open = ui.form.open;
        let mut sections = vec![self.cloud_runners_card(ui, cx)];
        if open {
            sections.push(Self::runner_form_section(&ui.form, cx));
        }
        sections.push(footnote(RUNNERS_NOTE, pal));
        pane_column(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::CloudRunnerId;

    #[test]
    fn a_row_tells_the_destination_and_the_state() -> Result<(), Box<dyn std::error::Error>> {
        let runner = CloudRunner {
            id: CloudRunnerId(1),
            name: "GPU".parse()?,
            destination: "me@gpu:2200".parse()?,
        };
        assert_eq!(
            runner_detail(&runner, &RunnerState::Connecting),
            "me@gpu:2200 · Connecting"
        );
        Ok(())
    }
}
