use krad_domain::review::{MergeRequestIid, MergeRequestQuery, MergeRequestState, StateFilter};
use krad_github::{GithubClient, GithubToken};
use std::error::Error;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{Ipv4Addr, TcpListener};
use std::thread::JoinHandle;

type TestResult = Result<(), Box<dyn Error>>;

const LINE_END: &str = "\r\n";
const AUTHORIZATION: &str = "authorization:";
const USER: &str = r#"{"id":1,"login":"octo","name":"Octo Cat"}"#;
const PULLS: &str = r#"[{"number":12,"title":"Fix the durations","state":"open","user":{"id":1,"login":"octo"},"head":{"ref":"fix-durations","sha":"abc1234","repo":{"full_name":"octo/app"}},"base":{"ref":"main","sha":"def5678","repo":{"full_name":"octo/app"}},"html_url":"https://github.com/octo/app/pull/12","created_at":"2026-11-01T10:00:00Z","updated_at":"2026-11-02T10:00:00Z"},{"number":9,"title":"Old work","state":"closed","merged_at":"2026-10-01T10:00:00Z","user":{"id":2,"login":"hub"},"head":{"ref":"old","sha":"0001111"},"base":{"ref":"main","sha":"def5678"},"html_url":"https://github.com/octo/app/pull/9","created_at":"2026-09-01T10:00:00Z","updated_at":"2026-10-01T10:00:00Z"}]"#;

struct Heard {
    line: String,
    authorization: Option<String>,
}

fn serve(bodies: Vec<&'static str>) -> std::io::Result<(String, JoinHandle<Vec<Heard>>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let url = format!("http://{}", listener.local_addr()?);
    let server = std::thread::spawn(move || {
        let mut heard = Vec::new();
        for body in bodies {
            let Ok((stream, _)) = listener.accept() else {
                break;
            };
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap_or_default();
            let mut authorization = None;
            let mut header = String::new();
            while reader
                .read_line(&mut header)
                .is_ok_and(|read| read > LINE_END.len())
            {
                let lower = header.to_ascii_lowercase();
                if let Some(value) = lower.strip_prefix(AUTHORIZATION) {
                    authorization = Some(value.trim().to_owned());
                }
                header.clear();
            }
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            (&stream).write_all(reply.as_bytes()).unwrap_or_default();
            heard.push(Heard {
                line: line.trim().to_owned(),
                authorization,
            });
        }
        heard
    });
    Ok((url, server))
}

fn token() -> GithubToken {
    GithubToken {
        access_token: "fixture-token".to_owned(),
    }
}

#[test]
fn the_signed_in_user_and_the_open_pull_requests_are_read_from_the_api() -> TestResult {
    let (url, server) = serve(vec![USER, PULLS])?;
    let client = GithubClient::at(&url, &token());
    let me = client.current_user()?;
    assert_eq!(
        (me.username.as_str(), me.name.as_str()),
        ("octo", "Octo Cat")
    );
    let open = client.merge_requests(&"octo/app".parse()?, MergeRequestQuery::default())?;
    let listed: Vec<(MergeRequestIid, MergeRequestState)> = open
        .iter()
        .map(|request| (request.iid, request.state))
        .collect();
    assert_eq!(
        listed,
        [
            (MergeRequestIid(12), MergeRequestState::Opened),
            (MergeRequestIid(9), MergeRequestState::Merged),
        ]
    );
    let heard = server.join().map_err(|_| "the server panicked")?;
    assert_eq!(heard[0].line, "GET /user HTTP/1.1");
    assert!(heard[1].line.starts_with("GET /repos/octo/app/pulls?"));
    assert!(heard[1].line.contains("state=open"));
    assert!(heard
        .iter()
        .all(|heard| heard.authorization.as_deref() == Some("bearer fixture-token")));
    Ok(())
}

#[test]
fn merged_pull_requests_are_the_closed_ones_with_a_merge() -> TestResult {
    let (url, server) = serve(vec![PULLS])?;
    let client = GithubClient::at(&url, &token());
    let merged = client.merge_requests(
        &"octo/app".parse()?,
        MergeRequestQuery {
            state: StateFilter::Merged,
            ..MergeRequestQuery::default()
        },
    )?;
    assert_eq!(
        merged.iter().map(|request| request.iid).collect::<Vec<_>>(),
        [MergeRequestIid(9)]
    );
    let heard = server.join().map_err(|_| "the server panicked")?;
    assert!(heard[0].line.contains("state=closed"));
    Ok(())
}
