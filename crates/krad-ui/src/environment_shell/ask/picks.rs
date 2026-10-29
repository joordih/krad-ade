use krad_chat::{PickedAnswer, Question, QuestionOption};
use std::collections::HashMap;

pub(super) const OTHER_LABEL: &str = "Other";

#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::environment_shell) struct Picks {
    labels: HashMap<String, Vec<String>>,
    step: usize,
}

fn is_other_label(label: &str) -> bool {
    label
        .trim()
        .trim_end_matches(['.', '…'])
        .eq_ignore_ascii_case(OTHER_LABEL)
}

pub(super) fn offered(question: &Question) -> impl Iterator<Item = &QuestionOption> {
    question
        .options
        .iter()
        .filter(|option| !is_other_label(&option.label))
}

impl Picks {
    fn pick(&mut self, question: &Question, label: &str) {
        let picks = self.labels.entry(question.text.clone()).or_default();
        if !question.multi_select {
            *picks = vec![label.to_owned()];
            return;
        }
        if let Some(index) = picks.iter().position(|picked| picked == label) {
            picks.remove(index);
        } else {
            picks.push(label.to_owned());
        }
    }

    pub(super) fn pick_in(&mut self, questions: &[Question], question: &Question, label: &str) {
        self.pick(question, label);
        let in_front = self.current(questions) == Some(question);
        if in_front && !question.multi_select && self.step + 1 < questions.len() {
            self.step += 1;
        }
    }

    pub(super) fn is_picked(&self, question: &str, label: &str) -> bool {
        self.labels
            .get(question)
            .is_some_and(|labels| labels.iter().any(|picked| picked == label))
    }

    pub(super) fn own_answer<'a>(&'a self, question: &Question) -> Option<&'a str> {
        let offered = |label: &str| question.options.iter().any(|option| option.label == label);
        self.labels
            .get(&question.text)?
            .iter()
            .find(|label| !offered(label))
            .map(String::as_str)
    }

    fn has_pick(&self, question: &Question) -> bool {
        self.labels
            .get(&question.text)
            .is_some_and(|labels| !labels.is_empty())
    }

    pub(super) fn current<'a>(&self, questions: &'a [Question]) -> Option<&'a Question> {
        questions.get(self.step).or_else(|| questions.last())
    }

    pub(super) fn step(&self, count: usize) -> usize {
        self.step.min(count.saturating_sub(1))
    }

    pub(super) fn show(&mut self, step: usize, count: usize) {
        self.step = step.min(count.saturating_sub(1));
    }

    pub(super) fn unanswered(&self, questions: &[Question]) -> Option<usize> {
        questions
            .iter()
            .position(|question| !self.has_pick(question))
    }

    pub(super) fn answers(&self) -> Vec<PickedAnswer> {
        self.labels
            .iter()
            .map(|(question, labels)| PickedAnswer {
                question: question.clone(),
                labels: labels.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(text: &str, multi_select: bool) -> Question {
        Question {
            text: text.to_owned(),
            header: None,
            options: vec![QuestionOption {
                label: "Lint".to_owned(),
                description: None,
            }],
            multi_select,
        }
    }

    #[test]
    fn a_single_choice_question_keeps_only_the_last_pick() {
        let single = question("Which checks?", false);
        let mut picks = Picks::default();
        picks.pick(&single, "Lint");
        picks.pick(&single, "Test");
        assert!(!picks.is_picked(&single.text, "Lint"));
        assert!(picks.is_picked(&single.text, "Test"));
        picks.pick(&single, "Test");
        assert!(
            picks.is_picked(&single.text, "Test"),
            "picking it again keeps it"
        );
    }

    #[test]
    fn a_multi_select_question_turns_each_pick_on_and_off() {
        let multi = question("Which checks?", true);
        let mut picks = Picks::default();
        picks.pick(&multi, "Lint");
        picks.pick(&multi, "Build");
        assert_eq!(
            picks.answers(),
            [PickedAnswer {
                question: multi.text.clone(),
                labels: vec!["Lint".to_owned(), "Build".to_owned()],
            }]
        );
        picks.pick(&multi, "Lint");
        assert!(!picks.is_picked(&multi.text, "Lint"));
        assert!(picks.is_picked(&multi.text, "Build"));
    }

    #[test]
    fn a_single_choice_pick_hands_over_to_the_next_question() {
        let questions = [
            question("Which checks?", false),
            question("Which CI?", false),
        ];
        let mut picks = Picks::default();
        assert_eq!(picks.current(&questions), questions.first());
        picks.pick_in(&questions, &questions[0], "Lint");
        assert_eq!(picks.current(&questions), questions.get(1));
        assert_eq!(picks.unanswered(&questions), Some(1));
        picks.pick_in(&questions, &questions[1], "GitHub Actions");
        assert_eq!(picks.unanswered(&questions), None);
        assert_eq!(
            picks.current(&questions),
            questions.last(),
            "stays on the last"
        );
        assert_eq!(picks.own_answer(&questions[1]), Some("GitHub Actions"));
        assert_eq!(picks.own_answer(&questions[0]), None);
    }

    #[test]
    fn a_multi_select_pick_stays_on_its_question() {
        let questions = [
            question("Which checks?", true),
            question("Which CI?", false),
        ];
        let mut picks = Picks::default();
        picks.pick_in(&questions, &questions[0], "Lint");
        assert_eq!(picks.step(questions.len()), 0);
        picks.show(9, questions.len());
        assert_eq!(picks.step(questions.len()), 1, "clamped to the last");
    }

    #[test]
    fn claude_codes_own_other_option_is_not_offered_twice() {
        let mut asked = question("Which checks?", false);
        for label in ["Other…", " other... ", "Others first"] {
            asked.options.push(QuestionOption {
                label: label.to_owned(),
                description: None,
            });
        }
        let labels: Vec<&str> = offered(&asked)
            .map(|option| option.label.as_str())
            .collect();
        assert_eq!(labels, ["Lint", "Others first"]);
    }
}
