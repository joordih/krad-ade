use std::net::{IpAddr, SocketAddr};

use crate::http::Request;

const LOOPBACK_NAMES: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
const SECURE_SCHEME: &str = "https";
const PLAIN_SCHEME: &str = "http";

pub(crate) struct Guard {
    names: Vec<String>,
    scheme: &'static str,
}

fn host_name(host: &str) -> &str {
    if host.starts_with('[') {
        return host
            .find(']')
            .and_then(|end| host.get(..=end))
            .unwrap_or(host);
    }
    host.split_once(':').map_or(host, |(name, _)| name)
}

fn bracketed(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

impl Guard {
    pub(crate) fn new(bind: SocketAddr, names: &[String], secure: bool) -> Self {
        let mut known: Vec<String> = LOOPBACK_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        known.push(bracketed(bind.ip()));
        known.extend(names.iter().map(|name| name.to_ascii_lowercase()));
        Self {
            names: known,
            scheme: if secure { SECURE_SCHEME } else { PLAIN_SCHEME },
        }
    }

    pub(crate) fn known_host(&self, request: &Request) -> bool {
        request.header("host").is_some_and(|host| {
            let name = host_name(host).to_ascii_lowercase();
            self.names.contains(&name)
        })
    }

    pub(crate) fn same_origin(&self, request: &Request) -> bool {
        let (Some(host), Some(origin)) = (request.header("host"), request.header("origin")) else {
            return false;
        };
        self.known_host(request)
            && origin.eq_ignore_ascii_case(&format!("{}://{host}", self.scheme))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::read_request;

    fn request(head: &str) -> Result<Request, Box<dyn std::error::Error>> {
        let raw = format!("GET /runtime HTTP/1.1\r\n{head}\r\n\r\n");
        let mut pending = Vec::new();
        Ok(read_request(&mut raw.as_bytes(), &mut pending)?.ok_or("no request")?)
    }

    fn tailnet_guard() -> Result<Guard, std::net::AddrParseError> {
        Ok(Guard::new(
            "100.101.102.103:7421".parse()?,
            &["machine.tailnet.ts.net".to_owned()],
            true,
        ))
    }

    #[test]
    fn the_page_served_from_a_known_name_is_the_same_origin(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let guard = tailnet_guard()?;
        let asked = request(
            "Host: machine.tailnet.ts.net:7421\r\nOrigin: https://machine.tailnet.ts.net:7421",
        )?;
        assert!(guard.same_origin(&asked));
        Ok(())
    }

    #[test]
    fn a_rebound_name_or_a_foreign_origin_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let guard = tailnet_guard()?;
        let rebound = request("Host: evil.example:7421\r\nOrigin: https://evil.example:7421")?;
        assert!(!guard.same_origin(&rebound));
        let foreign = request("Host: machine.tailnet.ts.net:7421\r\nOrigin: https://evil.example")?;
        assert!(!guard.same_origin(&foreign));
        let plain = request(
            "Host: machine.tailnet.ts.net:7421\r\nOrigin: http://machine.tailnet.ts.net:7421",
        )?;
        assert!(!guard.same_origin(&plain));
        let missing = request("Host: machine.tailnet.ts.net:7421")?;
        assert!(!guard.same_origin(&missing));
        Ok(())
    }

    #[test]
    fn loopback_hosts_with_ports_are_known() -> Result<(), Box<dyn std::error::Error>> {
        let guard = Guard::new("127.0.0.1:7421".parse()?, &[], false);
        for host in ["localhost:7421", "127.0.0.1:7421", "[::1]:7421"] {
            let asked = request(&format!("Host: {host}\r\nOrigin: http://{host}"))?;
            assert!(guard.same_origin(&asked), "{host}");
        }
        Ok(())
    }
}
