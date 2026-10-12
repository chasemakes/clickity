//! The persisted form, its validation rules, and the interval copy shown under the timing console.

use crate::model::{ClickButton, ClickConfig, PositionMode, RepeatMode};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub const DEFAULT_RUN_SHORTCUT: &str = "F6";
pub const DEFAULT_CAPTURE_SHORTCUT: &str = "F7";

const MAX_REPEAT_COUNT: i64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RepeatKind {
    Count,
    UntilStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionKind {
    Current,
    Fixed,
}

/// Everything Clickity remembers between launches. Only valid forms are saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hours: i64,
    pub minutes: i64,
    pub seconds: i64,
    pub milliseconds: i64,
    pub button: ClickButton,
    pub repeat_mode: RepeatKind,
    pub repeat_count: i64,
    pub position_mode: PositionKind,
    pub x: i64,
    pub y: i64,
    pub hotkey: String,
    pub capture_hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hours: 0,
            minutes: 0,
            seconds: 1,
            milliseconds: 0,
            button: ClickButton::Left,
            repeat_mode: RepeatKind::Count,
            repeat_count: 10,
            position_mode: PositionKind::Current,
            x: 0,
            y: 0,
            hotkey: DEFAULT_RUN_SHORTCUT.into(),
            capture_hotkey: DEFAULT_CAPTURE_SHORTCUT.into(),
        }
    }
}

impl Settings {
    /// Reads the saved settings, falling back to the defaults when none are readable.
    pub fn load() -> Self {
        settings_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path().ok_or("No configuration directory is available.")?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|error| format!("Could not save settings: {error}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Could not save settings: {error}"))?;
        fs::write(&path, json).map_err(|error| format!("Could not save settings: {error}"))
    }
}

fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("clickity").join("settings.json"))
}

/// The form as typed. Numeric fields are `None` while blank or not a whole number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draft {
    pub hours: Option<i64>,
    pub minutes: Option<i64>,
    pub seconds: Option<i64>,
    pub milliseconds: Option<i64>,
    pub button: ClickButton,
    pub repeat_mode: RepeatKind,
    pub repeat_count: Option<i64>,
    pub position_mode: PositionKind,
    pub x: Option<i64>,
    pub y: Option<i64>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ValidationErrors {
    pub interval: Option<&'static str>,
    pub repeat: Option<&'static str>,
    pub position: Option<&'static str>,
}

impl ValidationErrors {
    pub fn is_empty(&self) -> bool {
        self.interval.is_none() && self.repeat.is_none() && self.position.is_none()
    }
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Self {
        Self {
            hours: Some(settings.hours),
            minutes: Some(settings.minutes),
            seconds: Some(settings.seconds),
            milliseconds: Some(settings.milliseconds),
            button: settings.button,
            repeat_mode: settings.repeat_mode,
            repeat_count: Some(settings.repeat_count),
            position_mode: settings.position_mode,
            x: Some(settings.x),
            y: Some(settings.y),
        }
    }
}

/// Parses one numeric field the way the form reads it: blank and non-integer text is missing.
pub fn parse_field(text: &str) -> Option<i64> {
    text.trim().parse().ok()
}

impl Draft {
    /// The interval in milliseconds, or `None` while any part is missing.
    pub fn interval_ms(&self) -> Option<i64> {
        Some(
            self.hours? * 3_600_000
                + self.minutes? * 60_000
                + self.seconds? * 1_000
                + self.milliseconds?,
        )
    }

    pub fn validate(&self) -> ValidationErrors {
        let mut errors = ValidationErrors::default();

        let in_range =
            |value: Option<i64>, max: i64| value.is_some_and(|value| (0..=max).contains(&value));
        if !(in_range(self.hours, 9_999)
            && in_range(self.minutes, 59)
            && in_range(self.seconds, 59)
            && in_range(self.milliseconds, 999))
        {
            errors.interval = Some("Enter whole numbers within the shown ranges.");
        } else if self.interval_ms() == Some(0) {
            errors.interval = Some("Set an interval greater than 0.");
        }

        if self.repeat_mode == RepeatKind::Count
            && !self
                .repeat_count
                .is_some_and(|count| (1..=MAX_REPEAT_COUNT).contains(&count))
        {
            errors.repeat = Some("Enter a whole number from 1 to 1,000,000,000.");
        }

        let coordinate =
            |value: Option<i64>| value.is_some_and(|value| i32::try_from(value).is_ok());
        if self.position_mode == PositionKind::Fixed && !(coordinate(self.x) && coordinate(self.y))
        {
            errors.position = Some("Enter valid whole-number X and Y coordinates.");
        }

        errors
    }

    /// The engine configuration for this form, or `None` while it has validation errors.
    pub fn click_config(&self) -> Option<ClickConfig> {
        if !self.validate().is_empty() {
            return None;
        }

        Some(ClickConfig {
            interval_ms: u64::try_from(self.interval_ms()?).ok()?,
            button: self.button,
            repeat: match self.repeat_mode {
                RepeatKind::Count => RepeatMode::Count {
                    clicks: u64::try_from(self.repeat_count?).ok()?,
                },
                RepeatKind::UntilStopped => RepeatMode::UntilStopped,
            },
            position: match self.position_mode {
                PositionKind::Fixed => PositionMode::Fixed {
                    x: i32::try_from(self.x?).ok()?,
                    y: i32::try_from(self.y?).ok()?,
                },
                PositionKind::Current => PositionMode::Current,
            },
        })
    }

