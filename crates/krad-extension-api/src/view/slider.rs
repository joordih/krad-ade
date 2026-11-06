use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::ActionId;
use crate::view::element::Enabled;

pub const MAX_SLIDER_MAGNITUDE: f64 = 1.0e12;
pub const MAX_ADVANCE_RATE: f64 = 1.0e6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Slider {
    pub action: ActionId,
    pub value: f64,
    #[serde(default)]
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub format: SliderFormat,
    #[serde(default)]
    pub labels: SliderLabels,
    #[serde(default)]
    pub enabled: Enabled,
    #[serde(default)]
    pub advancing: Option<Advancing>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SliderFormat {
    #[default]
    Number,
    Percent,
    Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SliderLabels {
    #[default]
    None,
    Value,
    ElapsedRemaining,
    MinMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Advancing {
    pub rate: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SliderError {
    #[error("a slider's bounds must be finite numbers within the slider limits")]
    Bounds,
    #[error("a slider's minimum must be below its maximum")]
    Range,
    #[error("a slider's step must be a positive finite number")]
    Step,
    #[error("a slider's advancing rate must be a positive finite number")]
    Rate,
}

fn within_limits(number: f64) -> bool {
    number.is_finite() && number.abs() <= MAX_SLIDER_MAGNITUDE
}

impl Slider {
    /// # Errors
    ///
    /// Returns [`SliderError`] when a bound is not finite or too large, the minimum is not below
    /// the maximum, or the step or advancing rate is not a positive finite number. The value is
    /// brought within the range, the step within its span and the rate within its limit.
    pub fn bound(&mut self) -> Result<(), SliderError> {
        if !within_limits(self.min) || !within_limits(self.max) {
            return Err(SliderError::Bounds);
        }
        if self.min >= self.max {
            return Err(SliderError::Range);
        }
        let span = self.max - self.min;
        if let Some(step) = self.step.as_mut() {
            if !step.is_finite() || *step <= 0.0 {
                return Err(SliderError::Step);
            }
            *step = step.min(span);
        }
        if let Some(advancing) = self.advancing.as_mut() {
            if !advancing.rate.is_finite() || advancing.rate <= 0.0 {
                return Err(SliderError::Rate);
            }
            advancing.rate = advancing.rate.min(MAX_ADVANCE_RATE);
        }
        self.value = if self.value.is_finite() {
            self.value.clamp(self.min, self.max)
        } else {
            self.min
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn slider(value: f64, min: f64, max: f64) -> Result<Slider, crate::id::IdError> {
        Ok(Slider {
            action: "seek".parse()?,
            value,
            min,
            max,
            step: None,
            format: SliderFormat::Duration,
            labels: SliderLabels::ElapsedRemaining,
            enabled: Enabled::Yes,
            advancing: Some(Advancing { rate: 1.0 }),
        })
    }

    #[test]
    fn a_slider_reads_from_its_json_shape_with_defaults() -> TestResult {
        let read: Slider = serde_json::from_str(r#"{"action":"volume","value":0.5,"max":1}"#)?;
        assert_eq!(read.min, 0.0);
        assert_eq!(read.format, SliderFormat::Number);
        assert_eq!(read.labels, SliderLabels::None);
        assert_eq!(read.enabled, Enabled::Yes);
        assert_eq!(read.advancing, None);
        let full: Slider = serde_json::from_str(
            r#"{"action":"seek","value":12,"max":200,"step":5,"format":"duration","labels":"elapsed_remaining","advancing":{"rate":1}}"#,
        )?;
        assert_eq!(full.labels, SliderLabels::ElapsedRemaining);
        assert_eq!(full.advancing, Some(Advancing { rate: 1.0 }));
        let written: Slider = serde_json::from_value(serde_json::to_value(&full)?)?;
        assert_eq!(written, full);
        Ok(())
    }

    #[test]
    fn values_outside_the_range_are_clamped_and_large_rates_limited() -> TestResult {
        let mut above = slider(500.0, 0.0, 200.0)?;
        above.step = Some(1_000.0);
        above.advancing = Some(Advancing {
            rate: f64::from(u32::MAX),
        });
        above.bound()?;
        assert_eq!(above.value, 200.0);
        assert_eq!(above.step, Some(200.0));
        assert_eq!(
            above.advancing,
            Some(Advancing {
                rate: MAX_ADVANCE_RATE
            })
        );
        let mut below = slider(-3.0, 0.0, 200.0)?;
        below.bound()?;
        assert_eq!(below.value, 0.0);
        let mut unknown = slider(f64::NAN, 10.0, 20.0)?;
        unknown.bound()?;
        assert_eq!(unknown.value, 10.0);
        Ok(())
    }

    #[test]
    fn broken_bounds_steps_and_rates_are_refused() -> TestResult {
        assert_eq!(slider(1.0, 5.0, 5.0)?.bound(), Err(SliderError::Range));
        assert_eq!(slider(1.0, 9.0, 2.0)?.bound(), Err(SliderError::Range));
        assert_eq!(
            slider(1.0, 0.0, f64::INFINITY)?.bound(),
            Err(SliderError::Bounds)
        );
        assert_eq!(
            slider(1.0, -MAX_SLIDER_MAGNITUDE * 2.0, 0.0)?.bound(),
            Err(SliderError::Bounds)
        );
        let mut still = slider(1.0, 0.0, 10.0)?;
        still.advancing = Some(Advancing { rate: 0.0 });
        assert_eq!(still.bound(), Err(SliderError::Rate));
        let mut backwards = slider(1.0, 0.0, 10.0)?;
        backwards.advancing = Some(Advancing { rate: -1.0 });
        assert_eq!(backwards.bound(), Err(SliderError::Rate));
        let mut flat = slider(1.0, 0.0, 10.0)?;
        flat.step = Some(0.0);
        assert_eq!(flat.bound(), Err(SliderError::Step));
        Ok(())
    }
}
