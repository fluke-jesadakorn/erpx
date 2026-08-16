use std::collections::HashSet;

use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    LegalDocumentId, LegalDomainError, LegalMatterId, MAX_REFERENCE_BYTES, advance,
    stable_record_ref, validate_digest, validate_retention, validate_scope, validate_text,
};
use erpx_integration_contracts::{StableRecordRef, reference::ReferenceValidationError};

pub const MAX_SIGNATURE_RECIPIENTS: usize = 128;
pub const MAX_SIGNATURE_CALLBACKS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureEnvelopeId(Uuid);

impl SignatureEnvelopeId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, LegalDomainError> {
        if value.is_nil() {
            return Err(LegalDomainError::NilIdentifier {
                field: "signature_envelope_id",
            });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignatureRecipientId(Uuid);

impl SignatureRecipientId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, LegalDomainError> {
        if value.is_nil() {
            return Err(LegalDomainError::NilIdentifier {
                field: "signature_recipient_id",
            });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureOrder {
    Parallel,
    Serial,
}

/// A signatory subject contains no email address, phone number, raw token, or
/// provider SDK type. External access is represented only by a one-time grant
/// digest and an opaque subject reference within the Signatory realm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "realm", rename_all = "snake_case")]
pub enum SignatureSubject {
    Internal {
        principal_ref: String,
    },
    ExternalOneTime {
        subject_ref: String,
        access_grant_sha256: String,
        expires_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureRecipient {
    pub id: SignatureRecipientId,
    pub subject: SignatureSubject,
    pub order: u16,
    pub signed_at: Option<DateTime<Utc>>,
}

impl SignatureRecipient {
    pub fn try_new(
        id: SignatureRecipientId,
        subject: SignatureSubject,
        order: u16,
    ) -> Result<Self, LegalDomainError> {
        let value = Self {
            id,
            subject,
            order,
            signed_at: None,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), LegalDomainError> {
        if self.order == 0 {
            return Err(LegalDomainError::InvalidSignatureRecipient);
        }
        match &self.subject {
            SignatureSubject::Internal { principal_ref } => {
                validate_text(principal_ref, "principal_ref", MAX_REFERENCE_BYTES)?;
            }
            SignatureSubject::ExternalOneTime {
                subject_ref,
                access_grant_sha256,
                ..
            } => {
                validate_text(subject_ref, "external_subject_ref", MAX_REFERENCE_BYTES)?;
                validate_digest(access_grant_sha256)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    Draft,
    Submitted,
    InProgress,
    Completed,
    Failed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SignatureCallbackKind {
    Delivered,
    RecipientSigned { recipient_id: SignatureRecipientId },
    Completed { evidence_sha256: String },
    Failed { reason_code: String },
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedSignatureCallback {
    pub provider_event_id: String,
    pub provider_envelope_ref: String,
    pub sequence: u64,
    pub verified: bool,
    pub occurred_at: DateTime<Utc>,
    pub kind: SignatureCallbackKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackOutcome {
    Applied,
    Duplicate,
}

/// Provider-neutral e-signature lifecycle pinned to an immutable Legal
/// document revision and digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureEnvelope {
    id: SignatureEnvelopeId,
    version: u64,
    scope: TenantScope,
    document_id: LegalDocumentId,
    document_version: u64,
    document_sha256: String,
    provider_key: String,
    provider_policy_version: String,
    order: SignatureOrder,
    recipients: Vec<SignatureRecipient>,
    retention_until: DateTime<Utc>,
    status: SignatureStatus,
    provider_envelope_ref: Option<String>,
    last_provider_sequence: u64,
    processed_provider_events: Vec<String>,
    evidence_sha256: Option<String>,
    created_by: String,
    cancelled_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SignatureEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: SignatureEnvelopeId,
        scope: TenantScope,
        document_id: LegalDocumentId,
        document_version: u64,
        document_sha256: String,
        provider_key: String,
        provider_policy_version: String,
        order: SignatureOrder,
        recipients: Vec<SignatureRecipient>,
        retention_until: DateTime<Utc>,
        created_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            document_id,
            document_version,
            document_sha256,
            provider_key,
            provider_policy_version,
            order,
            recipients,
            retention_until,
            status: SignatureStatus::Draft,
            provider_envelope_ref: None,
            last_provider_sequence: 0,
            processed_provider_events: Vec::new(),
            evidence_sha256: None,
            created_by,
            cancelled_by: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn submit(
        &mut self,
        provider_envelope_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        if self.status != SignatureStatus::Draft {
            return Err(LegalDomainError::InvalidSignatureTransition);
        }
        validate_text(
            &provider_envelope_ref,
            "provider_envelope_ref",
            MAX_REFERENCE_BYTES,
        )?;
        let mut next = self.clone();
        advance(&mut next.version, &mut next.updated_at, at)?;
        next.provider_envelope_ref = Some(provider_envelope_ref);
        next.status = SignatureStatus::Submitted;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn apply_callback(
        &mut self,
        callback: VerifiedSignatureCallback,
        received_at: DateTime<Utc>,
    ) -> Result<CallbackOutcome, LegalDomainError> {
        if self
            .processed_provider_events
            .iter()
            .any(|value| value == &callback.provider_event_id)
        {
            return Ok(CallbackOutcome::Duplicate);
        }
        if !callback.verified {
            return Err(LegalDomainError::UnverifiedSignatureCallback);
        }
        if !matches!(
            self.status,
            SignatureStatus::Submitted | SignatureStatus::InProgress
        ) || self.provider_envelope_ref.as_deref() != Some(&callback.provider_envelope_ref)
        {
            return Err(LegalDomainError::InvalidSignatureTransition);
        }
        validate_text(
            &callback.provider_event_id,
            "provider_event_id",
            MAX_REFERENCE_BYTES,
        )?;
        if callback.sequence == 0
            || callback.sequence <= self.last_provider_sequence
            || callback.occurred_at > received_at
            || self.processed_provider_events.len() >= MAX_SIGNATURE_CALLBACKS
        {
            return Err(LegalDomainError::OutOfOrderSignatureCallback);
        }

        let mut next = self.clone();
        advance(&mut next.version, &mut next.updated_at, received_at)?;
        match callback.kind {
            SignatureCallbackKind::Delivered => next.status = SignatureStatus::InProgress,
            SignatureCallbackKind::RecipientSigned { recipient_id } => {
                let Some(index) = next
                    .recipients
                    .iter()
                    .position(|recipient| recipient.id == recipient_id)
                else {
                    return Err(LegalDomainError::InvalidSignatureRecipient);
                };
                if next.recipients[index].signed_at.is_some() {
                    return Err(LegalDomainError::InvalidSignatureTransition);
                }
                if next.order == SignatureOrder::Serial {
                    let expected = next
                        .recipients
                        .iter()
                        .filter(|recipient| recipient.signed_at.is_none())
                        .map(|recipient| recipient.order)
                        .min()
                        .ok_or(LegalDomainError::InvalidSignatureTransition)?;
                    if next.recipients[index].order != expected {
                        return Err(LegalDomainError::InvalidSignatureOrder);
                    }
                }
                next.recipients[index].signed_at = Some(callback.occurred_at);
                next.status = SignatureStatus::InProgress;
            }
            SignatureCallbackKind::Completed { evidence_sha256 } => {
                if next
                    .recipients
                    .iter()
                    .any(|recipient| recipient.signed_at.is_none())
                {
                    return Err(LegalDomainError::InvalidSignatureTransition);
                }
                validate_digest(&evidence_sha256)?;
                next.evidence_sha256 = Some(evidence_sha256);
                next.status = SignatureStatus::Completed;
            }
            SignatureCallbackKind::Failed { reason_code } => {
                validate_text(&reason_code, "failure_reason_code", MAX_REFERENCE_BYTES)?;
                next.status = SignatureStatus::Failed;
            }
            SignatureCallbackKind::Cancelled => next.status = SignatureStatus::Cancelled,
            SignatureCallbackKind::Expired => next.status = SignatureStatus::Expired,
        }
        next.last_provider_sequence = callback.sequence;
        next.processed_provider_events
            .push(callback.provider_event_id);
        next.validate()?;
        *self = next;
        Ok(CallbackOutcome::Applied)
    }

    pub fn cancel(&mut self, actor: String, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        if !matches!(
            self.status,
            SignatureStatus::Draft | SignatureStatus::Submitted | SignatureStatus::InProgress
        ) {
            return Err(LegalDomainError::InvalidSignatureTransition);
        }
        validate_text(&actor, "cancelled_by", MAX_REFERENCE_BYTES)?;
        let mut next = self.clone();
        advance(&mut next.version, &mut next.updated_at, at)?;
        next.cancelled_by = Some(actor);
        next.status = SignatureStatus::Cancelled;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), LegalDomainError> {
        validate_scope(&self.scope)?;
        if self.document_version == 0 {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        validate_digest(&self.document_sha256)?;
        validate_text(&self.provider_key, "provider_key", MAX_REFERENCE_BYTES)?;
        validate_text(
            &self.provider_policy_version,
            "provider_policy_version",
            MAX_REFERENCE_BYTES,
        )?;
        validate_text(&self.created_by, "created_by", MAX_REFERENCE_BYTES)?;
        validate_retention(self.retention_until, self.created_at)?;
        if self.recipients.is_empty() || self.recipients.len() > MAX_SIGNATURE_RECIPIENTS {
            return Err(LegalDomainError::InvalidSignatureRecipient);
        }
        let mut ids = HashSet::with_capacity(self.recipients.len());
        let mut orders = HashSet::with_capacity(self.recipients.len());
        for recipient in &self.recipients {
            recipient.validate()?;
            if !ids.insert(recipient.id)
                || (self.order == SignatureOrder::Serial && !orders.insert(recipient.order))
            {
                return Err(LegalDomainError::InvalidSignatureRecipient);
            }
            if let SignatureSubject::ExternalOneTime { expires_at, .. } = &recipient.subject
                && *expires_at <= self.created_at
            {
                return Err(LegalDomainError::InvalidSignatureRecipient);
            }
        }
        if matches!(
            self.status,
            SignatureStatus::Submitted
                | SignatureStatus::InProgress
                | SignatureStatus::Completed
                | SignatureStatus::Failed
                | SignatureStatus::Expired
        ) && self.provider_envelope_ref.is_none()
        {
            return Err(LegalDomainError::InvalidSignatureTransition);
        }
        if self.status == SignatureStatus::Completed
            && (self.evidence_sha256.is_none()
                || self
                    .recipients
                    .iter()
                    .any(|recipient| recipient.signed_at.is_none()))
        {
            return Err(LegalDomainError::InvalidSignatureTransition);
        }
        if let Some(value) = &self.evidence_sha256 {
            validate_digest(value)?;
        }
        Ok(())
    }

    pub const fn id(&self) -> SignatureEnvelopeId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn document_id(&self) -> LegalDocumentId {
        self.document_id
    }
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
    pub fn document_sha256(&self) -> &str {
        &self.document_sha256
    }
    pub const fn status(&self) -> SignatureStatus {
        self.status
    }
    pub fn recipients(&self) -> &[SignatureRecipient] {
        &self.recipients
    }
    pub fn provider_envelope_ref(&self) -> Option<&str> {
        self.provider_envelope_ref.as_deref()
    }
    pub fn evidence_sha256(&self) -> Option<&str> {
        self.evidence_sha256.as_deref()
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        stable_record_ref(self.id.as_uuid(), "legal.signature-envelope")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicySnapshot {
    pub policy_version: String,
    pub jurisdiction_version: String,
    pub retention_until: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "hold_status", rename_all = "snake_case")]
pub enum HoldEvidence {
    Clear {
        checked_at: DateTime<Utc>,
        evidence_sha256: String,
    },
    Active {
        matter_id: LegalMatterId,
        matter_version: u64,
        evidence_sha256: String,
    },
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionDecision {
    RetainUntilDeadline,
    RetainForLegalHold,
    EligibleForDisposal,
}

impl RetentionDecision {
    pub fn evaluate(
        policy: &RetentionPolicySnapshot,
        hold: &HoldEvidence,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        validate_text(
            &policy.policy_version,
            "retention_policy_version",
            MAX_REFERENCE_BYTES,
        )?;
        validate_text(
            &policy.jurisdiction_version,
            "jurisdiction_version",
            MAX_REFERENCE_BYTES,
        )?;
        match hold {
            HoldEvidence::Unavailable => Err(LegalDomainError::LegalHoldEvidenceUnavailable),
            HoldEvidence::Active {
                matter_version,
                evidence_sha256,
                ..
            } => {
                if *matter_version == 0 {
                    return Err(LegalDomainError::InvalidLegalHold);
                }
                validate_digest(evidence_sha256)?;
                Ok(Self::RetainForLegalHold)
            }
            HoldEvidence::Clear {
                checked_at,
                evidence_sha256,
            } => {
                if *checked_at > at {
                    return Err(LegalDomainError::LegalHoldEvidenceUnavailable);
                }
                validate_digest(evidence_sha256)?;
                Ok(if at < policy.retention_until {
                    Self::RetainUntilDeadline
                } else {
                    Self::EligibleForDisposal
                })
            }
        }
    }
}
