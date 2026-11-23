//! Native PMS dashboard/report read facts over PMS-owned rows.
//!
//! This is a read projection, never a command, receipt or source purchase. It
//! retains the formulas of the migrated `pmsRangeReport` day rows
//! (`packages/server-pms/src/adapters/pms/reports.ts`): occupancy, ADR and
//! RevPAR are derived from the per-day capacity/sold and the posted entry net
//! values, in minor units, using ECMAScript `Math.round` (ties toward
//! `+Infinity`) exactly as the retained reducer does. Rates use the retained
//! divisor guards, so a day without capacity or sold rooms contributes zero
//! instead of `NaN`.
//!
//! Core day/flow/night facts stay PMS-owned: `pms_properties`, `pms_records`
//! (`stay`, `entry`, `night`, `room`, `room_type`, `rate`, `block`,
//! `guest_account`, `shift`) and `pms_jobs`. The booking-observation `pace` and
//! `channels` windows read `pms_booking_observations` and
//! `pms_booking_history_coverage`; `finance_balances` reads the existing
//! `pms_reporting_facts` journal-line port. [`PmsReportBalances::source`] still
//! names the PMS-owned guest-account projection so a caller can never mistake
//! it for the Finance ledger.
use crate::domain::room_board::{
    PmsBoardActor, PmsPropertyRow, PmsPropertySummary, cp_len, parse_date,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

/// Retained `pmsReportRange` refine: `to - from < 93 * 86400000`, so a report
/// spans at most 93 calendar days including both ends.
pub const MAX_RANGE_DAYS: i64 = 93;
/// One row per calendar day of a bounded report range.
pub const MAX_DAYS: usize = 93;
/// Retained `pmsReportEntries` page bound (`z.number().int().min(1).max(10000)`).
pub const MAX_PAGE: u32 = 10_000;
/// Retained `pmsReportEntries` page size.
pub const PAGE_SIZE: usize = 50;
/// Bounded identifier width used by the native PMS transport.
pub const MAX_ID: usize = 200;

/// The retained report views: the bounded range summary and the bounded entry
/// ledger. `entries` is the retained `?view=entries` branch of the same route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmsReportKind {
    Range,
    Entries,
}

impl PmsReportKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "range" => Some(Self::Range),
            "entries" => Some(Self::Entries),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Range => "range",
            Self::Entries => "entries",
        }
    }
}

/// Retained `pmsRangeReport` scope guard: `pms.read` and `pms.reports` on a
/// live company/branch work context. Composition must still resolve the current
/// Account session; this check is not authority of its own. The retained
/// `actor.beta` refusal is composition's (the native board actor carries no
/// beta flag).
pub fn require_report(actor: &PmsBoardActor, now: i64) -> Result<(), BusinessError> {
    if !actor.has("pms.read") || !actor.has("pms.reports") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    if !actor.modules.iter().any(|module| module == "pms") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MODULE_FORBIDDEN",
            "Hotels are unavailable to the selected role.",
        ));
    }
    if actor.department_id.is_some() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "PMS_BRANCH_REQUIRED",
            "Select a company or branch area.",
        ));
    }
    if now >= actor.expires_at {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
            "Your selected work context changed. Review the current workspace and role.",
        ));
    }
    Ok(())
}

/// Retained Company/branch scope: another branch is concealed as `404`.
pub fn in_scope(actor: &PmsBoardActor, property: &PmsPropertyRow) -> bool {
    property.company_id == actor.company_id
        && actor
            .branch_id
            .as_deref()
            .is_none_or(|branch| branch == property.branch_id)
}

/// Transport-invalid report selection. Never used for a missing hotel, which
/// keeps the retained `404 PMS_NOT_FOUND` concealment.
pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_REPORT_REQUEST_INVALID",
        "Use one bounded hotel report selection.",
    )
}

/// Retained range refusal text (`Choose up to 93 days`).
pub fn range_invalid() -> BusinessError {
    BusinessError::invalid("PMS_REPORT_RANGE", "Choose up to 93 days")
}

/// Retained date field refusal (`z.string().date()`).
pub fn date_invalid() -> BusinessError {
    BusinessError::invalid("PMS_REPORT_DATE", "Choose a valid report date.")
}

