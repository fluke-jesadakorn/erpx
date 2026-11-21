//! Delivery policy shared by the native Field notification job partition.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

/// Normalized preference fields from the strict existing web-push form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushPreferenceInput {
    pub work: bool,
    pub mention: bool,
    pub system: bool,
    pub calendar: bool,
    pub notice: bool,
    pub quiet_start: Option<i32>,
    pub quiet_end: Option<i32>,
}

pub fn parse_push_preferences(value: &Value) -> Result<PushPreferenceInput, BusinessError> {
    let invalid = || BusinessError::invalid("INVALID_PUSH_PREFERENCES", "Choose valid push preferences and quiet hours.");
    let v = value.as_object().ok_or_else(invalid)?;
    let keys = ["work", "mention", "system", "calendar", "notice", "quietStartMinute", "quietEndMinute"];
    if v.len() != keys.len() || keys.iter().any(|key| !v.contains_key(*key)) {
        return Err(invalid());
    }
    let boolean = |key: &str| v.get(key).and_then(Value::as_bool).ok_or_else(invalid);
    let minute = |key: &str| -> Result<Option<i32>, BusinessError> {
        match v.get(key) {
            Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => {
                let n = n.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0 && (0.0..=1439.0).contains(n)).ok_or_else(invalid)?;
                Ok(Some(n as i32))
            }
            _ => Err(invalid()),
        }
    };
    let quiet_start = minute("quietStartMinute")?;
    let quiet_end = minute("quietEndMinute")?;
    if quiet_start.is_some() != quiet_end.is_some() || quiet_start.is_some() && quiet_start == quiet_end {
        return Err(invalid());
    }
    Ok(PushPreferenceInput { work: boolean("work")?, mention: boolean("mention")?, system: boolean("system")?, calendar: boolean("calendar")?, notice: boolean("notice")?, quiet_start, quiet_end })
}
#[derive(Clone, Copy)]
pub struct PushPreferences {
    pub enabled: bool,
    pub work: bool,
    pub mention: bool,
    pub system: bool,
    pub calendar: bool,
    pub notice: bool,
    pub quiet_start: Option<u16>,
    pub quiet_end: Option<u16>,
}
impl PushPreferences {
    pub fn permits(&self, category: &str) -> bool {
        self.enabled
            && match category {
                "work" => self.work,
                "mention" => self.mention,
                "system" => self.system,
                "calendar" => self.calendar,
                "notice" => self.notice,
                _ => false,
            }
    }
    pub fn quiet_delay(&self, now: i64) -> i64 {
        let (Some(start), Some(end)) = (self.quiet_start, self.quiet_end) else {
            return 0;
        };
        if start >= 1440 || end >= 1440 || start == end {
            return 0;
        }
        let day = (now.rem_euclid(86400000) + 7 * 60 * 60 * 1000).rem_euclid(86400000);
        let minute = day / 60000;
        let start = i64::from(start);
        let end = i64::from(end);
        let inside = if start < end {
            minute >= start && minute < end
        } else {
            minute >= start || minute < end
        };
        if !inside {
            return 0;
        }
        ((end - minute).rem_euclid(1440)) * 60000 - day.rem_euclid(60000)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushRetry {
    RemoveSubscription,
    Dead,
    RetryAt(i64),
}
pub fn retry(attempts: u32, status: Option<u16>, now: i64) -> PushRetry {
    if matches!(status, Some(404 | 410)) {
        PushRetry::RemoveSubscription
    } else if attempts >= 5 || status.is_some_and(|s| (400..500).contains(&s) && s != 429) {
        PushRetry::Dead
    } else {
        PushRetry::RetryAt(now.saturating_add((1000i64 << attempts.min(10)).min(3600000)))
    }
}
