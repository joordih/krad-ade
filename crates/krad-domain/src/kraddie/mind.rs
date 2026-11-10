use super::compass::KraddieCompass;
use super::memory::KraddieMemory;
use super::Kraddie;

pub const QUIET_REPLY: &str = "NOTHING_TO_SUGGEST";
const PARAGRAPH: &str = "\n\n";
const LINE: char = '\n';
const BULLET: &str = "- ";
const COMPASS_HEADING: &str = "Your compass, the guidance the user wants on every run:";
const MEMORY_HEADING: &str = "Keepsakes you carry from earlier runs:";
const MEMORY_HINT: &str = "To keep a note for later runs, run: krad kraddie remember";
const PROJECTS_HEADING: &str = "Other repositories you can look at:";
const MEMORY_HINT_TAIL: &str = "<text>";
const CHECK_IN_INTRO: &str = "a Kraddie of krad-ade, checking in after a quiet spell.";
const CHAT_INTRO: &str = "a Kraddie of krad-ade. The user opened this chat with you: answer as yourself and keep to your compass.";
const CHECK_IN_RULES: &str = "This is a read-only check-in. Do not edit files, run commands that change anything, commit or open pull requests. Look over what is below and suggest, in a few short lines, what would be worth doing next and why. The user decides; you only suggest.";
const CHECK_IN_QUIET: &str =
    "If nothing is worth suggesting, reply with exactly this word and nothing else:";
