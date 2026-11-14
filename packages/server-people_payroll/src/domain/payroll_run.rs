//! Native payroll run command policy.
//!
//! Create, calculate, cancel, post and detail retain the current People-owned
//! run facts, optimistic versions, readiness prerequisites, deterministic
//! payroll calculation (including the reviewed Thai statutory kernel),
//! independent approval, independent draft cancellation and accrual lines.
//!
//! This module never reads another owner's tables. The PostgreSQL adapter
//! supplies the current owner rows, Finance supplies the journal types and the
//! composition-owned Finance journal port persists the accounting entry in the
//! same serializable unit.
use erpx_server_benefits::public::payroll_source::BenefitPayrollInputRow;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CREATE_OPERATION: &str = "people.payroll.create";
pub const CALCULATE_OPERATION: &str = "people.payroll.calculate";
pub const CANCEL_OPERATION: &str = "people.payroll.cancel";
pub const POST_OPERATION: &str = "people.payroll.post";

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const MONTH_MICROS: i64 = 1_000_000;
const HOUR_SCALE: i64 = 1_000_000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn forbidden(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PAYROLL_RUN_NOT_FOUND",
        "Payroll run not found in this Company.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "IDEMPOTENCY_CONFLICT",
        "This Idempotency-Key belongs to a different Payroll request.",
    )
}

pub fn command_incomplete() -> BusinessError {
    conflict(
        "COMMAND_INCOMPLETE",
        "The previous Payroll command is still being finalized. Retry the same command.",
    )
}

pub fn readback_invalid() -> BusinessError {
    unavailable(
        "PAYROLL_READBACK_INVALID",
        "The Payroll command did not persist its expected owner row.",
    )
}

pub fn journal_port_unavailable() -> BusinessError {
    unavailable(
        "PAYROLL_FINANCE_JOURNAL_PORT_UNAVAILABLE",
        "Native payroll posting requires the composed Finance journal port.",
    )
}

pub fn statutory_evidence_required() -> BusinessError {
    BusinessError::invalid(
        "PAYROLL_STATUTORY_EVIDENCE_INCOMPLETE",
        "Verify annual deductions, eligibility, payment count and opening YTD before statutory calculation.",
    )
}

/// Fresh owner command authority supplied by composition. It is not a request
/// deserializer and grants no module or permission by itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollCommandActor {
    pub user_id: String,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub work_context: String,
    pub expires_at: i64,
}

impl PayrollCommandActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn require_prepare(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("payroll.prepare") {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant payroll preparation access.",
            ));
        }
        self.require_company_scope()?;
        self.require_current(now)
    }

    pub fn require_post(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.has("finance.approve") {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant Finance posting access.",
            ));
        }
        self.require_company_scope()?;
        self.require_current(now)
    }

    pub fn can_read_payroll(&self) -> bool {
        self.has("payroll.read")
    }

    pub fn require_read(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_read_payroll() && !self.has("payroll.payment.read") {
            return Err(forbidden(
                "PERMISSION_REQUIRED",
                "Your selected role does not grant payroll read access.",
            ));
        }
        self.require_company_scope()?;
        self.require_current(now)
    }

    pub fn require_company_scope(&self) -> Result<(), BusinessError> {
        if self.branch_id.is_some() || self.department_id.is_some() {
            return Err(forbidden(
                "COMPANY_PAYROLL_SCOPE_REQUIRED",
                "Company-wide payroll requires a holding-level assignment.",
            ));
        }
        Ok(())
    }

    fn require_current(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if self.user_id.is_empty()
            || self.assignment_id.is_empty()
            || self.role.is_empty()
            || self.work_context.len() != 64
            || !self
                .work_context
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(unavailable(
                "PAYROLL_AUTHORITY_INVALID",
                "The current payroll authority is unavailable.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn finance_actor(&self) -> Result<PayrollFinanceActor, BusinessError> {
        Ok(PayrollFinanceActor {
            user_id: UserId::new(self.user_id.clone())?,
            holding_id: self.holding_id.clone(),
            company_id: self.company_id.clone(),
            assignment_id: AssignmentId::new(self.assignment_id.clone())?,
        })
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "role": self.role,
            "assignmentId": self.assignment_id,
            "companyId": self.company_id.as_str(),
            "branchId": self.branch_id.as_ref().map(RecordId::as_str),
            "departmentId": self.department_id.as_ref().map(RecordId::as_str),
            "workContext": self.work_context,
            "userId": self.user_id,
        })
        .to_string()
    }
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_identifier(value: &str) -> bool {
    RecordId::new(value).is_ok()
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let year = value[..4].parse::<i32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    month >= 1 && month <= 12 && day >= 1 && day <= days_in_month(year, month)
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Howard Hinnant's civil-from-days algorithm. Dates are exact UTC calendar
/// dates; payroll never uses a floating point timestamp for a work day.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::from(era) * 146_097 + i64::from(doe) - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (y as i32 + i32::from(month <= 2), month as u32, day as u32)
}

pub fn date_days(value: &str) -> Result<i64, BusinessError> {
    if !valid_date(value) {
        return Err(invalid(
            "INVALID_PAYROLL_PERIOD",
            "Review the payroll period and pay date.",
        ));
    }
    Ok(days_from_civil(
        value[..4].parse().unwrap_or(0),
        value[5..7].parse().unwrap_or(0),
        value[8..].parse().unwrap_or(0),
    ))
}

