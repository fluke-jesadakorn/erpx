//! AI gateway policy, provider statement, verified cost and reconciliation
//! facts. Decisions are pure: providers select and write rows, the
//! composition owns the current Platform authority and transaction.
//!
//! The retained TypeScript contract is preserved byte-for-byte in behaviour:
//! Bangkok billing month bounds, positive integer limits, immutable statement
//! and cost evidence, exact conflict codes, and a reconciled statement only
//! when its evidence sums to the statement total in one currency.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

const HOUR_MS: i64 = 3_600_000;
const BANGKOK_OFFSET_MS: i64 = 7 * HOUR_MS;
const DAY_MS: i64 = 86_400_000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

pub fn policy_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AI_GATEWAY_POLICY_MISSING",
        "The AI gateway policy is unavailable.",
    )
}

pub fn reason_required() -> BusinessError {
    BusinessError::invalid(
        "AI_GATEWAY_REASON",
        "Record who changed the AI policy and why.",
    )
}

pub fn policy_invalid() -> BusinessError {
    BusinessError::invalid("AI_GATEWAY_POLICY", "Gateway limits must be positive.")
}

pub fn cost_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_COST_INVALID",
        "AI cost facts must be nonnegative integers.",
    )
}

pub fn statement_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_STATEMENT_INVALID",
        "Provide the provider statement, its checksum and the verifier.",
    )
}

pub fn statement_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "AI_STATEMENT_CONFLICT",
        "This provider statement ID already has different evidence.",
    )
}

pub fn cost_evidence_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_COST_EVIDENCE_INVALID",
        "Provide statement, currency, verifier and FX evidence for non-THB cost.",
    )
}

pub fn cost_evidence_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "AI_COST_EVIDENCE_CONFLICT",
        "Cost evidence is missing, not billable, or already verified with different values.",
    )
}

pub fn statement_unbalanced() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "AI_STATEMENT_UNBALANCED",
        "Provider request costs do not match the statement total.",
    )
}

pub fn policy_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "AI_GATEWAY_POLICY_CHANGED",
        "The AI gateway policy changed. Refresh before saving.",
    )
}

pub fn policy_unchanged() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "AI_GATEWAY_POLICY_UNCHANGED",
        "The AI gateway is already in that state.",
    )
}

pub fn month_invalid() -> BusinessError {
    BusinessError::invalid("AI_GATEWAY_MONTH_INVALID", "Use a YYYY-MM billing month.")
}

/// The retained `positiveInteger` guard: a negative or unsafe value is a cost
/// input error. Callers that require a strictly positive limit add the
/// `AI_GATEWAY_POLICY` rule afterwards.
pub fn positive_integer(value: i64) -> Result<(), BusinessError> {
    if !(0..=MAX_SAFE_INTEGER).contains(&value) {
        return Err(cost_invalid());
    }
    Ok(())
}

fn digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = (month + 9) % 12;
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (if month <= 2 { year + 1 } else { year }, month)
}

/// `billingMonth(now)`: the Bangkok wall-clock month in UTC+7.
pub fn billing_month(now: i64) -> String {
    let (year, month) = civil_from_days((now + BANGKOK_OFFSET_MS).div_euclid(DAY_MS));
    format!("{year:04}-{month:02}")
}

/// `monthBounds(month)`: the exact Bangkok month interval in UTC millis.
pub fn month_bounds(month: &str) -> Result<(i64, i64), BusinessError> {
    let bytes = month.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' || !digits(&month[..4]) || !digits(&month[5..]) {
        return Err(month_invalid());
    }
    let year: i64 = month[..4].parse().map_err(|_| month_invalid())?;
    let month_number: i64 = month[5..].parse().map_err(|_| month_invalid())?;
    if !(1..=12).contains(&month_number) {
        return Err(month_invalid());
    }
    let next_year = if month_number == 12 { year + 1 } else { year };
    let next_month = if month_number == 12 {
        1
    } else {
        month_number + 1
    };
    let start = days_from_civil(year, month_number, 1) * DAY_MS - BANGKOK_OFFSET_MS;
    let end = days_from_civil(next_year, next_month, 1) * DAY_MS - BANGKOK_OFFSET_MS;
    Ok((start, end))
}

pub fn currency_valid(currency: &str) -> bool {
    let bytes = currency.as_bytes();
    bytes.len() == 3 && bytes.iter().all(u8::is_ascii_uppercase)
}

pub fn digest_valid(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiPolicyFacts {
    pub platform_enabled: bool,
    pub monthly_request_limit: i64,
    pub monthly_spend_cap_satang: i64,
    pub reservation_satang: i64,
    pub updated_by: String,
    pub updated_at: i64,
    pub monthly_requests: i64,
    pub monthly_reserved_or_verified_satang: i64,
    pub pending_platform_cost_requests: i64,
}

/// Retained wire projection of the policy read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiGatewayPolicyView {
    pub platform_enabled: bool,
    pub monthly_request_limit: i64,
    pub monthly_spend_cap_satang: i64,
    pub reservation_satang: i64,
    pub monthly_requests: i64,
    pub monthly_reserved_or_verified_satang: i64,
    pub pending_platform_cost_requests: i64,
    pub updated_by: String,
    pub updated_at: i64,
}

