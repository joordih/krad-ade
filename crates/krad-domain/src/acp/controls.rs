use super::offers::{AcpConfigCategory, AcpConfigOption, AcpConfigOptionId, AcpConfigValue};
use crate::model::Effort;

const ULTRACODE_OPTION: &str = "ultracode";
const FAST_OPTION: &str = "fast";
const PERSONA_OPTION: &str = "agent";
const SWITCH_ON: &str = "on";
const SWITCH_OFF: &str = "off";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpOptionRole {
    Effort,
    Ultracode,
    Fast,
    Persona,
    Other,
}

impl AcpConfigOption {
    #[must_use]
    pub fn role(&self) -> AcpOptionRole {
        let id = self.id.0.as_str();
        match self.category {
            Some(AcpConfigCategory::ThoughtLevel) => AcpOptionRole::Effort,
            Some(AcpConfigCategory::ModelConfig) if id == ULTRACODE_OPTION && self.switches() => {
                AcpOptionRole::Ultracode
            }
            Some(AcpConfigCategory::ModelConfig) if id == FAST_OPTION && self.switches() => {
                AcpOptionRole::Fast
            }
            None if id == PERSONA_OPTION => AcpOptionRole::Persona,
            Some(_) | None => AcpOptionRole::Other,
        }
    }

    #[must_use]
    pub fn shows_as_pill(&self) -> bool {
        self.role() == AcpOptionRole::Other
    }

    fn switches(&self) -> bool {
        [SWITCH_ON, SWITCH_OFF]
            .into_iter()
            .all(|value| self.offers(&AcpConfigValue(value.to_owned())))
    }

    fn is_on(&self) -> bool {
        self.current.0 == SWITCH_ON
    }

    fn switched(&self, on: bool) -> Option<(AcpConfigOptionId, AcpConfigValue)> {
        (self.is_on() != on).then(|| {
            let value = if on { SWITCH_ON } else { SWITCH_OFF };
            (self.id.clone(), AcpConfigValue(value.to_owned()))
        })
    }