/// Retained booking-observation refusal (`PMS_PACE_RANGE`).
pub fn pace_range_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_PACE_RANGE",
        "Choose a past observation and an earlier comparison time",
    )
}

/// An observation carried by the retained range input must be RFC 3339 with an
/// explicit offset, exactly as `z.iso.datetime({ offset: true })` requires.
pub fn valid_observation(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339).is_ok()
}

pub fn read_limit(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "PMS_REPORT_READ_LIMIT", message)
}

pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_REPORT_READ_INVALID",
        "The hotel report could not be read.",
    )
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_NOT_FOUND",
        "Hotel not found in this workspace.",
    )
}

fn bounded_id(value: &str) -> bool {
    !value.is_empty() && cp_len(value) <= MAX_ID && !value.chars().any(char::is_control)
}

/// Retained `pmsReportRange` input: one hotel, one inclusive business-date
/// range and the optional booking-observation times consumed by the pace
/// windows. Both observation fields are RFC 3339 with an explicit offset.
#[derive(Clone, Debug)]
pub struct PmsReportRange {
    pub property_id: String,
    pub from: String,
    pub to: String,
    pub as_of: Option<String>,
    pub compare_as_of: Option<String>,
}

impl PmsReportRange {
    /// Returns the inclusive day distance (`to - from`) after validation.
    pub fn validate(&self) -> Result<i64, BusinessError> {
        if !bounded_id(&self.property_id)
            || self
                .as_of
                .as_deref()
                .is_some_and(|value| !valid_observation(value))
            || self
                .compare_as_of
                .as_deref()
                .is_some_and(|value| !valid_observation(value))
        {
            return Err(invalid());
        }
        validate_range(&self.from, &self.to)
    }

    /// Resolves the retained observation window. The reported `as_of` is the
    /// current clock; an unsupplied comparison is seven days earlier. A future
    /// or mis-ordered observation is refused exactly as the retained
    /// application refuses it.
    pub fn observation(&self, now_ms: i64) -> Result<(String, String), BusinessError> {
        let parse = |value: &str| OffsetDateTime::parse(value, &Rfc3339).ok();
        let now = OffsetDateTime::from_unix_timestamp_nanos(i128::from(now_ms) * 1_000_000)
            .map_err(|_| pace_range_invalid())?;
        let as_of = self.as_of.as_deref().and_then(parse).unwrap_or(now);
        let compare_as_of = match self.compare_as_of.as_deref().and_then(parse) {
            Some(value) => value,
            None => as_of - Duration::days(7),
        };
        if as_of > now || compare_as_of >= as_of {
            return Err(pace_range_invalid());
        }
        Ok((
            as_of.format(&Rfc3339).map_err(|_| pace_range_invalid())?,
            compare_as_of
                .format(&Rfc3339)
                .map_err(|_| pace_range_invalid())?,
        ))
    }
}

/// Retained `pmsReportEntries` input: one hotel, one ledger date and a page.
#[derive(Clone, Debug)]
pub struct PmsReportEntries {
    pub property_id: String,
    pub date: String,
    pub page: u32,
}

impl PmsReportEntries {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !bounded_id(&self.property_id) || !(1..=MAX_PAGE).contains(&self.page) {
            return Err(invalid());
        }
        if parse_date(&self.date).is_none() {
            return Err(date_invalid());
        }
        Ok(())
    }

    /// Retained `(page - 1) * 50` offset.
    pub fn offset(&self) -> i64 {
        i64::from(self.page.saturating_sub(1)) * PAGE_SIZE as i64
    }
}

/// Retained inclusive business-date range: calendar-valid, ordered and at most
/// [`MAX_RANGE_DAYS`] days apart.
pub fn validate_range(from: &str, to: &str) -> Result<i64, BusinessError> {
    let from = parse_date(from).ok_or_else(date_invalid)?;
    let to = parse_date(to).ok_or_else(date_invalid)?;
    if from > to || to - from >= MAX_RANGE_DAYS {
        return Err(range_invalid());
    }
    Ok(to - from)
}