pub fn date_add(value: &str, days: i64) -> String {
    let (year, month, day) = civil_from_days(date_days(value).unwrap_or(0) + days);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn current_date(now: TimestampMillis) -> String {
    let (year, month, day) = civil_from_days(now.get().div_euclid(86_400_000));
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn calculated_item_id(key: &str, index: usize) -> String {
    let suffix = format!("-{index}");
    let limit = 180_usize.saturating_sub("payitem-".len() + suffix.len());
    let key = if key.len() > limit {
        &key[..limit]
    } else {
        key
    };
    format!("payitem-{key}{suffix}")
}

pub fn stored_receipt(request_hash: &str) -> Result<(), BusinessError> {
    if request_hash.len() != 64 || !request_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unavailable(
            "PAYROLL_SOURCE_INVALID",
            "The current payroll command source is invalid.",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayrollRunMode {
    Preview,
    FinalPay,
    Production,
}

impl PayrollRunMode {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "preview" => Ok(Self::Preview),
            "final_pay" => Ok(Self::FinalPay),
            "production" => Ok(Self::Production),
            _ => Err(invalid(
                "INVALID_PAYROLL_PERIOD",
                "Review the payroll mode and pay date.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::FinalPay => "final_pay",
            Self::Production => "production",
        }
    }

    pub fn requires_statutory(self, country_pack_id: Option<&str>) -> bool {
        matches!(self, Self::Production) || (self == Self::FinalPay && country_pack_id.is_some())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePayrollRunInput {
    pub pay_group_id: String,
    #[serde(default)]
    pub country_pack_id: Option<String>,
    pub period_start: String,
    pub period_end: String,
    pub pay_date: String,
    pub currency: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "preview".to_owned()
}

impl CreatePayrollRunInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let input: Self = serde_json::from_value(raw.clone()).map_err(|_| {
            invalid(
                "PAYROLL_REQUEST_INVALID",
                "Review the payroll period and details.",
            )
        })?;
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.pay_group_id)
            || !valid_currency(&self.currency)
            || self
                .country_pack_id
                .as_deref()
                .is_some_and(|value| !valid_identifier(value))
            || !valid_date(&self.period_start)
            || !valid_date(&self.period_end)
            || !valid_date(&self.pay_date)
            || self.period_end < self.period_start
            || self.pay_date < self.period_end
        {
            return Err(invalid(
                "INVALID_PAYROLL_PERIOD",
                "Review the period and pay date.",
            ));
        }
        Ok(())
    }

    pub fn mode(&self) -> Result<PayrollRunMode, BusinessError> {
        PayrollRunMode::parse(&self.mode)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CalculatePayrollRunInput {}

impl CalculatePayrollRunInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        serde_json::from_value(raw.clone()).map_err(|_| {
            invalid(
                "PAYROLL_REQUEST_INVALID",
                "Recalculate with an empty payload.",
            )
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelPayrollRunInput {
    pub expected_version: i64,
    pub reason: String,
}

impl CancelPayrollRunInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let input: Self = serde_json::from_value(raw.clone()).map_err(|_| {
            invalid(
                "PAYROLL_REQUEST_INVALID",
                "Review the cancellation reason before continuing.",
            )
        })?;
        let reason = input.reason.trim();
        if input.expected_version < 1
            || reason.len() < 8
            || reason.len() > 1_000
            || reason != input.reason
        {
            return Err(invalid(
                "PAYROLL_REQUEST_INVALID",
                "Review the cancellation reason before continuing.",
            ));
        }
        Ok(input)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostPayrollInput {
    #[serde(default)]
    pub chart_id: Option<String>,
    #[serde(default)]
    pub period_id: Option<String>,
    #[serde(default)]
    pub entry_date: Option<String>,
    #[serde(default)]
    pub account_mapping_id: Option<String>,
    #[serde(default)]
    pub expense_account_id: Option<String>,
    #[serde(default)]
    pub net_payable_account_id: Option<String>,
    #[serde(default)]
    pub tax_payable_account_id: Option<String>,
    #[serde(default)]
    pub employer_contribution_account_id: Option<String>,
    #[serde(default)]
    pub employer_expense_account_id: Option<String>,
    #[serde(default)]
    pub social_security_payable_account_id: Option<String>,
    #[serde(default)]
    pub advance_receivable_account_id: Option<String>,
    #[serde(default)]
    pub benefit_payable_account_id: Option<String>,
    #[serde(default)]
    pub other_deduction_payable_account_id: Option<String>,
    #[serde(default)]
    pub service_charge_expense_account_id: Option<String>,
    #[serde(default)]
    pub overtime_expense_account_id: Option<String>,
    #[serde(default)]
    pub bonus_expense_account_id: Option<String>,
    pub expected_version: i64,
}

impl PostPayrollInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let input: Self = serde_json::from_value(raw.clone()).map_err(|_| {
            invalid(
                "PAYROLL_REQUEST_INVALID",
                "Review the payroll posting accounts.",
            )
        })?;
        if input.expected_version < 1
            || input
                .entry_date
                .as_deref()
                .is_some_and(|value| !valid_date(value))
        {
            return Err(invalid(
                "PAYROLL_REQUEST_INVALID",
                "Review the payroll posting accounts.",
            ));
        }
        Ok(input)
    }
}

/// Current owner run row, including the pay-group policy used by calculation.
#[derive(Clone, Debug)]
pub struct PayrollRun {
    pub id: String,
    pub company_id: String,
    pub pay_group_id: String,
    pub pay_group_currency: String,
    pub pay_group_status: String,
    pub overtime_multiplier_bps: i64,
    pub proration_method: String,
    pub country_pack_id: Option<String>,
    pub period_start: String,
    pub period_end: String,
    pub pay_date: String,
    pub currency: String,
    pub mode: String,
    pub status: String,
    pub revision: i64,
    pub totals_json: String,
    pub input_digest: Option<String>,
    pub version: i64,
    pub created_by: String,
    pub submitted_by: Option<String>,
    pub approved_by: Option<String>,
    pub posted_by: Option<String>,
    pub adjustment_of_run_id: Option<String>,
    pub scope_engagement_id: Option<String>,
}

impl PayrollRun {
    pub fn validate(&self, company: &CompanyId) -> Result<(), BusinessError> {
        if self.company_id != company.as_str()
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.pay_group_id)
            || !valid_currency(&self.currency)
            || !valid_currency(&self.pay_group_currency)
            || !valid_date(&self.period_start)
            || !valid_date(&self.period_end)
            || !valid_date(&self.pay_date)
            || self.period_end < self.period_start
            || self.revision < 1
            || self.version < 1
            || self.overtime_multiplier_bps < 0
        {
            return Err(unavailable(
                "PAYROLL_SOURCE_INVALID",
                "The current payroll run is invalid.",
            ));
        }
        PayrollRunMode::parse(&self.mode)?;
        Ok(())
    }

    pub fn mode(&self) -> Result<PayrollRunMode, BusinessError> {
        PayrollRunMode::parse(&self.mode)
    }

    pub fn requires_statutory(&self) -> bool {
        self.mode()
            .map(|mode| mode.requires_statutory(self.country_pack_id.as_deref()))
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug)]
pub struct PayGroupFact {
    pub id: String,
    pub currency: String,
    pub country_code: String,
    pub frequency: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct CountryPackFact {
    pub id: String,
    pub country_code: String,
    pub version: String,
    pub status: String,
    pub rules_json: String,
    pub adapter_id: Option<String>,
    pub source_digest: Option<String>,
    pub record_version: i64,
    pub effective_from: Option<String>,
    pub effective_to: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollEngagement {
    pub id: String,
    pub version: i64,
    pub person_id: String,
    pub worker_type: String,
    pub employment_type: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub job_title: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollPayee {
    pub id: String,
    pub version: i64,
    pub legal_name: String,
    pub preferred_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollCompensation {
    pub id: String,
    pub version: i64,
    pub base_amount_minor: i64,
    pub hourly_rate_minor: i64,
    pub daily_rate_minor: i64,
    pub standard_daily_hours: f64,
    pub currency: String,
    pub pay_frequency: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub status: String,
    pub components_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollTimeEntry {
    pub id: String,
    pub version: i64,
    pub work_date: String,
    pub regular_hours: f64,
    pub overtime_hours: f64,
    pub cost_assignment_id: Option<String>,
    pub cost_branch_id: Option<String>,
    pub cost_department_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollHandoff {
    pub id: String,
    pub request_id: String,
    pub request_version: i64,
    pub minutes: f64,
    pub work_date: String,
}

#[derive(Clone, Debug)]
pub struct PayrollAdvanceRepayment {
    pub id: String,
    pub version: i64,
    pub amount_minor: i64,
    pub due_period_start: String,
    pub due_period_end: String,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct PayrollCostAssignment {
    pub id: String,
    pub version: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub evidence_status: String,
}

#[derive(Clone, Debug)]
pub struct ThaiPayrollProfile {
    pub id: String,
    pub version: i64,
    pub tax_year: i64,
    pub effective_from: String,
    pub effective_to: String,
    pub annual_deductions_minor: i64,
    pub scheduled_pays: i64,
    pub social_security_eligible: bool,
    pub social_security_exemption_evidence: Option<String>,
    pub opening_through_date: String,
    pub opening_taxable_minor: i64,
    pub opening_irregular_minor: i64,
    pub opening_withheld_minor: i64,
    pub prior_income_complete: bool,
    pub evidence_ref: String,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct ThaiPriorItem {
    pub id: String,
    pub run_id: String,
    pub run_version: i64,
    pub input_digest: String,
    pub result_json: String,
    pub pay_date: String,
}

#[derive(Clone, Debug)]
pub struct ThaiStatutoryEvidence {
    pub pack_id: String,
    pub record_version: i64,
    pub source_digest: String,
    pub rules: ThaiPayrollRules,
    pub profile: ThaiPayrollProfile,
    pub prior_items: Vec<ThaiPriorItem>,
}

#[derive(Clone, Debug)]
pub struct PayrollCalculationInput {
    pub engagement: PayrollEngagement,
    pub payee: PayrollPayee,
    pub compensation: Option<PayrollCompensation>,
    pub time_entries: Vec<PayrollTimeEntry>,
    pub handoffs: Vec<PayrollHandoff>,
    pub advances: Vec<PayrollAdvanceRepayment>,
    pub cost_assignments: Vec<PayrollCostAssignment>,
    pub benefits: Vec<BenefitPayrollInputRow>,
    pub thai_statutory: Option<ThaiStatutoryEvidence>,
}

#[derive(Clone, Debug)]
pub struct PayrollCalculationContext {
    pub inputs: Vec<PayrollCalculationInput>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayrollComponentInput {
    pub code: String,
    pub label: String,
    pub kind: String,
    pub amount: String,
    #[serde(default)]
    pub taxable: Option<bool>,
    #[serde(default)]
    pub income_timing: Option<String>,
    #[serde(default)]
    pub social_security_wage: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredComponent {
    code: String,
    label: String,
    kind: String,
    amount: String,
    #[serde(default)]
    taxable: Option<bool>,
    #[serde(default)]
    income_timing: Option<String>,
    #[serde(default)]
    social_security_wage: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct CalculatedPayrollItem {
    pub id: String,
    pub engagement_id: String,
    pub person_id: String,
    pub worker_type: String,
    pub input_snapshot_json: String,
    pub result_json: String,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub employer_cost_minor: i64,
    pub net_minor: i64,
    pub currency: String,
}

#[derive(Clone, Debug)]
pub struct PayrollTotals {
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub employer_cost_minor: i64,
    pub net_minor: i64,
    pub headcount: i64,
}

impl PayrollTotals {
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "gross": self.gross_minor,
            "deductions": self.deductions_minor,
            "employer": self.employer_cost_minor,
            "net": self.net_minor,
            "headcount": self.headcount,
        })
        .to_string()
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.headcount < 1
            || [
                self.gross_minor,
                self.deductions_minor,
                self.employer_cost_minor,
                self.net_minor,
            ]
            .iter()
            .any(|value| *value < 0 || *value > MAX_SAFE_INTEGER)
        {
            return Err(invalid(
                "PAYROLL_TOTALS_INVALID",
                "Payroll totals exceed supported precision.",
            ));
        }
        Ok(())
    }
}

fn ratio(amount: i64, numerator: i64, denominator: i64) -> Result<i64, BusinessError> {
    if amount < 0 || numerator < 0 || denominator <= 0 {
        return Err(invalid(
            "PAYROLL_QUANTITY_INVALID",
            "Payroll quantities must use nonnegative reviewed precision.",
        ));
    }
    let product = i128::from(amount) * i128::from(numerator);
    let result = (product + i128::from(denominator) / 2) / i128::from(denominator);
    if result > i128::from(MAX_SAFE_INTEGER) {
        return Err(invalid(
            "MONEY_OVERFLOW",
            "Payroll exceeds supported precision.",
        ));
    }
    Ok(result as i64)
}

pub fn decimal_to_minor(value: &str) -> Result<i64, BusinessError> {
    let value = value.trim();
    if value.is_empty() || value != value.trim() {
        return Err(invalid("INVALID_MONEY", "Enter a valid money amount."));
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|value| {
            value.is_empty() || value.len() > 2 || !value.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(invalid(
            "INVALID_MONEY",
            "Enter a money amount with at most two decimals.",
        ));
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| invalid("MONEY_OVERFLOW", "Money amount is too large."))?;
    let fraction = fraction.unwrap_or("0");
    let fraction = if fraction.len() == 1 {
        format!("{fraction}0")
    } else {
        fraction.to_owned()
    };
    let fraction = fraction
        .parse::<i64>()
        .map_err(|_| invalid("INVALID_MONEY", "Enter a valid money amount."))?;
    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or_else(|| invalid("MONEY_OVERFLOW", "Money amount is too large."))
}

fn minor_to_decimal(value: i64) -> String {
    let sign = if value < 0 { "-" } else { "" };
    let value = value.unsigned_abs();
    format!("{sign}{}.{:02}", value / 100, value % 100)
}

fn hours_micros(value: f64) -> Result<i64, BusinessError> {
    if !value.is_finite() || value < 0.0 {
        return Err(invalid(
            "PAYROLL_QUANTITY_INVALID",
            "Payroll hours must be nonnegative and finite.",
        ));
    }
    let micros = (value * HOUR_SCALE as f64).round();
    if micros > MAX_SAFE_INTEGER as f64 {
        return Err(invalid(
            "MONEY_OVERFLOW",
            "Payroll hours exceed supported precision.",
        ));
    }
    Ok(micros as i64)
}

fn safe_add(items: impl IntoIterator<Item = i64>) -> Result<i64, BusinessError> {
    let mut total = 0_i64;
    for item in items {
        total = total
            .checked_add(item)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
    }
    Ok(total)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppliedComponent {
    code: String,
    label: String,
    kind: String,
    amount: String,
    amount_minor: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    taxable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    income_timing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    social_security_wage: Option<bool>,
}

fn component_amounts(
    components: &[PayrollComponentInput],
) -> Result<Vec<AppliedComponent>, BusinessError> {
    components
        .iter()
        .map(|component| {
            if !matches!(
                component.kind.as_str(),
                "earning" | "deduction" | "employer"
            ) {
                return Err(invalid(
                    "PAYROLL_COMPONENT_INVALID",
                    "Review the payroll component classification.",
                ));
            }
            let amount_minor = decimal_to_minor(&component.amount)?;
            if amount_minor < 0 {
                return Err(invalid(
                    "PAYROLL_COMPONENT_INVALID",
                    "Payroll component amounts must be nonnegative.",
                ));
            }
            Ok(AppliedComponent {
                code: component.code.clone(),
                label: component.label.clone(),
                kind: component.kind.clone(),
                amount: component.amount.clone(),
                amount_minor,
                taxable: component.taxable,
                income_timing: component.income_timing.clone(),
                social_security_wage: component.social_security_wage,
            })
        })
        .collect()
}

fn stored_components(
    compensation: &PayrollCompensation,
) -> Result<Vec<PayrollComponentInput>, BusinessError> {
    let Some(raw) = compensation.components_json.as_deref() else {
        return Ok(Vec::new());
    };
    let stored: Vec<StoredComponent> = serde_json::from_str(raw).map_err(|_| {
        invalid(
            "PAYROLL_COMPONENT_INVALID",
            "Review the retained payroll components.",
        )
    })?;
    Ok(stored
        .into_iter()
        .map(|component| PayrollComponentInput {
            code: component.code,
            label: component.label,
            kind: component.kind,
            amount: component.amount,
            taxable: component.taxable,
            income_timing: component.income_timing,
            social_security_wage: component.social_security_wage,
        })
        .collect())
}

#[derive(Clone, Debug)]
struct GrossCalculation {
    gross: i64,
    deductions: i64,
    employer: i64,
    net: i64,
    overtime_minor: i64,
    applied: Vec<AppliedComponent>,
}

fn calculate_gross(
    input: &PayrollCalculationInput,
    run: &PayrollRun,
) -> Result<GrossCalculation, BusinessError> {
    let compensation = input.compensation.as_ref().ok_or_else(|| {
        invalid(
            "PAYROLL_INPUT_INCOMPLETE",
            "Every employee needs approved compensation in the pay-group currency covering the full period.",
        )
    })?;
    if compensation.currency != run.currency
        || !matches!(compensation.status.as_str(), "active" | "approved")
    {
        return Err(invalid(
            "PAYROLL_INPUT_INCOMPLETE",
            "Every employee needs approved compensation in the pay-group currency covering the full period.",
        ));
    }
    if compensation.effective_from > run.period_end
        || compensation
            .effective_to
            .as_deref()
            .is_some_and(|value| value < run.period_start.as_str())
    {
        return Err(invalid(
            "PAYROLL_INPUT_INCOMPLETE",
            "Every employee needs approved compensation covering the full period.",
        ));
    }
    if !valid_date(&input.engagement.start_date)
        || !valid_date(&run.period_start)
        || input.payee.id.is_empty()
    {
        return Err(unavailable(
            "PAYROLL_SOURCE_INVALID",
            "The current payroll source is invalid.",
        ));
    }

    let regular_micros = safe_add(
        input
            .time_entries
            .iter()
            .map(|entry| hours_micros(entry.regular_hours))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let overtime_micros = safe_add(
        input
            .time_entries
            .iter()
            .map(|entry| hours_micros(entry.overtime_hours))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let handoff_micros = safe_add(
        input
            .handoffs
            .iter()
            .map(|handoff| {
                let minutes = hours_micros(handoff.minutes)?;
                Ok(minutes / 60)
            })
            .collect::<Result<Vec<_>, BusinessError>>()?,
    )?;
    let overtime_micros = overtime_micros.checked_add(handoff_micros).ok_or_else(|| {
        invalid(
            "MONEY_OVERFLOW",
            "Payroll hours exceed supported precision.",
        )
    })?;

    let daily_rate = compensation.daily_rate_minor;
    let daily_hours = if compensation.standard_daily_hours > 0.0 {
        hours_micros(compensation.standard_daily_hours)?
    } else {
        hours_micros(8.0)?
    };
    let mut regular_by_day: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    for entry in &input.time_entries {
        *regular_by_day.entry(entry.work_date.clone()).or_insert(0) +=
            hours_micros(entry.regular_hours)?;
    }
    if input.engagement.employment_type == "daily"
        && (daily_rate <= 0 || regular_by_day.values().any(|value| *value > daily_hours))
    {
        return Err(invalid(
            "DAILY_PAYROLL_TIME_INVALID",
            "Daily pay requires a positive approved rate and regular hours within the reviewed daily schedule; classify additional hours as overtime.",
        ));
    }
    let mut regular_days_micros = 0_i64;
    for value in regular_by_day.values() {
        let days = (i128::from(*value) * i128::from(MONTH_MICROS) + i128::from(daily_hours) / 2)
            / i128::from(daily_hours);
        regular_days_micros = regular_days_micros
            .checked_add(i64::try_from(days).map_err(|_| {
                invalid(
                    "MONEY_OVERFLOW",
                    "Payroll quantities exceed supported precision.",
                )
            })?)
            .ok_or_else(|| {
                invalid(
                    "MONEY_OVERFLOW",
                    "Payroll quantities exceed supported precision.",
                )
            })?;
    }

    let base_days = date_days(&run.period_start)?;
    let period_days = date_days(&run.period_end)? - base_days + 1;
    let active_start = if input.engagement.start_date > run.period_start {
        input.engagement.start_date.clone()
    } else {
        run.period_start.clone()
    };
    let active_end = match input.engagement.end_date.as_deref() {
        Some(value) if value < run.period_end.as_str() => value.to_owned(),
        _ => run.period_end.clone(),
    };
    let active_days = date_days(&active_end)? - date_days(&active_start)? + 1;
    let partial = active_days != period_days;
    if input.engagement.employment_type == "daily" && daily_rate <= 0 {
        return Err(invalid(
            "PAYROLL_INPUT_INCOMPLETE",
            "Daily pay requires an approved daily rate.",
        ));
    }

    let stored = stored_components(compensation)?;
    let normalized: Vec<PayrollComponentInput> = stored
        .into_iter()
        .filter(|component| {
            matches!(
                component.kind.as_str(),
                "earning" | "deduction" | "employer"
            )
        })
        .collect();
    let mut applied = component_amounts(&normalized)?;
    for advance in &input.advances {
        if advance.amount_minor < 0 {
            return Err(invalid(
                "PAYROLL_INPUT_INCOMPLETE",
                "Payroll advance repayment is invalid.",
            ));
        }
        applied.push(AppliedComponent {
            code: format!("payroll-advance:{}", advance.id),
            label: format!("Payroll advance repayment · {}", advance.due_period_start),
            kind: "deduction".to_owned(),
            amount: minor_to_decimal(advance.amount_minor),
            amount_minor: advance.amount_minor,
            taxable: None,
            income_timing: None,
            social_security_wage: None,
        });
    }

    let base = if partial && run.proration_method == "calendar_days" {
        ratio(compensation.base_amount_minor, active_days, period_days)?
    } else {
        compensation.base_amount_minor
    };
    if matches!(
        input.engagement.employment_type.as_str(),
        "salaried" | "daily"
    ) && overtime_micros > 0
        && compensation.hourly_rate_minor <= 0
    {
        return Err(invalid(
            "PAYROLL_INPUT_INCOMPLETE",
            "Approved hourly rate is required to pay salaried overtime.",
        ));
    }
    let overtime_weight = i128::from(overtime_micros) * i128::from(run.overtime_multiplier_bps)
        / i128::from(MONTH_MICROS);
    let overtime_minor = ratio(
        compensation.hourly_rate_minor,
        i64::try_from(overtime_weight).map_err(|_| {
            invalid(
                "MONEY_OVERFLOW",
                "Payroll overtime exceeds supported precision.",
            )
        })?,
        10_000,
    )?;
    let mut gross = match input.engagement.employment_type.as_str() {
        "salaried" => base.checked_add(overtime_minor),
        "daily" => {
            ratio(daily_rate, regular_days_micros, MONTH_MICROS)?.checked_add(overtime_minor)
        }
        "contractor" => Some(compensation.base_amount_minor),
        _ => {
            let weighted = i128::from(regular_micros) * 10_000
                + i128::from(overtime_micros) * i128::from(run.overtime_multiplier_bps);
            ratio(
                compensation.hourly_rate_minor,
                i64::try_from(weighted).map_err(|_| {
                    invalid(
                        "MONEY_OVERFLOW",
                        "Payroll overtime exceeds supported precision.",
                    )
                })?,
                10_000_000_000,
            )?
            .checked_add(0)
        }
    }
    .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;

    let mut deductions = 0_i64;
    let mut employer = 0_i64;
    for component in &applied {
        match component.kind.as_str() {
            "earning" => {
                gross = gross.checked_add(component.amount_minor).ok_or_else(|| {
                    invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
                })?
            }
            "deduction" => {
                deductions = deductions
                    .checked_add(component.amount_minor)
                    .ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
                    })?
            }
            "employer" => {
                employer = employer
                    .checked_add(component.amount_minor)
                    .ok_or_else(|| {
                        invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
                    })?
            }
            _ => {}
        }
    }

    Ok(GrossCalculation {
        gross,
        deductions,
        employer,
        net: gross - deductions,
        overtime_minor,
        applied,
    })
}

// ---------------------------------------------------------------------------
// Reviewed Thai statutory kernel. This is a deterministic calculation kernel,
// not an activated country pack; the pack row supplies eligibility, allowances
// and the retained rule revision.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiPayrollRules {
    pub country_code: String,
    pub currency: String,
    pub version: String,
    pub effective_from: String,
    pub effective_to: String,
    pub standard_expense_rate_bps: i64,
    pub standard_expense_cap_minor: i64,
    pub tax_bands: Vec<ThaiTaxBand>,
    pub social_security: ThaiSocialSecurityRules,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiTaxBand {
    pub up_to_minor: Option<i64>,
    pub rate_bps: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiSocialSecurityRules {
    pub employee_rate_bps: i64,
    pub employer_rate_bps: i64,
    pub minimum_wage_base_minor: i64,
    pub maximum_wage_base_minor: i64,
    pub rounding_unit_minor: i64,
}

fn stat_invalid(code: &'static str, message: &'static str) -> BusinessError {
    invalid(code, message)
}

fn multiply_bps(amount: i64, bps: i64) -> Result<i64, BusinessError> {
    ratio(amount, bps, 10_000)
}

fn validate_rules(rules: &ThaiPayrollRules, pay_date: &str) -> Result<(), BusinessError> {
    if rules.country_code != "TH"
        || rules.currency != "THB"
        || rules.version.trim().is_empty()
        || !valid_date(pay_date)
        || pay_date < rules.effective_from.as_str()
        || pay_date > rules.effective_to.as_str()
    {
        return Err(conflict(
            "COUNTRY_PACK_NOT_EFFECTIVE",
            "An effective, versioned Thai rule pack is required for the pay date.",
        ));
    }
    if rules.standard_expense_rate_bps < 0
        || rules.standard_expense_rate_bps > 10_000
        || rules.standard_expense_cap_minor < 0
        || rules.tax_bands.is_empty()
        || rules
            .tax_bands
            .last()
            .and_then(|band| band.up_to_minor)
            .is_some()
    {
        return Err(stat_invalid(
            "COUNTRY_PACK_INVALID",
            "The Thai payroll rule pack is invalid.",
        ));
    }
    let mut previous = 0_i64;
    for (index, band) in rules.tax_bands.iter().enumerate() {
        if band.rate_bps < 0 || band.rate_bps > 10_000 {
            return Err(stat_invalid(
                "COUNTRY_PACK_INVALID",
                "The Thai payroll tax bands are invalid.",
            ));
        }
        match band.up_to_minor {
            None if index + 1 != rules.tax_bands.len() => {
                return Err(stat_invalid(
                    "COUNTRY_PACK_INVALID",
                    "Only the final Thai payroll tax band may be unbounded.",
                ));
            }
            Some(upper) if upper <= previous => {
                return Err(stat_invalid(
                    "COUNTRY_PACK_INVALID",
                    "Thai payroll tax bands must have increasing limits.",
                ));
            }
            Some(upper) => previous = upper,
            None => {}
        }
    }
    let sso = &rules.social_security;
    if sso.minimum_wage_base_minor < 0
        || sso.maximum_wage_base_minor < sso.minimum_wage_base_minor
        || sso.rounding_unit_minor < 1
        || !(0..=10_000).contains(&sso.employee_rate_bps)
        || !(0..=10_000).contains(&sso.employer_rate_bps)
    {
        return Err(stat_invalid(
            "COUNTRY_PACK_INVALID",
            "The Thai social-security rule is invalid.",
        ));
    }
    Ok(())
}

fn annual_tax(
    income: i64,
    deductions: i64,
    rules: &ThaiPayrollRules,
) -> Result<i64, BusinessError> {
    let expense = multiply_bps(income, rules.standard_expense_rate_bps)?
        .min(rules.standard_expense_cap_minor);
    let mut remaining = income - expense - deductions;
    if remaining <= 0 {
        return Ok(0);
    }
    let mut lower = 0_i64;
    let mut tax = 0_i64;
    for band in &rules.tax_bands {
        let width = match band.up_to_minor {
            Some(upper) => upper - lower,
            None => remaining,
        };
        let taxable = remaining.min(width);
        tax = tax
            .checked_add(multiply_bps(taxable, band.rate_bps)?)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll tax exceeds supported precision."))?;
        remaining -= taxable;
        if remaining == 0 {
            break;
        }
        if let Some(upper) = band.up_to_minor {
            lower = upper;
        }
    }
    Ok(tax)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiPayrollCalculation {
    pub pay_date: String,
    pub regular_taxable_minor: i64,
    pub irregular_taxable_minor: i64,
    #[serde(default)]
    pub irregular_paid_before_minor: Option<i64>,
    #[serde(default)]
    pub irregular_occurrences_in_year: Option<i64>,
    #[serde(default)]
    pub termination_severance_minor: Option<i64>,
    pub non_taxable_minor: i64,
    pub social_security_wage_minor: i64,
    pub social_security_eligible: bool,
    #[serde(default)]
    pub social_security_exemption_evidence: Option<String>,
    pub other_employee_deductions_minor: i64,
    pub verified_annual_deductions_minor: i64,
    pub taxable_paid_before_minor: i64,
    pub tax_withheld_before_minor: i64,
    pub scheduled_pays_in_year: i64,
    pub last_pay_in_year: bool,
    pub prior_income_complete: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiPayrollCalculationResult {
    pub gross_minor: i64,
    pub tax_withheld_minor: i64,
    pub employee_sso_minor: i64,
    pub employer_sso_minor: i64,
    pub other_employee_deductions_minor: i64,
    pub net_minor: i64,
    pub taxable_ytd_minor: i64,
    pub tax_withheld_ytd_minor: i64,
    pub explanation: ThaiPayrollExplanation,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThaiPayrollExplanation {
    pub rule_version: String,
    pub rule_effective_from: String,
    pub rule_effective_to: String,
    pub ordinary_annual_tax_minor: i64,
    pub irregular_annual_tax_difference_minor: i64,
    pub irregular_paid_before_minor: i64,
    pub irregular_occurrences_in_year: i64,
    pub social_security_base_minor: i64,
    pub standard_expense_rate_bps: i64,
    pub scheduled_pays_in_year: i64,
    pub last_pay_in_year: bool,
}

fn round_unit(value: i64, unit: i64) -> i64 {
    ((value + unit / 2) / unit) * unit
}

pub fn calculate_thai_payroll(
    input: &ThaiPayrollCalculation,
    rules: &ThaiPayrollRules,
) -> Result<ThaiPayrollCalculationResult, BusinessError> {
    validate_rules(rules, &input.pay_date)?;
    if input.termination_severance_minor.unwrap_or(0) > 0 {
        return Err(invalid(
            "TERMINATION_TAX_UNSUPPORTED",
            "One-time termination income requires a separately reviewed tax rule.",
        ));
    }
    if !(1..=12).contains(&input.scheduled_pays_in_year) {
        return Err(invalid(
            "UNSUPPORTED_PAY_FREQUENCY",
            "Thai payroll needs a reviewed monthly payment count of 1-12.",
        ));
    }
    if !input.prior_income_complete {
        return Err(invalid(
            "YTD_INCOMPLETE",
            "Verified prior taxable income and withholding are required.",
        ));
    }
    if !input.social_security_eligible
        && !input
            .social_security_exemption_evidence
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(invalid(
            "SSO_ELIGIBILITY_UNVERIFIED",
            "A verified reason is required to exclude social security.",
        ));
    }
    let nonnegative = |value: i64| {
        if value < 0 {
            Err(invalid(
                "INVALID_PAYROLL_INPUT",
                "Payroll inputs must be nonnegative minor-unit integers.",
            ))
        } else {
            Ok(value)
        }
    };
    let regular = nonnegative(input.regular_taxable_minor)?;
    let irregular = nonnegative(input.irregular_taxable_minor)?;
    let non_taxable = nonnegative(input.non_taxable_minor)?;
    let sso_wage = nonnegative(input.social_security_wage_minor)?;
    let other_deductions = nonnegative(input.other_employee_deductions_minor)?;
    let deductions = nonnegative(input.verified_annual_deductions_minor)?;
    let paid_before = nonnegative(input.taxable_paid_before_minor)?;
    let withheld_before = nonnegative(input.tax_withheld_before_minor)?;
    let pays = input.scheduled_pays_in_year;
    let ordinary_annual_tax = annual_tax(regular.saturating_mul(pays), deductions, rules)?;
    let irregular_before = nonnegative(input.irregular_paid_before_minor.unwrap_or(0))?;
    let occurrences = input.irregular_occurrences_in_year.unwrap_or(1);
    if !(1..=12).contains(&occurrences) {
        return Err(invalid(
            "INVALID_PAYROLL_INPUT",
            "Occasional income needs a verified occurrence count of 1-12.",
        ));
    }
    let before = annual_tax(
        regular
            .saturating_mul(pays)
            .saturating_add(irregular_before),
        deductions,
        rules,
    )?;
    let with_irregular = annual_tax(
        regular
            .saturating_mul(pays)
            .saturating_add(irregular_before)
            .saturating_add(irregular.saturating_mul(occurrences)),
        deductions,
        rules,
    )?;
    let regular_withholding = ordinary_annual_tax / pays;
    let irregular_withholding = with_irregular - before;
    let annual_actual = annual_tax(
        paid_before
            .saturating_add(regular)
            .saturating_add(irregular),
        deductions,
        rules,
    )?;
    let tax = if input.last_pay_in_year {
        annual_actual - withheld_before
    } else {
        regular_withholding + irregular_withholding
    };
    if tax < 0 {
        return Err(invalid(
            "TAX_OVERWITHHELD_REVIEW",
            "Prior withholding exceeds final computed tax; an expert must review the correction.",
        ));
    }
    let sso = &rules.social_security;
    let sso_base = if !input.social_security_eligible || sso_wage == 0 {
        0
    } else if sso_wage < sso.minimum_wage_base_minor {
        sso.minimum_wage_base_minor
    } else if sso_wage > sso.maximum_wage_base_minor {
        sso.maximum_wage_base_minor
    } else {
        sso_wage
    };
    let employee_sso = round_unit(
        multiply_bps(sso_base, sso.employee_rate_bps)?,
        sso.rounding_unit_minor,
    );
    let employer_sso = round_unit(
        multiply_bps(sso_base, sso.employer_rate_bps)?,
        sso.rounding_unit_minor,
    );
    let gross = regular
        .saturating_add(irregular)
        .saturating_add(non_taxable);
    let net = gross
        .saturating_sub(tax)
        .saturating_sub(employee_sso)
        .saturating_sub(other_deductions);
    if net < 0 {
        return Err(invalid(
            "NET_PAY_NEGATIVE",
            "Deductions exceed earnings; review this employee's payroll input.",
        ));
    }
    Ok(ThaiPayrollCalculationResult {
        gross_minor: gross,
        tax_withheld_minor: tax,
        employee_sso_minor: employee_sso,
        employer_sso_minor: employer_sso,
        other_employee_deductions_minor: other_deductions,
        net_minor: net,
        taxable_ytd_minor: paid_before
            .saturating_add(regular)
            .saturating_add(irregular),
        tax_withheld_ytd_minor: withheld_before.saturating_add(tax),
        explanation: ThaiPayrollExplanation {
            rule_version: rules.version.clone(),
            rule_effective_from: rules.effective_from.clone(),
            rule_effective_to: rules.effective_to.clone(),
            ordinary_annual_tax_minor: ordinary_annual_tax,
            irregular_annual_tax_difference_minor: irregular_withholding,
            irregular_paid_before_minor: irregular_before,
            irregular_occurrences_in_year: occurrences,
            social_security_base_minor: sso_base,
            standard_expense_rate_bps: rules.standard_expense_rate_bps,
            scheduled_pays_in_year: pays,
            last_pay_in_year: input.last_pay_in_year,
        },
    })
}

fn prior_calculation(item: &ThaiPriorItem) -> Result<(i64, i64, i64), BusinessError> {
    let value: Value = serde_json::from_str(&item.result_json).map_err(|_| {
        invalid(
            "PAYROLL_YTD_UNVERIFIED",
            "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
        )
    })?;
    let statutory = value.get("statutory").ok_or_else(|| {
        invalid(
            "PAYROLL_YTD_UNVERIFIED",
            "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
        )
    })?;
    let adapter = statutory.get("adapterId").and_then(Value::as_str);
    if adapter != Some("thai-payroll-v1") {
        return Err(invalid(
            "PAYROLL_YTD_UNVERIFIED",
            "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
        ));
    }
    let input = statutory.get("input").ok_or_else(|| {
        invalid(
            "PAYROLL_YTD_UNVERIFIED",
            "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
        )
    })?;
    let calculation = statutory.get("calculation").ok_or_else(|| {
        invalid(
            "PAYROLL_YTD_UNVERIFIED",
            "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
        )
    })?;
    let number = |value: Option<&Value>| -> Result<i64, BusinessError> {
        value.and_then(Value::as_i64).ok_or_else(|| {
            invalid(
                "PAYROLL_YTD_UNVERIFIED",
                "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.",
            )
        })
    };
    Ok((
        number(input.get("regularTaxableMinor"))?,
        number(input.get("irregularTaxableMinor"))?,
        number(calculation.get("taxWithheldMinor"))?,
    ))
}

fn apply_thai_statutory(
    calculated: GrossCalculation,
    evidence: &ThaiStatutoryEvidence,
    run: &PayrollRun,
) -> Result<(i64, i64, i64, i64, Vec<AppliedComponent>, Value), BusinessError> {
    let mut applied = calculated.applied.clone();
    let earnings: Vec<&AppliedComponent> = applied
        .iter()
        .filter(|item| item.kind == "earning")
        .collect();
    if earnings
        .iter()
        .any(|item| item.income_timing.is_none() || item.social_security_wage.is_none())
    {
        return Err(invalid(
            "PAYROLL_COMPONENT_CLASSIFICATION_REQUIRED",
            "Classify each earning as regular, occasional or exempt and specify its Section 33 wage treatment.",
        ));
    }
    if applied.iter().any(|item| {
        item.kind != "earning"
            && (item.code.to_uppercase().contains(":TAX")
                || item.code.to_uppercase().contains(":PIT")
                || item.code.to_uppercase().contains(":PND1")
                || item.code.to_uppercase().contains(":SSO")
                || item.code.to_uppercase().contains(":SOCIAL"))
    }) {
        return Err(invalid(
            "PAYROLL_STATUTORY_COMPONENT_DUPLICATE",
            "Use calculated tax and social-security deductions instead of fixed statutory components.",
        ));
    }
    let earnings_minor = safe_add(earnings.iter().map(|item| item.amount_minor))?;
    let base_wages = calculated.gross - earnings_minor - calculated.overtime_minor;
    let regular = base_wages
        + safe_add(
            earnings
                .iter()
                .filter(|item| item.income_timing.as_deref() == Some("regular"))
                .map(|item| item.amount_minor),
        )?;
    let irregular = calculated.overtime_minor
        + safe_add(
            earnings
                .iter()
                .filter(|item| item.income_timing.as_deref() == Some("irregular"))
                .map(|item| item.amount_minor),
        )?;
    let non_taxable = safe_add(
        earnings
            .iter()
            .filter(|item| item.income_timing.as_deref() == Some("non_taxable"))
            .map(|item| item.amount_minor),
    )?;
    let mut prior_irregular = evidence.profile.opening_irregular_minor;
    let mut prior_taxable = evidence.profile.opening_taxable_minor;
    let mut prior_withheld = evidence.profile.opening_withheld_minor;
    for item in &evidence.prior_items {
        let (regular_taxable, irregular_taxable, withheld) = prior_calculation(item)?;
        prior_irregular = prior_irregular.saturating_add(irregular_taxable);
        prior_taxable = prior_taxable
            .saturating_add(regular_taxable)
            .saturating_add(irregular_taxable);
        prior_withheld = prior_withheld.saturating_add(withheld);
    }
    let social_security_wage = base_wages
        + safe_add(
            earnings
                .iter()
                .filter(|item| item.social_security_wage == Some(true))
                .map(|item| item.amount_minor),
        )?;
    let pay_date = run.pay_date.clone();
    let calculation = calculate_thai_payroll(
        &ThaiPayrollCalculation {
            pay_date: pay_date.clone(),
            regular_taxable_minor: regular,
            irregular_taxable_minor: irregular,
            irregular_paid_before_minor: Some(prior_irregular),
            irregular_occurrences_in_year: Some(1),
            termination_severance_minor: Some(0),
            non_taxable_minor: non_taxable,
            social_security_wage_minor: social_security_wage,
            social_security_eligible: evidence.profile.social_security_eligible,
            social_security_exemption_evidence: evidence
                .profile
                .social_security_exemption_evidence
                .clone(),
            other_employee_deductions_minor: calculated.deductions,
            verified_annual_deductions_minor: evidence.profile.annual_deductions_minor,
            taxable_paid_before_minor: prior_taxable,
            tax_withheld_before_minor: prior_withheld,
            scheduled_pays_in_year: evidence.profile.scheduled_pays,
            last_pay_in_year: run.mode() == Ok(PayrollRunMode::FinalPay)
                || pay_date.get(5..7) == Some("12"),
            prior_income_complete: evidence.profile.prior_income_complete,
        },
        &evidence.rules,
    )?;
    applied.push(AppliedComponent {
        code: "STATUTORY:TAX".to_owned(),
        label: "Thai payroll withholding tax".to_owned(),
        kind: "deduction".to_owned(),
        amount: minor_to_decimal(calculation.tax_withheld_minor),
        amount_minor: calculation.tax_withheld_minor,
        taxable: None,
        income_timing: None,
        social_security_wage: None,
    });
    applied.push(AppliedComponent {
        code: "STATUTORY:SSO".to_owned(),
        label: "Section 33 employee contribution".to_owned(),
        kind: "deduction".to_owned(),
        amount: minor_to_decimal(calculation.employee_sso_minor),
        amount_minor: calculation.employee_sso_minor,
        taxable: None,
        income_timing: None,
        social_security_wage: None,
    });
    applied.push(AppliedComponent {
        code: "STATUTORY:SSO:EMPLOYER".to_owned(),
        label: "Section 33 employer contribution".to_owned(),
        kind: "employer".to_owned(),
        amount: minor_to_decimal(calculation.employer_sso_minor),
        amount_minor: calculation.employer_sso_minor,
        taxable: None,
        income_timing: None,
        social_security_wage: None,
    });
    let statutory = serde_json::json!({
        "adapterId": "thai-payroll-v1",
        "sourceDigest": evidence.source_digest,
        "certified": false,
        "input": {
            "payDate": pay_date,
            "regularTaxableMinor": regular,
            "irregularTaxableMinor": irregular,
            "irregularPaidBeforeMinor": prior_irregular,
            "nonTaxableMinor": non_taxable,
            "socialSecurityWageMinor": social_security_wage,
            "socialSecurityEligible": evidence.profile.social_security_eligible,
            "otherEmployeeDeductionsMinor": calculated.deductions,
            "verifiedAnnualDeductionsMinor": evidence.profile.annual_deductions_minor,
            "taxablePaidBeforeMinor": prior_taxable,
            "taxWithheldBeforeMinor": prior_withheld,
            "scheduledPaysInYear": evidence.profile.scheduled_pays,
        },
        "calculation": calculation,
    });
    Ok((
        calculation.gross_minor,
        calculated.deductions + calculation.tax_withheld_minor + calculation.employee_sso_minor,
        calculated.employer + calculation.employer_sso_minor,
        calculation.net_minor,
        applied,
        statutory,
    ))
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotEngagement<'a> {
    id: &'a str,
    version: i64,
    person_id: &'a str,
    worker_type: &'a str,
    employment_type: &'a str,
    start_date: &'a str,
    end_date: Option<&'a str>,
    job_title: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotPayee<'a> {
    id: &'a str,
    version: i64,
    legal_name: &'a str,
    preferred_name: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotCompensation<'a> {
    id: &'a str,
    version: i64,
    base_amount_minor: i64,
    hourly_rate_minor: i64,
    daily_rate_minor: i64,
    standard_daily_hours: f64,
    currency: &'a str,
    pay_frequency: &'a str,
    effective_from: &'a str,
    effective_to: Option<&'a str>,
    status: &'a str,
    components_json: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotTimeEntry<'a> {
    id: &'a str,
    version: i64,
    work_date: &'a str,
    regular_hours: f64,
    overtime_hours: f64,
    cost_assignment_id: Option<&'a str>,
    cost_branch_id: Option<&'a str>,
    cost_department_id: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotHandoff<'a> {
    id: &'a str,
    request_id: &'a str,
    request_version: i64,
    minutes: f64,
    work_date: &'a str,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotAdvance<'a> {
    id: &'a str,
    version: i64,
    amount_minor: i64,
    due_period_start: &'a str,
    due_period_end: &'a str,
    reason: &'a str,
}

#[derive(Clone, Debug, Serialize)]
struct SnapshotCostAssignment<'a> {
    id: &'a str,
    version: i64,
    effective_from: &'a str,
    effective_to: Option<&'a str>,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    evidence_status: &'a str,
}

fn thai_snapshot(evidence: &ThaiStatutoryEvidence) -> Value {
    serde_json::json!({
        "pack": {
            "id": evidence.pack_id,
            "recordVersion": evidence.record_version,
            "sourceDigest": evidence.source_digest,
            "rulesJson": serde_json::to_string(&evidence.rules).unwrap_or_default(),
        },
        "profile": {
            "id": evidence.profile.id,
            "company_id": evidence.profile.id,
            "engagement_id": evidence.profile.id,
            "tax_year": evidence.profile.tax_year,
            "effective_from": evidence.profile.effective_from,
            "effective_to": evidence.profile.effective_to,
            "annual_deductions_minor": evidence.profile.annual_deductions_minor,
            "scheduled_pays": evidence.profile.scheduled_pays,
            "social_security_eligible": evidence.profile.social_security_eligible,
            "social_security_exemption_evidence": evidence.profile.social_security_exemption_evidence,
            "opening_through_date": evidence.profile.opening_through_date,
            "opening_taxable_minor": evidence.profile.opening_taxable_minor,
            "opening_irregular_minor": evidence.profile.opening_irregular_minor,
            "opening_withheld_minor": evidence.profile.opening_withheld_minor,
            "prior_income_complete": evidence.profile.prior_income_complete,
            "evidence_ref": evidence.profile.evidence_ref,
            "status": evidence.profile.status,
            "version": evidence.profile.version,
        },
        "priorItems": evidence.prior_items.iter().map(|item| serde_json::json!({
            "id": item.id,
            "run_id": item.run_id,
            "run_version": item.run_version,
            "input_digest": item.input_digest,
            "result_json": item.result_json,
            "pay_date": item.pay_date,
        })).collect::<Vec<_>>(),
    })
}

fn apply_benefits(
    benefits: &[BenefitPayrollInputRow],
    deductions_minor: &mut i64,
    employer_cost_minor: &mut i64,
    net_minor: &mut i64,
    applied: &mut Vec<AppliedComponent>,
) -> Result<(), BusinessError> {
    for benefit in benefits {
        if benefit.employee_deduction_minor < 0 || benefit.employer_cost_minor < 0 {
            return Err(invalid(
                "PAYROLL_INPUT_INCOMPLETE",
                "Released Benefits payroll input is invalid.",
            ));
        }
        if benefit.employee_deduction_minor > MAX_SAFE_INTEGER
            || benefit.employer_cost_minor > MAX_SAFE_INTEGER
        {
            return Err(invalid(
                "MONEY_OVERFLOW",
                "Payroll exceeds supported precision.",
            ));
        }
        if benefit.employee_deduction_minor > 0 {
            *deductions_minor = deductions_minor
                .checked_add(benefit.employee_deduction_minor)
                .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
            *net_minor = net_minor
                .checked_sub(benefit.employee_deduction_minor)
                .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
            applied.push(AppliedComponent {
                code: format!("benefit-deduction:{}", benefit.id),
                label: format!("Benefit · {}", benefit.plan_name),
                kind: "deduction".to_owned(),
                amount: minor_to_decimal(benefit.employee_deduction_minor),
                amount_minor: benefit.employee_deduction_minor,
                taxable: None,
                income_timing: None,
                social_security_wage: None,
            });
        }
        if benefit.employer_cost_minor > 0 {
            *employer_cost_minor = employer_cost_minor
                .checked_add(benefit.employer_cost_minor)
                .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
            applied.push(AppliedComponent {
                code: format!("benefit-employer:{}", benefit.id),
                label: format!("Benefit employer cost · {}", benefit.plan_name),
                kind: "employer".to_owned(),
                amount: minor_to_decimal(benefit.employer_cost_minor),
                amount_minor: benefit.employer_cost_minor,
                taxable: None,
                income_timing: None,
                social_security_wage: None,
            });
        }
    }
    Ok(())
}

pub fn calculate_payroll_item(
    input: &PayrollCalculationInput,
    run: &PayrollRun,
) -> Result<CalculatedPayrollItem, BusinessError> {
    let gross = calculate_gross(input, run)?;
    let overtime_minor = gross.overtime_minor;
    let compensation = input.compensation.as_ref().unwrap();
    let mut gross_minor = gross.gross;
    let mut deductions_minor = gross.deductions;
    let mut employer_cost_minor = gross.employer;
    let mut net_minor = gross.net;
    let mut applied = gross.applied.clone();
    let mut statutory = None;
    if run.requires_statutory() {
        let evidence = input
            .thai_statutory
            .as_ref()
            .ok_or_else(statutory_evidence_required)?;
        let (gross_value, deductions, employer, net, with_statutory, statutory_json) =
            apply_thai_statutory(gross, evidence, run)?;
        gross_minor = gross_value;
        deductions_minor = deductions;
        employer_cost_minor = employer;
        net_minor = net;
        applied = with_statutory;
        statutory = Some(statutory_json);
    }
    let mut benefits = input.benefits.clone();
    benefits.sort_by(|left, right| left.id.cmp(&right.id));
    apply_benefits(
        &benefits,
        &mut deductions_minor,
        &mut employer_cost_minor,
        &mut net_minor,
        &mut applied,
    )?;
    if net_minor < 0 {
        return Err(invalid(
            "NEGATIVE_NET_PAY",
            "Review deductions exceeding gross pay.",
        ));
    }
    for value in [
        gross_minor,
        deductions_minor,
        employer_cost_minor,
        net_minor,
    ] {
        if !(0..=MAX_SAFE_INTEGER).contains(&value) {
            return Err(invalid(
                "MONEY_OVERFLOW",
                "Payroll exceeds supported precision.",
            ));
        }
    }

    let base_days = date_days(&run.period_start)?;
    let period_days = date_days(&run.period_end)? - base_days + 1;
    let active_start = if input.engagement.start_date > run.period_start {
        input.engagement.start_date.clone()
    } else {
        run.period_start.clone()
    };
    let active_end = match input.engagement.end_date.as_deref() {
        Some(value) if value < run.period_end.as_str() => value.to_owned(),
        _ => run.period_end.clone(),
    };
    let active_days = date_days(&active_end)? - date_days(&active_start)? + 1;
    let snapshot = serde_json::json!({
        "engagement": SnapshotEngagement {
            id: &input.engagement.id,
            version: input.engagement.version,
            person_id: &input.engagement.person_id,
            worker_type: &input.engagement.worker_type,
            employment_type: &input.engagement.employment_type,
            start_date: &input.engagement.start_date,
            end_date: input.engagement.end_date.as_deref(),
            job_title: &input.engagement.job_title,
            branch_id: input.engagement.branch_id.as_deref(),
            department_id: input.engagement.department_id.as_deref(),
        },
        "payee": SnapshotPayee {
            id: &input.payee.id,
            version: input.payee.version,
            legal_name: &input.payee.legal_name,
            preferred_name: input.payee.preferred_name.as_deref(),
        },
        "compensation": SnapshotCompensation {
            id: &compensation.id,
            version: compensation.version,
            base_amount_minor: compensation.base_amount_minor,
            hourly_rate_minor: compensation.hourly_rate_minor,
            daily_rate_minor: compensation.daily_rate_minor,
            standard_daily_hours: compensation.standard_daily_hours,
            currency: &compensation.currency,
            pay_frequency: &compensation.pay_frequency,
            effective_from: &compensation.effective_from,
            effective_to: compensation.effective_to.as_deref(),
            status: &compensation.status,
            components_json: compensation.components_json.as_deref(),
        },
        "hours": {
            "handoffs": input.handoffs.iter().map(|handoff| SnapshotHandoff {
                id: &handoff.id,
                request_id: &handoff.request_id,
                request_version: handoff.request_version,
                minutes: handoff.minutes,
                work_date: &handoff.work_date,
            }).collect::<Vec<_>>(),
            "entries": input.time_entries.iter().map(|entry| SnapshotTimeEntry {
                id: &entry.id,
                version: entry.version,
                work_date: &entry.work_date,
                regular_hours: entry.regular_hours,
                overtime_hours: entry.overtime_hours,
                cost_assignment_id: entry.cost_assignment_id.as_deref(),
                cost_branch_id: entry.cost_branch_id.as_deref(),
                cost_department_id: entry.cost_department_id.as_deref(),
            }).collect::<Vec<_>>(),
        },
        "benefitInputs": benefits,
        "advanceRepayments": input.advances.iter().map(|advance| SnapshotAdvance {
            id: &advance.id,
            version: advance.version,
            amount_minor: advance.amount_minor,
            due_period_start: &advance.due_period_start,
            due_period_end: &advance.due_period_end,
            reason: &advance.reason,
        }).collect::<Vec<_>>(),
        "costAssignments": input.cost_assignments.iter().map(|assignment| SnapshotCostAssignment {
            id: &assignment.id,
            version: assignment.version,
            effective_from: &assignment.effective_from,
            effective_to: assignment.effective_to.as_deref(),
            branch_id: assignment.branch_id.as_deref(),
            department_id: assignment.department_id.as_deref(),
            evidence_status: &assignment.evidence_status,
        }).collect::<Vec<_>>(),
        "period": {"start": run.period_start, "end": run.period_end},
        "policy": {
            "overtimeMultiplierBps": run.overtime_multiplier_bps,
            "prorationMethod": run.proration_method,
            "activeDays": active_days,
            "periodDays": period_days,
        },
        "thaiStatutory": input.thai_statutory.as_ref().map(thai_snapshot),
    });
    let result = serde_json::json!({
        "grossMinor": gross_minor,
        "deductionsMinor": deductions_minor,
        "employerCostMinor": employer_cost_minor,
        "netMinor": net_minor,
        "statutory": statutory,
        "explanation": {
            "baseMinor": compensation.base_amount_minor,
            "hourlyRateMinor": compensation.hourly_rate_minor,
            "dailyRateMinor": compensation.daily_rate_minor,
            "standardDailyHours": compensation.standard_daily_hours,
            "regularHours": input.time_entries.iter().map(|entry| entry.regular_hours).sum::<f64>(),
            "overtimeHours": input.time_entries.iter().map(|entry| entry.overtime_hours).sum::<f64>()
                + input.handoffs.iter().map(|handoff| handoff.minutes / 60.0).sum::<f64>(),
            "overtimeMinor": overtime_minor,
            "overtimeMultiplierBps": run.overtime_multiplier_bps,
            "activeDays": active_days,
            "periodDays": period_days,
            "components": applied,
        },
    });
    Ok(CalculatedPayrollItem {
        id: String::new(),
        engagement_id: input.engagement.id.clone(),
        person_id: input.engagement.person_id.clone(),
        worker_type: input.engagement.worker_type.clone(),
        input_snapshot_json: snapshot.to_string(),
        result_json: result.to_string(),
        gross_minor,
        deductions_minor,
        employer_cost_minor,
        net_minor,
        currency: run.currency.clone(),
    })
}

pub fn calculation_digest_payload(items: &[CalculatedPayrollItem]) -> String {
    let snapshots: Vec<Value> = items
        .iter()
        .map(|item| serde_json::from_str(&item.input_snapshot_json).unwrap_or(Value::Null))
        .collect();
    serde_json::to_string(&snapshots).unwrap_or_default()
}

pub fn posting_digest_payload(items: &[PayrollPostingItem]) -> String {
    let snapshots: Vec<Value> = items
        .iter()
        .map(|item| serde_json::from_str(&item.input_snapshot_json).unwrap_or(Value::Null))
        .collect();
    serde_json::to_string(&snapshots).unwrap_or_default()
}

/// Validate the current calculation sources before calculating or posting.
/// These are the retained readiness prerequisites that belong to People; the
/// composition layer still supplies the current Account/IAM authority.
pub fn require_calculation_ready(
    run: &PayrollRun,
    context: &PayrollCalculationContext,
) -> Result<(), BusinessError> {
    if context.inputs.is_empty() {
        return Err(invalid(
            "PAYROLL_EMPTY",
            "Assign active employees to this pay group before calculating.",
        ));
    }
    for input in &context.inputs {
        if input.engagement.worker_type != "employee" {
            return Err(conflict(
                "CONTRACTOR_PAYABLE_ROUTE",
                "Contractor amounts must be handled through Finance payables, not posted as payroll.",
            ));
        }
        if input.compensation.as_ref().is_none_or(|compensation| {
            compensation.currency != run.currency
                || !matches!(compensation.status.as_str(), "active" | "approved")
        }) {
            return Err(invalid(
                "PAYROLL_INPUT_INCOMPLETE",
                "Every employee needs approved compensation in the pay-group currency covering the full period.",
            ));
        }
        require_cost_evidence(input, run)?;
    }
    Ok(())
}

fn assignment_on<'a>(
    assignments: &'a [PayrollCostAssignment],
    date: &str,
) -> Result<&'a PayrollCostAssignment, BusinessError> {
    let matches: Vec<&PayrollCostAssignment> = assignments
        .iter()
        .filter(|row| {
            row.effective_from.as_str() <= date
                && row.effective_to.as_deref().is_none_or(|end| end >= date)
        })
        .collect();
    if matches.len() != 1 || matches[0].evidence_status != "verified" {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Verify dated branch and department assignments for every work day before payroll review.",
        ));
    }
    Ok(matches[0])
}

fn require_cost_evidence(
    input: &PayrollCalculationInput,
    run: &PayrollRun,
) -> Result<(), BusinessError> {
    if input.cost_assignments.is_empty() {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Verify dated branch and department assignments for every work day before payroll review.",
        ));
    }
    let start = if input.engagement.start_date > run.period_start {
        input.engagement.start_date.clone()
    } else {
        run.period_start.clone()
    };
    let end = match input.engagement.end_date.as_deref() {
        Some(value) if value < run.period_end.as_str() => value.to_owned(),
        _ => run.period_end.clone(),
    };
    let start_day = date_days(&start)?;
    let end_day = date_days(&end)?;
    let mut day = start_day;
    while day <= end_day {
        let date = date_add(&start, day - start_day);
        assignment_on(&input.cost_assignments, &date)?;
        day += 1;
    }
    for entry in &input.time_entries {
        let assignment = assignment_on(&input.cost_assignments, &entry.work_date)?;
        if entry.cost_assignment_id.as_deref() != Some(assignment.id.as_str())
            || entry.cost_branch_id != assignment.branch_id
            || entry.cost_department_id != assignment.department_id
        {
            return Err(invalid(
                "PAYROLL_TIME_COST_EVIDENCE_MISSING",
                "Approve time again after its dated cost unit is verified.",
            ));
        }
    }
    Ok(())
}

pub fn cancellable(run: &PayrollRun) -> Result<(), BusinessError> {
    let mode = run.mode()?;
    if !matches!(
        run.status.as_str(),
        "draft" | "calculating" | "failed" | "preview"
    ) || mode == PayrollRunMode::FinalPay
        || run.submitted_by.is_some()
        || run.approved_by.is_some()
        || run.posted_by.is_some()
    {
        return Err(conflict(
            "PAYROLL_CANCELLATION_UNAVAILABLE",
            "Only an unsubmitted ordinary payroll draft or preview can be cancelled.",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PayrollPostingItem {
    pub id: String,
    pub worker_type: String,
    pub status: String,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub employer_cost_minor: i64,
    pub net_minor: i64,
    pub input_snapshot_json: String,
    pub result_json: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayrollAccountMapping {
    #[serde(default)]
    pub chart_id: String,
    #[serde(default)]
    pub expense_account_id: Option<String>,
    #[serde(default)]
    pub net_payable_account_id: Option<String>,
    #[serde(default)]
    pub tax_payable_account_id: Option<String>,
    #[serde(default)]
    pub employer_contribution_account_id: Option<String>,
    #[serde(default)]
    pub employer_expense_account_id: Option<String>,
    #[serde(default)]
    pub social_security_payable_account_id: Option<String>,
    #[serde(default)]
    pub advance_receivable_account_id: Option<String>,
    #[serde(default)]
    pub benefit_payable_account_id: Option<String>,
    #[serde(default)]
    pub other_deduction_payable_account_id: Option<String>,
    #[serde(default)]
    pub service_charge_expense_account_id: Option<String>,
    #[serde(default)]
    pub overtime_expense_account_id: Option<String>,
    #[serde(default)]
    pub bonus_expense_account_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayrollMappingFact {
    pub id: String,
    pub chart_id: String,
    pub accounts_json: String,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct PayrollPostingBasis {
    pub run: PayrollRun,
    pub items: Vec<PayrollPostingItem>,
    pub mapping: Option<PayrollMappingFact>,
}

/// Finance-facing actor fields required by the composition journal port.
/// Composition maps this to `erpx_server_finance::public::FinanceActor`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollFinanceActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
}

/// One balanced payroll journal line. Composition maps this to Finance's
/// public `JournalLineInput`; People never persists an accounting row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollJournalLine {
    pub account_id: String,
    pub description: String,
    pub debit: String,
    pub credit: String,
    pub dimensions: std::collections::BTreeMap<String, String>,
}

impl PayrollJournalLine {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(&self.account_id) {
            return Err(invalid(
                "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
                "Every payroll journal line needs a valid ledger account.",
            ));
        }
        let debit = decimal_to_minor(&self.debit).map_err(|_| {
            invalid(
                "PAYROLL_TOTALS_INVALID",
                "Payroll journal debits and credits must balance.",
            )
        })?;
        let credit = decimal_to_minor(&self.credit).map_err(|_| {
            invalid(
                "PAYROLL_TOTALS_INVALID",
                "Payroll journal debits and credits must balance.",
            )
        })?;
        if debit < 0 || credit < 0 || (debit > 0) == (credit > 0) {
            return Err(invalid(
                "PAYROLL_TOTALS_INVALID",
                "Payroll journal debits and credits must balance.",
            ));
        }
        Ok(())
    }
}

/// Finance public journal draft shape consumed by the composed journal port.
/// The composition adapter converts it to Finance's `JournalDraft` and calls
/// `prepare_journal` with a posted status in the same serializable unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollJournalDraft {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub fx_rate: Option<String>,
    pub is_opening: bool,
    pub memo: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub lines: Vec<PayrollJournalLine>,
}

/// Finance public prepared-journal result. Composition maps Finance's
/// `PreparedJournalResult` into this owner contract after persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollPreparedJournal {
    pub id: String,
    pub number: String,
    pub status: String,
    pub version: i64,
    pub debit: String,
    pub credit: String,
    pub fx_rate_micros: i64,
    pub base_currency: String,
}

#[derive(Clone, Debug)]
pub struct PayrollAccrual {
    pub lines: Vec<PayrollJournalLine>,
    pub totals: PayrollTotals,
    pub categories: std::collections::BTreeMap<String, i64>,
}

#[derive(Clone, Debug)]
pub struct ResolvedPayrollPost {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub mapping_id: Option<String>,
    pub mapping_version: Option<i64>,
    pub accounts: PayrollAccountMapping,
}

fn account<'a>(value: Option<&'a str>, code: &'static str) -> Result<&'a str, BusinessError> {
    value.ok_or_else(|| {
        invalid(
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            match code {
                "expense" => "Map a wage expense account before posting payroll.",
                "net" => "Map the net payable account before posting payroll.",
                _ => "Map the required payroll ledger account before posting.",
            },
        )
    })
}

fn category(code: &str, kind: &str) -> String {
    let normalized: String = code
        .to_uppercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    if kind == "earning" {
        if normalized.contains("SERVICE_CHARGE") {
            return "service_charge".to_owned();
        }
        if normalized.contains("BONUS") || normalized.contains("INCENTIVE") {
            return "bonus".to_owned();
        }
        if normalized.contains("OVERTIME")
            || normalized.starts_with("OT_")
            || normalized.ends_with("_OT")
            || normalized.contains("_OT_")
        {
            return "overtime".to_owned();
        }
        return "wages".to_owned();
    }
    if normalized.starts_with("PAYROLL_ADVANCE_") {
        return "advance".to_owned();
    }
    if normalized.starts_with("BENEFIT_") {
        return "benefit".to_owned();
    }
    if normalized.contains("SOCIAL") || normalized.contains("SSO") {
        return "social_security".to_owned();
    }
    if normalized.contains("TAX")
        || normalized.contains("WITHHOLD")
        || normalized.contains("PND1")
        || normalized == "PIT"
        || normalized.starts_with("PIT_")
        || normalized.ends_with("_PIT")
        || normalized.contains("_PIT_")
    {
        return "tax".to_owned();
    }
    if code.to_lowercase().starts_with("thai:") {
        return "unmapped".to_owned();
    }
    if kind == "employer" {
        "employer_other".to_owned()
    } else {
        "deduction_other".to_owned()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredExplanation {
    #[serde(default)]
    hourly_rate_minor: i64,
    #[serde(default)]
    overtime_hours: f64,
    #[serde(default)]
    overtime_minor: Option<i64>,
    #[serde(default)]
    overtime_multiplier_bps: Option<i64>,
    #[serde(default)]
    components: Option<Vec<StoredComponent>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredResult {
    #[serde(default)]
    explanation: Option<StoredExplanation>,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotEngagement {
    #[serde(default)]
    employment_type: Option<String>,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotPeriod {
    #[serde(default)]
    start: String,
    #[serde(default)]
    end: String,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotPolicy {
    #[serde(default)]
    overtime_multiplier_bps: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotHours {
    #[serde(default)]
    entries: Vec<StoredSnapshotTime>,
    #[serde(default)]
    handoffs: Vec<StoredSnapshotHandoff>,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotTime {
    #[serde(default)]
    work_date: String,
    #[serde(default)]
    regular_hours: f64,
    #[serde(default)]
    overtime_hours: f64,
    #[serde(default)]
    cost_assignment_id: Option<String>,
    #[serde(default)]
    cost_branch_id: Option<String>,
    #[serde(default)]
    cost_department_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotHandoff {
    #[serde(default)]
    work_date: String,
    #[serde(default)]
    minutes: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct StoredSnapshotAssignment {
    #[serde(default)]
    id: String,
    #[serde(default)]
    effective_from: String,
    #[serde(default)]
    effective_to: Option<String>,
    #[serde(default)]
    branch_id: Option<String>,
    #[serde(default)]
    department_id: Option<String>,
    #[serde(default)]
    evidence_status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshot {
    #[serde(default)]
    engagement: Option<StoredSnapshotEngagement>,
    #[serde(default)]
    period: Option<StoredSnapshotPeriod>,
    #[serde(default)]
    policy: Option<StoredSnapshotPolicy>,
    #[serde(default)]
    hours: Option<StoredSnapshotHours>,
    #[serde(default)]
    cost_assignments: Vec<StoredSnapshotAssignment>,
}

#[derive(Clone, Debug)]
struct CostUnit {
    branch_id: Option<String>,
    department_id: Option<String>,
    days: i64,
    regular_micros: i64,
    overtime_micros: i64,
}

fn allocate_minor(amount: i64, weights: &[i64]) -> Result<Vec<i64>, BusinessError> {
    if amount < 0 {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Cost allocation amount is invalid.",
        ));
    }
    if amount == 0 {
        return Ok(weights.iter().map(|_| 0).collect());
    }
    let total: i128 = weights.iter().map(|weight| i128::from(*weight)).sum();
    if total <= 0 {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Approved dated work or placement evidence is missing for a payroll amount.",
        ));
    }
    let mut parts: Vec<(usize, i64, i128)> = weights
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            let product = i128::from(amount) * i128::from(*weight);
            (index, (product / total) as i64, product % total)
        })
        .collect();
    let mut remaining = amount - parts.iter().map(|part| part.1).sum::<i64>();
    parts.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    for part in &mut parts {
        if remaining <= 0 {
            break;
        }
        part.1 += 1;
        remaining -= 1;
    }
    parts.sort_by_key(|part| part.0);
    Ok(parts.into_iter().map(|part| part.1).collect())
}

fn cost_unit_for<'a>(
    units: &'a mut std::collections::BTreeMap<(Option<String>, Option<String>), CostUnit>,
    assignment: &StoredSnapshotAssignment,
) -> &'a mut CostUnit {
    units
        .entry((
            assignment.branch_id.clone(),
            assignment.department_id.clone(),
        ))
        .or_insert_with(|| CostUnit {
            branch_id: assignment.branch_id.clone(),
            department_id: assignment.department_id.clone(),
            days: 0,
            regular_micros: 0,
            overtime_micros: 0,
        })
}

fn cost_units(snapshot: &StoredSnapshot) -> Result<Vec<CostUnit>, BusinessError> {
    let period = snapshot.period.as_ref().ok_or_else(|| {
        invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated cost assignments are missing from the approved payroll snapshot.",
        )
    })?;
    let engagement = snapshot.engagement.as_ref().ok_or_else(|| {
        invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated cost assignments are missing from the approved payroll snapshot.",
        )
    })?;
    let start_date = engagement.start_date.as_deref().ok_or_else(|| {
        invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated cost assignments are missing from the approved payroll snapshot.",
        )
    })?;
    if snapshot.cost_assignments.is_empty() || !valid_date(start_date) || !valid_date(&period.start)
    {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INCOMPLETE",
            "Dated cost assignments are missing from the approved payroll snapshot.",
        ));
    }
    let start = if start_date > period.start.as_str() {
        start_date.to_owned()
    } else {
        period.start.clone()
    };
    let end = match engagement.end_date.as_deref() {
        Some(value) if value < period.end.as_str() => value.to_owned(),
        _ => period.end.clone(),
    };
    let mut units: std::collections::BTreeMap<(Option<String>, Option<String>), CostUnit> =
        std::collections::BTreeMap::new();
    let at = |date: &str| -> Result<&StoredSnapshotAssignment, BusinessError> {
        let matches: Vec<&StoredSnapshotAssignment> = snapshot
            .cost_assignments
            .iter()
            .filter(|row| {
                row.effective_from.as_str() <= date
                    && row
                        .effective_to
                        .as_deref()
                        .is_none_or(|value| value >= date)
            })
            .collect();
        if matches.len() != 1 || matches[0].evidence_status != "verified" {
            return Err(invalid(
                "PAYROLL_COST_ALLOCATION_INCOMPLETE",
                "Dated branch and department evidence is incomplete or ambiguous.",
            ));
        }
        Ok(matches[0])
    };

    let start_day = date_days(&start)?;
    let end_day = date_days(&end)?;
    let mut day = start_day;
    while day <= end_day {
        let date = date_add(&start, day - start_day);
        let assignment = at(&date)?;
        cost_unit_for(&mut units, assignment).days += 1;
        day += 1;
    }
    for entry in snapshot
        .hours
        .as_ref()
        .map(|hours| hours.entries.as_slice())
        .unwrap_or(&[])
    {
        let assignment = at(&entry.work_date)?;
        if entry.cost_assignment_id.as_deref() != Some(assignment.id.as_str())
            || entry.cost_branch_id != assignment.branch_id
            || entry.cost_department_id != assignment.department_id
        {
            return Err(invalid(
                "PAYROLL_TIME_COST_EVIDENCE_MISSING",
                "Approved time does not match its dated cost assignment.",
            ));
        }
        let unit = cost_unit_for(&mut units, assignment);
        unit.regular_micros = unit
            .regular_micros
            .checked_add(hours_micros(entry.regular_hours)?)
            .ok_or_else(|| {
                invalid(
                    "MONEY_OVERFLOW",
                    "Payroll hours exceed supported precision.",
                )
            })?;
        unit.overtime_micros = unit
            .overtime_micros
            .checked_add(hours_micros(entry.overtime_hours)?)
            .ok_or_else(|| {
                invalid(
                    "MONEY_OVERFLOW",
                    "Payroll hours exceed supported precision.",
                )
            })?;
    }
    if let Some(hours) = &snapshot.hours {
        for handoff in &hours.handoffs {
            let assignment = at(&handoff.work_date)?;
            let minutes = hours_micros(handoff.minutes)?;
            let unit = cost_unit_for(&mut units, assignment);
            unit.overtime_micros =
                unit.overtime_micros
                    .checked_add(minutes / 60)
                    .ok_or_else(|| {
                        invalid(
                            "MONEY_OVERFLOW",
                            "Payroll hours exceed supported precision.",
                        )
                    })?;
        }
    }
    Ok(units.into_values().collect())
}

#[derive(Clone, Debug)]
struct AllocatedAmounts {
    units: Vec<CostUnit>,
    wages: Vec<i64>,
    overtime: Vec<i64>,
    earnings: Vec<Vec<i64>>,
    deductions: Vec<Vec<i64>>,
    employers: Vec<Vec<i64>>,
    net: Vec<i64>,
}

fn allocated_amounts(
    item: &PayrollPostingItem,
    snapshot: &StoredSnapshot,
    components: &[StoredComponent],
) -> Result<AllocatedAmounts, BusinessError> {
    let units = cost_units(snapshot)?;
    let result: StoredResult = serde_json::from_str(&item.result_json).map_err(|_| {
        invalid(
            "PAYROLL_COMPONENTS_MISSING",
            "Recalculate payroll to retain item components before posting.",
        )
    })?;
    let explanation = result.explanation.unwrap_or(StoredExplanation {
        hourly_rate_minor: 0,
        overtime_hours: 0.0,
        overtime_minor: None,
        overtime_multiplier_bps: None,
        components: None,
    });
    let earnings: Vec<&StoredComponent> = components
        .iter()
        .filter(|item| item.kind == "earning")
        .collect();
    let earnings_minor = safe_add(
        earnings
            .iter()
            .map(|component| decimal_to_minor(&component.amount))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let base_gross = item.gross_minor - earnings_minor;
    let multiplier = explanation
        .overtime_multiplier_bps
        .or(snapshot
            .policy
            .as_ref()
            .and_then(|policy| policy.overtime_multiplier_bps))
        .unwrap_or(15_000);
    let overtime = explanation.overtime_minor.unwrap_or_else(|| {
        ((explanation.hourly_rate_minor as f64 * explanation.overtime_hours * multiplier as f64)
            / 10_000.0)
            .round() as i64
    });
    let regular = base_gross - overtime;
    if regular < 0 {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Wage and overtime amounts do not reconcile.",
        ));
    }
    let salary = snapshot
        .engagement
        .as_ref()
        .and_then(|engagement| engagement.employment_type.as_deref())
        == Some("salaried");
    let time_weights: Vec<i64> = units
        .iter()
        .map(|unit| {
            unit.regular_micros
                .saturating_mul(10_000)
                .saturating_add(unit.overtime_micros.saturating_mul(multiplier))
        })
        .collect();
    let general_weights: Vec<i64> = if salary {
        units.iter().map(|unit| unit.days).collect()
    } else {
        time_weights.clone()
    };
    let regular_weights: Vec<i64> = if salary {
        units.iter().map(|unit| unit.days).collect()
    } else {
        units.iter().map(|unit| unit.regular_micros).collect()
    };
    let overtime_weights: Vec<i64> = units.iter().map(|unit| unit.overtime_micros).collect();
    let wages = allocate_minor(regular, &regular_weights)?;
    let overtime_amounts = allocate_minor(overtime, &overtime_weights)?;
    let mut earnings_allocations = Vec::new();
    for component in &earnings {
        let amount = decimal_to_minor(&component.amount)?;
        let weights = if category(&component.code, "earning") == "overtime" {
            &overtime_weights
        } else {
            &general_weights
        };
        earnings_allocations.push(allocate_minor(amount, weights)?);
    }
    let gross: Vec<i64> = (0..units.len())
        .map(|index| {
            wages[index]
                + overtime_amounts[index]
                + earnings_allocations
                    .iter()
                    .map(|parts| parts[index])
                    .sum::<i64>()
        })
        .collect();
    let gross_weights: Vec<i64> = gross.clone();
    let mut deductions = Vec::new();
    for component in components.iter().filter(|item| item.kind == "deduction") {
        deductions.push(allocate_minor(
            decimal_to_minor(&component.amount)?,
            &gross_weights,
        )?);
    }
    let mut employers = Vec::new();
    for component in components.iter().filter(|item| item.kind == "employer") {
        employers.push(allocate_minor(
            decimal_to_minor(&component.amount)?,
            &gross_weights,
        )?);
    }
    let net: Vec<i64> = (0..units.len())
        .map(|index| gross[index] - deductions.iter().map(|parts| parts[index]).sum::<i64>())
        .collect();
    if net.iter().any(|amount| *amount < 0) || net.iter().sum::<i64>() != item.net_minor {
        return Err(invalid(
            "PAYROLL_COST_ALLOCATION_INVALID",
            "Cost-unit payable amounts do not reconcile.",
        ));
    }
    Ok(AllocatedAmounts {
        units,
        wages,
        overtime: overtime_amounts,
        earnings: earnings_allocations,
        deductions,
        employers,
        net,
    })
}

fn mapping_accounts(mapping: &PayrollMappingFact) -> Result<PayrollAccountMapping, BusinessError> {
    serde_json::from_str(&mapping.accounts_json).map_err(|_| {
        invalid(
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            "An independently approved pay-group account mapping is required before payroll posting.",
        )
    })
}

pub fn resolve_posting(
    basis: &PayrollPostingBasis,
    input: &PostPayrollInput,
    now: TimestampMillis,
) -> Result<ResolvedPayrollPost, BusinessError> {
    let run = &basis.run;
    if run.status != "approved" {
        return Err(conflict(
            "PAYROLL_NOT_APPROVED",
            "An independently approved payroll run is required before GL posting.",
        ));
    }
    if run.version != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The payroll run changed. Refresh before posting.",
        ));
    }
    if run.input_digest.is_none() {
        return Err(conflict(
            "PAYROLL_PREVIEW_REQUIRED",
            "Calculate a payroll preview before posting.",
        ));
    }
    if input.expected_version <= 0 {
        return Err(invalid(
            "PAYROLL_REQUEST_INVALID",
            "Review the payroll posting accounts.",
        ));
    }
    let approved_by = run.approved_by.as_deref().ok_or_else(|| {
        conflict(
            "PAYROLL_NOT_APPROVED",
            "An independently approved payroll run is required before GL posting.",
        )
    })?;
    if run.submitted_by.as_deref() == Some(approved_by) {
        return Err(conflict(
            "SELF_REVIEW_FORBIDDEN",
            "A different person must approve the submitted payroll revision.",
        ));
    }
    let mapping = basis.mapping.as_ref().ok_or_else(|| {
        conflict(
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            "An independently approved pay-group account mapping is required before payroll posting.",
        )
    })?;
    let saved = mapping_accounts(mapping)?;
    if input
        .chart_id
        .as_deref()
        .is_some_and(|chart| chart != mapping.chart_id)
    {
        return Err(invalid(
            "PAYROLL_MAPPING_OVERRIDE_FORBIDDEN",
            "The chart must match the approved pay-group mapping.",
        ));
    }
    for (name, value) in [
        ("expenseAccountId", input.expense_account_id.as_deref()),
        (
            "netPayableAccountId",
            input.net_payable_account_id.as_deref(),
        ),
        (
            "taxPayableAccountId",
            input.tax_payable_account_id.as_deref(),
        ),
        (
            "employerContributionAccountId",
            input.employer_contribution_account_id.as_deref(),
        ),
        (
            "employerExpenseAccountId",
            input.employer_expense_account_id.as_deref(),
        ),
        (
            "socialSecurityPayableAccountId",
            input.social_security_payable_account_id.as_deref(),
        ),
        (
            "advanceReceivableAccountId",
            input.advance_receivable_account_id.as_deref(),
        ),
        (
            "benefitPayableAccountId",
            input.benefit_payable_account_id.as_deref(),
        ),
        (
            "otherDeductionPayableAccountId",
            input.other_deduction_payable_account_id.as_deref(),
        ),
        (
            "serviceChargeExpenseAccountId",
            input.service_charge_expense_account_id.as_deref(),
        ),
        (
            "overtimeExpenseAccountId",
            input.overtime_expense_account_id.as_deref(),
        ),
        (
            "bonusExpenseAccountId",
            input.bonus_expense_account_id.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            let saved_value = match name {
                "expenseAccountId" => saved.expense_account_id.as_deref(),
                "netPayableAccountId" => saved.net_payable_account_id.as_deref(),
                "taxPayableAccountId" => saved.tax_payable_account_id.as_deref(),
                "employerContributionAccountId" => {
                    saved.employer_contribution_account_id.as_deref()
                }
                "employerExpenseAccountId" => saved.employer_expense_account_id.as_deref(),
                "socialSecurityPayableAccountId" => {
                    saved.social_security_payable_account_id.as_deref()
                }
                "advanceReceivableAccountId" => saved.advance_receivable_account_id.as_deref(),
                "benefitPayableAccountId" => saved.benefit_payable_account_id.as_deref(),
                "otherDeductionPayableAccountId" => {
                    saved.other_deduction_payable_account_id.as_deref()
                }
                "serviceChargeExpenseAccountId" => {
                    saved.service_charge_expense_account_id.as_deref()
                }
                "overtimeExpenseAccountId" => saved.overtime_expense_account_id.as_deref(),
                "bonusExpenseAccountId" => saved.bonus_expense_account_id.as_deref(),
                _ => None,
            };
            if saved_value != Some(value) {
                return Err(invalid(
                    "PAYROLL_MAPPING_OVERRIDE_FORBIDDEN",
                    "Payroll accounts must match the approved pay-group mapping.",
                ));
            }
        }
    }
    let accounts = PayrollAccountMapping {
        chart_id: mapping.chart_id.clone(),
        expense_account_id: saved.expense_account_id,
        net_payable_account_id: saved.net_payable_account_id,
        tax_payable_account_id: saved.tax_payable_account_id,
        employer_contribution_account_id: saved.employer_contribution_account_id,
        employer_expense_account_id: saved.employer_expense_account_id,
        social_security_payable_account_id: saved.social_security_payable_account_id,
        advance_receivable_account_id: saved.advance_receivable_account_id,
        benefit_payable_account_id: saved.benefit_payable_account_id,
        other_deduction_payable_account_id: saved.other_deduction_payable_account_id,
        service_charge_expense_account_id: saved.service_charge_expense_account_id,
        overtime_expense_account_id: saved.overtime_expense_account_id,
        bonus_expense_account_id: saved.bonus_expense_account_id,
    };
    if accounts.expense_account_id.is_none() || accounts.net_payable_account_id.is_none() {
        return Err(invalid(
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            "The approved mapping needs wage expense and net payable accounts.",
        ));
    }
    if basis.items.is_empty() {
        return Err(conflict(
            "PAYROLL_ITEMS_INVALID",
            "Every payroll item must be calculated or approved before posting.",
        ));
    }
    if basis
        .items
        .iter()
        .any(|item| !matches!(item.status.as_str(), "calculated" | "approved"))
    {
        return Err(conflict(
            "PAYROLL_ITEMS_INVALID",
            "Every payroll item must be calculated or approved before posting.",
        ));
    }
    if basis
        .items
        .iter()
        .any(|item| item.worker_type != "employee")
    {
        return Err(conflict(
            "CONTRACTOR_PAYABLE_ROUTE",
            "Contractor amounts must be handled through Finance payables, not posted as payroll.",
        ));
    }
    let entry_date = input
        .entry_date
        .clone()
        .unwrap_or_else(|| run.period_end.clone());
    if !valid_date(&entry_date) || entry_date < run.period_end {
        return Err(invalid(
            "PAYROLL_ACCRUAL_DATE_INVALID",
            "Recognize payroll expense at period end or later in an open adjustment period.",
        ));
    }
    let period_id = input.period_id.clone().ok_or_else(|| {
        conflict(
            "PAYROLL_ACCOUNTING_PERIOD_REQUIRED",
            "Select one open accounting period for payroll accrual.",
        )
    })?;
    if !valid_identifier(&period_id) {
        return Err(invalid(
            "PAYROLL_REQUEST_INVALID",
            "Review the payroll posting accounts.",
        ));
    }
    let _ = now;
    Ok(ResolvedPayrollPost {
        chart_id: mapping.chart_id.clone(),
        period_id,
        entry_date,
        mapping_id: Some(mapping.id.clone()),
        mapping_version: Some(mapping.version),
        accounts,
    })
}

pub fn payroll_accrual(
    basis: &PayrollPostingBasis,
    resolved: &ResolvedPayrollPost,
) -> Result<PayrollAccrual, BusinessError> {
    let accounts = &resolved.accounts;
    let expense = account(accounts.expense_account_id.as_deref(), "expense")?;
    let net_account = account(accounts.net_payable_account_id.as_deref(), "net")?;
    let employer_expense = accounts
        .employer_expense_account_id
        .as_deref()
        .ok_or_else(|| {
            invalid(
                "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
                "Map employer costs to an expense account separate from wages.",
            )
        })?;
    if employer_expense == expense {
        return Err(invalid(
            "PAYROLL_ACCOUNT_MAPPING_REQUIRED",
            "Map employer costs to an expense account separate from wages.",
        ));
    }
    type GroupKey = (String, String, Vec<(String, String)>);
    let mut grouped: std::collections::BTreeMap<GroupKey, (i64, i64)> =
        std::collections::BTreeMap::new();
    let mut categories: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut totals = PayrollTotals {
        gross_minor: 0,
        deductions_minor: 0,
        employer_cost_minor: 0,
        net_minor: 0,
        headcount: basis.items.len() as i64,
    };
    let add = |account_id: &str,
               description: &str,
               debit: i64,
               credit: i64,
               dimensions: &std::collections::BTreeMap<String, String>,
               grouped: &mut std::collections::BTreeMap<GroupKey, (i64, i64)>| {
        if debit == 0 && credit == 0 {
            return;
        }
        let key = (
            account_id.to_owned(),
            description.to_owned(),
            dimensions
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        let entry = grouped.entry(key).or_insert((0, 0));
        entry.0 += debit;
        entry.1 += credit;
    };
    for item in &basis.items {
        let snapshot: StoredSnapshot =
            serde_json::from_str(&item.input_snapshot_json).map_err(|_| {
                invalid(
                    "PAYROLL_COST_ALLOCATION_INCOMPLETE",
                    "Dated cost assignments are missing from the approved payroll snapshot.",
                )
            })?;
        let result: StoredResult = serde_json::from_str(&item.result_json).map_err(|_| {
            invalid(
                "PAYROLL_COMPONENTS_MISSING",
                "Recalculate payroll to retain the item components before posting.",
            )
        })?;
        let components = result
            .explanation
            .as_ref()
            .and_then(|explanation| explanation.components.clone())
            .ok_or_else(|| {
                invalid(
                    "PAYROLL_COMPONENTS_MISSING",
                    "Recalculate payroll to retain the item components before posting.",
                )
            })?;
        let earnings: Vec<&StoredComponent> = components
            .iter()
            .filter(|item| item.kind == "earning")
            .collect();
        let deductions: Vec<&StoredComponent> = components
            .iter()
            .filter(|item| item.kind == "deduction")
            .collect();
        let employer: Vec<&StoredComponent> = components
            .iter()
            .filter(|item| item.kind == "employer")
            .collect();
        let deduction_total = safe_add(
            deductions
                .iter()
                .map(|component| decimal_to_minor(&component.amount))
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        let employer_total = safe_add(
            employer
                .iter()
                .map(|component| decimal_to_minor(&component.amount))
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        if deduction_total != item.deductions_minor
            || employer_total != item.employer_cost_minor
            || item.net_minor != item.gross_minor - item.deductions_minor
        {
            return Err(invalid(
                "PAYROLL_TOTALS_INVALID",
                "Payroll item totals do not balance.",
            ));
        }
        let allocation = allocated_amounts(item, &snapshot, &components)?;
        for (index, unit) in allocation.units.iter().enumerate() {
            let mut dimensions = std::collections::BTreeMap::new();
            if let Some(branch) = &unit.branch_id {
                dimensions.insert("branchId".to_owned(), branch.clone());
            }
            if let Some(department) = &unit.department_id {
                dimensions.insert("departmentId".to_owned(), department.clone());
            }
            add(
                expense,
                "Payroll wages",
                allocation.wages[index],
                0,
                &dimensions,
                &mut grouped,
            );
            add(
                accounts
                    .overtime_expense_account_id
                    .as_deref()
                    .unwrap_or(expense),
                "Payroll overtime",
                allocation.overtime[index],
                0,
                &dimensions,
                &mut grouped,
            );
            for (component_index, component) in earnings.iter().enumerate() {
                let kind = category(&component.code, "earning");
                let ledger = match kind.as_str() {
                    "service_charge" => account(
                        accounts.service_charge_expense_account_id.as_deref(),
                        "service charge expense",
                    )?,
                    "bonus" => accounts
                        .bonus_expense_account_id
                        .as_deref()
                        .unwrap_or(expense),
                    "overtime" => accounts
                        .overtime_expense_account_id
                        .as_deref()
                        .unwrap_or(expense),
                    _ => expense,
                };
                add(
                    ledger,
                    &format!("Payroll {}", kind.replace('_', " ")),
                    allocation.earnings[component_index][index],
                    0,
                    &dimensions,
                    &mut grouped,
                );
            }
            for (component_index, component) in deductions.iter().enumerate() {
                let kind = category(&component.code, "deduction");
                let ledger = match kind.as_str() {
                    "tax" => account(accounts.tax_payable_account_id.as_deref(), "tax payable")?,
                    "social_security" => account(
                        accounts
                            .social_security_payable_account_id
                            .as_deref()
                            .or(accounts.employer_contribution_account_id.as_deref()),
                        "social security payable",
                    )?,
                    "advance" => account(
                        accounts.advance_receivable_account_id.as_deref(),
                        "advance receivable",
                    )?,
                    "benefit" => account(
                        accounts.benefit_payable_account_id.as_deref(),
                        "benefit payable",
                    )?,
                    "unmapped" => {
                        return Err(invalid(
                            "PAYROLL_COMPONENT_UNMAPPED",
                            "No accounting classification exists for this statutory component.",
                        ));
                    }
                    _ => account(
                        accounts.other_deduction_payable_account_id.as_deref(),
                        "other deduction payable",
                    )?,
                };
                add(
                    ledger,
                    &format!("Payroll {}", kind.replace('_', " ")),
                    0,
                    allocation.deductions[component_index][index],
                    &dimensions,
                    &mut grouped,
                );
            }
            for (component_index, component) in employer.iter().enumerate() {
                let kind = category(&component.code, "employer");
                let ledger = match kind.as_str() {
                    "social_security" => account(
                        accounts
                            .social_security_payable_account_id
                            .as_deref()
                            .or(accounts.employer_contribution_account_id.as_deref()),
                        "social security payable",
                    )?,
                    "benefit" => account(
                        accounts.benefit_payable_account_id.as_deref(),
                        "benefit payable",
                    )?,
                    _ => account(
                        accounts.employer_contribution_account_id.as_deref(),
                        "employer contribution payable",
                    )?,
                };
                add(
                    employer_expense,
                    &format!("Payroll employer {} expense", kind.replace('_', " ")),
                    allocation.employers[component_index][index],
                    0,
                    &dimensions,
                    &mut grouped,
                );
                add(
                    ledger,
                    &format!("Payroll {}", kind.replace('_', " ")),
                    0,
                    allocation.employers[component_index][index],
                    &dimensions,
                    &mut grouped,
                );
            }
            add(
                net_account,
                "Net payroll payable",
                0,
                allocation.net[index],
                &dimensions,
                &mut grouped,
            );
        }
        for component in &earnings {
            let kind = category(&component.code, "earning");
            *categories.entry(kind).or_insert(0) += decimal_to_minor(&component.amount)?;
        }
        for component in &deductions {
            let kind = category(&component.code, "deduction");
            *categories.entry(kind).or_insert(0) += decimal_to_minor(&component.amount)?;
        }
        for component in &employer {
            let kind = format!("employer_{}", category(&component.code, "employer"));
            *categories.entry(kind).or_insert(0) += decimal_to_minor(&component.amount)?;
        }
        totals.gross_minor = totals
            .gross_minor
            .checked_add(item.gross_minor)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
        totals.deductions_minor = totals
            .deductions_minor
            .checked_add(item.deductions_minor)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
        totals.employer_cost_minor = totals
            .employer_cost_minor
            .checked_add(item.employer_cost_minor)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
        totals.net_minor = totals
            .net_minor
            .checked_add(item.net_minor)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
    }
    let mut lines = Vec::new();
    let mut debit_total = 0_i64;
    let mut credit_total = 0_i64;
    for ((account_id, description, dimensions), (debit, credit)) in grouped {
        debit_total = debit_total
            .checked_add(debit)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
        credit_total = credit_total
            .checked_add(credit)
            .ok_or_else(|| invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision."))?;
        lines.push(PayrollJournalLine {
            account_id,
            description,
            debit: minor_to_decimal(debit),
            credit: minor_to_decimal(credit),
            dimensions: dimensions.into_iter().collect(),
        });
    }
    if totals.net_minor <= 0 || debit_total != credit_total {
        return Err(invalid(
            "PAYROLL_TOTALS_INVALID",
            "Payroll accrual debits and credits must balance.",
        ));
    }
    if lines.len() > 500 {
        return Err(invalid(
            "PAYROLL_JOURNAL_TOO_LARGE",
            "Split this run before posting more than 500 grouped ledger lines.",
        ));
    }
    Ok(PayrollAccrual {
        lines,
        totals,
        categories,
    })
}

#[derive(Clone, Debug)]
pub struct PayrollJournalRequest {
    pub actor: PayrollFinanceActor,
    pub draft: PayrollJournalDraft,
    pub key: String,
}

pub fn journal_draft(
    basis: &PayrollPostingBasis,
    resolved: &ResolvedPayrollPost,
    accrual: &PayrollAccrual,
) -> Result<PayrollJournalDraft, BusinessError> {
    let mut lines = Vec::with_capacity(accrual.lines.len());
    for line in &accrual.lines {
        line.validate()?;
        lines.push(line.clone());
    }
    let run = &basis.run;
    let memo = if resolved.entry_date > run.period_end {
        format!(
            "Payroll {} to {} · adjustment for closed work period",
            run.period_start, run.period_end
        )
    } else {
        format!("Payroll {} to {}", run.period_start, run.period_end)
    };
    Ok(PayrollJournalDraft {
        chart_id: resolved.chart_id.clone(),
        period_id: resolved.period_id.clone(),
        entry_date: resolved.entry_date.clone(),
        currency: run.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo,
        source_type: "payroll".to_owned(),
        source_id: Some(run.id.clone()),
        lines,
    })
}

pub fn journal_result_valid(result: &PayrollPreparedJournal) -> Result<(), BusinessError> {
    if result.id.is_empty()
        || result.number.is_empty()
        || result.status != "posted"
        || result.version < 1
        || result.fx_rate_micros < 1
        || decimal_to_minor(&result.debit).is_err()
        || decimal_to_minor(&result.credit).is_err()
    {
        return Err(unavailable(
            "PAYROLL_FINANCE_JOURNAL_INVALID",
            "The Finance journal port returned an invalid posting.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> PayrollRun {
        PayrollRun {
            id: "run".into(),
            company_id: "company".into(),
            pay_group_id: "group".into(),
            pay_group_currency: "THB".into(),
            pay_group_status: "active".into(),
            overtime_multiplier_bps: 15_000,
            proration_method: "none".into(),
            country_pack_id: None,
            period_start: "2026-10-01".into(),
            period_end: "2026-10-31".into(),
            pay_date: "2026-10-31".into(),
            currency: "THB".into(),
            mode: "preview".into(),
            status: "calculating".into(),
            revision: 1,
            totals_json: "{}".into(),
            input_digest: None,
            version: 1,
            created_by: "owner".into(),
            submitted_by: None,
            approved_by: None,
            posted_by: None,
            adjustment_of_run_id: None,
            scope_engagement_id: None,
        }
    }

    fn input() -> PayrollCalculationInput {
        PayrollCalculationInput {
            engagement: PayrollEngagement {
                id: "engagement".into(),
                version: 1,
                person_id: "person".into(),
                worker_type: "employee".into(),
                employment_type: "salaried".into(),
                start_date: "2026-01-01".into(),
                end_date: None,
                job_title: "Engineer".into(),
                branch_id: None,
                department_id: None,
            },
            payee: PayrollPayee {
                id: "person".into(),
                version: 1,
                legal_name: "Person".into(),
                preferred_name: None,
            },
            compensation: Some(PayrollCompensation {
                id: "compensation".into(),
                version: 1,
                base_amount_minor: 3_000_000,
                hourly_rate_minor: 20_000,
                daily_rate_minor: 0,
                standard_daily_hours: 8.0,
                currency: "THB".into(),
                pay_frequency: "monthly".into(),
                effective_from: "2026-01-01".into(),
                effective_to: None,
                status: "approved".into(),
                components_json: Some(
                    r#"[{"code":"BONUS","label":"Bonus","kind":"earning","amount":"1000.00","incomeTiming":"irregular","socialSecurityWage":true}]"#
                        .into(),
                ),
            }),
            time_entries: vec![PayrollTimeEntry {
                id: "time".into(),
                version: 1,
                work_date: "2026-10-01".into(),
                regular_hours: 160.0,
                overtime_hours: 0.0,
                cost_assignment_id: Some("assignment".into()),
                cost_branch_id: Some("branch".into()),
                cost_department_id: None,
            }],
            handoffs: Vec::new(),
            advances: Vec::new(),
            cost_assignments: vec![PayrollCostAssignment {
                id: "assignment".into(),
                version: 1,
                effective_from: "2026-01-01".into(),
                effective_to: None,
                branch_id: Some("branch".into()),
                department_id: None,
                evidence_status: "verified".into(),
            }],
            benefits: Vec::new(),
            thai_statutory: None,
        }
    }

    #[test]
    fn salaried_calculation_adds_components_and_retains_snapshot() {
        let item = calculate_payroll_item(&input(), &run()).unwrap();
        assert_eq!(item.gross_minor, 3_100_000);
        assert_eq!(item.net_minor, 3_100_000);
        let snapshot: Value = serde_json::from_str(&item.input_snapshot_json).unwrap();
        assert_eq!(snapshot["engagement"]["id"], "engagement");
        assert_eq!(snapshot["policy"]["overtimeMultiplierBps"], 15_000);
        let result: Value = serde_json::from_str(&item.result_json).unwrap();
        assert_eq!(
            result["explanation"]["components"][0]["amountMinor"],
            100_000
        );
    }

    #[test]
    fn thai_statutory_kernel_withholds_tax_and_sso() {
        let rules: ThaiPayrollRules = serde_json::from_value(serde_json::json!({
            "countryCode": "TH",
            "currency": "THB",
            "version": "TH-2026-10-02-r1",
            "effectiveFrom": "2026-01-01",
            "effectiveTo": "2026-12-31",
            "standardExpenseRateBps": 5000,
            "standardExpenseCapMinor": 10000000,
            "taxBands": [
                {"upToMinor": 15000000, "rateBps": 0},
                {"upToMinor": 30000000, "rateBps": 500},
                {"upToMinor": 50000000, "rateBps": 1000},
                {"upToMinor": 75000000, "rateBps": 1500},
                {"upToMinor": 100000000, "rateBps": 2000},
                {"upToMinor": 200000000, "rateBps": 2500},
                {"upToMinor": 500000000, "rateBps": 3000},
                {"upToMinor": null, "rateBps": 3500}
            ],
            "socialSecurity": {
                "employeeRateBps": 500,
                "employerRateBps": 500,
                "minimumWageBaseMinor": 165000,
                "maximumWageBaseMinor": 1750000,
                "roundingUnitMinor": 100
            }
        }))
        .unwrap();
        let input = ThaiPayrollCalculation {
            pay_date: "2026-10-31".into(),
            regular_taxable_minor: 3_000_000,
            irregular_taxable_minor: 0,
            irregular_paid_before_minor: Some(0),
            irregular_occurrences_in_year: Some(1),
            termination_severance_minor: Some(0),
            non_taxable_minor: 0,
            social_security_wage_minor: 3_000_000,
            social_security_eligible: true,
            social_security_exemption_evidence: None,
            other_employee_deductions_minor: 0,
            verified_annual_deductions_minor: 0,
            taxable_paid_before_minor: 0,
            tax_withheld_before_minor: 0,
            scheduled_pays_in_year: 12,
            last_pay_in_year: false,
            prior_income_complete: true,
        };
        let result = calculate_thai_payroll(&input, &rules).unwrap();
        assert_eq!(result.employee_sso_minor, 87_500);
        assert_eq!(result.employer_sso_minor, 87_500);
        assert!(result.tax_withheld_minor >= 0);
        assert_eq!(
            result.net_minor,
            result.gross_minor - result.tax_withheld_minor - result.employee_sso_minor
        );
    }

    #[test]
    fn thai_kernel_matches_the_synthetic_reference_values() {
        let rules: ThaiPayrollRules = serde_json::from_value(serde_json::json!({
            "countryCode": "TH",
            "currency": "THB",
            "version": "synthetic-test-only",
            "effectiveFrom": "2026-01-01",
            "effectiveTo": "2026-12-31",
            "standardExpenseRateBps": 0,
            "standardExpenseCapMinor": 0,
            "taxBands": [
                {"upToMinor": 10000, "rateBps": 0},
                {"upToMinor": null, "rateBps": 1000}
            ],
            "socialSecurity": {
                "employeeRateBps": 500,
                "employerRateBps": 500,
                "minimumWageBaseMinor": 0,
                "maximumWageBaseMinor": 20000,
                "roundingUnitMinor": 1
            }
        }))
        .unwrap();
        let result = calculate_thai_payroll(
            &ThaiPayrollCalculation {
                pay_date: "2026-09-30".into(),
                regular_taxable_minor: 10_000,
                irregular_taxable_minor: 1_000,
                irregular_paid_before_minor: Some(0),
                irregular_occurrences_in_year: Some(1),
                termination_severance_minor: Some(0),
                non_taxable_minor: 0,
                social_security_wage_minor: 11_000,
                social_security_eligible: true,
                social_security_exemption_evidence: None,
                other_employee_deductions_minor: 0,
                verified_annual_deductions_minor: 0,
                taxable_paid_before_minor: 80_000,
                tax_withheld_before_minor: 5_000,
                scheduled_pays_in_year: 12,
                last_pay_in_year: false,
                prior_income_complete: true,
            },
            &rules,
        )
        .unwrap();
        assert_eq!(result.gross_minor, 11_000);
        assert_eq!(result.tax_withheld_minor, 1_016);
        assert_eq!(result.employee_sso_minor, 550);
        assert_eq!(result.employer_sso_minor, 550);
        assert_eq!(result.net_minor, 9_434);
        assert_eq!(result.explanation.ordinary_annual_tax_minor, 11_000);
        assert_eq!(
            result.explanation.irregular_annual_tax_difference_minor,
            100
        );
    }

    #[test]
    fn cancellation_rejects_submitted_or_posted_rows() {
        let mut cancelled = run();
        cancelled.status = "preview".into();
        cancelled.submitted_by = Some("submitter".into());
        assert_eq!(
            cancellable(&cancelled).unwrap_err().code,
            "PAYROLL_CANCELLATION_UNAVAILABLE"
        );
        cancelled.submitted_by = None;
        assert!(cancellable(&cancelled).is_ok());
    }

    #[test]
    fn date_helpers_are_exact() {
        assert_eq!(date_add("2026-10-31", 1), "2026-11-01");
        assert_eq!(date_add("2026-02-28", 1), "2026-03-01");
        assert_eq!(date_add("2025-12-31", 1), "2026-01-01");
        assert!(valid_date("2026-10-31"));
        assert!(!valid_date("2026-02-30"));
    }
}
