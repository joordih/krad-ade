use krad_domain::ToolDecision;
use serde_json::Value;

use super::{answered_input, ask_of, AskAnswer, ToolAsk};

impl ToolAsk {
    #[must_use]
    pub fn held(tool: String, input: Value, suggestions: Vec<Value>) -> Self {
        ask_of(String::new(), tool, input, None, suggestions)
    }

    #[must_use]
    pub fn decision(&self, answer: &AskAnswer) -> ToolDecision {
        match answer {
            AskAnswer::Allow => ToolDecision::Allow {
                updated_input: None,
                permissions: Vec::new(),
            },
            AskAnswer::AllowAlways => ToolDecision::Allow {
                updated_input: None,
                permissions: self.suggestions.clone(),
            },
            AskAnswer::Answers(answers) => ToolDecision::Allow {
                updated_input: Some(answered_input(&self.input, answers)),
                permissions: Vec::new(),
            },
            AskAnswer::Deny { message } => ToolDecision::Deny {
                message: message.clone(),
                interrupt: false,
            },
            AskAnswer::ApprovePlan { .. } => ToolDecision::Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AskKind, PickedAnswer};
    use super::*;
    use serde_json::json;

    #[test]
    fn a_held_question_is_answered_with_the_picked_labels_in_its_input() {
        let input = json!({ "questions": [{ "question": "Pick a color", "options": [{ "label": "Blue" }] }] });
        let asked = ToolAsk::held("AskUserQuestion".to_owned(), input, Vec::new());
        assert!(matches!(asked.kind, AskKind::Questions(ref questions) if questions.len() == 1));
        let answer = AskAnswer::Answers(vec![PickedAnswer {
            question: "Pick a color".to_owned(),
            labels: vec!["Blue".to_owned()],
        }]);
        let ToolDecision::Allow {
            updated_input: Some(updated),
            ..
        } = asked.decision(&answer)
        else {
            unreachable!("answered questions are allowed with their answers");
        };
        assert_eq!(updated["answers"], json!({ "Pick a color": "Blue" }));
    }

    #[test]
    fn allowing_a_held_permission_always_hands_back_its_suggestions() {
        let rule = json!({ "type": "addRules" });
        let asked = ToolAsk::held(
            "Bash".to_owned(),
            json!({ "command": "ls" }),
            vec![rule.clone()],
        );
        assert_eq!(
            asked.decision(&AskAnswer::AllowAlways),
            ToolDecision::Allow {
                updated_input: None,
                permissions: vec![rule],
            }
        );
    }
}