/// One retained day row. Every amount is a whole minor unit exactly as the
/// retained SQL reads `pms_records.data_json`, never a float.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportDay {
    pub date: String,
    pub capacity: i64,
    pub sold: i64,
    pub room_revenue: i64,
    pub net_revenue: i64,
    pub tax: i64,
    pub refunds: i64,
    pub cancellations: i64,
    pub no_shows: i64,
}

/// Retained reducer output plus the three derived rates. `occupancy` is the
/// unrounded `sold / capacity` fraction; `adr` and `revpar` are `Math.round`
/// minor units.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportTotals {
    pub capacity: i64,
    pub sold: i64,
    pub room_revenue: i64,
    pub net_revenue: i64,
    pub tax: i64,
    pub refunds: i64,
    pub cancellations: i64,
    pub no_shows: i64,
    pub occupancy: f64,
    pub adr: i64,
    pub revpar: i64,
}

/// Retained dashboard stay counts, anchored on the report's closing business
/// date `to`: `arrivals` are still-reserved stays arriving that date,
/// `departures` are in-house stays due on or before that date (overdue
/// included) and `inHouse` is every in-house stay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportFlow {
    pub arrivals: i64,
    pub departures: i64,
    pub in_house: i64,
}

/// Retained `controlBalances` projection: the PMS-owned guest-account balances
/// in minor units. This is **not** the Finance journal ledger; the retained
/// Finance-backed `balances` block needs the `pms_reporting_facts` port and is
/// omitted from this slice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportBalances {
    /// Provenance marker for callers: always `pms_guest_accounts`.
    pub source: String,
    pub due: i64,
    pub advance: i64,
    pub deposit: i64,
    pub total_accounts: i64,
    pub open_accounts: i64,
    pub credit_accounts: i64,
    pub accounts_with_due: i64,
}

/// One booking-observation pace point from `pms_booking_observations`, anchored
/// on the resolved `as_of` / `compare_as_of` window and its coverage start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsPacePoint {
    pub as_of: String,
    pub coverage_since: Option<String>,
    pub covered: bool,
    pub reservations: i64,
    pub room_nights: i64,
    pub quoted_total: i64,
}

/// Retained net pickup between two covered observation windows. It is null
/// unless both windows are covered, never a partial difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsNetPickup {
    pub reservations: i64,
    pub room_nights: i64,
    pub quoted_total: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportPace {
    pub current: Option<PmsPacePoint>,
    pub previous: Option<PmsPacePoint>,
    pub net_pickup: Option<PmsNetPickup>,
}

/// One retained per-source channel row from the PMS-owned reservation/stay
/// projections, bounded by the report range and observation clock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsChannelReport {
    pub source: String,
    pub reservations: i64,
    pub room_nights: i64,
    pub quoted_total: i64,
    pub cancelled: i64,
    pub no_shows: i64,
    pub recent_pickup: i64,
    pub unknown_creation: i64,
}

/// One signed account balance from the existing `pms_reporting_facts`
/// journal-line port. The SQL and provenance stay with that owner port; this
/// owner only consumes the bounded account/minor-unit projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PmsFinanceBalanceRow {
    pub account_id: String,
    pub balance: i64,
}

/// Retained Finance-backed dashboard balances, signed by the property's own
/// chart configuration exactly as `pmsRangeReport` signs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsFinanceBalances {
    pub receivable: i64,
    pub advance: i64,
    pub deposit: i64,
    pub refund_payable: i64,
    pub unallocated: i64,
}

/// The retained night-audit blocker summary that PMS-owned facts can answer.
/// Day-control counts (open bills, pending payments, unposted receipts,
/// unbalanced journals, ...) belong to Finance/POS/Workflow ports and are not
/// invented here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportNightAudit {
    pub business_date: String,
    /// `night` record state for the business date (`completed`, `pending`,
    /// `blocked`), absent before the first close attempt.
    pub night_state: Option<String>,
    /// `pms_jobs` state for the business date, absent before the worker runs.
    pub job_state: Option<String>,
    /// `blockers` is empty; never a proof that Finance day controls pass.
    pub ready: bool,
    pub blockers: Vec<String>,
}

