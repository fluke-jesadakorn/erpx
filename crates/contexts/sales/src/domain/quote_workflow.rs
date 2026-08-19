//! Sales-owned quote drafting and immutable version evidence.

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use super::QuoteId;

pub const THAI_VAT_RATE_BPS: u16 = 700;
pub const MAX_QUOTE_WORKFLOW_LINES: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteDocumentLanguage {
    Thai,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxClassification {
    StandardVat,
    ZeroRated,
    Exempt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeTaxProfile {
    pub profile_id: Uuid,
    pub version: u32,
    pub classification: TaxClassification,
    pub rate_basis_points: u16,
    pub authority_ref: String,
}

impl AuthoritativeTaxProfile {
    pub fn thailand_vat_7(profile_id: Uuid, version: u32) -> Result<Self, QuoteWorkflowError> {
        let value = Self {
            profile_id,
            version,
            classification: TaxClassification::StandardVat,
            rate_basis_points: THAI_VAT_RATE_BPS,
            authority_ref: "company-customer-item:th-vat-7".into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), QuoteWorkflowError> {
        let valid_rate = match self.classification {
            TaxClassification::StandardVat => self.rate_basis_points > 0,
            TaxClassification::ZeroRated | TaxClassification::Exempt => self.rate_basis_points == 0,
        };
        if self.profile_id.is_nil()
            || self.version == 0
            || self.rate_basis_points > 10_000
            || !valid_rate
            || !valid_text(&self.authority_ref, 256)
        {
            return Err(QuoteWorkflowError::InvalidTaxProfile);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum QuoteDraftLineSource {
    Catalog { item_id: Uuid, item_version: u64 },
    Custom { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteDraftLine {
    pub line_id: Uuid,
    pub source: QuoteDraftLineSource,
    pub item_code: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub discount_basis_points: u16,
    pub tax_profile: AuthoritativeTaxProfile,
}

impl QuoteDraftLine {
    pub fn net_minor(&self) -> Result<i64, QuoteWorkflowError> {
        let gross = self
            .quantity
            .checked_mul(self.unit_price_minor)
            .ok_or(QuoteWorkflowError::ArithmeticOverflow)?;
        let discount = rounded_basis_points(gross, self.discount_basis_points)?;
        gross
            .checked_sub(discount)
            .ok_or(QuoteWorkflowError::ArithmeticOverflow)
    }

    pub fn tax_minor(&self) -> Result<i64, QuoteWorkflowError> {
        rounded_basis_points(self.net_minor()?, self.tax_profile.rate_basis_points)
    }

    pub fn total_minor(&self) -> Result<i64, QuoteWorkflowError> {
        self.net_minor()?
            .checked_add(self.tax_minor()?)
            .ok_or(QuoteWorkflowError::ArithmeticOverflow)
    }

    pub const fn requires_finance_approval(&self) -> bool {
        matches!(self.source, QuoteDraftLineSource::Custom { .. })
    }

    fn validate(&self) -> Result<(), QuoteWorkflowError> {
        self.tax_profile.validate()?;
        if self.line_id.is_nil()
            || !valid_text(&self.item_code, 128)
            || !valid_text(&self.description, 512)
            || self.quantity <= 0
            || self.unit_price_minor < 0
            || self.discount_basis_points > 10_000
        {
            return Err(QuoteWorkflowError::InvalidLine);
        }
        match &self.source {
            QuoteDraftLineSource::Catalog {
                item_id,
                item_version,
            } if item_id.is_nil() || *item_version == 0 => {
                return Err(QuoteWorkflowError::InvalidCatalogLine);
            }
            QuoteDraftLineSource::Custom { reason } if !valid_text(reason, 512) => {
                return Err(QuoteWorkflowError::CustomLineReasonRequired);
            }
            _ => {}
        }
        self.total_minor()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteDraftPayload {
    pub customer_id: Uuid,
    pub customer_version: u64,
    pub currency: String,
    pub payment_terms_days: u16,
    pub valid_until: DateTime<Utc>,
    pub language: QuoteDocumentLanguage,
    pub notes: String,
    pub lines: Vec<QuoteDraftLine>,
}

/// Typed output contract for `sales.revenue-arrangement.v1`.
///
/// This remains advisory data: Sales must explicitly copy it into a draft and
/// validate that draft before starting approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueArrangementSuggestionPayload {
    pub currency: String,
    pub quote_net_minor: i64,
    pub obligations: Vec<RevenueObligationSuggestion>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevenueObligationSuggestion {
    pub obligation_id: Uuid,
    pub description: String,
    pub allocated_minor: i64,
    pub recognition_method: String,
}

impl RevenueArrangementSuggestionPayload {
    pub fn validate(&self) -> Result<(), QuoteWorkflowError> {
        if self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || self.quote_net_minor <= 0
            || self.obligations.is_empty()
            || self.obligations.len() > MAX_QUOTE_WORKFLOW_LINES
            || !valid_text(&self.rationale, 2_048)
        {
            return Err(QuoteWorkflowError::InvalidRevenueSuggestion);
        }
        let mut ids = std::collections::BTreeSet::new();
        let allocated = self.obligations.iter().try_fold(0_i64, |total, item| {
            if item.obligation_id.is_nil()
                || item.allocated_minor <= 0
                || !valid_text(&item.description, 512)
                || !valid_text(&item.recognition_method, 128)
                || !ids.insert(item.obligation_id)
            {
                return Err(QuoteWorkflowError::InvalidRevenueSuggestion);
            }
            total
                .checked_add(item.allocated_minor)
                .ok_or(QuoteWorkflowError::ArithmeticOverflow)
        })?;
        if allocated != self.quote_net_minor {
            return Err(QuoteWorkflowError::InvalidRevenueSuggestion);
        }
        Ok(())
    }
}

impl QuoteDraftPayload {
    pub fn subtotal_minor(&self) -> Result<i64, QuoteWorkflowError> {
        checked_sum(self.lines.iter().map(QuoteDraftLine::net_minor))
    }

    pub fn tax_minor(&self) -> Result<i64, QuoteWorkflowError> {
        checked_sum(self.lines.iter().map(QuoteDraftLine::tax_minor))
    }

    pub fn total_minor(&self) -> Result<i64, QuoteWorkflowError> {
        self.subtotal_minor()?
            .checked_add(self.tax_minor()?)
            .ok_or(QuoteWorkflowError::ArithmeticOverflow)
    }

    pub fn validate_against(
        &self,
        authoritative_tax_profiles: &[AuthoritativeTaxProfile],
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        if self.customer_id.is_nil()
            || self.customer_version == 0
            || self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || self.payment_terms_days > 365
            || self.valid_until <= at
            || self.notes.len() > 4_096
            || self.notes.chars().any(char::is_control)
            || self.lines.is_empty()
            || self.lines.len() > MAX_QUOTE_WORKFLOW_LINES
        {
            return Err(QuoteWorkflowError::InvalidDraft);
        }
        let mut line_ids = std::collections::BTreeSet::new();
        for line in &self.lines {
            line.validate()?;
            if !line_ids.insert(line.line_id) {
                return Err(QuoteWorkflowError::DuplicateLine);
            }
            let authoritative = authoritative_tax_profiles.iter().any(|profile| {
                profile.profile_id == line.tax_profile.profile_id
                    && profile.version == line.tax_profile.version
                    && profile.classification == line.tax_profile.classification
                    && profile.rate_basis_points == line.tax_profile.rate_basis_points
                    && profile.authority_ref == line.tax_profile.authority_ref
            });
            if !authoritative {
                return Err(QuoteWorkflowError::TaxProfileDrift);
            }
        }
        self.total_minor()?;
        Ok(())
    }

    pub fn has_custom_lines(&self) -> bool {
        self.lines
            .iter()
            .any(QuoteDraftLine::requires_finance_approval)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteWorkflowStatus {
    Draft,
    PendingApproval,
    ApprovalRejected,
    Approved,
    PdfGenerated,
    CustomerAccepted,
    CustomerRejected,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedQuoteSuggestion {
    pub suggestion_id: Uuid,
    pub source_quote_version: u64,
    pub applied_quote_version: u64,
    pub payload_digest: String,
    pub output_digest: String,
    pub provider_label: String,
    pub applied_by: String,
    pub applied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalEvidence {
    pub approval_session_id: Uuid,
    pub quote_version: u64,
    pub policy_version: u32,
    pub sales_approver: String,
    pub finance_approver: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalRejectionEvidence {
    pub approval_session_id: Uuid,
    pub quote_version: u64,
    pub policy_version: u32,
    pub rejected_gate: String,
    pub rejected_by: String,
    pub reason_code: String,
    pub rejected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteAccountingPlanEvidence {
    pub accounting_plan_id: Uuid,
    pub accounting_plan_version: u64,
    pub quote_version: u64,
    pub approved_by: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotePdfArtifact {
    pub artifact_id: Uuid,
    pub quote_version: u64,
    pub language: QuoteDocumentLanguage,
    pub object_key: String,
    pub sha256_digest: String,
    pub byte_length: u64,
    pub generated_at: DateTime<Utc>,
}

impl QuotePdfArtifact {
    fn validate(&self) -> Result<(), QuoteWorkflowError> {
        if self.artifact_id.is_nil()
            || self.quote_version == 0
            || !valid_object_key(&self.object_key)
            || !valid_digest(&self.sha256_digest)
            || self.byte_length == 0
        {
            return Err(QuoteWorkflowError::InvalidPdfArtifact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalQuoteDecisionKind {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalQuoteDecision {
    pub decision: ExternalQuoteDecisionKind,
    pub quote_version: u64,
    pub pdf_artifact_id: Uuid,
    pub evidence_reference: String,
    pub attachment_object_key: Option<String>,
    pub recorded_by: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersededQuoteVersionEvidence {
    pub quote_version: u64,
    pub suggestion: Option<AppliedQuoteSuggestion>,
    pub approval: Option<QuoteApprovalEvidence>,
    pub approval_rejection: Option<QuoteApprovalRejectionEvidence>,
    pub accounting_plan: Option<QuoteAccountingPlanEvidence>,
    pub pdf: Option<QuotePdfArtifact>,
    pub superseded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteWorkflow {
    quote_id: QuoteId,
    aggregate_version: u64,
    quote_version: u64,
    scope: AuthorityScope,
    quote_code: String,
    maker: String,
    payload: QuoteDraftPayload,
    status: QuoteWorkflowStatus,
    applied_suggestion: Option<AppliedQuoteSuggestion>,
    approval: Option<QuoteApprovalEvidence>,
    approval_rejection: Option<QuoteApprovalRejectionEvidence>,
    accounting_plan: Option<QuoteAccountingPlanEvidence>,
    pdf: Option<QuotePdfArtifact>,
    external_decision: Option<ExternalQuoteDecision>,
    superseded_versions: Vec<SupersededQuoteVersionEvidence>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl QuoteWorkflow {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        quote_id: QuoteId,
        scope: AuthorityScope,
        quote_code: String,
        maker: String,
        payload: QuoteDraftPayload,
        authoritative_tax_profiles: &[AuthoritativeTaxProfile],
        at: DateTime<Utc>,
    ) -> Result<Self, QuoteWorkflowError> {
        scope
            .validate()
            .map_err(|_| QuoteWorkflowError::InvalidScope)?;
        if !valid_text(&quote_code, 128) || !valid_text(&maker, 256) {
            return Err(QuoteWorkflowError::InvalidDraft);
        }
        payload.validate_against(authoritative_tax_profiles, at)?;
        Ok(Self {
            quote_id,
            aggregate_version: 1,
            quote_version: 1,
            scope,
            quote_code,
            maker,
            payload,
            status: QuoteWorkflowStatus::Draft,
            applied_suggestion: None,
            approval: None,
            approval_rejection: None,
            accounting_plan: None,
            pdf: None,
            external_decision: None,
            superseded_versions: Vec::new(),
            created_at: at,
            updated_at: at,
        })
    }

    pub fn update_draft(
        &mut self,
        expected_quote_version: u64,
        payload: QuoteDraftPayload,
        authoritative_tax_profiles: &[AuthoritativeTaxProfile],
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        self.ensure_editable(expected_quote_version)?;
        payload.validate_against(authoritative_tax_profiles, at)?;
        self.preserve_current_version(at);
        self.advance_content(at)?;
        self.payload = payload;
        self.status = QuoteWorkflowStatus::Draft;
        self.clear_dependent_evidence();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_suggestion(
        &mut self,
        expected_quote_version: u64,
        suggested_payload: QuoteDraftPayload,
        authoritative_tax_profiles: &[AuthoritativeTaxProfile],
        suggestion_id: Uuid,
        payload_digest: String,
        output_digest: String,
        provider_label: String,
        applied_by: String,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        self.ensure_editable(expected_quote_version)?;
        if suggestion_id.is_nil()
            || !valid_digest(&payload_digest)
            || !valid_digest(&output_digest)
            || !valid_text(&provider_label, 128)
            || !valid_text(&applied_by, 256)
        {
            return Err(QuoteWorkflowError::InvalidSuggestion);
        }
        suggested_payload.validate_against(authoritative_tax_profiles, at)?;
        self.preserve_current_version(at);
        let source_quote_version = self.quote_version;
        self.advance_content(at)?;
        self.payload = suggested_payload;
        self.status = QuoteWorkflowStatus::Draft;
        self.clear_dependent_evidence();
        self.applied_suggestion = Some(AppliedQuoteSuggestion {
            suggestion_id,
            source_quote_version,
            applied_quote_version: self.quote_version,
            payload_digest,
            output_digest,
            provider_label,
            applied_by,
            applied_at: at,
        });
        Ok(())
    }

    pub fn submit_for_approval(
        &mut self,
        expected_quote_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        if self.status != QuoteWorkflowStatus::Draft || expected_quote_version != self.quote_version
        {
            return Err(QuoteWorkflowError::StaleQuoteVersion);
        }
        self.advance_aggregate(at)?;
        self.status = QuoteWorkflowStatus::PendingApproval;
        Ok(())
    }

    pub fn attach_accounting_plan(
        &mut self,
        evidence: QuoteAccountingPlanEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        if self.status != QuoteWorkflowStatus::PendingApproval
            || evidence.quote_version != self.quote_version
            || evidence.accounting_plan_id.is_nil()
            || evidence.accounting_plan_version == 0
            || !valid_text(&evidence.approved_by, 256)
        {
            return Err(QuoteWorkflowError::InvalidAccountingPlanEvidence);
        }
        self.advance_aggregate(at)?;
        self.accounting_plan = Some(evidence);
        Ok(())
    }

    pub fn approve(
        &mut self,
        evidence: QuoteApprovalEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        if self.status != QuoteWorkflowStatus::PendingApproval
            || evidence.quote_version != self.quote_version
            || evidence.approval_session_id.is_nil()
            || evidence.policy_version == 0
            || evidence.sales_approver == self.maker
            || evidence.finance_approver == self.maker
            || evidence.sales_approver == evidence.finance_approver
            || !valid_text(&evidence.sales_approver, 256)
            || !valid_text(&evidence.finance_approver, 256)
            || self.accounting_plan.as_ref().is_none_or(|plan| {
                plan.quote_version != self.quote_version
                    || plan.approved_by != evidence.finance_approver
            })
        {
            return Err(QuoteWorkflowError::InvalidApprovalEvidence);
        }
        self.advance_aggregate(at)?;
        self.approval = Some(evidence);
        self.status = QuoteWorkflowStatus::Approved;
        Ok(())
    }

    pub fn reject_approval(
        &mut self,
        evidence: QuoteApprovalRejectionEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        if self.status != QuoteWorkflowStatus::PendingApproval
            || evidence.quote_version != self.quote_version
            || evidence.approval_session_id.is_nil()
            || evidence.policy_version == 0
            || evidence.rejected_by == self.maker
            || !valid_text(&evidence.rejected_gate, 128)
            || !valid_text(&evidence.rejected_by, 256)
            || !valid_text(&evidence.reason_code, 128)
            || evidence.rejected_at > at
        {
            return Err(QuoteWorkflowError::InvalidApprovalEvidence);
        }
        self.advance_aggregate(at)?;
        self.approval_rejection = Some(evidence);
        self.status = QuoteWorkflowStatus::ApprovalRejected;
        Ok(())
    }

    pub fn attach_pdf(
        &mut self,
        artifact: QuotePdfArtifact,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        artifact.validate()?;
        if self.status != QuoteWorkflowStatus::Approved
            || artifact.quote_version != self.quote_version
            || artifact.language != self.payload.language
        {
            return Err(QuoteWorkflowError::InvalidPdfArtifact);
        }
        self.advance_aggregate(at)?;
        self.pdf = Some(artifact);
        self.status = QuoteWorkflowStatus::PdfGenerated;
        Ok(())
    }

    pub fn record_external_decision(
        &mut self,
        decision: ExternalQuoteDecision,
        at: DateTime<Utc>,
    ) -> Result<(), QuoteWorkflowError> {
        let Some(pdf) = self.pdf.as_ref() else {
            return Err(QuoteWorkflowError::InvalidExternalDecision);
        };
        if self.status != QuoteWorkflowStatus::PdfGenerated
            || decision.quote_version != self.quote_version
            || decision.pdf_artifact_id != pdf.artifact_id
            || !valid_text(&decision.evidence_reference, 512)
            || !valid_text(&decision.recorded_by, 256)
            || decision.decided_at < pdf.generated_at
            || decision
                .attachment_object_key
                .as_deref()
                .is_some_and(|key| !valid_object_key(key))
        {
            return Err(QuoteWorkflowError::InvalidExternalDecision);
        }
        self.advance_aggregate(at)?;
        self.status = match decision.decision {
            ExternalQuoteDecisionKind::Accepted => QuoteWorkflowStatus::CustomerAccepted,
            ExternalQuoteDecisionKind::Rejected => QuoteWorkflowStatus::CustomerRejected,
        };
        self.external_decision = Some(decision);
        Ok(())
    }

    fn ensure_editable(&self, expected_quote_version: u64) -> Result<(), QuoteWorkflowError> {
        if expected_quote_version != self.quote_version
            || matches!(
                self.status,
                QuoteWorkflowStatus::CustomerAccepted
                    | QuoteWorkflowStatus::CustomerRejected
                    | QuoteWorkflowStatus::Superseded
            )
        {
            return Err(QuoteWorkflowError::StaleQuoteVersion);
        }
        Ok(())
    }

    fn preserve_current_version(&mut self, at: DateTime<Utc>) {
        if self.applied_suggestion.is_some()
            || self.approval.is_some()
            || self.approval_rejection.is_some()
            || self.accounting_plan.is_some()
            || self.pdf.is_some()
        {
            self.superseded_versions
                .push(SupersededQuoteVersionEvidence {
                    quote_version: self.quote_version,
                    suggestion: self.applied_suggestion.clone(),
                    approval: self.approval.clone(),
                    approval_rejection: self.approval_rejection.clone(),
                    accounting_plan: self.accounting_plan.clone(),
                    pdf: self.pdf.clone(),
                    superseded_at: at,
                });
        }
    }

    fn clear_dependent_evidence(&mut self) {
        self.applied_suggestion = None;
        self.approval = None;
        self.approval_rejection = None;
        self.accounting_plan = None;
        self.pdf = None;
        self.external_decision = None;
    }

    fn advance_content(&mut self, at: DateTime<Utc>) -> Result<(), QuoteWorkflowError> {
        self.quote_version = self
            .quote_version
            .checked_add(1)
            .ok_or(QuoteWorkflowError::VersionOverflow)?;
        self.advance_aggregate(at)
    }

    fn advance_aggregate(&mut self, at: DateTime<Utc>) -> Result<(), QuoteWorkflowError> {
        if at < self.updated_at {
            return Err(QuoteWorkflowError::InvalidTimestamp);
        }
        self.aggregate_version = self
            .aggregate_version
            .checked_add(1)
            .ok_or(QuoteWorkflowError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn quote_id(&self) -> QuoteId {
        self.quote_id
    }
    pub const fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }
    pub const fn quote_version(&self) -> u64 {
        self.quote_version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn quote_code(&self) -> &str {
        &self.quote_code
    }
    pub fn maker(&self) -> &str {
        &self.maker
    }
    pub const fn payload(&self) -> &QuoteDraftPayload {
        &self.payload
    }
    pub const fn status(&self) -> QuoteWorkflowStatus {
        self.status
    }
    pub const fn applied_suggestion(&self) -> Option<&AppliedQuoteSuggestion> {
        self.applied_suggestion.as_ref()
    }
    pub const fn approval(&self) -> Option<&QuoteApprovalEvidence> {
        self.approval.as_ref()
    }
    pub const fn approval_rejection(&self) -> Option<&QuoteApprovalRejectionEvidence> {
        self.approval_rejection.as_ref()
    }
    pub const fn accounting_plan(&self) -> Option<&QuoteAccountingPlanEvidence> {
        self.accounting_plan.as_ref()
    }
    pub const fn pdf(&self) -> Option<&QuotePdfArtifact> {
        self.pdf.as_ref()
    }
    pub const fn external_decision(&self) -> Option<&ExternalQuoteDecision> {
        self.external_decision.as_ref()
    }
    pub fn superseded_versions(&self) -> &[SupersededQuoteVersionEvidence] {
        &self.superseded_versions
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuoteWorkflowError {
    #[error("quote workflow scope is invalid")]
    InvalidScope,
    #[error("quote draft is invalid")]
    InvalidDraft,
    #[error("revenue arrangement suggestion is invalid")]
    InvalidRevenueSuggestion,
    #[error("quote line is invalid")]
    InvalidLine,
    #[error("catalog quote line is invalid")]
    InvalidCatalogLine,
    #[error("custom quote line requires a reason")]
    CustomLineReasonRequired,
    #[error("quote line identifiers must be unique")]
    DuplicateLine,
    #[error("authoritative tax profile is invalid")]
    InvalidTaxProfile,
    #[error("tax profile changed or is not authoritative")]
    TaxProfileDrift,
    #[error("quote suggestion evidence is invalid")]
    InvalidSuggestion,
    #[error("quote version is stale or no longer editable")]
    StaleQuoteVersion,
    #[error("accounting plan evidence is invalid")]
    InvalidAccountingPlanEvidence,
    #[error("approval evidence is invalid or violates reviewer separation")]
    InvalidApprovalEvidence,
    #[error("quote PDF artifact is invalid")]
    InvalidPdfArtifact,
    #[error("external quote decision is invalid")]
    InvalidExternalDecision,
    #[error("quote amount arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("quote timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("quote workflow version overflowed")]
    VersionOverflow,
}

fn checked_sum(
    values: impl IntoIterator<Item = Result<i64, QuoteWorkflowError>>,
) -> Result<i64, QuoteWorkflowError> {
    values.into_iter().try_fold(0_i64, |total, value| {
        total
            .checked_add(value?)
            .ok_or(QuoteWorkflowError::ArithmeticOverflow)
    })
}

fn rounded_basis_points(value: i64, basis_points: u16) -> Result<i64, QuoteWorkflowError> {
    let scaled = value
        .checked_mul(i64::from(basis_points))
        .ok_or(QuoteWorkflowError::ArithmeticOverflow)?;
    scaled
        .checked_add(5_000)
        .ok_or(QuoteWorkflowError::ArithmeticOverflow)
        .map(|rounded| rounded / 10_000)
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_object_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && !value.starts_with('/')
        && !value.split('/').any(|part| part.is_empty() || part == "..")
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn tax() -> AuthoritativeTaxProfile {
        AuthoritativeTaxProfile::thailand_vat_7(Uuid::from_u128(10), 3).unwrap()
    }

    fn payload(custom: bool) -> QuoteDraftPayload {
        QuoteDraftPayload {
            customer_id: Uuid::from_u128(11),
            customer_version: 2,
            currency: "THB".into(),
            payment_terms_days: 30,
            valid_until: at() + chrono::Duration::days(14),
            language: QuoteDocumentLanguage::Thai,
            notes: "ราคานี้รวมภาษีมูลค่าเพิ่มตามรายการ".into(),
            lines: vec![QuoteDraftLine {
                line_id: Uuid::from_u128(12),
                source: if custom {
                    QuoteDraftLineSource::Custom {
                        reason: "บริการติดตั้งเฉพาะโครงการ".into(),
                    }
                } else {
                    QuoteDraftLineSource::Catalog {
                        item_id: Uuid::from_u128(13),
                        item_version: 5,
                    }
                },
                item_code: "CONSULT-01".into(),
                description: "Implementation service".into(),
                quantity: 2,
                unit_price_minor: 10_000,
                discount_basis_points: 0,
                tax_profile: tax(),
            }],
        }
    }

    fn workflow() -> QuoteWorkflow {
        QuoteWorkflow::create(
            QuoteId::try_from_uuid(Uuid::from_u128(20)).unwrap(),
            AuthorityScope::Company {
                holding_id: Uuid::from_u128(21),
                company_id: Uuid::from_u128(22),
            },
            "QT-2026-0001".into(),
            "sales-maker".into(),
            payload(true),
            &[tax()],
            at(),
        )
        .unwrap()
    }

    #[test]
    fn suggestion_changes_nothing_before_explicit_apply_and_stale_apply_fails() {
        let mut value = workflow();
        let original = value.payload().clone();
        let suggested = payload(false);
        assert_eq!(value.payload(), &original);
        value
            .apply_suggestion(
                1,
                suggested.clone(),
                &[tax()],
                Uuid::from_u128(30),
                "a".repeat(64),
                "b".repeat(64),
                "Fixture AI".into(),
                "sales-maker".into(),
                at(),
            )
            .unwrap();
        assert_eq!(value.payload(), &suggested);
        assert_eq!(value.quote_version(), 2);
        assert_eq!(
            value.apply_suggestion(
                1,
                original,
                &[tax()],
                Uuid::from_u128(31),
                "c".repeat(64),
                "d".repeat(64),
                "Fixture AI".into(),
                "sales-maker".into(),
                at(),
            ),
            Err(QuoteWorkflowError::StaleQuoteVersion)
        );
    }

    #[test]
    fn authoritative_thai_tax_is_seven_percent_and_ai_tax_drift_is_denied() {
        let value = workflow();
        assert_eq!(value.payload().subtotal_minor().unwrap(), 20_000);
        assert_eq!(value.payload().tax_minor().unwrap(), 1_400);
        let mut drifted = payload(false);
        drifted.lines[0].tax_profile.rate_basis_points = 500;
        assert_eq!(
            drifted.validate_against(&[tax()], at()),
            Err(QuoteWorkflowError::TaxProfileDrift)
        );
    }

    #[test]
    fn rejected_human_approval_is_visible_and_the_quote_can_be_revised() {
        let mut value = workflow();
        value.submit_for_approval(1, at()).unwrap();
        value
            .reject_approval(
                QuoteApprovalRejectionEvidence {
                    approval_session_id: Uuid::from_u128(50),
                    quote_version: 1,
                    policy_version: 1,
                    rejected_gate: "sales_commercial".into(),
                    rejected_by: "sales-approver".into(),
                    reason_code: "discount_out_of_policy".into(),
                    rejected_at: at(),
                },
                at(),
            )
            .unwrap();
        assert_eq!(value.status(), QuoteWorkflowStatus::ApprovalRejected);
        assert_eq!(
            value.approval_rejection().unwrap().reason_code,
            "discount_out_of_policy"
        );
        value
            .update_draft(1, payload(false), &[tax()], at())
            .unwrap();
        assert_eq!(value.status(), QuoteWorkflowStatus::Draft);
        assert!(value.approval_rejection().is_none());
        assert_eq!(value.superseded_versions().len(), 1);
        assert!(value.superseded_versions()[0].approval_rejection.is_some());
    }

    #[test]
    fn editing_preserves_and_supersedes_old_suggestion_approval_plan_and_pdf() {
        let mut value = workflow();
        value
            .apply_suggestion(
                1,
                payload(true),
                &[tax()],
                Uuid::from_u128(30),
                "a".repeat(64),
                "b".repeat(64),
                "Fixture AI".into(),
                "sales-maker".into(),
                at(),
            )
            .unwrap();
        value.submit_for_approval(2, at()).unwrap();
        value
            .attach_accounting_plan(
                QuoteAccountingPlanEvidence {
                    accounting_plan_id: Uuid::from_u128(40),
                    accounting_plan_version: 3,
                    quote_version: 2,
                    approved_by: "finance-approver".into(),
                    approved_at: at(),
                },
                at(),
            )
            .unwrap();
        value
            .approve(
                QuoteApprovalEvidence {
                    approval_session_id: Uuid::from_u128(41),
                    quote_version: 2,
                    policy_version: 1,
                    sales_approver: "sales-approver".into(),
                    finance_approver: "finance-approver".into(),
                    approved_at: at(),
                },
                at(),
            )
            .unwrap();
        value
            .attach_pdf(
                QuotePdfArtifact {
                    artifact_id: Uuid::from_u128(42),
                    quote_version: 2,
                    language: QuoteDocumentLanguage::Thai,
                    object_key: "quotes/qt-2026-0001/v2-th.pdf".into(),
                    sha256_digest: "c".repeat(64),
                    byte_length: 2_048,
                    generated_at: at(),
                },
                at(),
            )
            .unwrap();
        value
            .update_draft(2, payload(false), &[tax()], at())
            .unwrap();
        assert_eq!(value.status(), QuoteWorkflowStatus::Draft);
        assert!(value.applied_suggestion().is_none());
        assert!(value.approval().is_none());
        assert!(value.accounting_plan().is_none());
        assert!(value.pdf().is_none());
        assert_eq!(value.superseded_versions().len(), 1);
        assert!(value.superseded_versions()[0].pdf.is_some());
    }
}
