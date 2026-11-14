use crate::backdrop_image::Backdrops;
use crate::cloud::CloudRunnerId;
use crate::environment::EnvironmentData;
use crate::group::ShownExtensionViews;
use crate::ids::{EnvironmentName, TerminalId};
use crate::notes::NoteTabs;
use crate::side_panel::SidePanelLayout;
use crate::sidebar_width::SidebarWidth;
use crate::toolchain::ToolchainChoices;
use crate::view::EnvironmentView;
use crate::work_mode::WorkMode;
use serde::{Deserialize, Serialize};

use super::LAYOUT_SCHEMA_VERSION;

const fn current_version() -> u32 {
    LAYOUT_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewPreferences {
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<EnvironmentName>,
    #[serde(default)]
    pub work_mode: WorkMode,
    #[serde(default)]
    pub sidebar_width: SidebarWidth,
    #[serde(default, skip_serializing_if = "ToolchainChoices::is_empty")]
    pub toolchains: ToolchainChoices,
    #[serde(default, skip_serializing_if = "Backdrops::is_empty")]
    pub backdrops: Backdrops,
    #[serde(default)]
    pub environments: Vec<EnvironmentViewPreferences>,
}

impl Default for ViewPreferences {
    fn default() -> Self {
        Self {
            version: LAYOUT_SCHEMA_VERSION,
            active: None,
            work_mode: WorkMode::default(),
            sidebar_width: SidebarWidth::default(),
            toolchains: ToolchainChoices::default(),
            backdrops: Backdrops::default(),
            environments: Vec::new(),
        }
    }
}

impl ViewPreferences {
    pub fn dress(&mut self, environment: &mut EnvironmentData) {
        let Some(at) = self
            .environments
            .iter()
            .position(|view| view.name == environment.name)
        else {
            return;
        };
        environment.restore_view(self.environments.swap_remove(at));
    }

    #[must_use]
    pub fn take(&mut self, name: &EnvironmentName) -> EnvironmentViewPreferences {
        self.environments
            .iter()
            .position(|view| view.name == *name)
            .map_or_else(
                || EnvironmentViewPreferences::new(name.clone()),
                |at| self.environments.swap_remove(at),
            )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentViewPreferences {
    pub name: EnvironmentName,
    #[serde(default, deserialize_with = "crate::view::lenient_view")]
    pub view: EnvironmentView,
    #[serde(default)]
    pub focus: Option<TerminalId>,
    #[serde(default)]
    pub side_panel: SidePanelLayout,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub note_tabs: Vec<NoteTabs>,
    #[serde(default, skip_serializing_if = "ShownExtensionViews::is_empty")]
    pub extension_views: ShownExtensionViews,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_on: Option<CloudRunnerId>,
}

impl EnvironmentViewPreferences {
    #[must_use]
    pub fn new(name: EnvironmentName) -> Self {
        Self {
            name,
            view: EnvironmentView::default(),
            focus: None,
            side_panel: SidePanelLayout::default(),
            note_tabs: Vec::new(),
            extension_views: ShownExtensionViews::default(),
            run_on: None,
        }
    }
}

impl From<&EnvironmentData> for EnvironmentViewPreferences {
    fn from(environment: &EnvironmentData) -> Self {
        Self {
            name: environment.name.clone(),
            view: environment.view,
            focus: environment.focus,
            side_panel: environment.side_panel.clone(),
            note_tabs: environment.note_tabs.clone(),
            extension_views: environment.extension_views.clone(),
            run_on: None,
        }
    }
}

impl EnvironmentData {
    pub fn take_view(&mut self) -> EnvironmentViewPreferences {
        EnvironmentViewPreferences {
            name: self.name.clone(),
            view: std::mem::take(&mut self.view),
            focus: self.focus.take(),
            side_panel: std::mem::take(&mut self.side_panel),
            note_tabs: std::mem::take(&mut self.note_tabs),
            extension_views: std::mem::take(&mut self.extension_views),
            run_on: None,
        }
    }

    pub fn restore_view(&mut self, view: EnvironmentViewPreferences) {
        self.view = view.view;
        self.focus = view.focus;
        self.side_panel = view.side_panel;
        self.note_tabs = view.note_tabs;
        self.extension_views = view.extension_views;
    }
}
