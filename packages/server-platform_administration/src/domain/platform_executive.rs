//! Platform Executive metric and source facts.
//!
//! These are portable owner facts over the Control-plane provisioning receipts
//! and the Control-read health observations. They never mint a session,
//! Platform role, Holding binding or permission; the composition backend
//! resolves the current Account, Platform assignment, Company and
//! `platform.executive.read` before calling the application use case.
use serde::{Deserialize, Serialize};

pub const EXECUTIVE_SCOPE: &str = "platform";
pub const EXECUTIVE_PERMISSION: &str = "platform.executive.read";
pub const EXECUTIVE_LABEL_MAX: usize = 250;
pub const EXECUTIVE_SOURCE_PAGE_SIZE: usize = 30;
pub const EXECUTIVE_SOURCE_PAGE_MAX: u32 = 100_000;
pub const EXECUTIVE_RANGE_DAYS_MAX: i64 = 366;
pub const DAY_MS: i64 = 86_400_000;

/// Current Platform identity supplied by the composition authority resolver.
/// The permission list is the same current IAM metadata used by the retained
/// route; it is never a grant invented by this owner.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformExecutiveMetricId {
    FailedProvisioning,
    AbnormalPlatformServices,
}

impl PlatformExecutiveMetricId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FailedProvisioning => "failed-provisioning",
            Self::AbnormalPlatformServices => "abnormal-platform-services",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "failed-provisioning" => Some(Self::FailedProvisioning),
            "abnormal-platform-services" => Some(Self::AbnormalPlatformServices),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformExecutiveMetricStatus {
    Ready,
    Empty,
}

