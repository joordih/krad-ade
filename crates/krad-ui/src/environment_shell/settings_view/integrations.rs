use gpui_kit::{AnyElement, Context, IntoElement, StatefulInteractiveElement};

use super::super::dialog_parts::sheet_group;
use super::gitlab::page::KEYRING_NOTE;
use super::sections::{disclosure, footnote, pane_column, settings_section};
use super::{EnvironmentShell, SettingsUi};
use crate::theme::palette;

const ACCOUNTS_TITLE: &str = "Accounts";
const ACCOUNTS_HINT: &str =
    "Sign in to see the merge requests and pull requests of your groups in Review.";
const GROUPS_TITLE: &str = "Your groups";
const GROUPS_HINT: &str = "Each group's git remote decides where its reviews come from.";
const ADVANCED_LABEL: &str = "Advanced: your own GitLab server";
const FOLD_ID: &str = "integrations-gitlab-fold";

impl EnvironmentShell {
    pub(super) fn integrations_pane(&self, ui: &SettingsUi, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let gitlab = &ui.gitlab;
        let unfolded = gitlab.unfolded;
        let accounts = sheet_group(
            [
                Self::gitlab_account(&gitlab.status, cx),
                Self::github_account(&ui.github.status, cx),
            ],
            pal,
        );
        let fold = disclosure(FOLD_ID, ADVANCED_LABEL, unfolded, pal)
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                if let Some(ui) = this.settings_ui.as_mut() {
                    ui.gitlab.unfolded = !ui.gitlab.unfolded;
                }
                cx.notify();
            }))
            .into_any_element();
        let mut sections = vec![
            settings_section(ACCOUNTS_TITLE, Some(ACCOUNTS_HINT), [accounts], pal),
            settings_section(
                GROUPS_TITLE,
                Some(GROUPS_HINT),
                [self.projects_card(gitlab, cx)],
                pal,
            ),
            fold,
        ];
        if unfolded {
            sections.push(Self::oauth_section(gitlab, cx));
            sections.push(self.method_section(cx));
        }
        sections.push(footnote(KEYRING_NOTE, pal));
        pane_column(sections)
    }
}
