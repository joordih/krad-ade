const SHARED_SKILL_INTRO: &str = "Use the skill defined in";
const SHARED_SKILL_STEPS: &str = "read that file first and follow its instructions.";
const SHARED_SKILL_ARGUMENTS: &str = "Arguments:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    User,
    Assistant,
}

#[must_use]
pub fn shared_skill_message(message: &str, skill_file: &str) -> String {
    let arguments = message
        .trim()
        .split_once(char::is_whitespace)
        .map_or("", |(_, rest)| rest.trim());
    let order = format!("{SHARED_SKILL_INTRO} {skill_file}: {SHARED_SKILL_STEPS}");
    if arguments.is_empty() {
        return order;
    }
    format!("{order}\n{SHARED_SKILL_ARGUMENTS} {arguments}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_skill_becomes_an_order_to_follow_its_file() {
        let message = shared_skill_message(
            " /lint the api folder",
            "/repo/.agents/skills/lint/SKILL.md",
        );
        assert_eq!(
            message,
            "Use the skill defined in /repo/.agents/skills/lint/SKILL.md: read that file first \
             and follow its instructions.\nArguments: the api folder"
        );
        assert!(!shared_skill_message("/lint", "x").contains(SHARED_SKILL_ARGUMENTS));
    }
}
