//! Delivery policy shared by the native Field notification job partition.
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
