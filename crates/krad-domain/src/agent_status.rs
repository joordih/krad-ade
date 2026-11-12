use crate::ResetTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub context_used: Option<u16>,
    pub effort: Option<String>,
    pub thinking: Option<bool>,
    pub five_hour_used: Option<u16>,
    pub seven_day_used: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour_resets_at: Option<ResetTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day_resets_at: Option<ResetTime>,
}
