//! Native Payroll summary and run-list read facts.
//!
//! These are owner read projections only. They are not an authorization proof,
//! purchase admission or command capability. The composition backend builds an
//! actor from the retained Account/Holding authority and the owner read still
//! enforces the Company payroll scope, selected-context expiry and permissions.
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use serde::Serialize;

const SUMMARY_LIMIT: usize = 120;
const RUN_LIMIT: usize = 2_000;

#[derive(Clone, Debug)]
pub struct PayrollReadActor {
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl PayrollReadActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn can_read_payroll(&self) -> bool {
        self.has("payroll.read")
    }

    fn require_permission(&self) -> Result<(), BusinessError> {
        if !self.can_read_payroll() && !self.has("payroll.payment.read") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant payroll read access.",
            ));
        }
        Ok(())
    }

    fn require_company_scope(&self) -> Result<(), BusinessError> {
        if self.branch_id.is_some() || self.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "COMPANY_PAYROLL_SCOPE_REQUIRED",
                "Company-wide payroll requires a holding-level assignment.",
            ));
        }
        Ok(())
    }

    fn require_current(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    pub fn require_summary(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_read_payroll() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant payroll read access.",
            ));
        }
        self.require_company_scope()?;
        self.require_current(now)
    }

    pub fn require_runs(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_permission()?;
        self.require_company_scope()?;
        self.require_current(now)
    }

    /// Work rows feed the retained Workflow Work selector, which reads payroll
    /// only for holders of the preparation read permission.
    pub fn require_work_rows(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if !self.can_read_payroll() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant payroll read access.",
            ));
        }
        self.require_company_scope()?;
        self.require_current(now)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayrollMonth(String);

impl PayrollMonth {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        let bytes = value.as_bytes();
        if bytes.len() != 7
            || bytes[4] != b'-'
            || !bytes[..4].iter().all(u8::is_ascii_digit)
            || !bytes[5..].iter().all(u8::is_ascii_digit)
        {
            return Err(invalid_month());
        }
        let month = i32::from(bytes[5] - b'0') * 10 + i32::from(bytes[6] - b'0');
        if !(1..=12).contains(&month) {
            return Err(invalid_month());
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn invalid_month() -> BusinessError {
    BusinessError::invalid(
        "INVALID_PAY_MONTH",
        "Use a calendar payroll month in YYYY-MM form.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PAYROLL_READ_SOURCE_INVALID",
        "The current payroll read source is invalid.",
    )
}

fn validate_month(value: &str) -> Result<(), BusinessError> {
    PayrollMonth::parse(value).map(|_| ())
}

fn validate_currency(value: &str) -> Result<(), BusinessError> {
    if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(source_invalid())
    }
}

fn validate_date(value: &str) -> Result<(), BusinessError> {
    let bytes = value.as_bytes();
    if bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..].iter().all(u8::is_ascii_digit)
    {
        Ok(())
    } else {
        Err(source_invalid())
    }
}

