use std::process::{Command, Stdio};

const GSETTINGS: &str = "gsettings";
const READ: &str = "get";
const INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const ANIMATIONS_KEY: &str = "enable-animations";
const ANIMATIONS_OFF: &str = "false";

pub(super) fn reduces_motion() -> bool {
    let output = Command::new(GSETTINGS)
        .args([READ, INTERFACE_SCHEMA, ANIMATIONS_KEY])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            animations_off(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(_) => false,
        Err(error) => {
            tracing::debug!(%error, "gsettings unavailable, keeping animations on");
            false
        }
    }
}

fn animations_off(answer: &str) -> bool {
    answer.trim() == ANIMATIONS_OFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_false_answer_turns_motion_off() {
        assert!(animations_off("false\n"));
        assert!(!animations_off("true\n"));
        assert!(!animations_off(""));
    }
}
