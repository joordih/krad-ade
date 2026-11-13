use super::*;

#[test]
fn every_typed_edit_is_answered_and_a_client_diff_of_runtime_items_is_ignored() -> TestResult {
    let folder = Folder::new("answers");
    let seeded = payments()?;
    let runtime = start(
        RuntimeState {
            environments: vec![seeded.clone()],
            ..RuntimeState::default()
        },
        &folder.0,
    )?;
    let (mut events, _) = subscribed(&runtime)?;
    let mut forged = seeded.clone();
    forged.nodes.clear();
    let ignored = EnvironmentCommand::Change {
        environment: seeded.name.clone(),
        changes: EnvironmentChange::between(&seeded, &forged),
    };
    assert_eq!(send(&runtime, ignored)?, Reply::Accepted);
    let renamed = runtime.next_request();
    let rename: Command = (
        seeded.name.clone(),
        EnvironmentEdit::from(GroupEdit::Rename {
            group: GroupId(1),
            name: "payments-api".to_owned(),
        }),
    )
        .into();
    assert_eq!(block_on(runtime.send(renamed, rename))?, Reply::Accepted);
    let lost = runtime.next_request();
    let elsewhere: Command = (
        name("nowhere"),
        EnvironmentEdit::from(GroupEdit::Remove { group: GroupId(1) }),
    )
        .into();
    assert_eq!(
        block_on(runtime.send(lost, elsewhere))?,
        Reply::Refused {
            code: ErrorCode::NotFound,
            message: "this runtime has no environment of that name".to_owned(),
        }
    );
    let heard = events_until_pong(&runtime, &mut events)?;
    let answered: Vec<_> = heard
        .iter()
        .filter_map(|kind| match kind {
            EventKind::EnvironmentChanged { answers, .. } => *answers,
            _ => None,
        })
        .collect();
    assert_eq!(answered, vec![renamed, lost]);
    let snapshot = block_on(runtime.snapshot(Scope::Runtime))?.environments;
    let kept = snapshot.first().ok_or("payments is kept")?;
    assert_eq!(kept.nodes, seeded.nodes);
    assert_eq!(
        kept.group(GroupId(1)).map(|group| group.name.as_str()),
        Some("payments-api")
    );
    Ok(())
}
