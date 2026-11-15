use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const TAILNET_V4: Ipv4Addr = Ipv4Addr::new(100, 64, 0, 0);
const TAILNET_V4_PREFIX: u32 = 10;
const TAILNET_V6: Ipv6Addr = Ipv6Addr::new(0xfd7a, 0x115c, 0xa1e0, 0, 0, 0, 0, 0);
const TAILNET_V6_PREFIX: u32 = 48;

fn mask_v4(prefix: u32) -> u32 {
    u32::MAX.checked_shl(u32::BITS - prefix).unwrap_or(0)
}

fn mask_v6(prefix: u32) -> u128 {
    u128::MAX.checked_shl(u128::BITS - prefix).unwrap_or(0)
}

#[must_use]
pub fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

#[must_use]
pub fn is_private_network(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let mask = mask_v4(TAILNET_V4_PREFIX);
            u32::from(ip) & mask == u32::from(TAILNET_V4) & mask
        }
        IpAddr::V6(ip) => {
            let mask = mask_v6(TAILNET_V6_PREFIX);
            u128::from(ip) & mask == u128::from(TAILNET_V6) & mask
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tailnet_ranges_are_private_and_their_neighbours_are_not(
    ) -> Result<(), std::net::AddrParseError> {
        for private in ["100.64.0.1", "100.127.255.254", "fd7a:115c:a1e0:ab12::1"] {
            assert!(is_private_network(private.parse()?), "{private}");
        }
        for public in [
            "100.63.255.255",
            "100.128.0.0",
            "fd7a:115c:a1e1::1",
            "10.0.0.1",
        ] {
            assert!(!is_private_network(public.parse()?), "{public}");
        }
        Ok(())
    }
}
