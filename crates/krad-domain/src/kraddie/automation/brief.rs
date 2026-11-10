use super::super::name::KraddieName;
use super::super::trigger::KraddieWake;
use super::Automation;

const PARAGRAPH: &str = "\n\n";
const INTRO: &str = "You are";
const INTRO_TAIL: &str = "a Kraddie of krad-ade: a persistent worker that wakes on its own.";
const ROUTINE: &str = "This run is your automation";
const WOKEN_BY: &str = "Woken by:";
const ASKED: &str = "This time you were asked:";
const CLOSING: &str =
    "When you finish, end with a short report of what you did. If you need the user, ask and wait.";

impl Automation {
    #[must_use]
    pub fn brief(&self, kraddie: &KraddieName, wake: KraddieWake, prompt: Option<&str>) -> String {
        let mut brief = format!(
            "{INTRO} {kraddie}, {INTRO_TAIL} {ROUTINE} \u{201c}{}\u{201d}.",
            self.name
        );
        let instructions = self.instructions.trim();
        if !instructions.is_empty() {
            brief.push_str(PARAGRAPH);
            brief.push_str(instructions);
        }
        brief.push_str(PARAGRAPH);
        brief.push_str(WOKEN_BY);
        brief.push(' ');
        brief.push_str(wake.label());
        if let Some(prompt) = prompt.map(str::trim).filter(|prompt| !prompt.is_empty()) {
            brief.push_str(PARAGRAPH);
            brief.push_str(ASKED);
            brief.push(' ');
            brief.push_str(prompt);
        }
        brief.push_str(PARAGRAPH);
        brief.push_str(CLOSING);
        brief
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AppTrigger, AutomationId};
    use super::*;

    #[test]
    fn the_brief_names_the_kraddie_its_automation_its_wake_and_the_extra_prompt() {
        let kraddie: KraddieName = "Nightly".parse().unwrap_or_else(|_| unreachable!());
        let mut automation = Automation::new(
            AutomationId::FIRST,
            "Keep green".parse().unwrap_or_else(|_| unreachable!()),
        );
        automation.instructions = "  Keep the build green.  ".to_owned();
        let brief = automation.brief(
            &kraddie,
            KraddieWake::App(AppTrigger::GitChanged),
            Some(" fix job 7 "),
        );
        assert!(brief.starts_with("You are Nightly, a Kraddie"), "{brief}");
        assert!(brief.contains("\u{201c}Keep green\u{201d}"), "{brief}");
        assert!(brief.contains("\n\nKeep the build green.\n\n"), "{brief}");
        assert!(brief.contains("Woken by: Git changes"), "{brief}");
        assert!(brief.contains("asked: fix job 7\n\n"), "{brief}");
        let plain = automation.brief(&kraddie, KraddieWake::Manual, Some("   "));
        assert!(!plain.contains(ASKED), "{plain}");
    }
}
