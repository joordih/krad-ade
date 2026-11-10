use gpui_kit::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement};
use krad_domain::ApiDiscoveryFile;

use super::super::dialog_parts::{segment, SegmentSize};
use super::{form_row, segmented_track, EnvironmentShell};
use crate::theme::palette;

const BTN_API_FILE_ON: &str = "api-discovery-file-on";
const BTN_API_FILE_OFF: &str = "api-discovery-file-off";
const LABEL_ON: &str = "On";
const LABEL_OFF: &str = "Off";
const API_FILE_LABEL: &str = "Let other programs find the agents API";
const API_FILE_HINT: &str = "Leaves api.json in the app's config folder with the API's local address and key, readable only by your user account, so your own scripts can list and follow your agents from any terminal. Off keeps the API to the app's own terminals.";

impl EnvironmentShell {
    pub(super) fn api_file_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let pal = palette(cx);
        let current = self.settings.api_discovery_file;
        let option = |id: &'static str, label: &'static str, choice: ApiDiscoveryFile| {
            segment(
                id,
                label,
                None,
                current == choice,
                SegmentSize::Compact,
                pal,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                if this.settings.api_discovery_file == choice {
                    return;
                }
                this.settings.api_discovery_file = choice;
                this.save_settings(cx);
                this.sync_api_file(cx);
                cx.notify();
            }))
        };
        let segmented = segmented_track(pal)
            .child(option(BTN_API_FILE_ON, LABEL_ON, ApiDiscoveryFile::On))
            .child(option(BTN_API_FILE_OFF, LABEL_OFF, ApiDiscoveryFile::Off))
            .into_any_element();
        form_row(API_FILE_LABEL, Some(API_FILE_HINT), segmented, pal)
    }
}