/// One bounded report response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportData {
    pub property: PmsPropertySummary,
    pub business_date: String,
    pub currency: String,
    pub from: String,
    pub to: String,
    pub days: Vec<PmsReportDay>,
    pub totals: PmsReportTotals,
    pub flow: PmsReportFlow,
    pub balances: PmsReportBalances,
    pub night_audit: PmsReportNightAudit,
    /// Observation clock the retained range response reports.
    pub as_of: String,
    /// Booking-observation pickup windows.
    pub pace: PmsReportPace,
    /// Per-source channel projection.
    pub channels: Vec<PmsChannelReport>,
    /// Finance-backed balances from the `pms_reporting_facts` port.
    pub finance_balances: PmsFinanceBalances,
}

/// One retained entry-ledger row. `data` is the stored `entry` document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PmsReportEntry {
    pub id: String,
    pub data: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsReportEntriesData {
    pub date: String,
    pub page: u32,
    pub rows: Vec<PmsReportEntry>,
    pub has_next: bool,
}

/// ECMAScript `Math.round`: nearest integer with ties toward `+Infinity`
/// (so `-2.5` is `-2`, unlike Rust's `f64::round`).
pub fn js_round(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    let floor = value.floor();
    let rounded = if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    };
    if rounded >= i64::MAX as f64 {
        i64::MAX
    } else if rounded <= i64::MIN as f64 {
        i64::MIN
    } else {
        rounded as i64
    }
}

/// Retained reducer over the day rows, with the retained divisor guards.
pub fn summarize(days: &[PmsReportDay]) -> PmsReportTotals {
    let mut totals = PmsReportTotals {
        capacity: 0,
        sold: 0,
        room_revenue: 0,
        net_revenue: 0,
        tax: 0,
        refunds: 0,
        cancellations: 0,
        no_shows: 0,
        occupancy: 0.0,
        adr: 0,
        revpar: 0,
    };
    for day in days {
        totals.capacity += day.capacity;
        totals.sold += day.sold;
        totals.room_revenue += day.room_revenue;
        totals.net_revenue += day.net_revenue;
        totals.tax += day.tax;
        totals.refunds += day.refunds;
        totals.cancellations += day.cancellations;
        totals.no_shows += day.no_shows;
    }
    totals.occupancy = if totals.capacity != 0 {
        totals.sold as f64 / totals.capacity as f64
    } else {
        0.0
    };
    totals.adr = if totals.sold != 0 {
        js_round(totals.room_revenue as f64 / totals.sold as f64)
    } else {
        0
    };
    totals.revpar = if totals.capacity != 0 {
        js_round(totals.room_revenue as f64 / totals.capacity as f64)
    } else {
        0
    };
    totals
}

/// Retained pace window assembly: the first two SQL rows are current and
/// previous, and `netPickup` exists only when both windows are covered.
pub fn assemble_pace(points: Vec<PmsPacePoint>) -> PmsReportPace {
    let mut rows = points.into_iter();
    let current = rows.next();
    let previous = rows.next();
    let net_pickup = match (&current, &previous) {
        (Some(current), Some(previous)) if current.covered && previous.covered => {
            Some(PmsNetPickup {
                reservations: current.reservations - previous.reservations,
                room_nights: current.room_nights - previous.room_nights,
                quoted_total: current.quoted_total - previous.quoted_total,
            })
        }
        _ => None,
    };
    PmsReportPace {
        current,
        previous,
        net_pickup,
    }
}

/// Signs the reporting-facts account balances with the property's own chart
/// configuration. Assets stay positive; advances, deposits and the online
/// accounting liabilities are inverted exactly as the retained reducer does.
pub fn finance_balances(
    rows: &[PmsFinanceBalanceRow],
    config: &serde_json::Value,
) -> PmsFinanceBalances {
    fn account<'a>(parent: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
        parent
            .and_then(|value| value.get(key))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
    }
    let signed = |name: Option<&str>, sign: i64| {
        name.map_or(0, |name| {
            rows.iter()
                .filter(|row| row.account_id == name)
                .map(|row| row.balance.saturating_mul(sign))
                .sum()
        })
    };
    let accounts = config.get("accounts");
    let online = config.get("onlineAccounting");
    PmsFinanceBalances {
        receivable: signed(account(accounts, "receivable"), 1),
        advance: signed(account(accounts, "advance"), -1),
        deposit: signed(account(accounts, "deposit"), -1),
        refund_payable: signed(account(online, "refundPayable"), -1),
        unallocated: signed(account(online, "unallocated"), -1),
    }
}

