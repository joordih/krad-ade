use gpui_kit::SharedString;
use krad_extensions::api::view::{Slider, SliderFormat, SliderLabels};

use super::motion::key_step;

const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_HOUR: i64 = 60;
const SECONDS_PER_HOUR: i64 = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const PERCENT: f64 = 100.0;
const DECIMAL_BASE: i64 = 10;
const DECIMAL_SCALE: f64 = 10.0;
const WHOLE_STEP: f64 = 1.0;
const TENTH_STEP: f64 = 0.1;
const MAX_DECIMALS: u8 = 2;
const REMAINING_SIGN: &str = "-";
const NEGATIVE_SIGN: &str = "-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rounding {
    Down,
    Up,
    Nearest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reading {
    units: i64,
    remaining: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LabelText {
    readings: Option<(Option<Reading>, Option<Reading>)>,
    pub(super) start: Option<SharedString>,
    pub(super) end: Option<SharedString>,
}

impl LabelText {
    pub(super) fn update(&mut self, spec: &Slider, value: f64) {
        let readings = readings(spec, value);
        if self.readings == Some(readings) {
            return;
        }
        self.readings = Some(readings);
        let (start, end) = readings;
        self.start = start.map(|reading| written(spec, reading).into());
        self.end = end.map(|reading| written(spec, reading).into());
    }
}

#[allow(clippy::cast_possible_truncation)]
fn whole(amount: f64) -> i64 {
    amount as i64
}

fn decimals(spec: &Slider) -> u8 {
    let unit = key_step(spec);
    if unit >= WHOLE_STEP {
        0
    } else if unit >= TENTH_STEP {
        1
    } else {
        MAX_DECIMALS
    }
}

fn round(amount: f64, rounding: Rounding) -> f64 {
    match rounding {
        Rounding::Down => amount.floor(),
        Rounding::Up => amount.ceil(),
        Rounding::Nearest => amount.round(),
    }
}

fn reading(spec: &Slider, amount: f64, remaining: bool) -> Reading {
    let units = match spec.format {
        SliderFormat::Duration => {
            let rounding = if remaining {
                Rounding::Up
            } else {
                Rounding::Down
            };
            whole(round(amount, rounding))
        }
        SliderFormat::Percent => whole((amount / (spec.max - spec.min) * PERCENT).round()),
        SliderFormat::Number => {
            let scale = DECIMAL_SCALE.powi(i32::from(decimals(spec)));
            whole(round(amount * scale, Rounding::Nearest))
        }
    };
    Reading { units, remaining }
}

fn readings(spec: &Slider, value: f64) -> (Option<Reading>, Option<Reading>) {
    let offset = if spec.format == SliderFormat::Percent {
        spec.min
    } else {
        0.0
    };
    let shown = |amount: f64| reading(spec, amount - offset, false);
    match spec.labels {
        SliderLabels::None => (None, None),
        SliderLabels::Value => (None, Some(shown(value))),
        SliderLabels::MinMax => (Some(shown(spec.min)), Some(shown(spec.max))),
        SliderLabels::ElapsedRemaining => (
            Some(shown(value)),
            Some(reading(spec, spec.max - value, true)),
        ),
    }
}

fn sign(units: i64) -> &'static str {
    if units < 0 {
        NEGATIVE_SIGN
    } else {
        ""
    }
}

fn written(spec: &Slider, reading: Reading) -> String {
    let prefix = if reading.remaining {
        REMAINING_SIGN
    } else {
        ""
    };
    let units = reading.units;
    let body = match spec.format {
        SliderFormat::Duration => duration(units),
        SliderFormat::Percent => format!("{units}%"),
        SliderFormat::Number => number(units, decimals(spec)),
    };
    format!("{prefix}{body}")
}

fn duration(units: i64) -> String {
    let seconds = units.abs();
    let hours = seconds / SECONDS_PER_HOUR;
    let minutes = seconds % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    let rest = seconds % SECONDS_PER_MINUTE;
    let sign = sign(units);
    if hours > 0 {
        format!("{sign}{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{sign}{minutes}:{rest:02}")
    }
}

