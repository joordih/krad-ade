const BRIEF: &str = "You are answering in the Krad ADE chat panel. Answer concisely in Markdown. Read files when it helps; do not edit files or run commands that change anything unless the user asks.";
const WORKSPACE_LABEL: &str = "Workspace";
const CONTEXT_HEADING: &str = "Context the user attached:";
const FILE_LABEL: &str = "File";
const FOLDER_LABEL: &str = "Folder";
const COMMAND_PREFIX: char = '/';
const SHARED_SKILL_INTRO: &str = "Use the skill defined in";
const SHARED_SKILL_STEPS: &str = "read that file first and follow its instructions.";
const SHARED_SKILL_ARGUMENTS: &str = "Arguments:";
const TERMINAL_LABEL: &str = "Terminal";
const HISTORY_HEADING: &str = "Conversation so far:";
const USER_LABEL: &str = "User";
const ASSISTANT_LABEL: &str = "Assistant";
const FENCE: &str = "```";
const HISTORY_MESSAGES: usize = 8;
const HISTORY_MESSAGE_CHARS: usize = 4_000;
const TRUNCATION_MARK: &str = "…";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorMessage {
    pub speaker: Speaker,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatContext {
    pub workspace: Option<String>,
    pub files: Vec<String>,
    pub folders: Vec<String>,
    pub terminals: Vec<(String, String)>,
    pub orchestration: Option<String>,
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(HISTORY_MESSAGE_CHARS) {
        Some((end, _)) => format!("{}{TRUNCATION_MARK}", &text[..end]),
        None => text.to_owned(),
    }
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

#[must_use]
pub fn compose_prompt(message: &str, context: &ChatContext, history: &[PriorMessage]) -> String {
    let message = message.trim();
    let sections = [
        orchestration_section(context),
        workspace_section(context),
        context_section(context),
        history_section(history),
    ]
    .concat();
    if message.starts_with(COMMAND_PREFIX) {
        return format!("{message}{sections}");
    }
    format!("{BRIEF}{sections}\n\n{USER_LABEL}: {message}")
}

fn orchestration_section(context: &ChatContext) -> String {
    context
        .orchestration
        .as_ref()
        .map_or_else(String::new, |guide| format!("\n\n{}\n", guide.trim()))
}

fn workspace_section(context: &ChatContext) -> String {
    context
        .workspace
        .as_ref()
        .map_or_else(String::new, |workspace| {
            format!("\n{WORKSPACE_LABEL}: {workspace}")
        })
}

fn context_section(context: &ChatContext) -> String {
    if context.files.is_empty() && context.folders.is_empty() && context.terminals.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!("\n\n{CONTEXT_HEADING}")];
    for file in &context.files {
        parts.push(format!("\n- {FILE_LABEL}: {file}"));
    }
    for folder in &context.folders {
        parts.push(format!("\n- {FOLDER_LABEL}: {folder}"));
    }
    for (title, screen) in &context.terminals {
        parts.push(format!(
            "\n- {TERMINAL_LABEL} \"{title}\":\n{FENCE}\n{}\n{FENCE}",
            screen.trim_end()
        ));
    }
    parts.concat()
}

fn history_section(history: &[PriorMessage]) -> String {
    let recent = &history[history.len().saturating_sub(HISTORY_MESSAGES)..];
    if recent.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!("\n\n{HISTORY_HEADING}")];
    for prior in recent {
        let label = match prior.speaker {
            Speaker::User => USER_LABEL,
            Speaker::Assistant => ASSISTANT_LABEL,
        };
        parts.push(format!("\n{label}: {}", cut(prior.text.trim())));
    }
    parts.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(speaker: Speaker, text: &str) -> PriorMessage {
        PriorMessage {
            speaker,
            text: text.to_owned(),
        }
    }

    #[test]
    fn prompt_lists_context_then_history_then_the_message() {
        let context = ChatContext {
            workspace: Some("/work/portfolio".to_owned()),
            files: vec!["server/auth/refresh.ts".to_owned()],
            folders: vec!["server/auth".to_owned()],
            terminals: vec![("claude · payments".to_owned(), "> waiting\n\n".to_owned())],
            orchestration: None,
        };
        let history = [
            said(Speaker::User, "Why does it race?"),
            said(Speaker::Assistant, "Two tabs rotate at once."),
        ];
        let prompt = compose_prompt("  How do I fix it? ", &context, &history);
        let order = [
            "Workspace: /work/portfolio",
            "- File: server/auth/refresh.ts",
            "- Folder: server/auth",
            "- Terminal \"claude · payments\":\n```\n> waiting\n```",
            "User: Why does it race?",
            "Assistant: Two tabs rotate at once.",
            "User: How do I fix it?",
        ];
        let mut from = 0;
        for piece in order {
            let at = prompt[from..].find(piece).map(|at| at + from);
            assert!(at.is_some(), "{piece:?} missing or out of order");
            from = at.unwrap_or(from);
        }
        assert!(prompt.ends_with("User: How do I fix it?"));
    }

    #[test]
    fn only_the_latest_history_is_resent_and_long_messages_are_cut() {
        let long = "x".repeat(HISTORY_MESSAGE_CHARS + 10);
        let mut history: Vec<PriorMessage> = (0..HISTORY_MESSAGES + 3)
            .map(|index| said(Speaker::User, &format!("old {index}")))
            .collect();
        history.push(said(Speaker::Assistant, &long));
        let prompt = compose_prompt("next", &ChatContext::default(), &history);
        assert!(!prompt.contains("old 0"));
        assert!(prompt.contains(&format!("old {}", HISTORY_MESSAGES + 2)));
        assert!(prompt.contains(TRUNCATION_MARK));
        assert!(!prompt.contains(&long));
    }

    #[test]
    fn a_slash_command_leads_the_prompt_with_the_context_as_its_arguments() {
        let context = ChatContext {
            workspace: Some("/work".to_owned()),
            ..ChatContext::default()
        };
        let prompt = compose_prompt(" /review the auth flow", &context, &[]);
        assert!(prompt.starts_with("/review the auth flow\nWorkspace: /work"));
        assert!(!prompt.contains(BRIEF));
    }

    #[test]
    fn the_orchestration_guide_comes_before_the_workspace() {
        let context = ChatContext {
            workspace: Some("/work".to_owned()),
            orchestration: Some("Drive agents with the bus.".to_owned()),
            ..ChatContext::default()
        };
        let prompt = compose_prompt("build it", &context, &[]);
        let guide = prompt.find("Drive agents with the bus.");
        let workspace = prompt.find("Workspace: /work");
        assert!(guide.is_some() && guide < workspace);
    }

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

    #[test]
    fn a_bare_message_needs_no_sections() {
        let prompt = compose_prompt("hi", &ChatContext::default(), &[]);
        assert!(!prompt.contains(CONTEXT_HEADING));
        assert!(!prompt.contains(HISTORY_HEADING));
        assert!(prompt.ends_with("User: hi"));
    }
}