fn validate_identifier(value: &str) -> Result<(), BusinessError> {
    RecordId::new(value)
        .map(|_| ())
        .map_err(|_| source_invalid())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayrollSummaryRow {
    #[serde(skip_serializing)]
    pub company_id: String,
    pub pay_month: String,
    pub currency: String,
    pub run_count: i64,
    pub headcount: i64,
    pub gross_minor: i64,
    pub deductions_minor: i64,
    pub employer_cost_minor: i64,
    pub net_minor: i64,
}

impl PayrollSummaryRow {
    pub fn validate(&self, company: &CompanyId) -> Result<(), BusinessError> {
        if self.company_id != company.as_str()
            || validate_month(&self.pay_month).is_err()
            || validate_currency(&self.currency).is_err()
            || self.run_count < 1
            || self.headcount < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayrollRunRow {
    #[serde(skip_serializing)]
    pub company_id: String,
    pub id: String,
    /// Selected pay group. Only the full run list projects it; the payment-only
    /// reader leaves it absent so a payment reader cannot group payroll.
    pub pay_group_id: Option<String>,
    pub period_start: String,
    pub period_end: String,
    pub pay_date: String,
    pub currency: String,
    pub status: String,
    pub mode: String,
    pub revision: i64,
    pub version: i64,
    pub totals_json: Option<String>,
    pub payment_batch_id: Option<String>,
    pub payment_status: Option<String>,
    pub net_payable_minor: Option<i64>,
}

impl PayrollRunRow {
    pub fn validate(&self, company: &CompanyId) -> Result<(), BusinessError> {
        if self.company_id != company.as_str()
            || validate_identifier(&self.id).is_err()
            || validate_date(&self.period_start).is_err()
            || validate_date(&self.period_end).is_err()
            || validate_date(&self.pay_date).is_err()
            || validate_currency(&self.currency).is_err()
            || validate_identifier(&self.status).is_err()
            || validate_identifier(&self.mode).is_err()
            || self.revision < 1
            || self.version < 1
            || self.period_end < self.period_start
            || self.net_payable_minor.is_some_and(|value| value < 0)
        {
            return Err(source_invalid());
        }
        if let Some(totals) = &self.totals_json {
            let value: serde_json::Value =
                serde_json::from_str(totals).map_err(|_| source_invalid())?;
            if !value.is_object() {
                return Err(source_invalid());
            }
        }
        if let Some(pay_group_id) = &self.pay_group_id {
            validate_identifier(pay_group_id)?;
        }
        if let Some(batch) = &self.payment_batch_id {
            validate_identifier(batch)?;
        }
        if let Some(status) = &self.payment_status {
            validate_identifier(status)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_summary_rows(
    company: &CompanyId,
    rows: Vec<PayrollSummaryRow>,
) -> Result<Vec<PayrollSummaryRow>, BusinessError> {
    if rows.len() > SUMMARY_LIMIT {
        return Err(source_invalid());
    }
    let mut previous: Option<(&str, &str)> = None;
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        row.validate(company)?;
        if !seen.insert((row.pay_month.as_str(), row.currency.as_str())) {
            return Err(source_invalid());
        }
        if let Some((month, currency)) = previous
            && (month < row.pay_month.as_str()
                || (month == row.pay_month.as_str() && currency >= row.currency.as_str()))
        {
            return Err(source_invalid());
        }
        previous = Some((row.pay_month.as_str(), row.currency.as_str()));
    }
    Ok(rows)
}

pub(crate) fn validate_run_rows(
    company: &CompanyId,
    rows: Vec<PayrollRunRow>,
) -> Result<Vec<PayrollRunRow>, BusinessError> {
    if rows.len() > RUN_LIMIT {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &rows {
        row.validate(company)?;
        if !seen.insert(row.id.as_str()) {
            return Err(source_invalid());
        }
    }
    Ok(rows)
}

/// Payroll run facts required by the retained Workflow Work selector: the
/// owner row identity, its pay group, the period title, the creating actor and
/// the authoritative `updated_at` used for recency and own-item checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PayrollWorkRow {
    #[serde(skip_serializing)]
    pub company_id: String,
    pub id: String,
    pub pay_group_id: String,
    pub pay_date: String,
    pub status: String,
    pub mode: String,
    pub created_by: String,
    pub updated_at: i64,
}

impl PayrollWorkRow {
    pub fn validate(&self, company: &CompanyId) -> Result<(), BusinessError> {
        if self.company_id != company.as_str()
            || validate_identifier(&self.id).is_err()
            || validate_identifier(&self.pay_group_id).is_err()
            || validate_date(&self.pay_date).is_err()
            || validate_identifier(&self.status).is_err()
            || validate_identifier(&self.mode).is_err()
            || validate_identifier(&self.created_by).is_err()
            || self.updated_at < 0
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

pub(crate) fn validate_work_rows(
    company: &CompanyId,
    rows: Vec<PayrollWorkRow>,
) -> Result<Vec<PayrollWorkRow>, BusinessError> {
    if rows.len() > RUN_LIMIT {
        return Err(source_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut previous: Option<i64> = None;
    for row in &rows {
        row.validate(company)?;
        if !seen.insert(row.id.as_str()) {
            return Err(source_invalid());
        }
        if let Some(updated_at) = previous
            && updated_at < row.updated_at
        {
            return Err(source_invalid());
        }
        previous = Some(row.updated_at);
    }
    Ok(rows)
}