/// Fail closed when a read does not describe exactly the requested range in
/// strictly ascending calendar order; a report never silently renders a gap.
pub fn validate_days(from: &str, to: &str, days: &[PmsReportDay]) -> Result<(), BusinessError> {
    let from = parse_date(from).ok_or_else(date_invalid)?;
    let to = parse_date(to).ok_or_else(date_invalid)?;
    let expected = (to - from + 1) as usize;
    if days.len() != expected {
        return Err(read_invalid());
    }
    if days.len() > MAX_DAYS {
        return Err(read_limit("The hotel report exceeds its bounded range."));
    }
    let mut previous: Option<i64> = None;
    for day in days {
        let current = parse_date(&day.date).ok_or_else(read_invalid)?;
        if current < from || current > to || previous.is_some_and(|value| current <= value) {
            return Err(read_invalid());
        }
        previous = Some(current);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, capacity: i64, sold: i64, revenue: i64) -> PmsReportDay {
        PmsReportDay {
            date: date.to_owned(),
            capacity,
            sold,
            room_revenue: revenue,
            net_revenue: revenue,
            tax: 0,
            refunds: 0,
            cancellations: 0,
            no_shows: 0,
        }
    }

    #[test]
    fn rounding_matches_ecmascript_half_up() {
        assert_eq!(js_round(0.5), 1);
        assert_eq!(js_round(1.5), 2);
        assert_eq!(js_round(2.5), 3);
        assert_eq!(js_round(-0.5), 0);
        assert_eq!(js_round(-1.5), -1);
        assert_eq!(js_round(-2.5), -2);
        assert_eq!(js_round(2.49), 2);
        assert_eq!(js_round(f64::NAN), 0);
    }

    #[test]
    fn totals_keep_the_retained_divisor_guards() {
        let totals = summarize(&[day("2026-11-01", 3, 1, 1000)]);
        assert_eq!(totals.adr, 1000);
        assert_eq!(totals.revpar, 333);
        assert!((totals.occupancy - 1.0 / 3.0).abs() < f64::EPSILON);

        let empty = summarize(&[]);
        assert_eq!((empty.adr, empty.revpar, empty.occupancy), (0, 0, 0.0));
    }

    #[test]
    fn ranges_stay_within_the_retained_ninety_three_days() {
        assert_eq!(validate_range("2026-11-01", "2026-11-01"), Ok(0));
        assert_eq!(validate_range("2026-11-01", "2027-02-01").unwrap(), 92);
        assert_eq!(
            validate_range("2026-11-01", "2027-02-02").unwrap_err().code,
            "PMS_REPORT_RANGE"
        );
        assert_eq!(
            validate_range("2026-11-02", "2026-11-01").unwrap_err().code,
            "PMS_REPORT_RANGE"
        );
        assert_eq!(
            validate_range("2026-02-29", "2026-03-01").unwrap_err().code,
            "PMS_REPORT_DATE"
        );
        assert_eq!(validate_range("2028-02-29", "2028-02-29"), Ok(0));
    }

    #[test]
    fn day_rows_must_cover_the_range_in_order() {
        let rows = [day("2026-11-01", 1, 1, 1), day("2026-11-02", 1, 1, 1)];
        assert!(validate_days("2026-11-01", "2026-11-02", &rows).is_ok());
        assert_eq!(
            validate_days("2026-11-01", "2026-11-03", &rows)
                .unwrap_err()
                .code,
            "PMS_REPORT_READ_INVALID"
        );
        let reversed = [day("2026-11-02", 1, 1, 1), day("2026-11-01", 1, 1, 1)];
        assert_eq!(
            validate_days("2026-11-01", "2026-11-02", &reversed)
                .unwrap_err()
                .code,
            "PMS_REPORT_READ_INVALID"
        );
    }

    #[test]
    fn selections_reject_unbounded_inputs() {
        let entries = PmsReportEntries {
            property_id: "property".into(),
            date: "2026-11-01".into(),
            page: MAX_PAGE + 1,
        };
        assert_eq!(
            entries.validate().unwrap_err().code,
            "PMS_REPORT_REQUEST_INVALID"
        );
        let entries = PmsReportEntries {
            property_id: "property".into(),
            date: "2026-11-01".into(),
            page: 3,
        };
        assert_eq!(entries.validate(), Ok(()));
        assert_eq!(entries.offset(), 100);
        let range = PmsReportRange {
            property_id: String::new(),
            from: "2026-11-01".into(),
            to: "2026-11-01".into(),
            as_of: None,
            compare_as_of: None,
        };
        assert_eq!(
            range.validate().unwrap_err().code,
            "PMS_REPORT_REQUEST_INVALID"
        );
        let bad_observation = PmsReportRange {
            property_id: "property".into(),
            from: "2026-11-01".into(),
            to: "2026-11-01".into(),
            as_of: Some("not-a-date".into()),
            compare_as_of: None,
        };
        assert_eq!(
            bad_observation.validate().unwrap_err().code,
            "PMS_REPORT_REQUEST_INVALID"
        );
    }

    #[test]
    fn observations_are_past_ordered_and_default_to_a_seven_day_window() {
        let now = 1_700_000_000_000_i64;
        let range = PmsReportRange {
            property_id: "property".into(),
            from: "2026-11-01".into(),
            to: "2026-11-01".into(),
            as_of: None,
            compare_as_of: None,
        };
        let (as_of, compare_as_of) = range.observation(now).unwrap();
        assert_eq!(as_of, "2023-11-14T22:13:20Z");
        assert_eq!(compare_as_of, "2023-11-07T22:13:20Z");

        let future = PmsReportRange {
            as_of: Some("2099-01-01T00:00:00Z".into()),
            ..range.clone()
        };
        assert_eq!(
            future.observation(now).unwrap_err().code,
            "PMS_PACE_RANGE"
        );
        let misordered = PmsReportRange {
            as_of: Some("2023-11-14T00:00:00Z".into()),
            compare_as_of: Some("2023-11-14T00:00:00Z".into()),
            ..range.clone()
        };
        assert_eq!(
            misordered.observation(now).unwrap_err().code,
            "PMS_PACE_RANGE"
        );
        let requested = PmsReportRange {
            as_of: Some("2023-11-10T00:00:00+07:00".into()),
            compare_as_of: Some("2023-11-01T00:00:00+07:00".into()),
            ..range
        };
        let (as_of, compare_as_of) = requested.observation(now).unwrap();
        assert_eq!(as_of, "2023-11-10T00:00:00+07:00");
        assert_eq!(compare_as_of, "2023-11-01T00:00:00+07:00");
    }

    #[test]
    fn pace_and_finance_balances_follow_the_retained_reducers() {
        let point = |as_of: &str, covered: bool, reservations: i64| PmsPacePoint {
            as_of: as_of.into(),
            coverage_since: None,
            covered,
            reservations,
            room_nights: reservations * 2,
            quoted_total: reservations * 100,
        };
        let pace = assemble_pace(vec![
            point("now", true, 10),
            point("before", true, 7),
        ]);
        assert_eq!(pace.net_pickup.unwrap().reservations, 3);
        assert_eq!(assemble_pace(vec![point("now", false, 10)]).net_pickup, None);

        let config = serde_json::json!({
            "accounts": {"receivable": "rec", "advance": "adv", "deposit": "dep"},
            "onlineAccounting": {"refundPayable": "refund", "unallocated": "unalloc"}
        });
        let balances = finance_balances(
            &[
                PmsFinanceBalanceRow { account_id: "rec".into(), balance: 1_000 },
                PmsFinanceBalanceRow { account_id: "adv".into(), balance: 200 },
                PmsFinanceBalanceRow { account_id: "dep".into(), balance: 300 },
                PmsFinanceBalanceRow { account_id: "refund".into(), balance: 40 },
                PmsFinanceBalanceRow { account_id: "unalloc".into(), balance: 50 },
            ],
            &config,
        );
        assert_eq!(
            balances,
            PmsFinanceBalances {
                receivable: 1_000,
                advance: -200,
                deposit: -300,
                refund_payable: -40,
                unallocated: -50,
            }
        );
    }
}