impl PlatformExecutiveMetricStatus {
    pub fn for_rows(rows: &[PlatformExecutiveGroup]) -> Self {
        if rows.is_empty() {
            Self::Empty
        } else {
            Self::Ready
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveGroup {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveSourceReference {
    pub dataset: String,
    pub revision: String,
    pub referenced_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveMetric {
    pub id: PlatformExecutiveMetricId,
    pub label: String,
    pub formula: String,
    pub date_basis: String,
    pub total: String,
    pub rows: Vec<PlatformExecutiveGroup>,
    pub status: PlatformExecutiveMetricStatus,
    pub source_references: Vec<PlatformExecutiveSourceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveFilters {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveSnapshot {
    pub scope: String,
    pub reference_at: i64,
    pub filters: PlatformExecutiveFilters,
    pub source_revision: String,
    pub metrics: Vec<PlatformExecutiveMetric>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutiveDateRangeInput {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProvisioningFailureSourceRow {
    pub created_at: i64,
    pub action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AbnormalHealthSourceRow {
    pub component: String,
    pub status: String,
    pub checked_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PlatformExecutiveSourceRow {
    ProvisioningFailure(ProvisioningFailureSourceRow),
    AbnormalHealth(AbnormalHealthSourceRow),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformExecutiveSources {
    pub id: String,
    pub name: String,
    pub metric: PlatformExecutiveMetricId,
    pub filters: PlatformExecutiveFilters,
    pub rows: Vec<PlatformExecutiveSourceRow>,
    pub page: u32,
    pub has_next: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformExecutiveSourceRequest {
    pub metric: PlatformExecutiveMetricId,
    pub label: String,
    pub filters: PlatformExecutiveFilters,
    pub from_at: i64,
    pub to_at: i64,
    pub page: u32,
}

fn invalid(code: &'static str, message: &'static str) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::invalid(code, message)
}

fn forbidden(
    code: &'static str,
    message: &'static str,
) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Forbidden,
        code,
        message,
    )
}

fn unavailable(
    code: &'static str,
    message: &'static str,
) -> erpx_server_shared::public::BusinessError {
    erpx_server_shared::public::BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        code,
        message,
    )
}

pub fn require_platform_executive(
    actor: &PlatformExecutiveActor,
) -> Result<(), erpx_server_shared::public::BusinessError> {
    if matches!(actor.role.as_str(), "platform_admin" | "platform_auditor")
        && actor
            .permissions
            .iter()
            .any(|permission| permission == EXECUTIVE_PERMISSION)
    {
        Ok(())
    } else {
        Err(forbidden(
            "PLATFORM_EXECUTIVE_FORBIDDEN",
            "Your Platform role cannot read Executive insights.",
        ))
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

fn format_date(year: i64, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Parse one exact ISO calendar date at UTC midnight.
pub fn parse_utc_date(value: &str) -> Result<i64, erpx_server_shared::public::BusinessError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a valid UTC date range.",
        ));
    }
    let year = value[0..4].parse::<i64>().map_err(|_| {
        invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a valid UTC date range.",
        )
    })?;
    let month = value[5..7].parse::<u32>().map_err(|_| {
        invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a valid UTC date range.",
        )
    })?;
    let day = value[8..10].parse::<u32>().map_err(|_| {
        invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a valid UTC date range.",
        )
    })?;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return Err(invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a valid UTC date range.",
        ));
    }
    Ok(days_from_civil(year, month, day) * DAY_MS)
}

fn date_range(
    from: &str,
    to: &str,
) -> Result<(PlatformExecutiveFilters, i64, i64), erpx_server_shared::public::BusinessError> {
    let from_at = parse_utc_date(from)?;
    let to_start = parse_utc_date(to)?;
    if from_at > to_start || to_start - from_at > EXECUTIVE_RANGE_DAYS_MAX * DAY_MS {
        return Err(invalid(
            "PLATFORM_EXECUTIVE_FILTER_INVALID",
            "Choose a date range of up to one year.",
        ));
    }
    let to_at = to_start + DAY_MS;
    Ok((
        PlatformExecutiveFilters {
            from: from.to_owned(),
            to: to.to_owned(),
        },
        from_at,
        to_at,
    ))
}

/// Resolve the bounded UTC interval. Omitted filters keep the retained
/// year-to-date default; a partial or malformed interval fails closed.
pub fn resolve_filters(
    raw: Option<&ExecutiveDateRangeInput>,
    now: i64,
) -> Result<(PlatformExecutiveFilters, i64, i64), erpx_server_shared::public::BusinessError> {
    match raw {
        None => {
            let today = now.div_euclid(DAY_MS);
            let (year, _, _) = civil_from_days(today);
            let from = format_date(year, 1, 1);
            let (year, month, day) = civil_from_days(today);
            let to = format_date(year, month, day);
            date_range(&from, &to)
        }
        Some(range) => match (range.from.as_deref(), range.to.as_deref()) {
            (Some(from), Some(to)) if !from.is_empty() && !to.is_empty() => date_range(from, to),
            _ => Err(invalid(
                "PLATFORM_EXECUTIVE_FILTER_INVALID",
                "Choose a date range of up to one year.",
            )),
        },
    }
}

pub fn validate_source_request(
    metric: &str,
    label: &str,
    from: &str,
    to: &str,
    page: Option<&str>,
) -> Result<PlatformExecutiveSourceRequest, erpx_server_shared::public::BusinessError> {
    let metric = PlatformExecutiveMetricId::parse(metric).ok_or_else(|| {
        invalid(
            "PLATFORM_EXECUTIVE_SOURCE_INVALID",
            "Choose a supported Executive metric.",
        )
    })?;
    if label.is_empty() || label.len() > EXECUTIVE_LABEL_MAX {
        return Err(invalid(
            "PLATFORM_EXECUTIVE_SOURCE_INVALID",
            "Choose an Executive metric group.",
        ));
    }
    let (filters, from_at, to_at) = date_range(from, to)?;
    let page = match page {
        None => 1,
        Some(raw) => {
            let page = raw.parse::<u32>().map_err(|_| {
                invalid(
                    "PLATFORM_EXECUTIVE_SOURCE_INVALID",
                    "Choose a valid Executive source page.",
                )
            })?;
            if !(1..=EXECUTIVE_SOURCE_PAGE_MAX).contains(&page) {
                return Err(invalid(
                    "PLATFORM_EXECUTIVE_SOURCE_INVALID",
                    "Choose a valid Executive source page.",
                ));
            }
            page
        }
    };
    Ok(PlatformExecutiveSourceRequest {
        metric,
        label: label.to_owned(),
        filters,
        from_at,
        to_at,
        page,
    })
}

/// Sum exact integer metric values without losing precision. A malformed or
/// overflowing value is a source failure, never a silently truncated number.
pub fn metric_total(
    rows: &[PlatformExecutiveGroup],
) -> Result<String, erpx_server_shared::public::BusinessError> {
    let mut total: i128 = 0;
    for row in rows {
        let bytes = row.value.as_bytes();
        let digits = match bytes.first() {
            Some(b'-') => &bytes[1..],
            _ => bytes,
        };
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return Err(unavailable(
                "PLATFORM_METRIC_INVALID",
                "The control plane returned an invalid metric value.",
            ));
        }
        let value = row.value.parse::<i128>().map_err(|_| {
            unavailable(
                "PLATFORM_METRIC_INVALID",
                "The control plane returned an invalid metric value.",
            )
        })?;
        total = total.checked_add(value).ok_or_else(|| {
            unavailable(
                "PLATFORM_METRIC_NOT_EXACT",
                "The control plane returned a value that cannot be represented exactly.",
            )
        })?;
    }
    Ok(total.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(role: &str, permissions: &[&str]) -> PlatformExecutiveActor {
        PlatformExecutiveActor {
            user_id: "operator".into(),
            assignment_id: "platform-assignment".into(),
            role: role.into(),
            permissions: permissions.iter().map(|value| (*value).into()).collect(),
        }
    }

    #[test]
    fn executive_access_requires_the_current_platform_permission() {
        assert!(
            require_platform_executive(&actor("platform_admin", &[EXECUTIVE_PERMISSION])).is_ok()
        );
        assert!(
            require_platform_executive(&actor("platform_auditor", &[EXECUTIVE_PERMISSION])).is_ok()
        );
        assert_eq!(
            require_platform_executive(&actor("platform_admin", &[]))
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_FORBIDDEN"
        );
        assert_eq!(
            require_platform_executive(&actor("legal_editor", &[EXECUTIVE_PERMISSION]))
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_FORBIDDEN"
        );
    }

    #[test]
    fn filters_use_year_to_date_and_reject_partial_or_oversized_ranges() {
        let (filters, from_at, to_at) = resolve_filters(None, 1_800_000_000_000).unwrap();
        assert_eq!(filters.from, "2027-01-01");
        assert_eq!(filters.to, "2027-01-15");
        assert_eq!(to_at - from_at, 15 * DAY_MS);
        let range = ExecutiveDateRangeInput {
            from: Some("2026-06-01".into()),
            to: Some("2026-06-30".into()),
        };
        let (filters, from_at, to_at) = resolve_filters(Some(&range), 0).unwrap();
        assert_eq!(filters.from, "2026-06-01");
        assert_eq!(filters.to, "2026-06-30");
        assert_eq!(to_at - from_at, 30 * DAY_MS);
        let partial = ExecutiveDateRangeInput {
            from: Some("2026-06-01".into()),
            to: None,
        };
        assert_eq!(
            resolve_filters(Some(&partial), 0).unwrap_err().code,
            "PLATFORM_EXECUTIVE_FILTER_INVALID"
        );
        let reversed = ExecutiveDateRangeInput {
            from: Some("2026-06-02".into()),
            to: Some("2026-06-01".into()),
        };
        assert!(resolve_filters(Some(&reversed), 0).is_err());
        let oversized = ExecutiveDateRangeInput {
            from: Some("2024-01-01".into()),
            to: Some("2025-01-02".into()),
        };
        assert!(resolve_filters(Some(&oversized), 0).is_err());
        assert!(parse_utc_date("2026-02-29").is_err());
        assert!(parse_utc_date("2024-02-29").is_ok());
        assert!(parse_utc_date("2026-6-1").is_err());
        assert!(parse_utc_date("2026-13-01").is_err());
    }

    #[test]
    fn source_requests_accept_only_known_metrics_labels_and_bounded_pages() {
        let request = validate_source_request(
            "failed-provisioning",
            "2026-06",
            "2026-06-01",
            "2026-06-30",
            Some("2"),
        )
        .unwrap();
        assert_eq!(
            request.metric,
            PlatformExecutiveMetricId::FailedProvisioning
        );
        assert_eq!(request.page, 2);
        assert_eq!(request.from_at, 1_780_272_000_000);
        assert_eq!(request.to_at, 1_782_864_000_000);
        assert_eq!(
            validate_source_request("unknown", "label", "2026-06-01", "2026-06-30", None)
                .unwrap_err()
                .code,
            "PLATFORM_EXECUTIVE_SOURCE_INVALID"
        );
        assert!(
            validate_source_request(
                "abnormal-platform-services",
                "",
                "2026-06-01",
                "2026-06-30",
                None
            )
            .is_err()
        );
        assert!(
            validate_source_request(
                "abnormal-platform-services",
                &"x".repeat(EXECUTIVE_LABEL_MAX + 1),
                "2026-06-01",
                "2026-06-30",
                None
            )
            .is_err()
        );
        for page in ["0", "100001", "abc", ""] {
            assert!(
                validate_source_request(
                    "abnormal-platform-services",
                    "label",
                    "2026-06-01",
                    "2026-06-30",
                    Some(page)
                )
                .is_err(),
                "page {page}"
            );
        }
    }

    #[test]
    fn metric_totals_are_exact_and_malformed_values_fail() {
        let rows = vec![
            PlatformExecutiveGroup {
                label: "a".into(),
                value: "9007199254740992".into(),
            },
            PlatformExecutiveGroup {
                label: "b".into(),
                value: "1".into(),
            },
        ];
        assert_eq!(metric_total(&rows).unwrap(), "9007199254740993");
        for value in ["", "1.5", "x", " 1", "+1"] {
            assert_eq!(
                metric_total(&[PlatformExecutiveGroup {
                    label: "a".into(),
                    value: value.into()
                }])
                .unwrap_err()
                .code,
                "PLATFORM_METRIC_INVALID"
            );
        }
    }
}
