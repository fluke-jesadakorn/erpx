//! Native payroll schedule validation.
//!
//! This mirrors the retained TypeScript oracle in
//! `modules/people-payroll/server/src/domain/payroll-readiness.ts`
//! (`payrollScheduleSchema`) and the decision rules in
//! `modules/people-payroll/server/src/application/payroll-readiness.ts`
//! (`savePayrollSchedule`): the strict schedule shape, the version gate and
//! the weekly/fortnightly anchor-date requirement. No IO lives here; the
//! adapter owns the caller-transaction reads and writes.

/// Pay-group frequencies, byte-identical to the retained database constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleFrequency {
    Weekly,
    Fortnightly,
    Semimonthly,
    Monthly,
}

impl ScheduleFrequency {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "weekly" => Some(Self::Weekly),
            "fortnightly" => Some(Self::Fortnightly),
            "semimonthly" => Some(Self::Semimonthly),
            "monthly" => Some(Self::Monthly),
            _ => None,
        }
    }

    /// Weekly and fortnightly payrolls need an anchor date; monthly and
    /// semimonthly payrolls derive their periods from the calendar month.
    pub fn anchor_required(self) -> bool {
        matches!(self, Self::Weekly | Self::Fortnightly)
    }
}

/// Validated schedule input. Field names mirror the TypeScript schema so the
/// future transport can accept the retained wire shape unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayrollScheduleInput {
    pub enabled: bool,
    pub pay_delay_days: i64,
    pub anchor_date: Option<String>,
    pub expected_version: i64,
}

/// Retained schedule read model, mirroring the TypeScript
/// `getPayrollSchedule` return shape. Note the read applies **no**
/// status filter, exactly like the retained
/// `readGetPayrollScheduleGroup` query; the active-group gate lives
/// only on the save path.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PayrollScheduleView {
    pub id: String,
    pub frequency: String,
    pub timezone: String,
    pub currency: String,
    pub enabled: bool,
    #[serde(rename = "payDelayDays")]
    pub pay_delay_days: i64,
    #[serde(rename = "anchorDate")]
    pub anchor_date: Option<String>,
    pub version: i64,
}
/// Current command identity. The composition supplies this after the retained
/// Holding proof; the adapter never constructs or widens authority.
pub const SCHEDULE_OPERATION: &str = "people.payroll.schedule";

/// Failure codes are byte-identical to the TypeScript oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRefusal {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

pub fn holding_required() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 403,
        code: "COMPANY_PAYROLL_SCOPE_REQUIRED",
        message: "Company-wide payroll requires a holding-level assignment.",
    }
}

pub fn group_missing() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 404,
        code: "PAY_GROUP_NOT_FOUND",
        message: "Active pay group not found.",
    }
}

/// Read-path miss. The retained `getPayrollSchedule` reports a missing group
/// without the active-group qualifier; the save path keeps `group_missing`.
pub fn group_missing_read() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 404,
        code: "PAY_GROUP_NOT_FOUND",
        message: "Pay group not found.",
    }
}

pub fn version_conflict() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 409,
        code: "VERSION_CONFLICT",
        message: "Pay group schedule changed. Refresh before saving.",
    }
}

pub fn anchor_required() -> ScheduleRefusal {
    ScheduleRefusal {
        status: 422,
        code: "PAYROLL_ANCHOR_REQUIRED",
        message: "Set an anchor date for weekly or fortnightly payroll.",
    }
}

fn invalid_input(message: &'static str) -> ScheduleRefusal {
    ScheduleRefusal {
        status: 422,
        code: "PAYROLL_SCHEDULE_INPUT",
        message,
    }
}

fn valid_date(value: &str) -> bool {
    // Shape plus real calendar validity, matching the retained zod `.date()`
    // check; the TEXT column only enforces the shape, so the adapter rejects
    // impossible months and days here.
    let bytes = value.as_bytes();
    if value.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let year: i64 = value[0..4].parse().unwrap_or(0);
    let month: i64 = value[5..7].parse().unwrap_or(0);
    let day: i64 = value[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) || day < 1 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= last
}

/// Strict schedule-input validation. Out-of-range pay delays, malformed
/// anchor dates and non-positive versions fail closed here.
pub fn validate_schedule(
    enabled: bool,
    pay_delay_days: i64,
    anchor_date: Option<&str>,
    expected_version: i64,
) -> Result<PayrollScheduleInput, ScheduleRefusal> {
    if !(1..=45).contains(&pay_delay_days) {
        return Err(invalid_input("Pay delay must be between 1 and 45 days."));
    }
    if expected_version <= 0 {
        return Err(invalid_input("Choose the current schedule version."));
    }
    let anchor_date = match anchor_date {
        None => None,
        Some(raw) if raw.is_empty() => None,
        Some(raw) => {
            if !valid_date(raw) {
                return Err(invalid_input("Choose a valid anchor date."));
            }
            Some(raw.to_owned())
        }
    };
    Ok(PayrollScheduleInput {
        enabled,
        pay_delay_days,
        anchor_date,
        expected_version,
    })
}

