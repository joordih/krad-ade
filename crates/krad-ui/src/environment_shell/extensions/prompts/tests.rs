use super::*;

fn item(value: &str) -> PickItem {
    PickItem {
        value: value.to_owned(),
        label: value.to_uppercase(),
        detail: None,
        icon: None,
    }
}

#[test]
fn a_many_pick_returns_what_stays_ticked_in_the_offered_order() {
    let items = [item("web"), item("db"), item("cache")];
    let mut chosen = BTreeSet::new();
    for value in ["cache", "web", "db", "db"] {
        toggle(&mut chosen, value.to_owned());
    }
    assert_eq!(chosen_values(&items, &chosen), ["web", "cache"]);
}

#[test]
fn a_many_pick_with_nothing_ticked_returns_an_empty_choice() {
    assert_eq!(
        chosen_values(&[item("web")], &BTreeSet::new()),
        Vec::<String>::new()
    );
}

fn call_ids(count: usize) -> Result<Vec<CallId>, Box<dyn std::error::Error>> {
    use futures::StreamExt as _;
    use krad_extensions::program::{ProgramConnection, ProgramEvent, REQUEST_TIMEOUT};
    use std::io::{pipe, BufReader, Write as _};
    let (app_reads, mut program_writes) = pipe()?;
    let (_program_reads, app_writes) = pipe()?;
    let (_connection, mut events) =
        ProgramConnection::over(BufReader::new(app_reads), app_writes, REQUEST_TIMEOUT)?;
    for index in 0..count {
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":{index},"method":"window/confirm","params":{{"title":"?","confirm":"Yes"}}}}"#
        );
        write!(
            program_writes,
            "Content-Length: {}\r\n\r\n{body}",
            body.len()
        )?;
    }
    let mut ids = Vec::new();
    while ids.len() < count {
        match futures::executor::block_on(events.next()) {
            Some(ProgramEvent::Call { id, .. }) => ids.push(id),
            _ => return Err("the program stopped".into()),
        }
    }
    Ok(ids)
}

fn confirm(key: &SlotKey, id: CallId) -> Prompt {
    Prompt::Confirm(
        key.clone(),
        id,
        ConfirmParams {
            title: "Remove?".to_owned(),
            detail: None,
            confirm: "Remove".to_owned(),
            tone: Tone::Error,
        },
    )
}

#[test]
fn waiting_prompts_are_counted_per_extension() -> Result<(), Box<dyn std::error::Error>> {
    let docker = SlotKey {
        extension: "docker".parse()?,
        environment: None,
    };
    let other = SlotKey {
        extension: "other".parse()?,
        environment: None,
    };
    let mut queue = VecDeque::new();
    for id in call_ids(MAX_PENDING_PROMPTS)? {
        queue.push_back(confirm(&docker, id));
    }
    assert_eq!(waiting_for(&queue, &docker), MAX_PENDING_PROMPTS);
    assert_eq!(waiting_for(&queue, &other), 0);
    Ok(())
}
