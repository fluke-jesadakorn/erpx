//! Released Benefits payroll-source facts for the People payroll consumer.
//!
//! These are owner read projections only. The caller keeps its serializable
//! Holding transaction and its own People payroll authority; this domain
//! enforces current Company scope, the exact engagement/currency/period
//! selection, released-input shape, deterministic ordering and a bounded page.
use crate::domain::workspace::{
    js_trim, source_invalid, valid_currency, valid_date, valid_identifier, valid_name, valid_text,
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use serde::Serialize;

pub const PAYROLL_SOURCE_LIMIT: usize = 1_000;
const PLAN_NAME_LIMIT: usize = 160;

fn source_too_large() -> BusinessError {
    BusinessError::invalid(
        "BENEFITS_PAYROLL_SOURCE_TOO_LARGE",
        "Narrow the payroll period or engagement before reading Benefits payroll inputs.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitsPayrollScope {
    company_id: CompanyId,
}

impl BenefitsPayrollScope {
    pub fn new(company_id: CompanyId) -> Self {
        Self { company_id }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company_id
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_identifier(self.company_id.as_str()) {
            return Err(source_invalid());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenefitsPayrollQuery {
    pub engagement_id: String,
    pub currency: String,
    pub period_start: String,
    pub period_end: String,
}

impl BenefitsPayrollQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        let currency = js_trim(&self.currency);
        if !valid_identifier(&self.engagement_id)
            || currency != self.currency
            || !valid_currency(&self.currency)
            || !valid_date(&self.period_start)
            || !valid_date(&self.period_end)
            || self.period_start > self.period_end
        {
            return Err(BusinessError::invalid(
                "BENEFITS_PAYROLL_SOURCE_INVALID",
                "Use one current engagement, currency and payroll period for Benefits payroll inputs.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenefitPayrollInputRow {
    pub id: String,
    pub election_id: String,
    pub company_id: String,
    pub person_id: String,
    pub engagement_id: String,
    pub currency: String,
    pub employee_deduction_minor: i64,
    pub employer_cost_minor: i64,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub source_election_version: i64,
    pub status: String,
    pub released_by: String,
    pub created_at: i64,
    pub plan_name: String,
}

impl BenefitPayrollInputRow {
    pub fn validate(
        &self,
        scope: &BenefitsPayrollScope,
        query: &BenefitsPayrollQuery,
    ) -> Result<(), BusinessError> {
        if self.company_id != scope.company_id.as_str()
            || self.engagement_id != query.engagement_id
            || self.currency != query.currency
            || !valid_identifier(&self.id)
            || !valid_identifier(&self.election_id)
            || !valid_identifier(&self.person_id)
            || !valid_identifier(&self.engagement_id)
            || !valid_currency(&self.currency)
            || self.employee_deduction_minor < 0
            || self.employer_cost_minor < 0
            || !valid_date(&self.effective_from)
            || self.effective_from.as_str() > query.period_end.as_str()
            || self
                .effective_to
                .as_deref()
                .is_some_and(|value| !valid_date(value) || value < self.effective_from.as_str())
            || self
                .effective_to
                .as_deref()
                .is_some_and(|value| value < query.period_start.as_str())
            || self.status != "released"
            || self.source_election_version < 1
            || !valid_identifier(&self.released_by)
            || self.created_at < 0
            || !valid_name(&self.plan_name, PLAN_NAME_LIMIT)
            || self.plan_name.is_empty()
            || !valid_text(&self.plan_name, PLAN_NAME_LIMIT)
        {
            return Err(source_invalid());
        }
        Ok(())
    }
}

pub fn released_payroll_inputs(
    scope: &BenefitsPayrollScope,
    query: &BenefitsPayrollQuery,
    rows: Vec<BenefitPayrollInputRow>,
) -> Result<Vec<BenefitPayrollInputRow>, BusinessError> {
    scope.validate()?;
    query.validate()?;
    if rows.len() > PAYROLL_SOURCE_LIMIT {
        return Err(source_too_large());
    }
    let mut previous: Option<&str> = None;
    for row in &rows {
        row.validate(scope, query)?;
        if previous.is_some_and(|value| value >= row.id.as_str()) {
            return Err(source_invalid());
        }
        previous = Some(row.id.as_str());
    }
    Ok(rows)
}

/// Retained payroll-readiness guard 29: the exact released-input count for the
/// engagement/currency/period selection.
pub fn released_payroll_count(rows: &[BenefitPayrollInputRow]) -> i64 {
    rows.len() as i64
}

/// Retained payroll-readiness guard 30: the exact released row identity and
/// employer/employee money facts must still be present.
pub fn released_payroll_matches(
    rows: &[BenefitPayrollInputRow],
    id: &str,
    engagement_id: &str,
    employee_deduction_minor: i64,
    employer_cost_minor: i64,
) -> bool {
    rows.iter().any(|row| {
        row.id == id
            && row.engagement_id == engagement_id
            && row.status == "released"
            && row.employee_deduction_minor == employee_deduction_minor
            && row.employer_cost_minor == employer_cost_minor
    })
}