fn number(units: i64, decimals: u8) -> String {
    let sign = sign(units);
    let scale = DECIMAL_BASE.pow(u32::from(decimals));
    let whole_part = units.abs() / scale;
    let fraction = units.abs() % scale;
    if decimals == 0 {
        return format!("{sign}{whole_part}");
    }
    let width = usize::from(decimals);
    format!("{sign}{whole_part}.{fraction:0width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_extensions::api::view::Enabled;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn spec(
        min: f64,
        max: f64,
        format: SliderFormat,
        labels: SliderLabels,
    ) -> Result<Slider, Box<dyn std::error::Error>> {
        Ok(Slider {
            action: "seek".parse()?,
            value: min,
            min,
            max,
            step: None,
            format,
            labels,
            enabled: Enabled::Yes,
            advancing: None,
        })
    }

    fn shown(spec: &Slider, value: f64) -> (Option<String>, Option<String>) {
        let mut text = LabelText::default();
        text.update(spec, value);
        (
            text.start.map(|label| label.to_string()),
            text.end.map(|label| label.to_string()),
        )
    }

    fn pair(start: &str, end: &str) -> (Option<String>, Option<String>) {
        (Some(start.to_owned()), Some(end.to_owned()))
    }

    #[test]
    fn durations_read_as_minutes_and_seconds_or_hours() -> TestResult {
        let song = spec(
            0.0,
            245.0,
            SliderFormat::Duration,
            SliderLabels::ElapsedRemaining,
        )?;
        assert_eq!(shown(&song, 0.0), pair("0:00", "-4:05"));
        assert_eq!(shown(&song, 65.4), pair("1:05", "-3:00"));
        let film = spec(0.0, 7_384.0, SliderFormat::Duration, SliderLabels::MinMax)?;
        assert_eq!(shown(&film, 10.0), pair("0:00", "2:03:04"));
        Ok(())
    }

    #[test]
    fn percent_reads_the_position_within_the_range() -> TestResult {
        let volume = spec(0.0, 1.0, SliderFormat::Percent, SliderLabels::Value)?;
        assert_eq!(shown(&volume, 0.25), (None, Some("25%".to_owned())));
        let offset = spec(50.0, 150.0, SliderFormat::Percent, SliderLabels::MinMax)?;
        assert_eq!(shown(&offset, 75.0), pair("0%", "100%"));
        Ok(())
    }

    #[test]
    fn numbers_show_as_many_decimals_as_their_step_needs() -> TestResult {
        let mut gain = spec(-1.0, 1.0, SliderFormat::Number, SliderLabels::Value)?;
        assert_eq!(shown(&gain, -0.5), (None, Some("-0.5".to_owned())));
        gain.step = Some(0.05);
        assert_eq!(shown(&gain, -0.25), (None, Some("-0.25".to_owned())));
        gain.step = Some(1.0);
        assert_eq!(shown(&gain, 1.0), (None, Some("1".to_owned())));
        let count = spec(0.0, 200.0, SliderFormat::Number, SliderLabels::MinMax)?;
        assert_eq!(shown(&count, 3.0), pair("0", "200"));
        assert_eq!(
            shown(
                &spec(0.0, 10.0, SliderFormat::Number, SliderLabels::None)?,
                3.0
            ),
            (None, None)
        );
        Ok(())
    }

    #[test]
    fn labels_are_written_again_only_when_what_they_show_changes() -> TestResult {
        let song = spec(
            0.0,
            245.0,
            SliderFormat::Duration,
            SliderLabels::ElapsedRemaining,
        )?;
        let mut text = LabelText::default();
        text.update(&song, 10.1);
        let first = text.readings;
        text.update(&song, 10.7);
        assert_eq!(text.readings, first);
        assert_eq!(text.start.as_ref().map(SharedString::as_ref), Some("0:10"));
        text.update(&song, 11.0);
        assert_ne!(text.readings, first);
        assert_eq!(text.start.as_ref().map(SharedString::as_ref), Some("0:11"));
        Ok(())
    }
}
