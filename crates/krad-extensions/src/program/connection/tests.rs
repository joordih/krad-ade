use super::*;
use futures::executor::block_on;
use futures::StreamExt as _;
use krad_extension_api::protocol::{Answer, Confirm, Empty, Shutdown, Toast};
use serde_json::json;
use std::io::{pipe, BufReader, PipeReader, PipeWriter};
use std::time::Duration;

use crate::program::rpc::{write_message, Incoming, METHOD_NOT_FOUND};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const QUICK: Duration = Duration::from_millis(50);

struct FakeProgram {
    from_app: BufReader<PipeReader>,
    to_app: PipeWriter,
}

impl FakeProgram {
    fn read(&mut self) -> Result<Incoming, Box<dyn std::error::Error>> {
        read_message(&mut self.from_app)?.ok_or_else(|| "the app closed its pipe".into())
    }

    fn send(&mut self, message: &Outgoing) -> Result<(), ProgramError> {
        write_message(&mut self.to_app, message)
    }
}

fn connect(
    timeout: Duration,
) -> Result<(ProgramConnection, mpsc::Receiver<ProgramEvent>, FakeProgram), ProgramError> {
    let (app_reads, program_writes) = pipe()?;
    let (program_reads, app_writes) = pipe()?;
    let (connection, events) =
        ProgramConnection::over(BufReader::new(app_reads), app_writes, timeout)?;
    let program = FakeProgram {
        from_app: BufReader::new(program_reads),
        to_app: program_writes,
    };
    Ok((connection, events, program))
}

#[test]
fn a_request_resolves_with_the_programs_answer() -> TestResult {
    let (connection, _events, mut program) = connect(REQUEST_TIMEOUT)?;
    let answered = thread::spawn(
        move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let Ok(Incoming::Request { id, method, .. }) =
                program.read().map_err(|e| e.to_string())
            else {
                return Err("expected a request".into());
            };
            assert_eq!(method, "shutdown");
            program.send(&Outgoing::Reply {
                id,
                outcome: Ok(json!({})),
            })?;
            Ok(())
        },
    );
    let result = block_on(connection.request::<Shutdown>(&Empty {}))?;
    assert_eq!(result, Empty {});
    answered
        .join()
        .map_err(|_| "the fake program panicked")?
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn a_request_without_an_answer_times_out() -> TestResult {
    let (connection, _events, _program) = connect(QUICK)?;
    let outcome = block_on(connection.request::<Shutdown>(&Empty {}));
    assert!(matches!(outcome, Err(ProgramError::Timeout)));
    Ok(())
}

#[test]
fn a_program_request_arrives_typed_and_takes_the_apps_answer() -> TestResult {
    let (connection, mut events, mut program) = connect(REQUEST_TIMEOUT)?;
    program.send(&Outgoing::Request {
        id: 9,
        method: Confirm::METHOD,
        params: json!({"title": "Stop web?", "confirm": "Stop"}),
    })?;
    let Some(ProgramEvent::Call {
        id,
        call: AppCall::Confirm(params),
    }) = block_on(events.next())
    else {
        return Err("expected a confirm call".into());
    };
    assert_eq!(params.title, "Stop web?");
    connection.answer::<Confirm>(id, &Answer::Confirmed)?;
    assert_eq!(
        program.read()?,
        Incoming::Response {
            id: 9,
            outcome: Ok(json!("confirmed"))
        }
    );
    Ok(())
}

#[test]
fn notifications_arrive_typed_and_unknown_requests_are_refused() -> TestResult {
    let (_connection, mut events, mut program) = connect(REQUEST_TIMEOUT)?;
    program.send(&Outgoing::Request {
        id: 3,
        method: "window/teleport",
        params: json!({}),
    })?;
    program.send(&Outgoing::Notification {
        method: Toast::METHOD,
        params: json!({"message": "Pulled"}),
    })?;
    let refusal = program.read()?;
    assert!(matches!(
        refusal,
        Incoming::Response {
            id: 3,
            outcome: Err(ResponseError {
                code: METHOD_NOT_FOUND,
                ..
            })
        }
    ));
    assert!(matches!(
        block_on(events.next()),
        Some(ProgramEvent::Notice(AppNotice::Toast(_)))
    ));
    Ok(())
}

#[test]
fn a_closed_program_fails_what_waits_and_says_it_stopped() -> TestResult {
    let (connection, mut events, program) = connect(REQUEST_TIMEOUT)?;
    let waiting = connection.request::<Shutdown>(&Empty {});
    drop(program);
    assert!(matches!(block_on(waiting), Err(ProgramError::Stopped)));
    assert_eq!(block_on(events.next()), Some(ProgramEvent::Stopped));
    Ok(())
}
