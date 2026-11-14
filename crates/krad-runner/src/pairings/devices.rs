use krad_domain::PairedDevice;
use krad_host::runner::{paired_installs, pairing_file};
use std::path::Path;

use super::stored::read_stored;

pub(crate) fn paired_devices(root: &Path) -> Vec<PairedDevice> {
    paired_installs(root)
        .into_iter()
        .filter_map(|install| {
            let stored = read_stored(&pairing_file(root, &install))?;
            Some(PairedDevice {
                install,
                device: stored.device,
                paired_at: stored.paired_at,
                last_used: stored.last_used,
                permissions: stored.permissions,
            })
        })
        .collect()
}
