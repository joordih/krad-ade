use krad_domain::DeliveryOutcome;
use std::io::{self, Write};
use std::iter;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::webhook::WebhookRequest;

const CURL_PROGRAM: &str = "curl";
const FIXED_ARGS: [&str; 9] = [
    "--disable",
    "--silent",
    "--output",
    "/dev/null",
    "--write-out",
    "%{http_code}",
    "--proto",
    "=http,https",
    "--globoff",
];
const MAX_TIME_FLAG: &str = "--max-time";
const CONNECT_TIMEOUT_FLAG: &str = "--connect-timeout";
const CONFIG_FROM_STDIN: [&str; 2] = ["--config", "-"];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const NO_EXPECT_HEADER: &str = "Expect:";

pub(super) struct NoCurl;

fn quoted(value: &str) -> String {
    let mut text = String::with_capacity(value.len() + 2);
    text.push('"');
    for character in value.chars() {
        match character {
            '\\' => text.push_str("\\\\"),
            '"' => text.push_str("\\\""),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\u{b}' => text.push_str("\\v"),
            other => text.push(other),
        }
    }
    text.push('"');
    text
}

fn config(request: &WebhookRequest) -> String {
    let headers = request
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .chain(iter::once(NO_EXPECT_HEADER.to_owned()))
        .map(|header| format!("header = {}\n", quoted(&header)));
    iter::once(format!("url = {}\n", quoted(&request.url)))
        .chain(headers)
        .chain(iter::once(format!(
            "data-raw = {}\n",
            quoted(&request.body)
        )))
        .collect()
}

pub(super) fn post_with_curl(
    request: &WebhookRequest,
    timeout: Duration,
) -> Result<DeliveryOutcome, NoCurl> {
    let spawned = Command::new(CURL_PROGRAM)
        .args(FIXED_ARGS)
        .arg(MAX_TIME_FLAG)
        .arg(timeout.as_secs_f64().to_string())
        .arg(CONNECT_TIMEOUT_FLAG)
        .arg(CONNECT_TIMEOUT.as_secs_f64().to_string())
        .args(CONFIG_FROM_STDIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(NoCurl),
        Err(_) => return Ok(DeliveryOutcome::Failed),
    };
    let written = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(config(request).as_bytes()));
    let Ok(output) = child.wait_with_output() else {
        return Ok(DeliveryOutcome::Failed);
    };
    let status = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u16>();
    match (written, output.status.success(), status) {
        (Some(Ok(())), true, Ok(status)) => Ok(DeliveryOutcome::of_status(status)),
        _ => Ok(DeliveryOutcome::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_quotes_every_value_and_keeps_the_body_exact() {
        let request = WebhookRequest {
            url: "http://127.0.0.1:1/krad".to_owned(),
            headers: vec![("x-krad-timestamp", "now".to_owned())],
            body: "{\"detail\":\"a \\\"quote\\\"\\n\"}\n\t".to_owned(),
        };
        assert_eq!(
            config(&request),
            "url = \"http://127.0.0.1:1/krad\"\n\
             header = \"x-krad-timestamp: now\"\n\
             header = \"Expect:\"\n\
             data-raw = \"{\\\"detail\\\":\\\"a \\\\\\\"quote\\\\\\\"\\\\n\\\"}\\n\\t\"\n"
        );
    }
}