    /// Copies a valid form into `settings`. Fields the current modes ignore keep their
    /// saved values when left blank.
    pub fn apply_to(&self, settings: &mut Settings) {
        settings.hours = self.hours.unwrap_or(settings.hours);
        settings.minutes = self.minutes.unwrap_or(settings.minutes);
        settings.seconds = self.seconds.unwrap_or(settings.seconds);
        settings.milliseconds = self.milliseconds.unwrap_or(settings.milliseconds);
        settings.button = self.button;
        settings.repeat_mode = self.repeat_mode;
        settings.repeat_count = self.repeat_count.unwrap_or(settings.repeat_count);
        settings.position_mode = self.position_mode;
        settings.x = self.x.unwrap_or(settings.x);
        settings.y = self.y.unwrap_or(settings.y);
    }
}

/// Natural-language interval copy, e.g. "1 minute 30 seconds" or "250 ms".
pub fn format_duration(milliseconds: i64) -> String {
    if milliseconds <= 0 {
        return "Not set".into();
    }
    if milliseconds < 1_000 {
        return format!("{milliseconds} ms");
    }

    let units = [(3_600_000, "hour"), (60_000, "minute"), (1_000, "second")];
    let mut remaining = milliseconds;
    let mut parts = Vec::new();

    for (size, label) in units {
        let amount = remaining / size;
        if amount > 0 {
            parts.push(format!(
                "{amount} {label}{}",
                if amount == 1 { "" } else { "s" }
            ));
            remaining %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }

    if remaining > 0 && parts.len() < 2 {
        parts.push(format!("{remaining} ms"));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        Draft {
            hours: Some(0),
            minutes: Some(0),
            seconds: Some(1),
            milliseconds: Some(0),
            button: ClickButton::Left,
            repeat_mode: RepeatKind::Count,
            repeat_count: Some(10),
            position_mode: PositionKind::Current,
            x: Some(0),
            y: Some(0),
        }
    }

    #[test]
    fn default_form_is_valid() {
        assert!(draft().validate().is_empty());
        assert_eq!(draft().click_config(), Some(ClickConfig::default()));
    }

    #[test]
    fn zero_interval_is_rejected() {
        let form = Draft {
            seconds: Some(0),
            ..draft()
        };
        assert_eq!(
            form.validate().interval,
            Some("Set an interval greater than 0.")
        );
        assert_eq!(form.click_config(), None);
    }

    #[test]
    fn out_of_range_or_blank_interval_parts_are_rejected() {
        for form in [
            Draft {
                minutes: Some(60),
                ..draft()
            },
            Draft {
                milliseconds: Some(-1),
                ..draft()
            },
            Draft {
                hours: None,
                ..draft()
            },
        ] {
            assert_eq!(
                form.validate().interval,
                Some("Enter whole numbers within the shown ranges.")
            );
        }
    }

    #[test]
    fn repeat_count_only_matters_in_count_mode() {
        let blank = Draft {
            repeat_count: None,
            ..draft()
        };
        assert!(blank.validate().repeat.is_some());

        let until_stopped = Draft {
            repeat_mode: RepeatKind::UntilStopped,
            ..blank
        };
        assert!(until_stopped.validate().is_empty());
    }

    #[test]
    fn fixed_position_needs_in_range_coordinates() {
        let form = Draft {
            position_mode: PositionKind::Fixed,
            x: Some(i64::from(i32::MAX) + 1),
            ..draft()
        };
        assert!(form.validate().position.is_some());

        let form = Draft {
            x: Some(-120),
            y: Some(450),
            ..form
        };
        assert_eq!(
            form.click_config().map(|config| config.position),
            Some(PositionMode::Fixed { x: -120, y: 450 })
        );
    }

    #[test]
    fn blank_fields_keep_saved_values() {
        let mut settings = Settings {
            repeat_count: 25,
            ..Settings::default()
        };
        Draft {
            repeat_mode: RepeatKind::UntilStopped,
            repeat_count: None,
            ..draft()
        }
        .apply_to(&mut settings);
        assert_eq!(settings.repeat_mode, RepeatKind::UntilStopped);
        assert_eq!(settings.repeat_count, 25);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(format_duration(0), "Not set");
        assert_eq!(format_duration(5), "5 ms");
        assert_eq!(format_duration(1_000), "1 second");
        assert_eq!(format_duration(90_000), "1 minute 30 seconds");
        assert_eq!(format_duration(1_250), "1 second 250 ms");
        assert_eq!(format_duration(3_723_000), "1 hour 2 minutes");
    }

    #[test]
    fn settings_round_trip_and_fill_missing_fields() {
        let settings = Settings {
            button: ClickButton::Right,
            hotkey: "CmdOrCtrl+Shift+K".into(),
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("\"captureHotkey\":\"F7\""));
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);

        let partial: Settings = serde_json::from_str(r#"{"seconds": 3}"#).unwrap();
        assert_eq!(partial.seconds, 3);
        assert_eq!(partial.hotkey, DEFAULT_RUN_SHORTCUT);
    }
}