    fn effort_value(&self, effort: Effort) -> Option<AcpConfigValue> {
        self.choices
            .iter()
            .find(|choice| choice.value.0.parse::<Effort>().ok() == Some(effort))
            .map(|choice| choice.value.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpEffort {
    pub levels: Vec<Effort>,
    pub current: Option<Effort>,
    pub fast: Option<bool>,
}

fn with_role(options: &[AcpConfigOption], role: AcpOptionRole) -> Option<&AcpConfigOption> {
    options.iter().find(|option| option.role() == role)
}

impl AcpEffort {
    #[must_use]
    pub fn of(options: &[AcpConfigOption]) -> Option<Self> {
        let effort = with_role(options, AcpOptionRole::Effort);
        let ultracode = with_role(options, AcpOptionRole::Ultracode);
        let fast = with_role(options, AcpOptionRole::Fast);
        if effort.is_none() && ultracode.is_none() && fast.is_none() {
            return None;
        }
        let mut levels: Vec<Effort> = effort.map_or_default(|effort| {
            effort
                .choices
                .iter()
                .filter_map(|choice| choice.value.0.parse().ok())
                .collect()
        });
        levels.sort_by_key(|level: &Effort| level.rank());
        levels.dedup();
        if ultracode.is_some() {
            levels.retain(|level| *level != Effort::Ultra);
            levels.push(Effort::Ultra);
        }
        let current = if ultracode.is_some_and(AcpConfigOption::is_on) {
            Some(Effort::Ultra)
        } else {
            effort.and_then(|effort| effort.current.0.parse().ok())
        };
        Some(Self {
            levels,
            current,
            fast: fast.map(AcpConfigOption::is_on),
        })
    }

    #[must_use]
    pub fn choose(
        options: &[AcpConfigOption],
        wanted: Effort,
    ) -> Vec<(AcpConfigOptionId, AcpConfigValue)> {
        let effort = with_role(options, AcpOptionRole::Effort);
        let ultracode = with_role(options, AcpOptionRole::Ultracode);
        let ultra = wanted == Effort::Ultra && ultracode.is_some();
        let target = if ultra { Effort::Max } else { wanted };
        let level = effort.and_then(|effort| {
            let value = effort.effort_value(target)?;
            (value != effort.current).then(|| (effort.id.clone(), value))
        });
        let switch = ultracode.and_then(|ultracode| ultracode.switched(ultra));
        switch.into_iter().chain(level).collect()
    }

    #[must_use]
    pub fn switch_fast(
        options: &[AcpConfigOption],
        on: bool,
    ) -> Option<(AcpConfigOptionId, AcpConfigValue)> {
        with_role(options, AcpOptionRole::Fast)?.switched(on)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::offers::AcpConfigChoice;

    fn option(
        id: &str,
        category: Option<AcpConfigCategory>,
        current: &str,
        values: &[&str],
    ) -> AcpConfigOption {
        AcpConfigOption {
            id: AcpConfigOptionId(id.to_owned()),
            name: id.to_owned(),
            description: None,
            category,
            current: AcpConfigValue(current.to_owned()),
            choices: values
                .iter()
                .map(|value| AcpConfigChoice {
                    value: AcpConfigValue((*value).to_owned()),
                    name: (*value).to_owned(),
                    description: None,
                })
                .collect(),
        }
    }

    fn claude_agent_acp(effort: &str, ultracode: &str, fast: &str) -> Vec<AcpConfigOption> {
        vec![
            option(
                "effort",
                Some(AcpConfigCategory::ThoughtLevel),
                effort,
                &["default", "low", "medium", "high", "xhigh", "max"],
            ),
            option(
                "fast",
                Some(AcpConfigCategory::ModelConfig),
                fast,
                &["on", "off"],
            ),
            option(
                "ultracode",
                Some(AcpConfigCategory::ModelConfig),
                ultracode,
                &["on", "off"],
            ),
            option("agent", None, "default", &["default", "reviewer"]),
            option(
                "style",
                Some(AcpConfigCategory::Other),
                "plain",
                &["plain", "rich"],
            ),
        ]
    }

    fn set(id: &str, value: &str) -> (AcpConfigOptionId, AcpConfigValue) {
        (
            AcpConfigOptionId(id.to_owned()),
            AcpConfigValue(value.to_owned()),
        )
    }

    #[test]
    fn the_reported_options_map_to_their_roles_by_category_and_id() {
        let roles: Vec<AcpOptionRole> = claude_agent_acp("medium", "off", "off")
            .iter()
            .map(AcpConfigOption::role)
            .collect();
        assert_eq!(
            roles,
            [
                AcpOptionRole::Effort,
                AcpOptionRole::Fast,
                AcpOptionRole::Ultracode,
                AcpOptionRole::Persona,
                AcpOptionRole::Other,
            ]
        );
        let unswitched = option(
            "fast",
            Some(AcpConfigCategory::ModelConfig),
            "a",
            &["a", "b"],
        );
        assert_eq!(unswitched.role(), AcpOptionRole::Other);
    }

    #[test]
    fn ultracode_is_the_top_effort_step_and_fast_rides_along() {
        let effort = AcpEffort::of(&claude_agent_acp("medium", "off", "on"));
        assert_eq!(
            effort,
            Some(AcpEffort {
                levels: vec![
                    Effort::Low,
                    Effort::Medium,
                    Effort::High,
                    Effort::ExtraHigh,
                    Effort::Max,
                    Effort::Ultra
                ],
                current: Some(Effort::Medium),
                fast: Some(true),
            })
        );
        let ultra = AcpEffort::of(&claude_agent_acp("high", "on", "off"));
        assert_eq!(ultra.and_then(|ultra| ultra.current), Some(Effort::Ultra));
        let default = AcpEffort::of(&claude_agent_acp("default", "off", "off"));
        assert_eq!(default.and_then(|default| default.current), None);
        assert_eq!(AcpEffort::of(&[]), None);
    }

    #[test]
    fn choosing_ultra_turns_ultracode_on_at_max_and_a_level_turns_it_off() {
        let options = claude_agent_acp("medium", "off", "off");
        assert_eq!(
            AcpEffort::choose(&options, Effort::Ultra),
            [set("ultracode", "on"), set("effort", "max")]
        );
        let ultra = claude_agent_acp("max", "on", "off");
        assert_eq!(
            AcpEffort::choose(&ultra, Effort::Ultra),
            Vec::<(AcpConfigOptionId, AcpConfigValue)>::new()
        );
        assert_eq!(
            AcpEffort::choose(&ultra, Effort::High),
            [set("ultracode", "off"), set("effort", "high")]
        );
        assert_eq!(
            AcpEffort::choose(&options, Effort::ExtraHigh),
            [set("effort", "xhigh")]
        );
        assert_eq!(
            AcpEffort::choose(&options, Effort::Minimal),
            Vec::<(AcpConfigOptionId, AcpConfigValue)>::new()
        );
    }

    #[test]
    fn fast_mode_switches_only_when_it_changes() {
        let options = claude_agent_acp("medium", "off", "off");
        assert_eq!(
            AcpEffort::switch_fast(&options, true),
            Some(set("fast", "on"))
        );
        assert_eq!(AcpEffort::switch_fast(&options, false), None);
        assert!(!options[0].shows_as_pill());
        assert!(options[4].shows_as_pill());
    }
}
