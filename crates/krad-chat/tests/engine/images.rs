use krad_chat::{new_session_id, read_engine_transcript, LiveEvent, LiveSession};
use krad_domain::{ChatEntry, Mention, Prompt};
use std::fs;
use std::io;

use super::support::{completions, config, fake_gateway, finished, scratch, start, until};

#[test]
fn an_image_mention_reaches_the_model_and_stays_in_the_history() -> io::Result<()> {
    let root = scratch("images");
    fs::create_dir_all(root.join("work"))?;
    let shot = root.join("shot.png");
    fs::write(&shot, b"png")?;
    let (url, seen) = fake_gateway(Vec::new())?;
    let id = new_session_id();
    let (chat, events) = start(config(&url, &root), &LiveSession::Start(id.clone()));
    until(&events, |event| matches!(event, LiveEvent::Ready(_)));
    chat.send(Prompt::new(
        "what is this?",
        vec![Mention::from(shot.clone())],
    ));
    until(&events, finished);
    drop(chat);
    let bodies = completions(&seen);
    let sent: serde_json::Value = serde_json::from_str(&bodies[0])?;
    let content = &sent["messages"][1]["content"];
    assert_eq!(content[0]["image_url"]["url"], "data:image/png;base64,cG5n");
    assert_eq!(content[1]["text"], "what is this?");
    let entries = read_engine_transcript(&root.join("sessions"), &root.join("work"), &id)?;
    assert!(matches!(
        entries.iter().find(|entry| matches!(entry, ChatEntry::User { .. })),
        Some(ChatEntry::User { images, .. }) if images.iter().map(|image| &image.path).eq([&shot])
    ));
    Ok(())
}
