use krad_store::{ChatImages, Secrets};

use crate::app_events::EventsSetup;
use crate::bus::BusSetup;
use crate::environments::EnvironmentsSetup;
use crate::kraddies::KraddiesSetup;
use crate::settings::SettingsSetup;

#[derive(Clone, Debug)]
pub struct RuntimeSetup {
    pub bus: Option<BusSetup>,
    pub events: Option<EventsSetup>,
    pub environments: EnvironmentsSetup,
    pub settings: SettingsSetup,
    pub kraddies: KraddiesSetup,
    pub secrets: Secrets,
    pub chat_images: Option<ChatImages>,
}

impl Default for RuntimeSetup {
    fn default() -> Self {
        Self {
            bus: None,
            events: None,
            environments: EnvironmentsSetup::default(),
            settings: SettingsSetup::default(),
            kraddies: KraddiesSetup::default(),
            secrets: Secrets::in_memory(),
            chat_images: None,
        }
    }
}
