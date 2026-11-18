use krad_domain::{PairingCode, PairingPermissions};
use std::io;

pub trait WebAccessHost: Send + Sync {
    fn address(&self) -> Option<String>;

    /// # Errors
    ///
    /// Fails when this computer cannot be served to the tailnet.
    fn turn_on(&self) -> io::Result<()>;

    /// # Errors
    ///
    /// Fails when the web access settings cannot be removed.
    fn turn_off(&self) -> io::Result<()>;

    /// # Errors
    ///
    /// Fails when web access is off or no code could be made.
    fn pairing_code(&self, permissions: PairingPermissions) -> io::Result<PairingCode>;
}