const RECENT_WORK: &str = "Your recent runs:";
const SESSIONS_LEFT: &str = "Sessions you left open:";
const NOTES: &str = "The user's notes:";
const INBOX: &str = "The attention inbox:";
const NOTHING: &str = "(none)";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KraddieMind {
    pub compass: KraddieCompass,
    pub memory: KraddieMemory,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckInContext {
    pub recent_work: Vec<String>,
    pub sessions_left: Vec<String>,
    pub notes: Vec<String>,
    pub inbox: Vec<String>,
}

fn push_section(text: &mut String, heading: &str, lines: &[String]) {
    text.push_str(PARAGRAPH);
    text.push_str(heading);
    if lines.is_empty() {
        text.push(LINE);
        text.push_str(NOTHING);
        return;
    }
    for line in lines {
        text.push(LINE);
        text.push_str(BULLET);
        text.push_str(line);
    }
}

impl KraddieMind {
    #[must_use]
    pub fn inject(&self, brief: &str, kraddie: &Kraddie) -> String {
        let mut text = brief.to_owned();
        if let Some(compass) = self.compass.standing() {
            text.push_str(PARAGRAPH);
            text.push_str(COMPASS_HEADING);
            text.push_str(PARAGRAPH);
            text.push_str(compass);
        }
        if !self.memory.is_empty() {
            push_section(&mut text, MEMORY_HEADING, self.memory.lines());
        }
        let projects: Vec<String> = kraddie
            .settings
            .projects
            .iter()
            .map(|project| project.repo.display().to_string())
            .collect();
        if !projects.is_empty() {
            push_section(&mut text, PROJECTS_HEADING, &projects);
        }
        text.push_str(PARAGRAPH);
        text.push_str(MEMORY_HINT);
        text.push(' ');
        text.push_str(kraddie.id().as_str());
        text.push(' ');
        text.push_str(MEMORY_HINT_TAIL);
        text
    }

    #[must_use]
    pub fn chat_brief(&self, kraddie: &Kraddie) -> String {
        let intro = format!("You are {}, {CHAT_INTRO}", kraddie.name);
        self.inject(&intro, kraddie)
    }

    #[must_use]
    pub fn check_in_brief(&self, kraddie: &Kraddie, context: &CheckInContext) -> String {
        let mut text = format!("You are {}, {CHECK_IN_INTRO}", kraddie.name);
        text.push_str(PARAGRAPH);
        text.push_str(CHECK_IN_RULES);
        text.push(' ');
        text.push_str(CHECK_IN_QUIET);
        text.push(' ');
        text.push_str(QUIET_REPLY);
        push_section(&mut text, RECENT_WORK, &context.recent_work);
        push_section(&mut text, SESSIONS_LEFT, &context.sessions_left);
        push_section(&mut text, NOTES, &context.notes);
        push_section(&mut text, INBOX, &context.inbox);
        self.inject(&text, kraddie)
    }
}

#[must_use]
pub fn is_quiet_reply(reply: &str) -> bool {
    let reply = reply
        .trim()
        .trim_matches(|char: char| !char.is_alphanumeric() && char != '_');
    reply.is_empty() || reply == QUIET_REPLY
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentPreset;
    use crate::kraddie::KraddieWake;

    fn kraddie() -> Kraddie {
        Kraddie::new(
            "Night Owl".parse().unwrap_or_else(|_| unreachable!()),
            AgentPreset::ClaudeCode,
        )
    }

    #[test]
    fn every_run_carries_the_compass_and_the_memory() {
        let mut kraddie = kraddie();
        kraddie
            .settings
            .add_project(crate::kraddie::KraddieWorkspace {
                environment: crate::ids::EnvironmentName::new("main".to_owned()),
                repo: std::path::PathBuf::from("/work/docs"),
            });
        let mut mind = KraddieMind {
            compass: KraddieCompass::try_from("# Compass\n\nNever push to main.".to_owned())
                .unwrap_or_else(|_| unreachable!()),
            ..KraddieMind::default()
        };
        assert!(mind.memory.add("CI runs on the self-hosted runner").is_ok());
        let automation = crate::kraddie::Automation::new(
            crate::kraddie::AutomationId::FIRST,
            "Check in".parse().unwrap_or_else(|_| unreachable!()),
        );
        let brief = mind.inject(
            &automation.brief(&kraddie.name, KraddieWake::Manual, None),
            &kraddie,
        );
        assert!(brief.starts_with("You are Night Owl"), "{brief}");
        assert!(brief.contains("wants on every run:\n\n# Compass\n\nNever push to main."));
        assert!(brief.contains("earlier runs:\n- CI runs on the self-hosted runner"));
        assert!(brief.contains("look at:\n- /work/docs"), "{brief}");
        assert!(
            brief.ends_with("krad kraddie remember night-owl <text>"),
            "{brief}"
        );
    }

    #[test]
    fn a_fresh_mind_adds_only_the_memory_hint() {
        let kraddie = kraddie();
        let brief = KraddieMind::default().inject("Base.", &kraddie);
        assert!(!brief.contains(COMPASS_HEADING));
        assert!(!brief.contains(MEMORY_HEADING));
        assert!(brief.starts_with("Base.\n\n"));
    }

    #[test]
    fn a_check_in_is_read_only_and_names_what_it_looks_over() {
        let context = CheckInContext {
            recent_work: vec!["Mon 09:00 · Schedule · Finished".to_owned()],
            notes: vec!["Release checklist".to_owned()],
            ..CheckInContext::default()
        };
        let brief = KraddieMind::default().check_in_brief(&kraddie(), &context);
        assert!(brief.contains("read-only check-in"), "{brief}");
        assert!(brief.contains(QUIET_REPLY));
        assert!(brief.contains("recent runs:\n- Mon 09:00 · Schedule · Finished"));
        assert!(brief.contains("left open:\n(none)"));
        assert!(brief.contains("notes:\n- Release checklist"));
    }

    #[test]
    fn a_kraddie_chat_starts_from_its_identity_compass_and_keepsakes() {
        let mut mind = KraddieMind::default();
        assert!(mind.memory.add("Prefers rebase over merge").is_ok());
        let brief = mind.chat_brief(&kraddie());
        assert!(
            brief.starts_with("You are Night Owl, a Kraddie of krad-ade."),
            "{brief}"
        );
        assert!(brief.contains("- Prefers rebase over merge"));
    }

    #[test]
    fn only_a_bare_quiet_word_or_silence_counts_as_quiet() {
        assert!(is_quiet_reply("NOTHING_TO_SUGGEST"));
        assert!(is_quiet_reply("  `NOTHING_TO_SUGGEST`.\n"));
        assert!(is_quiet_reply(""));
        assert!(!is_quiet_reply("Run the flaky test again."));
        assert!(!is_quiet_reply("NOTHING_TO_SUGGEST, but maybe update deps"));
    }
}
