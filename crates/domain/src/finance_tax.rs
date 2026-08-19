//! Thai statutory tax facts linked to immutable posted Finance journals.
//!
//! Tax facts are explicit source records.  Reports never infer tax from a
//! journal description or a client-provided total; a fact must identify an
//! immutable posted journal line and is then aggregated deterministically.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::CommandContext;

pub const THAI_TAX_CONTRACT_VERSION: u16 = 1;
pub const THAI_TAX_RECONCILIATION_CONTRACT_VERSION: u16 = 1;
pub const MAX_THAI_TAX_PERIOD_BYTES: usize = 7;
pub const MAX_THAI_TAX_IDENTIFIER_BYTES: usize = 255;
pub const MAX_THAI_TAX_RECONCILIATION_ISSUES: usize = 10_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ThaiTaxError {
    #[error("{0} cannot be nil")]
    NilIdentifier(&'static str),
    #[error("{0} is invalid")]
    InvalidField(&'static str),
    #[error("{0} is too long")]
    FieldTooLong(&'static str),
    #[error("tax amount arithmetic overflowed")]
    AmountOverflow,
    #[error("tax fact company or holding scope is invalid")]
    InvalidScope,
    #[error("tax period must be YYYY-MM")]
    InvalidPeriod,
    #[error("tax currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("tax base must be positive and tax must be non-negative")]
    InvalidAmount,
    #[error("VAT rate basis points must be between 0 and 10000")]
    InvalidRate,
    #[error("tax amount {actual} does not equal the calculated amount {expected}")]
    TaxAmountMismatch { expected: i64, actual: i64 },
    #[error("tax fact identity is duplicated")]
    DuplicateFact,
    #[error("tax facts do not share one report scope")]
    MixedReportScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThaiTaxKind {
    OutputVat,
    InputVat,
    WithholdingPayable,
}

impl ThaiTaxKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutputVat => "output_vat",
            Self::InputVat => "input_vat",
            Self::WithholdingPayable => "withholding_payable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxFact {
    pub fact_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub journal_id: Uuid,
    pub line_id: Uuid,
    pub tax_date: NaiveDate,
    pub tax_period: String,
    pub kind: ThaiTaxKind,
    pub currency: String,
    pub base_minor: i64,
    pub tax_minor: i64,
    pub tax_rate_basis_points: u32,
    pub tax_identifier: String,
    pub context: CommandContext,
    pub recorded_at: DateTime<Utc>,
}

impl ThaiTaxFact {
    /// Calculate statutory tax in minor currency units using deterministic
    /// half-up rounding.  The calculation is deliberately integer-only so a
    /// persisted fact cannot drift because of floating-point conversion.
    pub fn calculate_tax_minor(
        base_minor: i64,
        tax_rate_basis_points: u32,
    ) -> Result<i64, ThaiTaxError> {
        if base_minor <= 0 {
            return Err(ThaiTaxError::InvalidAmount);
        }
        if tax_rate_basis_points > 10_000 {
            return Err(ThaiTaxError::InvalidRate);
        }
        let numerator = i128::from(base_minor)
            .checked_mul(i128::from(tax_rate_basis_points))
            .and_then(|value| value.checked_add(5_000))
            .ok_or(ThaiTaxError::AmountOverflow)?;
        i64::try_from(numerator / 10_000).map_err(|_| ThaiTaxError::AmountOverflow)
    }

    pub fn validate(&self) -> Result<(), ThaiTaxError> {
        for (id, name) in [
            (self.fact_id, "fact_id"),
            (self.holding_id, "holding_id"),
            (self.company_id, "company_id"),
            (self.journal_id, "journal_id"),
            (self.line_id, "line_id"),
        ] {
            if id.is_nil() {
                return Err(ThaiTaxError::NilIdentifier(name));
            }
        }
        validate_tax_period(&self.tax_period)?;
        if self.tax_date.format("%Y-%m").to_string() != self.tax_period {
            return Err(ThaiTaxError::InvalidPeriod);
        }
        validate_tax_currency(&self.currency)?;
        if self.base_minor <= 0 || self.tax_minor < 0 {
            return Err(ThaiTaxError::InvalidAmount);
        }
        if self.tax_rate_basis_points > 10_000 {
            return Err(ThaiTaxError::InvalidRate);
        }
        let expected_tax_minor =
            Self::calculate_tax_minor(self.base_minor, self.tax_rate_basis_points)?;
        if self.tax_minor != expected_tax_minor {
            return Err(ThaiTaxError::TaxAmountMismatch {
                expected: expected_tax_minor,
                actual: self.tax_minor,
            });
        }
        if self.tax_identifier.trim().is_empty()
            || self.tax_identifier.len() > MAX_THAI_TAX_IDENTIFIER_BYTES
            || self.tax_identifier.chars().any(char::is_control)
        {
            return Err(
                if self.tax_identifier.len() > MAX_THAI_TAX_IDENTIFIER_BYTES {
                    ThaiTaxError::FieldTooLong("tax_identifier")
                } else {
                    ThaiTaxError::InvalidField("tax_identifier")
                },
            );
        }
        self.context
            .validate()
            .map_err(|_| ThaiTaxError::InvalidScope)?;
        if !self
            .context
            .active_scope
            .contains(&crate::contracts::AuthorityScope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            })
        {
            return Err(ThaiTaxError::InvalidScope);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxReport {
    pub contract_version: u16,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub tax_period: String,
    pub currency: String,
    pub fact_count: u32,
    pub output_vat_base_minor: i64,
    pub output_vat_minor: i64,
    pub input_vat_base_minor: i64,
    pub input_vat_minor: i64,
    pub withholding_base_minor: i64,
    pub withholding_minor: i64,
    pub facts: Vec<ThaiTaxFact>,
}

/// The report totals are kept separate from the fact list so reconciliation
/// evidence can be consumed without copying command context or tax
/// identifiers into an operational status payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxReportTotals {
    pub output_vat_base_minor: i64,
    pub output_vat_minor: i64,
    pub input_vat_base_minor: i64,
    pub input_vat_minor: i64,
    pub withholding_base_minor: i64,
    pub withholding_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxFactProjectionIdentity {
    pub fact_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub journal_id: Uuid,
    pub line_id: Uuid,
    pub tax_date: NaiveDate,
    pub tax_period: String,
    pub kind: ThaiTaxKind,
    pub currency: String,
    pub base_minor: i64,
    pub tax_minor: i64,
    pub tax_rate_basis_points: u32,
    pub tax_identifier: String,
    pub recorded_at: DateTime<Utc>,
}

impl From<&ThaiTaxFact> for ThaiTaxFactProjectionIdentity {
    fn from(fact: &ThaiTaxFact) -> Self {
        Self {
            fact_id: fact.fact_id,
            holding_id: fact.holding_id,
            company_id: fact.company_id,
            journal_id: fact.journal_id,
            line_id: fact.line_id,
            tax_date: fact.tax_date,
            tax_period: fact.tax_period.clone(),
            kind: fact.kind,
            currency: fact.currency.clone(),
            base_minor: fact.base_minor,
            tax_minor: fact.tax_minor,
            tax_rate_basis_points: fact.tax_rate_basis_points,
            tax_identifier: fact.tax_identifier.clone(),
            recorded_at: fact.recorded_at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThaiTaxReconciliationIssueKind {
    MissingProjectedFact,
    UnexpectedProjectedFact,
    FactMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxReconciliationIssue {
    pub fact_id: Uuid,
    pub kind: ThaiTaxReconciliationIssueKind,
}

/// Bounded evidence that a statutory report was derived from the same
/// authoritative facts as its disposable read model. The source-stability
/// fence is deliberately separate from fact equality: a concurrent append
/// must keep the report non-green even if the append is outside this report's
/// company/period scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThaiTaxStatutoryReconciliationReport {
    pub contract_version: u16,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub tax_period: String,
    pub currency: String,
    pub source_fact_count: u64,
    pub projected_fact_count: u64,
    pub source_totals: ThaiTaxReportTotals,
    pub projected_totals: ThaiTaxReportTotals,
    pub source_stable: bool,
    pub issues_truncated: bool,
    pub issues: Vec<ThaiTaxReconciliationIssue>,
}

impl ThaiTaxStatutoryReconciliationReport {
    pub fn from_fact_sets(
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: impl Into<String>,
        currency: impl Into<String>,
        source_facts: Vec<ThaiTaxFact>,
        projected_facts: Vec<ThaiTaxFact>,
        source_stable: bool,
    ) -> Result<Self, ThaiTaxError> {
        let source_report =
            ThaiTaxReport::from_facts(holding_id, company_id, tax_period, currency, source_facts)?;
        let projected_report = ThaiTaxReport::from_facts(
            holding_id,
            company_id,
            source_report.tax_period.clone(),
            source_report.currency.clone(),
            projected_facts,
        )?;
        let source_by_id = source_report
            .facts
            .iter()
            .map(|fact| (fact.fact_id, ThaiTaxFactProjectionIdentity::from(fact)))
            .collect::<BTreeMap<_, _>>();
        let projected_by_id = projected_report
            .facts
            .iter()
            .map(|fact| (fact.fact_id, ThaiTaxFactProjectionIdentity::from(fact)))
            .collect::<BTreeMap<_, _>>();

        let mut issues = Vec::new();
        let mut issues_truncated = false;
        for (fact_id, source) in &source_by_id {
            let kind = match projected_by_id.get(fact_id) {
                None => Some(ThaiTaxReconciliationIssueKind::MissingProjectedFact),
                Some(projected) if projected != source => {
                    Some(ThaiTaxReconciliationIssueKind::FactMismatch)
                }
                Some(_) => None,
            };
            if let Some(kind) = kind {
                push_reconciliation_issue(
                    &mut issues,
                    &mut issues_truncated,
                    ThaiTaxReconciliationIssue {
                        fact_id: *fact_id,
                        kind,
                    },
                );
            }
        }
        for fact_id in projected_by_id.keys() {
            if !source_by_id.contains_key(fact_id) {
                push_reconciliation_issue(
                    &mut issues,
                    &mut issues_truncated,
                    ThaiTaxReconciliationIssue {
                        fact_id: *fact_id,
                        kind: ThaiTaxReconciliationIssueKind::UnexpectedProjectedFact,
                    },
                );
            }
        }

        Ok(Self {
            contract_version: THAI_TAX_RECONCILIATION_CONTRACT_VERSION,
            holding_id,
            company_id,
            tax_period: source_report.tax_period.clone(),
            currency: source_report.currency.clone(),
            source_fact_count: u64::from(source_report.fact_count),
            projected_fact_count: u64::from(projected_report.fact_count),
            source_totals: source_report.totals(),
            projected_totals: projected_report.totals(),
            source_stable,
            issues_truncated,
            issues,
        })
    }

    pub fn is_reconciled(&self) -> bool {
        self.source_stable && !self.issues_truncated && self.issues.is_empty()
    }
}

fn push_reconciliation_issue(
    issues: &mut Vec<ThaiTaxReconciliationIssue>,
    issues_truncated: &mut bool,
    issue: ThaiTaxReconciliationIssue,
) {
    if issues.len() < MAX_THAI_TAX_RECONCILIATION_ISSUES {
        issues.push(issue);
    } else {
        *issues_truncated = true;
    }
}

impl ThaiTaxReport {
    pub fn from_facts(
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: impl Into<String>,
        currency: impl Into<String>,
        mut facts: Vec<ThaiTaxFact>,
    ) -> Result<Self, ThaiTaxError> {
        let tax_period = tax_period.into();
        let currency = currency.into();
        if holding_id.is_nil() {
            return Err(ThaiTaxError::NilIdentifier("holding_id"));
        }
        if company_id.is_nil() {
            return Err(ThaiTaxError::NilIdentifier("company_id"));
        }
        validate_tax_period(&tax_period)?;
        validate_tax_currency(&currency)?;
        let mut seen = BTreeMap::new();
        let mut totals = [0_i64; 6];
        for fact in &facts {
            fact.validate()?;
            if fact.holding_id != holding_id
                || fact.company_id != company_id
                || fact.tax_period != tax_period
                || fact.currency != currency
            {
                return Err(ThaiTaxError::MixedReportScope);
            }
            if seen.insert(fact.fact_id, ()).is_some() {
                return Err(ThaiTaxError::DuplicateFact);
            }
            let (base_index, tax_index) = match fact.kind {
                ThaiTaxKind::OutputVat => (0, 1),
                ThaiTaxKind::InputVat => (2, 3),
                ThaiTaxKind::WithholdingPayable => (4, 5),
            };
            totals[base_index] = totals[base_index]
                .checked_add(fact.base_minor)
                .ok_or(ThaiTaxError::AmountOverflow)?;
            totals[tax_index] = totals[tax_index]
                .checked_add(fact.tax_minor)
                .ok_or(ThaiTaxError::AmountOverflow)?;
        }
        facts.sort_by_key(|fact| (fact.tax_date, fact.fact_id));
        Ok(Self {
            contract_version: THAI_TAX_CONTRACT_VERSION,
            holding_id,
            company_id,
            tax_period,
            currency,
            fact_count: u32::try_from(facts.len()).map_err(|_| ThaiTaxError::AmountOverflow)?,
            output_vat_base_minor: totals[0],
            output_vat_minor: totals[1],
            input_vat_base_minor: totals[2],
            input_vat_minor: totals[3],
            withholding_base_minor: totals[4],
            withholding_minor: totals[5],
            facts,
        })
    }

    pub fn totals(&self) -> ThaiTaxReportTotals {
        ThaiTaxReportTotals {
            output_vat_base_minor: self.output_vat_base_minor,
            output_vat_minor: self.output_vat_minor,
            input_vat_base_minor: self.input_vat_base_minor,
            input_vat_minor: self.input_vat_minor,
            withholding_base_minor: self.withholding_base_minor,
            withholding_minor: self.withholding_minor,
        }
    }
}

fn validate_tax_period(value: &str) -> Result<(), ThaiTaxError> {
    if value.len() != MAX_THAI_TAX_PERIOD_BYTES
        || value.as_bytes().get(4) != Some(&b'-')
        || !value.chars().enumerate().all(|(index, character)| {
            (index == 4 && character == '-') || (index != 4 && character.is_ascii_digit())
        })
    {
        return Err(ThaiTaxError::InvalidPeriod);
    }
    let year = value[..4]
        .parse::<i32>()
        .map_err(|_| ThaiTaxError::InvalidPeriod)?;
    let month = value[5..]
        .parse::<u32>()
        .map_err(|_| ThaiTaxError::InvalidPeriod)?;
    if NaiveDate::from_ymd_opt(year, month, 1).is_none() {
        return Err(ThaiTaxError::InvalidPeriod);
    }
    Ok(())
}

fn validate_tax_currency(value: &str) -> Result<(), ThaiTaxError> {
    if value.len() != 3
        || !value
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(ThaiTaxError::InvalidCurrency);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind, AuthorityScope};

    fn context(holding_id: Uuid, company_id: Uuid) -> CommandContext {
        CommandContext::try_new(
            "thai-tax-test-idempotency",
            ActorAttribution {
                principal_ref: "finance-tax-test".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "thai_tax_fact_test",
        )
        .unwrap()
    }

    fn fact(kind: ThaiTaxKind) -> ThaiTaxFact {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        ThaiTaxFact {
            fact_id: Uuid::new_v4(),
            holding_id,
            company_id,
            journal_id: Uuid::new_v4(),
            line_id: Uuid::new_v4(),
            tax_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            tax_period: "2026-07".into(),
            kind,
            currency: "THB".into(),
            base_minor: 100_000,
            tax_minor: 7_000,
            tax_rate_basis_points: 700,
            tax_identifier: "tax-invoice-001".into(),
            context: context(holding_id, company_id),
            recorded_at: Utc::now(),
        }
    }

    #[test]
    fn report_aggregates_explicit_facts() {
        let output = fact(ThaiTaxKind::OutputVat);
        let mut input = fact(ThaiTaxKind::InputVat);
        input.holding_id = output.holding_id;
        input.company_id = output.company_id;
        input.context = context(output.holding_id, output.company_id);
        let report = ThaiTaxReport::from_facts(
            output.holding_id,
            output.company_id,
            "2026-07",
            "THB",
            vec![output, input],
        )
        .unwrap();
        assert_eq!(report.fact_count, 2);
        assert_eq!(report.output_vat_minor, 7_000);
        assert_eq!(report.input_vat_minor, 7_000);
    }

    #[test]
    fn report_rejects_mixed_scope() {
        let output = fact(ThaiTaxKind::OutputVat);
        let mut other = fact(ThaiTaxKind::InputVat);
        other.context = context(other.holding_id, other.company_id);
        let result = ThaiTaxReport::from_facts(
            output.holding_id,
            output.company_id,
            "2026-07",
            "THB",
            vec![output, other],
        );
        assert_eq!(result, Err(ThaiTaxError::MixedReportScope));
    }

    #[test]
    fn tax_amount_is_derived_with_integer_half_up_rounding() {
        assert_eq!(ThaiTaxFact::calculate_tax_minor(1, 5_000), Ok(1));
        assert_eq!(ThaiTaxFact::calculate_tax_minor(1, 4_999), Ok(0));

        let mut invalid = fact(ThaiTaxKind::OutputVat);
        invalid.tax_minor = 6_999;
        assert_eq!(
            invalid.validate(),
            Err(ThaiTaxError::TaxAmountMismatch {
                expected: 7_000,
                actual: 6_999,
            })
        );
    }

    #[test]
    fn statutory_reconciliation_detects_missing_and_changed_facts() {
        let source = fact(ThaiTaxKind::OutputVat);
        let green = ThaiTaxStatutoryReconciliationReport::from_fact_sets(
            source.holding_id,
            source.company_id,
            "2026-07",
            "THB",
            vec![source.clone()],
            vec![source.clone()],
            true,
        )
        .expect("valid matching reconciliation inputs");
        assert!(green.is_reconciled());

        let mut projected = source.clone();
        projected.tax_identifier = "changed-tax-invoice".into();
        let mismatch = ThaiTaxStatutoryReconciliationReport::from_fact_sets(
            source.holding_id,
            source.company_id,
            "2026-07",
            "THB",
            vec![source.clone()],
            vec![projected],
            true,
        )
        .expect("valid reconciliation inputs");
        assert!(!mismatch.is_reconciled());
        assert_eq!(mismatch.issues[0].fact_id, source.fact_id);
        assert_eq!(
            mismatch.issues[0].kind,
            ThaiTaxReconciliationIssueKind::FactMismatch
        );

        let missing = ThaiTaxStatutoryReconciliationReport::from_fact_sets(
            source.holding_id,
            source.company_id,
            "2026-07",
            "THB",
            vec![source],
            Vec::new(),
            true,
        )
        .expect("valid missing-fact inputs");
        assert!(!missing.is_reconciled());
        assert_eq!(
            missing.issues[0].kind,
            ThaiTaxReconciliationIssueKind::MissingProjectedFact
        );
    }

    #[test]
    fn statutory_reconciliation_requires_a_stable_source_fence() {
        let source = fact(ThaiTaxKind::OutputVat);
        let report = ThaiTaxStatutoryReconciliationReport::from_fact_sets(
            source.holding_id,
            source.company_id,
            "2026-07",
            "THB",
            vec![source.clone()],
            vec![source],
            false,
        )
        .expect("valid reconciliation inputs");
        assert!(!report.is_reconciled());
        assert!(report.issues.is_empty());
    }

    #[test]
    fn empty_reports_require_a_real_company_scope() {
        assert_eq!(
            ThaiTaxReport::from_facts(Uuid::nil(), Uuid::new_v4(), "2026-07", "THB", Vec::new(),),
            Err(ThaiTaxError::NilIdentifier("holding_id"))
        );
        assert_eq!(
            ThaiTaxReport::from_facts(Uuid::new_v4(), Uuid::nil(), "2026-07", "THB", Vec::new(),),
            Err(ThaiTaxError::NilIdentifier("company_id"))
        );
    }
}
