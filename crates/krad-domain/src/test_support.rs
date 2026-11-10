const FLOAT_EPSILON: f32 = 1e-6;

pub(crate) fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < FLOAT_EPSILON,
        "{actual} differs from {expected}"
    );
}

pub(crate) fn user(text: &str) -> crate::ChatEntry {
    crate::ChatEntry::User {
        text: text.to_owned(),
        preset: crate::AgentPreset::ClaudeCode,
        model: None,
        images: Vec::new(),
    }
}

pub(crate) fn agent_text(text: &str) -> crate::ChatEntry {
    crate::ChatEntry::Event(crate::ChatEvent::Text(text.to_owned()))
}

pub(crate) fn agent_delta(text: &str) -> crate::ChatEntry {
    crate::ChatEntry::Event(crate::ChatEvent::TextDelta(text.to_owned()))
}
