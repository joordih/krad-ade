use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn root(name: &str) -> Root {
    Root(std::env::temp_dir().join(format!("krad-pairings-{name}-{}", std::process::id())))
}

fn mode(path: &Path) -> io::Result<u32> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

fn laptop() -> Result<DeviceName, Box<dyn std::error::Error>> {
    Ok("laptop".parse()?)
}

#[test]
fn a_pairing_keeps_only_the_token_s_hash_in_a_private_file() -> TestResult {
    let root = root("hash");
    let install: InstallId = "0123456789abcdef".parse()?;
    let token = pair(&root.0, &install, laptop()?, PairingPermissions::all())?;
    let file = pairing_file(&root.0, &install);
    assert!(!fs::read_to_string(&file)?.contains(token.expose()));
    assert_eq!(mode(&file)?, 0o600);
    assert_eq!(mode(&clients_dir(&root.0))?, 0o700);
    let pairing = Pairing {
        install: install.clone(),
        token,
    };
    assert_eq!(
        authorize(&root.0, &pairing),
        Some(PairingPermissions::all())
    );
    let renewed = pair(&root.0, &install, laptop()?, PairingPermissions::all())?;
    assert_eq!(authorize(&root.0, &pairing), None);
    let renewed = Pairing {
        install: install.clone(),
        token: renewed,
    };
    assert!(authorize(&root.0, &renewed).is_some());
    let devices = paired_devices(&root.0);
    assert_eq!(devices.len(), 1);
    assert!(devices[0].last_used.is_some());
    assert!(unpair(&root.0, &install)?);
    assert_eq!(authorize(&root.0, &renewed), None);
    assert!(!unpair(&root.0, &install)?);
    Ok(())
}
