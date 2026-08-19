//! Finance-owned accounting plans and final posting packets for Sales facts.
//!
//! A plan is non-posting evidence. Actual Invoice/AR/GL/Revenue instructions
//! become available only after a separate human approval of a PostingPacket.

use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{BoundedContext, DimensionSet, RecordRef};
use erpx_kernel::{AuthorityScope, EventId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::AccountId;

macro_rules! posting_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, SalesPostingError> {
                if value.is_nil() {
                    return Err(SalesPostingError::InvalidIdentifier);
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

posting_id!(AccountingPlanId);
posting_id!(PostingPacketId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingPlanStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingPacketStatus {
    Draft,
    Submitted,
    Approved,
    Rejected,
    Posted,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceTaxClassification {
    StandardVat,
    ZeroRated,
    Exempt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceTaxProfileSnapshot {
    pub profile_id: Uuid,
    pub version: u32,
    pub classification: FinanceTaxClassification,
    pub rate_basis_points: u16,
    pub authority_ref: String,
}

impl FinanceTaxProfileSnapshot {
    pub fn validate(&self) -> Result<(), SalesPostingError> {
        let valid_rate = match self.classification {
            FinanceTaxClassification::StandardVat => self.rate_basis_points > 0,
            FinanceTaxClassification::ZeroRated | FinanceTaxClassification::Exempt => {
                self.rate_basis_points == 0
            }
        };
        if self.profile_id.is_nil()
            || self.version == 0
            || self.rate_basis_points > 10_000
            || !valid_rate
            || !valid_text(&self.authority_ref, 256)
        {
            return Err(SalesPostingError::InvalidTaxProfile);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesAccountRoles {
    pub accounts_receivable: AccountId,
    pub output_vat: AccountId,
    pub contract_liability: AccountId,
    pub revenue: AccountId,
}

impl SalesAccountRoles {
    fn validate(&self) -> Result<(), SalesPostingError> {
        let accounts = self.all();
        if accounts
            .iter()
            .enumerate()
            .any(|(index, account)| accounts[..index].contains(account))
        {
            return Err(SalesPostingError::DuplicateAccountRole);
        }
        Ok(())
    }

    fn all(&self) -> [AccountId; 4] {
        [
            self.accounts_receivable,
            self.output_vat,
            self.contract_liability,
            self.revenue,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingObligation {
    pub obligation_id: Uuid,
    pub allocated_minor: i64,
    pub template_code: String,
}

impl AccountingObligation {
    fn validate(&self) -> Result<(), SalesPostingError> {
        if self.obligation_id.is_nil()
            || self.allocated_minor <= 0
            || !valid_text(&self.template_code, 128)
        {
            return Err(SalesPostingError::InvalidObligation);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingPlanSuggestionProvenance {
    pub suggestion_id: Uuid,
    pub payload_digest: String,
    pub output_digest: String,
    pub provider_label: String,
}

/// Typed, non-posting output for `finance.accounting-plan.v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingPlanSuggestionPayload {
    pub currency: String,
    pub quote_net_minor: i64,
    pub quote_tax_minor: i64,
    pub tax_profile: FinanceTaxProfileSnapshot,
    pub account_roles: SalesAccountRoles,
    pub obligations: Vec<AccountingObligation>,
    pub rationale: String,
}

impl AccountingPlanSuggestionPayload {
    pub fn validate(&self) -> Result<(), SalesPostingError> {
        self.tax_profile.validate()?;
        self.account_roles.validate()?;
        if self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || self.quote_net_minor <= 0
            || self.quote_tax_minor < 0
            || self.obligations.is_empty()
            || self.obligations.len() > 1_000
            || !valid_text(&self.rationale, 2_048)
        {
            return Err(SalesPostingError::InvalidPlanSuggestion);
        }
        let mut ids = BTreeSet::new();
        let allocated = self.obligations.iter().try_fold(0_i64, |total, item| {
            item.validate()?;
            if !ids.insert(item.obligation_id) {
                return Err(SalesPostingError::InvalidPlanSuggestion);
            }
            total
                .checked_add(item.allocated_minor)
                .ok_or(SalesPostingError::ArithmeticOverflow)
        })?;
        if allocated != self.quote_net_minor {
            return Err(SalesPostingError::AllocationMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingReviewRecommendation {
    ReadyForHumanApproval,
    RevisePlan,
    Investigate,
}

/// Typed, advisory-only output for `finance.posting-review.v1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingReviewSuggestionPayload {
    pub recommendation: PostingReviewRecommendation,
    pub anomaly_codes: Vec<String>,
    pub rationale: String,
}

impl PostingReviewSuggestionPayload {
    pub fn validate(&self) -> Result<(), SalesPostingError> {
        if self.anomaly_codes.len() > 128
            || self.anomaly_codes.iter().any(|code| !valid_text(code, 128))
            || !valid_text(&self.rationale, 2_048)
        {
            return Err(SalesPostingError::InvalidPostingReviewSuggestion);
        }
        Ok(())
    }
}

impl AccountingPlanSuggestionProvenance {
    fn validate(&self) -> Result<(), SalesPostingError> {
        if self.suggestion_id.is_nil()
            || !valid_digest(&self.payload_digest)
            || !valid_digest(&self.output_digest)
            || !valid_text(&self.provider_label, 128)
        {
            return Err(SalesPostingError::InvalidSuggestionProvenance);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingPlan {
    id: AccountingPlanId,
    version: u64,
    scope: AuthorityScope,
    quote: RecordRef,
    currency: String,
    quote_net_minor: i64,
    quote_tax_minor: i64,
    dimensions: DimensionSet,
    tax_profile: FinanceTaxProfileSnapshot,
    account_roles: SalesAccountRoles,
    obligations: Vec<AccountingObligation>,
    preparer: String,
    sales_approver: String,
    finance_approver: Option<String>,
    suggestion: Option<AccountingPlanSuggestionProvenance>,
    status: AccountingPlanStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AccountingPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: AccountingPlanId,
        scope: AuthorityScope,
        quote: RecordRef,
        currency: String,
        quote_net_minor: i64,
        quote_tax_minor: i64,
        dimensions: DimensionSet,
        tax_profile: FinanceTaxProfileSnapshot,
        account_roles: SalesAccountRoles,
        obligations: Vec<AccountingObligation>,
        preparer: String,
        sales_approver: String,
        suggestion: Option<AccountingPlanSuggestionProvenance>,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesPostingError> {
        scope
            .validate()
            .map_err(|_| SalesPostingError::InvalidScope)?;
        quote
            .validate()
            .map_err(|_| SalesPostingError::InvalidQuoteSource)?;
        tax_profile.validate()?;
        account_roles.validate()?;
        if quote.context != BoundedContext::Sales
            || quote.aggregate_kind.as_str() != "quote"
            || currency.len() != 3
            || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || quote_net_minor <= 0
            || quote_tax_minor < 0
            || !valid_text(&preparer, 256)
            || !valid_text(&sales_approver, 256)
            || preparer == sales_approver
            || obligations.is_empty()
            || obligations.len() > 1_000
        {
            return Err(SalesPostingError::InvalidPlan);
        }
        let mut obligation_ids = BTreeSet::new();
        let allocated = obligations.iter().try_fold(0_i64, |total, obligation| {
            obligation.validate()?;
            if !obligation_ids.insert(obligation.obligation_id) {
                return Err(SalesPostingError::InvalidObligation);
            }
            total
                .checked_add(obligation.allocated_minor)
                .ok_or(SalesPostingError::ArithmeticOverflow)
        })?;
        if allocated != quote_net_minor {
            return Err(SalesPostingError::AllocationMismatch);
        }
        if let Some(value) = &suggestion {
            value.validate()?;
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            quote,
            currency,
            quote_net_minor,
            quote_tax_minor,
            dimensions,
            tax_profile,
            account_roles,
            obligations,
            preparer,
            sales_approver,
            finance_approver: None,
            suggestion,
            status: AccountingPlanStatus::Draft,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn submit(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        self.transition(
            AccountingPlanStatus::Draft,
            AccountingPlanStatus::Submitted,
            at,
        )
    }

    pub fn approve(
        &mut self,
        finance_approver: String,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        if !valid_text(&finance_approver, 256)
            || finance_approver == self.preparer
            || finance_approver == self.sales_approver
        {
            return Err(SalesPostingError::ReviewerSeparationViolation);
        }
        self.transition(
            AccountingPlanStatus::Submitted,
            AccountingPlanStatus::Approved,
            at,
        )?;
        self.finance_approver = Some(finance_approver);
        Ok(())
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        self.transition(
            AccountingPlanStatus::Submitted,
            AccountingPlanStatus::Rejected,
            at,
        )
    }

    pub fn supersede(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        if matches!(
            self.status,
            AccountingPlanStatus::Rejected | AccountingPlanStatus::Superseded
        ) {
            return Err(SalesPostingError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = AccountingPlanStatus::Superseded;
        Ok(())
    }

    fn transition(
        &mut self,
        expected: AccountingPlanStatus,
        next: AccountingPlanStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        if self.status != expected {
            return Err(SalesPostingError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = next;
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        if at < self.updated_at {
            return Err(SalesPostingError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesPostingError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> AccountingPlanId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn quote(&self) -> &RecordRef {
        &self.quote
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub const fn quote_net_minor(&self) -> i64 {
        self.quote_net_minor
    }
    pub const fn quote_tax_minor(&self) -> i64 {
        self.quote_tax_minor
    }
    pub const fn dimensions(&self) -> &DimensionSet {
        &self.dimensions
    }
    pub const fn tax_profile(&self) -> &FinanceTaxProfileSnapshot {
        &self.tax_profile
    }
    pub const fn account_roles(&self) -> &SalesAccountRoles {
        &self.account_roles
    }
    pub fn obligations(&self) -> &[AccountingObligation] {
        &self.obligations
    }
    pub fn preparer(&self) -> &str {
        &self.preparer
    }
    pub fn sales_approver(&self) -> &str {
        &self.sales_approver
    }
    pub fn finance_approver(&self) -> Option<&str> {
        self.finance_approver.as_deref()
    }
    pub const fn suggestion(&self) -> Option<&AccountingPlanSuggestionProvenance> {
        self.suggestion.as_ref()
    }
    pub const fn status(&self) -> AccountingPlanStatus {
        self.status
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingActuals {
    pub source_event_id: EventId,
    pub source_aggregate_version: u64,
    pub billing_event_id: Uuid,
    pub fulfillment_id: Uuid,
    pub obligation_id: Uuid,
    pub entry_date: NaiveDate,
    pub net_minor: i64,
    pub tax_minor: i64,
    pub fulfilled_minor: i64,
    pub currency: String,
    pub tax_profile: FinanceTaxProfileSnapshot,
}

impl PostingActuals {
    fn validate(&self) -> Result<(), SalesPostingError> {
        self.tax_profile.validate()?;
        if self.source_event_id.as_uuid().is_nil()
            || self.source_aggregate_version == 0
            || self.billing_event_id.is_nil()
            || self.fulfillment_id.is_nil()
            || self.obligation_id.is_nil()
            || self.net_minor <= 0
            || self.tax_minor < 0
            || self.fulfilled_minor <= 0
            || self.fulfilled_minor > self.net_minor
            || self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        {
            return Err(SalesPostingError::InvalidActuals);
        }
        Ok(())
    }

    pub fn gross_minor(&self) -> Result<i64, SalesPostingError> {
        self.net_minor
            .checked_add(self.tax_minor)
            .ok_or(SalesPostingError::ArithmeticOverflow)
    }
}

pub struct PostingValidationContext<'a> {
    pub period_open: bool,
    pub active_accounts: &'a BTreeSet<AccountId>,
    pub current_tax_profile: &'a FinanceTaxProfileSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingSide {
    Debit,
    Credit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingEntry {
    pub account_id: AccountId,
    pub side: PostingSide,
    pub amount_minor: i64,
    pub memo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingInstructions {
    pub packet_id: PostingPacketId,
    pub invoice_net_minor: i64,
    pub invoice_tax_minor: i64,
    pub receivable_minor: i64,
    pub billing_journal: Vec<PostingEntry>,
    pub fulfillment_journal: Vec<PostingEntry>,
    pub revenue_to_recognize_minor: i64,
}

impl PostingInstructions {
    pub fn validate_balanced(&self) -> Result<(), SalesPostingError> {
        for entries in [&self.billing_journal, &self.fulfillment_journal] {
            let debit = total_side(entries, PostingSide::Debit)?;
            let credit = total_side(entries, PostingSide::Credit)?;
            if debit != credit || debit <= 0 {
                return Err(SalesPostingError::UnbalancedPosting);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingReceipt {
    pub invoice_id: Uuid,
    pub receivable_id: Uuid,
    pub billing_journal_id: Uuid,
    pub fulfillment_journal_id: Uuid,
    pub revenue_schedule_id: Uuid,
    pub posted_at: DateTime<Utc>,
}

impl PostingReceipt {
    fn validate(&self) -> Result<(), SalesPostingError> {
        let ids = [
            self.invoice_id,
            self.receivable_id,
            self.billing_journal_id,
            self.fulfillment_journal_id,
            self.revenue_schedule_id,
        ];
        if ids.iter().any(Uuid::is_nil)
            || ids
                .iter()
                .enumerate()
                .any(|(index, id)| ids[..index].contains(id))
        {
            return Err(SalesPostingError::InvalidPostingReceipt);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostingPacket {
    id: PostingPacketId,
    version: u64,
    scope: AuthorityScope,
    plan_id: AccountingPlanId,
    plan_version: u64,
    quote: RecordRef,
    actuals: PostingActuals,
    account_roles: SalesAccountRoles,
    preparer: String,
    sales_approver: String,
    final_finance_approver: Option<String>,
    status: PostingPacketStatus,
    receipt: Option<PostingReceipt>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PostingPacket {
    pub fn draft_from_plan(
        id: PostingPacketId,
        plan: &AccountingPlan,
        actuals: PostingActuals,
        preparer: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SalesPostingError> {
        if plan.status() != AccountingPlanStatus::Approved
            || !valid_text(&preparer, 256)
            || actuals.currency != plan.currency
            || actuals.net_minor != plan.quote_net_minor
            || actuals.tax_minor != plan.quote_tax_minor
            || !plan
                .obligations
                .iter()
                .any(|obligation| obligation.obligation_id == actuals.obligation_id)
        {
            return Err(SalesPostingError::PlanActualMismatch);
        }
        actuals.validate()?;
        Ok(Self {
            id,
            version: 1,
            scope: plan.scope.clone(),
            plan_id: plan.id,
            plan_version: plan.version,
            quote: plan.quote.clone(),
            actuals,
            account_roles: plan.account_roles.clone(),
            preparer,
            sales_approver: plan.sales_approver.clone(),
            final_finance_approver: None,
            status: PostingPacketStatus::Draft,
            receipt: None,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn submit(
        &mut self,
        validation: &PostingValidationContext<'_>,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        self.validate_for_posting(validation)?;
        self.transition(
            PostingPacketStatus::Draft,
            PostingPacketStatus::Submitted,
            at,
        )
    }

    pub fn approve(
        &mut self,
        finance_approver: String,
        validation: &PostingValidationContext<'_>,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        if !valid_text(&finance_approver, 256)
            || finance_approver == self.preparer
            || finance_approver == self.sales_approver
        {
            return Err(SalesPostingError::ReviewerSeparationViolation);
        }
        self.validate_for_posting(validation)?;
        self.transition(
            PostingPacketStatus::Submitted,
            PostingPacketStatus::Approved,
            at,
        )?;
        self.final_finance_approver = Some(finance_approver);
        Ok(())
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        self.transition(
            PostingPacketStatus::Submitted,
            PostingPacketStatus::Rejected,
            at,
        )
    }

    pub fn posting_instructions(&self) -> Result<PostingInstructions, SalesPostingError> {
        if self.status != PostingPacketStatus::Approved {
            return Err(SalesPostingError::FinalApprovalRequired);
        }
        let instructions = self.posting_shape()?;
        instructions.validate_balanced()?;
        Ok(instructions)
    }

    /// Replaying the same receipt is a no-op; a different receipt conflicts.
    pub fn mark_posted(
        &mut self,
        receipt: PostingReceipt,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        receipt.validate()?;
        if self.status == PostingPacketStatus::Posted {
            return if self.receipt.as_ref() == Some(&receipt) {
                Ok(())
            } else {
                Err(SalesPostingError::DuplicatePostingConflict)
            };
        }
        if self.status != PostingPacketStatus::Approved {
            return Err(SalesPostingError::FinalApprovalRequired);
        }
        self.advance(at)?;
        self.receipt = Some(receipt);
        self.status = PostingPacketStatus::Posted;
        Ok(())
    }

    fn validate_for_posting(
        &self,
        validation: &PostingValidationContext<'_>,
    ) -> Result<(), SalesPostingError> {
        if !validation.period_open {
            return Err(SalesPostingError::ClosedPeriod);
        }
        if self
            .account_roles
            .all()
            .iter()
            .any(|account| !validation.active_accounts.contains(account))
        {
            return Err(SalesPostingError::InactiveAccount);
        }
        if validation.current_tax_profile != &self.actuals.tax_profile {
            return Err(SalesPostingError::TaxProfileDrift);
        }
        self.posting_shape()?.validate_balanced()
    }

    fn posting_shape(&self) -> Result<PostingInstructions, SalesPostingError> {
        let gross = self.actuals.gross_minor()?;
        let mut billing_journal = vec![
            PostingEntry {
                account_id: self.account_roles.accounts_receivable,
                side: PostingSide::Debit,
                amount_minor: gross,
                memo: "Customer invoice receivable".into(),
            },
            PostingEntry {
                account_id: self.account_roles.contract_liability,
                side: PostingSide::Credit,
                amount_minor: self.actuals.net_minor,
                memo: "Billed contract liability".into(),
            },
        ];
        if self.actuals.tax_minor > 0 {
            billing_journal.push(PostingEntry {
                account_id: self.account_roles.output_vat,
                side: PostingSide::Credit,
                amount_minor: self.actuals.tax_minor,
                memo: "Output VAT".into(),
            });
        }
        Ok(PostingInstructions {
            packet_id: self.id,
            invoice_net_minor: self.actuals.net_minor,
            invoice_tax_minor: self.actuals.tax_minor,
            receivable_minor: gross,
            billing_journal,
            fulfillment_journal: vec![
                PostingEntry {
                    account_id: self.account_roles.contract_liability,
                    side: PostingSide::Debit,
                    amount_minor: self.actuals.fulfilled_minor,
                    memo: "Accepted fulfillment".into(),
                },
                PostingEntry {
                    account_id: self.account_roles.revenue,
                    side: PostingSide::Credit,
                    amount_minor: self.actuals.fulfilled_minor,
                    memo: "Recognized revenue".into(),
                },
            ],
            revenue_to_recognize_minor: self.actuals.fulfilled_minor,
        })
    }

    fn transition(
        &mut self,
        expected: PostingPacketStatus,
        next: PostingPacketStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SalesPostingError> {
        if self.status != expected {
            return Err(SalesPostingError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = next;
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SalesPostingError> {
        if at < self.updated_at {
            return Err(SalesPostingError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SalesPostingError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> PostingPacketId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn plan_id(&self) -> AccountingPlanId {
        self.plan_id
    }
    pub const fn plan_version(&self) -> u64 {
        self.plan_version
    }
    pub const fn quote(&self) -> &RecordRef {
        &self.quote
    }
    pub const fn actuals(&self) -> &PostingActuals {
        &self.actuals
    }
    pub fn preparer(&self) -> &str {
        &self.preparer
    }
    pub fn sales_approver(&self) -> &str {
        &self.sales_approver
    }
    pub fn final_finance_approver(&self) -> Option<&str> {
        self.final_finance_approver.as_deref()
    }
    pub const fn status(&self) -> PostingPacketStatus {
        self.status
    }
    pub const fn receipt(&self) -> Option<&PostingReceipt> {
        self.receipt.as_ref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SalesPostingError {
    #[error("Finance Sales-posting identifier is invalid")]
    InvalidIdentifier,
    #[error("Finance Sales-posting scope is invalid")]
    InvalidScope,
    #[error("quote source is invalid")]
    InvalidQuoteSource,
    #[error("accounting plan is invalid")]
    InvalidPlan,
    #[error("accounting plan suggestion is invalid")]
    InvalidPlanSuggestion,
    #[error("posting review suggestion is invalid")]
    InvalidPostingReviewSuggestion,
    #[error("tax profile is invalid")]
    InvalidTaxProfile,
    #[error("account roles must use distinct accounts")]
    DuplicateAccountRole,
    #[error("performance obligation is invalid")]
    InvalidObligation,
    #[error("performance obligation allocation does not match quote net amount")]
    AllocationMismatch,
    #[error("AI suggestion provenance is invalid")]
    InvalidSuggestionProvenance,
    #[error("Finance reviewer separation was violated")]
    ReviewerSeparationViolation,
    #[error("accounting plan and actual posting amounts do not match")]
    PlanActualMismatch,
    #[error("posting actuals are invalid")]
    InvalidActuals,
    #[error("accounting period is closed")]
    ClosedPeriod,
    #[error("posting references an inactive account")]
    InactiveAccount,
    #[error("authoritative tax profile changed after plan approval")]
    TaxProfileDrift,
    #[error("posting journal is not balanced")]
    UnbalancedPosting,
    #[error("final Finance approval is required before posting")]
    FinalApprovalRequired,
    #[error("posting receipt is invalid")]
    InvalidPostingReceipt,
    #[error("source event was already posted with different owner IDs")]
    DuplicatePostingConflict,
    #[error("Finance Sales-posting transition is invalid")]
    InvalidTransition,
    #[error("Finance amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("Finance Sales-posting timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("Finance Sales-posting version overflowed")]
    VersionOverflow,
}

fn total_side(entries: &[PostingEntry], side: PostingSide) -> Result<i64, SalesPostingError> {
    entries
        .iter()
        .filter(|entry| entry.side == side)
        .try_fold(0_i64, |total, entry| {
            total
                .checked_add(entry.amount_minor)
                .ok_or(SalesPostingError::ArithmeticOverflow)
        })
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_integration_contracts::organization;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn account(value: u128) -> AccountId {
        AccountId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn tax(version: u32) -> FinanceTaxProfileSnapshot {
        FinanceTaxProfileSnapshot {
            profile_id: Uuid::from_u128(10),
            version,
            classification: FinanceTaxClassification::StandardVat,
            rate_basis_points: 700,
            authority_ref: "company-customer-item:th-vat-7".into(),
        }
    }

    fn roles() -> SalesAccountRoles {
        SalesAccountRoles {
            accounts_receivable: account(20),
            output_vat: account(21),
            contract_liability: account(22),
            revenue: account(23),
        }
    }

    fn plan() -> AccountingPlan {
        let mut value = AccountingPlan::draft(
            AccountingPlanId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            AuthorityScope::Company {
                holding_id: Uuid::from_u128(31),
                company_id: Uuid::from_u128(32),
            },
            RecordRef::try_new(BoundedContext::Sales, "quote", Uuid::from_u128(33), 4).unwrap(),
            "THB".into(),
            100_000,
            7_000,
            DimensionSet::for_legal_entity(
                organization::CompanyRef::try_from_uuid(Uuid::from_u128(32)).unwrap(),
            ),
            tax(3),
            roles(),
            vec![AccountingObligation {
                obligation_id: Uuid::from_u128(34),
                allocated_minor: 100_000,
                template_code: "goods-default-v1".into(),
            }],
            "finance-preparer".into(),
            "sales-approver".into(),
            None,
            at(),
        )
        .unwrap();
        value.submit(at()).unwrap();
        value.approve("finance-approver".into(), at()).unwrap();
        value
    }

    fn packet() -> PostingPacket {
        PostingPacket::draft_from_plan(
            PostingPacketId::try_from_uuid(Uuid::from_u128(40)).unwrap(),
            &plan(),
            PostingActuals {
                source_event_id: EventId::try_from_uuid(Uuid::from_u128(43)).unwrap(),
                source_aggregate_version: 8,
                billing_event_id: Uuid::from_u128(41),
                fulfillment_id: Uuid::from_u128(42),
                obligation_id: Uuid::from_u128(34),
                entry_date: NaiveDate::from_ymd_opt(2027, 1, 15).unwrap(),
                net_minor: 100_000,
                tax_minor: 7_000,
                fulfilled_minor: 100_000,
                currency: "THB".into(),
                tax_profile: tax(3),
            },
            "posting-preparer".into(),
            at(),
        )
        .unwrap()
    }

    fn active() -> BTreeSet<AccountId> {
        roles().all().into_iter().collect()
    }

    #[test]
    fn final_finance_approval_is_required_before_posting_instructions() {
        let mut value = packet();
        assert_eq!(
            value.posting_instructions(),
            Err(SalesPostingError::FinalApprovalRequired)
        );
        let active = active();
        let current_tax = tax(3);
        let validation = PostingValidationContext {
            period_open: true,
            active_accounts: &active,
            current_tax_profile: &current_tax,
        };
        value.submit(&validation, at()).unwrap();
        value
            .approve("final-finance-reviewer".into(), &validation, at())
            .unwrap();
        let instructions = value.posting_instructions().unwrap();
        instructions.validate_balanced().unwrap();
        assert_eq!(instructions.receivable_minor, 107_000);
        assert_eq!(instructions.revenue_to_recognize_minor, 100_000);
    }

    #[test]
    fn closed_period_inactive_account_and_tax_drift_fail_closed() {
        let mut value = packet();
        let active = active();
        let current_tax = tax(3);
        assert_eq!(
            value.submit(
                &PostingValidationContext {
                    period_open: false,
                    active_accounts: &active,
                    current_tax_profile: &current_tax,
                },
                at(),
            ),
            Err(SalesPostingError::ClosedPeriod)
        );
        let mut inactive = active.clone();
        inactive.remove(&roles().revenue);
        assert_eq!(
            value.submit(
                &PostingValidationContext {
                    period_open: true,
                    active_accounts: &inactive,
                    current_tax_profile: &current_tax,
                },
                at(),
            ),
            Err(SalesPostingError::InactiveAccount)
        );
        assert_eq!(
            value.submit(
                &PostingValidationContext {
                    period_open: true,
                    active_accounts: &active,
                    current_tax_profile: &tax(4),
                },
                at(),
            ),
            Err(SalesPostingError::TaxProfileDrift)
        );
    }

    #[test]
    fn posting_receipt_replay_is_idempotent_and_conflict_is_denied() {
        let mut value = packet();
        let active = active();
        let current_tax = tax(3);
        let validation = PostingValidationContext {
            period_open: true,
            active_accounts: &active,
            current_tax_profile: &current_tax,
        };
        value.submit(&validation, at()).unwrap();
        value
            .approve("final-finance-reviewer".into(), &validation, at())
            .unwrap();
        let receipt = PostingReceipt {
            invoice_id: Uuid::from_u128(50),
            receivable_id: Uuid::from_u128(51),
            billing_journal_id: Uuid::from_u128(52),
            fulfillment_journal_id: Uuid::from_u128(53),
            revenue_schedule_id: Uuid::from_u128(54),
            posted_at: at(),
        };
        value.mark_posted(receipt.clone(), at()).unwrap();
        value.mark_posted(receipt, at()).unwrap();
        let conflict = PostingReceipt {
            invoice_id: Uuid::from_u128(60),
            receivable_id: Uuid::from_u128(61),
            billing_journal_id: Uuid::from_u128(62),
            fulfillment_journal_id: Uuid::from_u128(63),
            revenue_schedule_id: Uuid::from_u128(64),
            posted_at: at(),
        };
        assert_eq!(
            value.mark_posted(conflict, at()),
            Err(SalesPostingError::DuplicatePostingConflict)
        );
    }
}