/// Retained anchor rule: an enabled weekly/fortnightly schedule needs an
/// anchor date; disabled schedules and monthly/semimonthly payrolls do not.
pub fn check_anchor(
    frequency: ScheduleFrequency,
    input: &PayrollScheduleInput,
) -> Result<(), ScheduleRefusal> {
    if input.enabled && frequency.anchor_required() && input.anchor_date.is_none() {
        return Err(anchor_required());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_parse_and_name_their_anchor_rule() {
        assert_eq!(ScheduleFrequency::parse("weekly"), Some(ScheduleFrequency::Weekly));
        assert_eq!(ScheduleFrequency::parse("fortnightly"), Some(ScheduleFrequency::Fortnightly));
        assert_eq!(ScheduleFrequency::parse("semimonthly"), Some(ScheduleFrequency::Semimonthly));
        assert_eq!(ScheduleFrequency::parse("monthly"), Some(ScheduleFrequency::Monthly));
        assert_eq!(ScheduleFrequency::parse("daily"), None);
        assert!(ScheduleFrequency::Weekly.anchor_required());
        assert!(ScheduleFrequency::Fortnightly.anchor_required());
        assert!(!ScheduleFrequency::Monthly.anchor_required());
        assert!(!ScheduleFrequency::Semimonthly.anchor_required());
    }

    #[test]
    fn strict_shape_rejects_bad_delays_dates_and_versions() {
        assert!(validate_schedule(true, 0, None, 1).is_err());
        assert!(validate_schedule(true, 46, None, 1).is_err());
        assert!(validate_schedule(true, 7, Some("30-11-2026"), 1).is_err());
        assert!(validate_schedule(true, 7, Some("2026-13-01"), 1).is_err());
        assert!(validate_schedule(true, 7, Some("2026-02-30"), 1).is_err());
        assert!(validate_schedule(true, 7, None, 0).is_err());
        let kept = validate_schedule(true, 7, Some("2026-11-30"), 3).unwrap();
        assert_eq!(kept.pay_delay_days, 7);
        assert_eq!(kept.anchor_date.as_deref(), Some("2026-11-30"));
        let empty_anchor = validate_schedule(false, 1, Some(""), 1).unwrap();
        assert_eq!(empty_anchor.anchor_date, None);
    }

    #[test]
    fn anchor_rule_matches_the_retained_oracle() {
        let weekly = validate_schedule(true, 7, None, 1).unwrap();
        let refusal = check_anchor(ScheduleFrequency::Weekly, &weekly)
            .expect_err("enabled weekly payroll needs an anchor date");
        assert_eq!(refusal.code, "PAYROLL_ANCHOR_REQUIRED");
        assert_eq!(refusal.status, 422);
        assert!(check_anchor(ScheduleFrequency::Weekly, &validate_schedule(false, 7, None, 1).unwrap()).is_ok());
        assert!(check_anchor(ScheduleFrequency::Monthly, &validate_schedule(true, 7, None, 1).unwrap()).is_ok());
        assert!(check_anchor(
            ScheduleFrequency::Fortnightly,
            &validate_schedule(true, 7, Some("2026-11-30"), 1).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn schedule_view_wire_shape_matches_the_retained_read_model() {
        let view = PayrollScheduleView {
            id: "grp-1".to_owned(),
            frequency: "weekly".to_owned(),
            timezone: "Asia/Bangkok".to_owned(),
            currency: "THB".to_owned(),
            enabled: true,
            pay_delay_days: 7,
            anchor_date: Some("2026-11-30".to_owned()),
            version: 3,
        };
        let wire = serde_json::to_value(&view).unwrap();
        assert_eq!(wire.get("payDelayDays").and_then(|v| v.as_i64()), Some(7));
        assert_eq!(wire.get("anchorDate").and_then(|v| v.as_str()), Some("2026-11-30"));
        assert_eq!(wire.get("version").and_then(|v| v.as_i64()), Some(3));
    }

    #[test]
    fn refusals_keep_the_retained_codes_and_messages() {
        let held = holding_required();
        assert_eq!((held.status, held.code), (403, "COMPANY_PAYROLL_SCOPE_REQUIRED"));
        let missing = group_missing();
        assert_eq!((missing.status, missing.code), (404, "PAY_GROUP_NOT_FOUND"));
        let missing_read = group_missing_read();
        assert_eq!(
            (missing_read.status, missing_read.code, missing_read.message),
            (404, "PAY_GROUP_NOT_FOUND", "Pay group not found.")
        );
        let conflict = version_conflict();
        assert_eq!(conflict.code, "VERSION_CONFLICT");
        assert_eq!(conflict.message, "Pay group schedule changed. Refresh before saving.");
    }
}
