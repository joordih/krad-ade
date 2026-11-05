use krad_extension_api::view::{
    Advancing, Element, Enabled, Node, Slider, SliderFormat, SliderLabels,
};
use krad_extension_api::{ActionId, NodeKey};

#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct SliderNode {
    key: NodeKey,
    slider: Slider,
}

pub fn slider(key: NodeKey, action: ActionId, value: f64, max: f64) -> SliderNode {
    SliderNode {
        key,
        slider: Slider {
            action,
            value,
            min: 0.0,
            max,
            step: None,
            format: SliderFormat::default(),
            labels: SliderLabels::default(),
            enabled: Enabled::default(),
            advancing: None,
        },
    }
}

impl SliderNode {
    pub fn min(mut self, min: f64) -> Self {
        self.slider.min = min;
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.slider.step = Some(step);
        self
    }

    pub fn format(mut self, format: SliderFormat) -> Self {
        self.slider.format = format;
        self
    }

    pub fn labels(mut self, labels: SliderLabels) -> Self {
        self.slider.labels = labels;
        self
    }

    pub fn enabled(mut self, enabled: Enabled) -> Self {
        self.slider.enabled = enabled;
        self
    }

    pub fn advancing(mut self, rate: f64) -> Self {
        self.slider.advancing = Some(Advancing { rate });
        self
    }
}

impl From<SliderNode> for Node {
    fn from(built: SliderNode) -> Self {
        Self {
            key: built.key,
            element: Element::Slider(built.slider),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extension_api::view::ViewEvent;
    use serde_json::json;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn a_timeline_slider_writes_the_shape_the_app_reads() -> TestResult {
        let node: Node = slider("timeline".parse()?, "seek".parse()?, 30.0, 245.0)
            .format(SliderFormat::Duration)
            .labels(SliderLabels::ElapsedRemaining)
            .advancing(1.0)
            .into();
        assert_eq!(
            serde_json::to_value(&node)?,
            json!({
                "key": "timeline",
                "type": "slider",
                "action": "seek",
                "value": 30.0,
                "min": 0.0,
                "max": 245.0,
                "step": null,
                "format": "duration",
                "labels": "elapsed_remaining",
                "enabled": "yes",
                "advancing": {"rate": 1.0}
            })
        );
        Ok(())
    }

    #[test]
    fn a_volume_slider_keeps_its_step_range_and_state() -> TestResult {
        let node: Node = slider("volume".parse()?, "volume".parse()?, 0.5, 1.0)
            .min(0.0)
            .step(0.05)
            .format(SliderFormat::Percent)
            .labels(SliderLabels::Value)
            .enabled(Enabled::No)
            .into();
        let Element::Slider(read) =
            serde_json::from_value::<Node>(serde_json::to_value(&node)?)?.element
        else {
            return Err("the node reads back as a slider".into());
        };
        assert_eq!(read.step, Some(0.05));
        assert_eq!(read.enabled, Enabled::No);
        assert_eq!(read.advancing, None);
        Ok(())
    }

    #[test]
    fn slider_events_read_as_typed_change_and_commit() -> TestResult {
        let change: ViewEvent = serde_json::from_value(
            json!({"kind": "slider_change", "action": "seek", "value": 12.5}),
        )?;
        assert_eq!(
            change,
            ViewEvent::SliderChange {
                action: "seek".parse()?,
                value: 12.5
            }
        );
        let commit: ViewEvent = serde_json::from_value(
            json!({"kind": "slider_commit", "action": "seek", "value": 80}),
        )?;
        assert_eq!(
            commit,
            ViewEvent::SliderCommit {
                action: "seek".parse()?,
                value: 80.0
            }
        );
        Ok(())
    }
}