impl From<AiPolicyFacts> for AiGatewayPolicyView {
    fn from(facts: AiPolicyFacts) -> Self {
        Self {
            platform_enabled: facts.platform_enabled,
            monthly_request_limit: facts.monthly_request_limit,
            monthly_spend_cap_satang: facts.monthly_spend_cap_satang,
            reservation_satang: facts.reservation_satang,
            monthly_requests: facts.monthly_requests,
            monthly_reserved_or_verified_satang: facts.monthly_reserved_or_verified_satang,
            pending_platform_cost_requests: facts.pending_platform_cost_requests,
            updated_by: facts.updated_by,
            updated_at: facts.updated_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSetEnabledInput {
    pub enabled: bool,
    pub expected_updated_at: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiPolicyChange {
    pub enabled: bool,
    pub monthly_request_limit: i64,
    pub monthly_spend_cap_satang: i64,
    pub reservation_satang: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiProviderStatement {
    pub id: String,
    pub holding_id: String,
    pub provider: String,
    pub month: String,
    pub currency: String,
    pub amount_minor: i64,
    pub statement_digest: String,
    pub verified_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiStatementEvidence {
    pub holding_id: String,
    pub provider: String,
    pub month: String,
    pub currency: String,
    pub amount_minor: i64,
    pub statement_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiVerifiedCost {
    pub request_id: String,
    pub provider_statement_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub thb_satang: i64,
    pub fx_evidence_ref: Option<String>,
    pub verified_by: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiExistingCost {
    pub provider_statement_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub thb_satang: i64,
    pub fx_evidence_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiReconciliationFact {
    pub reconciled_at: Option<i64>,
}

/// Safe gateway attribution row; prompts, completions and credentials are
/// never part of the ledger or this projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiGatewayRow {
    pub id: String,
    pub usage_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub connection_id: Option<String>,
    pub funding_source: String,
    pub provider: String,
    pub model: String,
    pub feature: String,
    pub status: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub units_json: String,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub provider_statement_id: Option<String>,
    pub currency: Option<String>,
    pub amount_minor: Option<i64>,
    pub thb_satang: Option<i64>,
    pub verified_at: Option<i64>,
    pub reconciled_at: Option<i64>,
}

/// The retained reservation eligibility predicate. The retained TypeScript
/// reservation still consumes this decision until `ai-transport` moves.
pub fn reservation_allowed(
    policy: Option<&AiPolicyFacts>,
    funding: &str,
    scope_valid: bool,
    already_reserved: bool,
) -> bool {
    let Some(policy) = policy else {
        return false;
    };
    scope_valid
        && !already_reserved
        && (policy.platform_enabled || funding != "erpx")
        && policy.monthly_requests < policy.monthly_request_limit
        && (funding != "erpx"
            || policy.monthly_reserved_or_verified_satang + policy.reservation_satang
                <= policy.monthly_spend_cap_satang)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bangkok_bounds_match_the_retained_billing_helpers() {
        assert_eq!(billing_month(0), "1970-01");
        assert_eq!(billing_month(1_784_000_000_000), "2026-07");
        // 2026-11-01T00:00+07:00 is 2026-10-31T17:00Z.
        assert_eq!(
            month_bounds("2026-11").unwrap(),
            (1_793_466_000_000, 1_796_058_000_000)
        );
        assert_eq!(month_bounds("2026-12").unwrap().0, 1_796_058_000_000);
        for invalid in ["", "2026-1", "2026-13", "2026-00", "abcd-01", "2026-01x"] {
            assert_eq!(
                month_bounds(invalid).unwrap_err().code,
                "AI_GATEWAY_MONTH_INVALID"
            );
        }
    }

    #[test]
    fn reservation_predicate_keeps_the_kill_switch_and_caps() {
        let policy = AiPolicyFacts {
            platform_enabled: false,
            monthly_request_limit: 10,
            monthly_spend_cap_satang: 100,
            reservation_satang: 10,
            updated_by: "owner".into(),
            updated_at: 42,
            monthly_requests: 1,
            monthly_reserved_or_verified_satang: 0,
            pending_platform_cost_requests: 0,
        };
        assert!(!reservation_allowed(Some(&policy), "erpx", true, false));
        assert!(reservation_allowed(Some(&policy), "byok", true, false));
        assert!(!reservation_allowed(Some(&policy), "byok", false, false));
        assert!(!reservation_allowed(Some(&policy), "byok", true, true));
        assert!(!reservation_allowed(
            Some(&AiPolicyFacts {
                platform_enabled: true,
                monthly_requests: 10,
                ..policy.clone()
            }),
            "erpx",
            true,
            false
        ));
        assert!(!reservation_allowed(
            Some(&AiPolicyFacts {
                platform_enabled: true,
                monthly_reserved_or_verified_satang: 91,
                ..policy
            }),
            "erpx",
            true,
            false
        ));
    }
}
