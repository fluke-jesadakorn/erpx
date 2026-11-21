//! Pure time-preference domain for the native Account command side.
//!
//! This module owns the exact retained `accountTimePreferenceInput` validation
//! and the create/change transition decision. It performs no IO, opens no
//! transaction and emits no SQL: the application layer turns its typed plan
//! into repository descriptors and the owner adapter supplies the SQL.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TIME_PREFERENCE_CHANGE_OPERATION: &str = "account.time_preference.change";
pub const TIME_PREFERENCE_RECORD_TYPE: &str = "account_time_preferences";
const MAX_TIME_ZONE: usize = 80;

/// The public preference projection. Kept byte-compatible with the retained
/// camelCase D1 shape so the read side and the command reply agree.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimePreference {
    pub mode: String,
    pub time_zone: Option<String>,
    pub hour_cycle: String,
    pub version: i64,
}

impl Default for TimePreference {
    fn default() -> Self {
        Self {
            mode: "system".to_owned(),
            time_zone: None,
            hour_cycle: "h23".to_owned(),
            version: 0,
        }
    }
}

/// A validated write input. `version == 0` creates the first row; any later
/// change must carry the current version (optimistic concurrency).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TimePreferenceInput {
    pub mode: String,
    pub time_zone: Option<String>,
    pub hour_cycle: String,
    pub version: i64,
}

impl TimePreferenceInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let mode = text(raw, "mode")?;
        if mode != "system" && mode != "manual" {
            return Err(invalid());
        }
        let hour_cycle = text(raw, "hourCycle")?;
        if hour_cycle != "h12" && hour_cycle != "h23" {
            return Err(invalid());
        }
        let version = match raw.get("version") {
            Some(Value::Number(number)) => number.as_i64(),
            _ => None,
        }
        .filter(|value| *value >= 0)
        .ok_or_else(invalid)?;
        let time_zone = match raw.get("timeZone") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => return Err(invalid()),
        };
        if mode == "system" {
            if time_zone.is_some() {
                return Err(invalid());
            }
        } else if time_zone
            .as_deref()
            .is_none_or(|value| !valid_time_zone(value))
        {
            return Err(invalid());
        }
        Ok(Self {
            mode,
            time_zone,
            hour_cycle,
            version,
        })
    }

    pub fn into_preference(self, version: i64) -> TimePreference {
        TimePreference {
            mode: self.mode,
            time_zone: self.time_zone,
            hour_cycle: self.hour_cycle,
            version,
        }
    }
}

/// Whether the write creates the first row (`version == 0`) or updates it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimePreferenceWriteKind {
    Create,
    Change { expected_version: i64 },
}

/// Pure transition plan: it checks the current projection against the input
/// and returns the kind of repository write plus the resulting version.
pub fn plan_change(
    input: &TimePreferenceInput,
    current: Option<&TimePreference>,
) -> Result<(TimePreferenceWriteKind, i64), BusinessError> {
    match (input.version, current) {
        (0, _) => Ok((TimePreferenceWriteKind::Create, 1)),
        (expected, Some(row)) if expected == row.version => Ok((
            TimePreferenceWriteKind::Change {
                expected_version: expected,
            },
            expected + 1,
        )),
        (_, _) => Err(changed()),
    }
}

/// Bounded IANA-style check. The full browser zone list lives in the read-side
/// `validTimeZone`; the native write guard only needs to reject junk and cap
/// the stored identifier before the CHECK constraint in PostgreSQL.
pub fn valid_time_zone(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_TIME_ZONE {
        return false;
    }
    let has_slash = value.contains('/');
    let chars_ok = value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+' | b'.')
    });
    (value == "UTC" || has_slash) && chars_ok
}

fn text(raw: &Value, field: &str) -> Result<String, BusinessError> {
    match raw.get(field) {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= MAX_TIME_ZONE => {
            Ok(value.clone())
        }
        _ => Err(invalid()),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVALID_TIME_PREFERENCE",
        "Choose a valid IANA timezone for Manual mode.",
    )
}

pub fn changed() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "TIME_PREFERENCE_CHANGED",
        "Reload your time settings and try again.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "TIME_PREFERENCE_UNAVAILABLE",
        "The native time preference service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(version: i64, mode: &str, zone: Option<&str>) -> Value {
        json!({
            "mode": mode,
            "timeZone": zone,
            "hourCycle": "h23",
            "version": version,
        })
    }

    #[test]
    fn parses_system_and_manual_modes() {
        let system = TimePreferenceInput::parse(&raw(0, "system", None)).unwrap();
        assert_eq!(system.mode, "system");
        assert!(system.time_zone.is_none());
        let manual = TimePreferenceInput::parse(&raw(1, "manual", Some("Asia/Bangkok"))).unwrap();
        assert_eq!(manual.time_zone.as_deref(), Some("Asia/Bangkok"));
    }

    #[test]
    fn rejects_invalid_mode_zone_pairs() {
        assert!(TimePreferenceInput::parse(&raw(0, "manual", None)).is_err());
        assert!(TimePreferenceInput::parse(&raw(0, "system", Some("Asia/Bangkok"))).is_err());
        assert!(TimePreferenceInput::parse(&raw(0, "manual", Some("junk zone"))).is_err());
    }

    #[test]
    fn version_zero_creates_and_matching_version_changes() {
        let input = TimePreferenceInput::parse(&raw(0, "system", None)).unwrap();
        assert_eq!(
            plan_change(&input, None).unwrap().0,
            TimePreferenceWriteKind::Create
        );
        let change = TimePreferenceInput::parse(&raw(2, "system", None)).unwrap();
        let current = TimePreference {
            version: 2,
            ..TimePreference::default()
        };
        assert_eq!(
            plan_change(&change, Some(&current)).unwrap(),
            (
                TimePreferenceWriteKind::Change {
                    expected_version: 2
                },
                3
            )
        );
        assert!(
            plan_change(
                &change,
                Some(&TimePreference {
                    version: 5,
                    ..TimePreference::default()
                })
            )
            .is_err()
        );
    }
}
