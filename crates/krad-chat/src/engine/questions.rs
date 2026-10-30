use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;

use super::wire::ToolSpec;
use crate::control::{PickedAnswer, Question};

pub(crate) const QUESTIONS_TOOL: &str = "ask_user";
pub(crate) const QUESTIONS_LABEL: &str = "AskUserQuestion";
const QUESTIONS_DESCRIPTION: &str = "Ask the user one to four multiple-choice questions and wait for the answers. Use it when a decision is the user's to make, such as picking between approaches, languages or names; do not use it for things you can find out yourself. Each question has two to four options; the user can always answer with their own text instead.";
const NO_QUESTIONS_TEXT: &str = "ask_user needs at least one question with options";
const BAD_QUESTIONS_TEXT: &str = "Invalid arguments for ask_user:";
const ANSWERS_INTRO: &str = "The user answered:";
const DISMISSED_TEXT: &str = "The user dismissed the questions without answering.";
const LABEL_SEPARATOR: &str = ", ";
const MAX_QUESTIONS: usize = 4;

#[derive(Deserialize)]
struct QuestionsArgs {
    questions: Vec<Question>,
}

pub(crate) fn spec() -> ToolSpec {
    ToolSpec {
        name: Cow::Borrowed(QUESTIONS_TOOL),
        description: Cow::Borrowed(QUESTIONS_DESCRIPTION),
        parameters: json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_QUESTIONS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string", "description": "The full question, ending with a question mark"},
                            "header": {"type": "string", "description": "A short label of at most 12 characters"},
                            "multiSelect": {"type": "boolean", "description": "Whether several options can be picked"},
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {"type": "string", "description": "The choice, in one to five words"},
                                        "description": {"type": "string", "description": "What picking it means"}
                                    },
                                    "required": ["label"]
                                }
                            }
                        },
                        "required": ["question", "options"]
                    }
                }
            },
            "required": ["questions"]
        }),
    }
}

pub(crate) fn parse(arguments: &Value) -> Result<Vec<Question>, String> {
    let QuestionsArgs { questions } = QuestionsArgs::deserialize(arguments)
        .map_err(|error| format!("{BAD_QUESTIONS_TEXT} {error}"))?;
    let questions: Vec<Question> = questions
        .into_iter()
        .filter(|question| !question.text.trim().is_empty() && !question.options.is_empty())
        .take(MAX_QUESTIONS)
        .collect();
    if questions.is_empty() {
        return Err(NO_QUESTIONS_TEXT.to_owned());
    }
    Ok(questions)
}

pub(crate) fn summary(questions: &[Question]) -> String {
    questions
        .first()
        .map_or_else(String::new, |question| question.text.clone())
}

pub(crate) fn answered(answers: &[PickedAnswer]) -> String {
    if answers.is_empty() {
        return DISMISSED_TEXT.to_owned();
    }
    let lines: Vec<String> = answers
        .iter()
        .map(|answer| {
            format!(
                "- {}: {}",
                answer.question,
                answer.labels.join(LABEL_SEPARATOR)
            )
        })
        .collect();
    format!("{ANSWERS_INTRO}\n{}", lines.join("\n"))
}

pub(crate) const fn dismissed() -> &'static str {
    DISMISSED_TEXT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_parse_from_the_model_and_need_options() {
        let questions = parse(&json!({"questions": [
            {"question": "Which sort?", "header": "Sort", "options": [
                {"label": "Quicksort", "description": "Fast on average"},
                {"label": "Merge sort"}
            ]},
            {"question": "  ", "options": [{"label": "x"}]}
        ]}))
        .unwrap_or_default();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].options.len(), 2);
        assert_eq!(summary(&questions), "Which sort?");
        assert!(parse(&json!({"questions": []})).is_err());
        assert!(parse(&json!({"question": "no list"})).is_err());
    }

    #[test]
    fn answers_go_back_to_the_model_as_text() {
        let text = answered(&[PickedAnswer {
            question: "Which sort?".to_owned(),
            labels: vec!["Quicksort".to_owned()],
        }]);
        assert_eq!(text, "The user answered:\n- Which sort?: Quicksort");
        assert_eq!(answered(&[]), dismissed());
    }
}
