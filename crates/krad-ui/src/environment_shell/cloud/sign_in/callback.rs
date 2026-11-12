const LOOPBACK_STARTS: [&str; 2] = ["http://localhost:", "http://127.0.0.1:"];
const ENCODED: [(&str, &str); 2] = [("%3a", ":"), ("%2f", "/")];
const URL_ENDS: [char; 8] = ['"', '\'', '<', '>', ')', '&', '\u{1b}', '\u{7}'];
const MAX_PORT_DIGITS: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::environment_shell) struct LoopbackCallback {
    pub(in crate::environment_shell) port: u16,
    pub(in crate::environment_shell) url: String,
}

fn decoded(text: &str) -> String {
    let mut plain = text.to_owned();
    for (encoded, character) in ENCODED {
        plain = replace_ignoring_case(&plain, encoded, character);
    }
    plain
}

fn replace_ignoring_case(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in lower.match_indices(needle) {
        out.push_str(&text[last..at]);
        out.push_str(with);
        last = at + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

fn callback_at(rest: &str, start: &str) -> Option<LoopbackCallback> {
    let digits: String = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .take(MAX_PORT_DIGITS + 1)
        .collect();
    if digits.len() > MAX_PORT_DIGITS || rest.len() == digits.len() {
        return None;
    }
    let port: u16 = digits.parse().ok().filter(|port| *port != 0)?;
    let tail: String = rest[digits.len()..]
        .chars()
        .take_while(|character| !character.is_whitespace() && !URL_ENDS.contains(character))
        .collect();
    Some(LoopbackCallback {
        port,
        url: format!("{start}{port}{tail}"),
    })
}

pub(in crate::environment_shell) fn loopback_callbacks(text: &str) -> Vec<LoopbackCallback> {
    let plain = decoded(text);
    let mut found: Vec<LoopbackCallback> = Vec::new();
    for start in LOOPBACK_STARTS {
        for (at, _) in plain.match_indices(start) {
            let Some(callback) = callback_at(&plain[at + start.len()..], start) else {
                continue;
            };
            if !found.iter().any(|known| known.port == callback.port) {
                found.push(callback);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports(text: &str) -> Vec<u16> {
        loopback_callbacks(text)
            .into_iter()
            .map(|callback| callback.port)
            .collect()
    }

    #[test]
    fn a_printed_redirect_names_its_port_and_url() {
        let found = loopback_callbacks("Listening on http://localhost:1455/auth/callback\r\n");
        assert_eq!(
            found,
            [LoopbackCallback {
                port: 1455,
                url: "http://localhost:1455/auth/callback".to_owned()
            }]
        );
        assert_eq!(ports("open http://127.0.0.1:7171 now"), [7171]);
    }

    #[test]
    fn a_redirect_inside_an_authorize_link_is_found_once() {
        let link = "https://claude.ai/oauth/authorize?client_id=x&redirect_uri=http%3A%2F%2Flocalhost%3A54545%2Fcallback&state=y";
        let found = loopback_callbacks(&format!("{link}\n{link}"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].port, 54545);
        assert_eq!(found[0].url, "http://localhost:54545/callback");
        assert_eq!(
            ports("redirect_uri=http%3a%2f%2f127.0.0.1%3a8085&x"),
            [8085]
        );
        assert_eq!(
            ports("still arriving http://localhost:14"),
            Vec::<u16>::new()
        );
    }

    #[test]
    fn colors_and_other_hosts_are_not_callbacks() {
        assert_eq!(
            loopback_callbacks("\u{1b}[4mhttp://localhost:3000\u{1b}[0m"),
            [LoopbackCallback {
                port: 3000,
                url: "http://localhost:3000".to_owned()
            }]
        );
        assert_eq!(ports("https://github.com/login/device"), Vec::<u16>::new());
        assert_eq!(
            ports("http://localhost:/x http://localhost:0"),
            Vec::<u16>::new()
        );
        assert_eq!(ports("http://localhost:999999"), Vec::<u16>::new());
        assert_eq!(ports("http://localhost:70000"), Vec::<u16>::new());
    }
}
