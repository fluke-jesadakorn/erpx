//! Human-controlled AI suggestion envelopes.
//!
//! The envelope deliberately stores only digests and provenance. Typed Sales
//! or Finance payloads stay in their owning context and can be copied into a
//! draft only after the owner validates the source version and payload digest.

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    FieldPatchConflict, HumanFieldPatchReview, RecordFieldPatch, record_editable_field_schema,
};
use erpx_kernel::{ActorKind, AuthorityScope, HumanDecisionProof, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const SUGGESTION_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SuggestionId(Uuid);

impl SuggestionId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, SuggestionError> {
        if value.is_nil() {
            return Err(SuggestionError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

pub fn suggestion_apply_decision_ref(suggestion_id: SuggestionId) -> String {
    format!("agent.suggestion:{}:apply", suggestion_id.as_uuid())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionOwner {
    Sales,
    Finance,
    RecordOwner(erpx_integration_contracts::catalog::ContextId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuggestionCapability {
    #[serde(rename = "sales.quote.compose.v1")]
    SalesQuoteCompose,
    #[serde(rename = "sales.revenue-arrangement.v1")]
    SalesRevenueArrangement,
    #[serde(rename = "finance.accounting-plan.v1")]
    FinanceAccountingPlan,
    #[serde(rename = "finance.posting-review.v1")]
    FinancePostingReview,
    #[serde(rename = "record.field-patch.v2")]
    RecordFieldPatch {
        owner: erpx_integration_contracts::catalog::ContextId,
    },
}

impl SuggestionCapability {
    pub const fn owner(self) -> SuggestionOwner {
        match self {
            Self::SalesQuoteCompose | Self::SalesRevenueArrangement => SuggestionOwner::Sales,
            Self::FinanceAccountingPlan | Self::FinancePostingReview => SuggestionOwner::Finance,
            Self::RecordFieldPatch { owner } => SuggestionOwner::RecordOwner(owner),
        }
    }

    pub const fn token(self) -> &'static str {
        match self {
            Self::SalesQuoteCompose => "sales.quote.compose.v1",
            Self::SalesRevenueArrangement => "sales.revenue-arrangement.v1",
            Self::FinanceAccountingPlan => "finance.accounting-plan.v1",
            Self::FinancePostingReview => "finance.posting-review.v1",
            Self::RecordFieldPatch { .. } => "record.field-patch.v2",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionSource {
    pub record_kind: String,
    pub record_id: Uuid,
    pub record_version: u64,
}

impl SuggestionSource {
    fn validate(&self) -> Result<(), SuggestionError> {
        if !valid_text(&self.record_kind, 128)
            || self.record_id.is_nil()
            || self.record_version == 0
        {
            return Err(SuggestionError::InvalidSource);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionProviderKind {
    Fixture,
    Ollama,
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionProvenance {
    pub provider_kind: SuggestionProviderKind,
    pub provider_ref: String,
    pub model_ref: String,
    pub prompt_digest: String,
    pub invocation_evidence_ref: String,
}

impl SuggestionProvenance {
    fn validate(&self, local_development: bool) -> Result<(), SuggestionError> {
        if self.provider_kind == SuggestionProviderKind::Fixture && !local_development {
            return Err(SuggestionError::FixtureProviderDenied);
        }
        if !valid_text(&self.provider_ref, 128)
            || !valid_text(&self.model_ref, 128)
            || !valid_text(&self.invocation_evidence_ref, 256)
            || !valid_digest(&self.prompt_digest)
        {
            return Err(SuggestionError::InvalidProvenance);
        }
        Ok(())
    }

    pub const fn display_label(&self) -> &'static str {
        match self.provider_kind {
            SuggestionProviderKind::Fixture => "Fixture AI",
            SuggestionProviderKind::Ollama => "Ollama",
            SuggestionProviderKind::OpenAiCompatible => "OpenAI-compatible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Queued,
    Running,
    Ready,
    Conflicted,
    Applied,
    Rejected,
    Superseded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionEnvelope {
    id: SuggestionId,
    version: u64,
    scope: AuthorityScope,
    owner: SuggestionOwner,
    capability: SuggestionCapability,
    source: SuggestionSource,
    schema_version: u16,
    policy_version: u32,
    payload_digest: String,
    output_digest: Option<String>,
    #[serde(default)]
    field_patch: Option<RecordFieldPatch>,
    #[serde(default)]
    field_patch_conflicts: Vec<FieldPatchConflict>,
    #[serde(default)]
    human_field_patch_review: Option<HumanFieldPatchReview>,
    provenance: SuggestionProvenance,
    status: SuggestionStatus,
    requested_by: String,
    applied_by: Option<String>,
    #[serde(default)]
    applied_actor_kind: Option<ActorKind>,
    #[serde(default)]
    applied_decision_ref: Option<String>,
    #[serde(default)]
    applied_session_id: Option<SessionId>,
    failure_code: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SuggestionEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn request(
        id: SuggestionId,
        scope: AuthorityScope,
        capability: SuggestionCapability,
        source: SuggestionSource,
        policy_version: u32,
        payload_digest: String,
        provenance: SuggestionProvenance,
        requested_by: String,
        local_development: bool,
        at: DateTime<Utc>,
    ) -> Result<Self, SuggestionError> {
        scope
            .validate()
            .map_err(|_| SuggestionError::InvalidScope)?;
        source.validate()?;
        provenance.validate(local_development)?;
        if let SuggestionCapability::RecordFieldPatch { owner } = capability {
            let schema = record_editable_field_schema(&source.record_kind)
                .filter(|schema| schema.owner == owner && schema.supports_agent_patches())
                .ok_or(SuggestionError::FieldSchemaUnavailable)?;
            if schema.version != u64::from(SUGGESTION_SCHEMA_VERSION) {
                return Err(SuggestionError::FieldSchemaUnavailable);
            }
        }
        if policy_version == 0 || !valid_digest(&payload_digest) || !valid_text(&requested_by, 256)
        {
            return Err(SuggestionError::InvalidEnvelope);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            owner: capability.owner(),
            capability,
            source,
            schema_version: SUGGESTION_SCHEMA_VERSION,
            policy_version,
            payload_digest,
            output_digest: None,
            field_patch: None,
            field_patch_conflicts: Vec::new(),
            human_field_patch_review: None,
            provenance,
            status: SuggestionStatus::Queued,
            requested_by,
            applied_by: None,
            applied_actor_kind: None,
            applied_decision_ref: None,
            applied_session_id: None,
            failure_code: None,
            created_at: at,
            updated_at: at,
        })
    }

    /// Revalidates a deserialized durable envelope before it crosses an
    /// application or projection boundary.
    pub fn validate(&self) -> Result<(), SuggestionError> {
        self.scope
            .validate()
            .map_err(|_| SuggestionError::InvalidScope)?;
        self.source.validate()?;
        self.provenance.validate(true)?;
        if self.version == 0
            || self.schema_version != SUGGESTION_SCHEMA_VERSION
            || self.policy_version == 0
            || self.owner != self.capability.owner()
            || !valid_digest(&self.payload_digest)
            || !valid_text(&self.requested_by, 256)
            || self.updated_at < self.created_at
        {
            return Err(SuggestionError::InvalidEnvelope);
        }
        let requires_output = matches!(
            self.status,
            SuggestionStatus::Ready
                | SuggestionStatus::Conflicted
                | SuggestionStatus::Applied
                | SuggestionStatus::Rejected
        );
        if (requires_output
            && self
                .output_digest
                .as_deref()
                .is_none_or(|value| !valid_digest(value)))
            || self
                .output_digest
                .as_deref()
                .is_some_and(|value| !valid_digest(value))
        {
            return Err(SuggestionError::InvalidEnvelope);
        }
        match self.capability {
            SuggestionCapability::RecordFieldPatch { owner } => {
                let schema = record_editable_field_schema(&self.source.record_kind)
                    .filter(|schema| schema.owner == owner && schema.supports_agent_patches())
                    .ok_or(SuggestionError::FieldSchemaUnavailable)?;
                if schema.version != u64::from(self.schema_version) {
                    return Err(SuggestionError::FieldSchemaUnavailable);
                }
                if self.output_digest.is_some() {
                    self.field_patch
                        .as_ref()
                        .ok_or(SuggestionError::TypedFieldPatchRequired)?
                        .validate()
                        .map_err(|_| SuggestionError::InvalidFieldPatch)?;
                }
            }
            _ if self.field_patch.is_some()
                || !self.field_patch_conflicts.is_empty()
                || self.human_field_patch_review.is_some() =>
            {
                return Err(SuggestionError::InvalidFieldPatch);
            }
            _ => {}
        }
        if self.status == SuggestionStatus::Conflicted && self.field_patch_conflicts.is_empty() {
            return Err(SuggestionError::InvalidFieldPatch);
        }
        if self.status == SuggestionStatus::Applied {
            if self.applied_actor_kind != Some(ActorKind::HoldingUser)
                || self
                    .applied_by
                    .as_deref()
                    .is_none_or(|value| !valid_text(value, 256))
                || self
                    .applied_decision_ref
                    .as_deref()
                    .is_none_or(|value| !valid_text(value, 256))
                || self.applied_session_id.is_none()
            {
                return Err(SuggestionError::HumanCommandRequired);
            }
            if matches!(
                self.capability,
                SuggestionCapability::RecordFieldPatch { .. }
            ) {
                self.human_field_patch_review
                    .as_ref()
                    .ok_or(SuggestionError::HumanSessionRequired)?
                    .validate()
                    .map_err(|_| SuggestionError::InvalidFieldPatch)?;
            }
        }
        Ok(())
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        self.transition(SuggestionStatus::Queued, SuggestionStatus::Running, at)
    }

    pub fn complete(
        &mut self,
        output_digest: String,
        at: DateTime<Utc>,
    ) -> Result<(), SuggestionError> {
        if matches!(
            self.capability,
            SuggestionCapability::RecordFieldPatch { .. }
        ) {
            return Err(SuggestionError::TypedFieldPatchRequired);
        }
        if !valid_digest(&output_digest) {
            return Err(SuggestionError::InvalidDigest);
        }
        self.transition(SuggestionStatus::Running, SuggestionStatus::Ready, at)?;
        self.output_digest = Some(output_digest);
        Ok(())
    }

    pub fn complete_field_patch(
        &mut self,
        patch: RecordFieldPatch,
        output_digest: String,
        at: DateTime<Utc>,
    ) -> Result<(), SuggestionError> {
        if !matches!(
            self.capability,
            SuggestionCapability::RecordFieldPatch { .. }
        ) || !valid_digest(&output_digest)
        {
            return Err(SuggestionError::TypedFieldPatchRequired);
        }
        patch
            .validate()
            .map_err(|_| SuggestionError::InvalidFieldPatch)?;
        if patch.source.record().record_type().as_str() != self.source.record_kind
            || patch.source.record().id().as_uuid() != self.source.record_id
            || patch.source.version() != self.source.record_version
        {
            return Err(SuggestionError::InvalidFieldPatch);
        }
        self.transition(SuggestionStatus::Running, SuggestionStatus::Ready, at)?;
        self.output_digest = Some(output_digest);
        self.field_patch = Some(patch);
        Ok(())
    }

    pub fn apply_field_patch(
        &mut self,
        edited_patch: RecordFieldPatch,
        current_source_version: u64,
        current_payload_digest: &str,
        conflicts: Vec<FieldPatchConflict>,
        decision: HumanDecisionProof,
    ) -> Result<(), SuggestionError> {
        if !matches!(
            self.capability,
            SuggestionCapability::RecordFieldPatch { .. }
        ) || decision.authority_scope() != &self.scope
            || decision.decision_ref() != suggestion_apply_decision_ref(self.id)
        {
            return Err(SuggestionError::HumanCommandRequired);
        }
        edited_patch
            .validate()
            .map_err(|_| SuggestionError::InvalidFieldPatch)?;
        if self.field_patch.is_none()
            || edited_patch.source.record().record_type().as_str() != self.source.record_kind
            || edited_patch.source.record().id().as_uuid() != self.source.record_id
            || edited_patch.source.version() != self.source.record_version
        {
            return Err(SuggestionError::InvalidFieldPatch);
        }
        if current_source_version != self.source.record_version
            || current_payload_digest != self.payload_digest
        {
            if conflicts.is_empty()
                || conflicts.iter().any(|conflict| {
                    conflict.current_record_version == 0
                        || conflict.field.record() != edited_patch.source.record()
                })
            {
                return Err(SuggestionError::StaleSuggestion);
            }
            self.transition(
                SuggestionStatus::Ready,
                SuggestionStatus::Conflicted,
                decision.decided_at(),
            )?;
            self.field_patch_conflicts = conflicts;
            return Ok(());
        }
        if !conflicts.is_empty() {
            return Err(SuggestionError::InvalidFieldPatch);
        }
        let session_id = decision
            .actor()
            .session_id
            .ok_or(SuggestionError::HumanSessionRequired)?;
        let edited_patch_digest = digest_patch(&edited_patch)?;
        let review = HumanFieldPatchReview {
            reviewer_principal_ref: decision.principal_ref().to_owned(),
            reviewer_session_id: session_id,
            reviewed_at: decision.decided_at(),
            edited_patch_digest,
        };
        review
            .validate()
            .map_err(|_| SuggestionError::InvalidFieldPatch)?;
        self.transition(
            SuggestionStatus::Ready,
            SuggestionStatus::Applied,
            decision.decided_at(),
        )?;
        self.applied_by = Some(decision.principal_ref().to_owned());
        self.applied_actor_kind = Some(decision.actor().kind);
        self.applied_decision_ref = Some(decision.decision_ref().to_owned());
        self.applied_session_id = Some(session_id);
        self.field_patch = Some(edited_patch);
        self.human_field_patch_review = Some(review);
        Ok(())
    }

    /// Records a human Apply command. The typed payload is copied by the owner
    /// only after this guard succeeds; this method never mutates ERP data.
    pub fn apply(
        &mut self,
        current_source_version: u64,
        current_payload_digest: &str,
        decision: HumanDecisionProof,
    ) -> Result<(), SuggestionError> {
        if decision.authority_scope() != &self.scope
            || decision.decision_ref() != suggestion_apply_decision_ref(self.id)
        {
            return Err(SuggestionError::HumanCommandRequired);
        }
        if current_source_version != self.source.record_version
            || current_payload_digest != self.payload_digest
        {
            return Err(SuggestionError::StaleSuggestion);
        }
        let applied_by = decision.principal_ref().to_owned();
        let applied_actor_kind = decision.actor().kind;
        let applied_decision_ref = decision.decision_ref().to_owned();
        let applied_session_id = decision.actor().session_id;
        let at = decision.decided_at();
        self.transition(SuggestionStatus::Ready, SuggestionStatus::Applied, at)?;
        self.applied_by = Some(applied_by);
        self.applied_actor_kind = Some(applied_actor_kind);
        self.applied_decision_ref = Some(applied_decision_ref);
        self.applied_session_id = applied_session_id;
        Ok(())
    }

    pub fn reject(&mut self, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        self.transition(SuggestionStatus::Ready, SuggestionStatus::Rejected, at)
    }

    pub fn supersede(&mut self, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        if matches!(
            self.status,
            SuggestionStatus::Rejected | SuggestionStatus::Superseded | SuggestionStatus::Failed
        ) {
            return Err(SuggestionError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = SuggestionStatus::Superseded;
        Ok(())
    }

    pub fn fail(&mut self, failure_code: String, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        if !matches!(
            self.status,
            SuggestionStatus::Queued | SuggestionStatus::Running
        ) || !valid_text(&failure_code, 128)
        {
            return Err(SuggestionError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = SuggestionStatus::Failed;
        self.failure_code = Some(failure_code);
        Ok(())
    }

    fn transition(
        &mut self,
        expected: SuggestionStatus,
        next: SuggestionStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SuggestionError> {
        if self.status != expected {
            return Err(SuggestionError::InvalidTransition);
        }
        self.advance(at)?;
        self.status = next;
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        if at < self.updated_at {
            return Err(SuggestionError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SuggestionError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> SuggestionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn owner(&self) -> SuggestionOwner {
        self.owner
    }
    pub const fn capability(&self) -> SuggestionCapability {
        self.capability
    }
    pub const fn source(&self) -> &SuggestionSource {
        &self.source
    }
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }
    pub const fn policy_version(&self) -> u32 {
        self.policy_version
    }
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
    pub fn output_digest(&self) -> Option<&str> {
        self.output_digest.as_deref()
    }
    pub const fn field_patch(&self) -> Option<&RecordFieldPatch> {
        self.field_patch.as_ref()
    }
    pub fn field_patch_conflicts(&self) -> &[FieldPatchConflict] {
        &self.field_patch_conflicts
    }
    pub const fn human_field_patch_review(&self) -> Option<&HumanFieldPatchReview> {
        self.human_field_patch_review.as_ref()
    }
    pub const fn provenance(&self) -> &SuggestionProvenance {
        &self.provenance
    }
    pub const fn status(&self) -> SuggestionStatus {
        self.status
    }
    pub fn requested_by(&self) -> &str {
        &self.requested_by
    }
    pub fn applied_by(&self) -> Option<&str> {
        self.applied_by.as_deref()
    }
    pub const fn applied_actor_kind(&self) -> Option<ActorKind> {
        self.applied_actor_kind
    }
    pub fn applied_decision_ref(&self) -> Option<&str> {
        self.applied_decision_ref.as_deref()
    }
    pub const fn applied_session_id(&self) -> Option<SessionId> {
        self.applied_session_id
    }
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SuggestionError {
    #[error("suggestion identifier is invalid")]
    InvalidIdentifier,
    #[error("suggestion scope is invalid")]
    InvalidScope,
    #[error("suggestion source is invalid")]
    InvalidSource,
    #[error("suggestion envelope is invalid")]
    InvalidEnvelope,
    #[error("suggestion digest is invalid")]
    InvalidDigest,
    #[error("owner field schema is unavailable")]
    FieldSchemaUnavailable,
    #[error("a typed field patch is required")]
    TypedFieldPatchRequired,
    #[error("typed field patch is invalid")]
    InvalidFieldPatch,
    #[error("suggestion provenance is invalid")]
    InvalidProvenance,
    #[error("Fixture AI is available only in local development")]
    FixtureProviderDenied,
    #[error("suggestion transition is invalid")]
    InvalidTransition,
    #[error("suggestion is stale for the current source version or payload")]
    StaleSuggestion,
    #[error("a human Apply command is required")]
    HumanCommandRequired,
    #[error("an authenticated human session is required")]
    HumanSessionRequired,
    #[error("suggestion timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("suggestion version overflowed")]
    VersionOverflow,
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn digest_patch(patch: &RecordFieldPatch) -> Result<String, SuggestionError> {
    let bytes = serde_json::to_vec(patch).map_err(|_| SuggestionError::InvalidFieldPatch)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use erpx_integration_contracts::{
        FieldPatchConflictReason, RecordTypeId, StableFieldRef, StableRecordRef,
        SuggestedFieldChange, SuggestedFieldValue, catalog::ContextId,
    };
    use erpx_kernel::{
        ActorAttribution, ActorKind, HumanDecisionError, HumanDecisionProof, RecordId, SessionId,
    };

    use super::*;

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding {
            holding_id: Uuid::from_u128(2),
        }
    }

    fn human_decision(reference: &str, at: DateTime<Utc>) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "human:maker".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(
                    SessionId::try_from_uuid(Uuid::from_u128(9)).expect("non-nil session"),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            reference,
            at,
        )
        .unwrap()
    }

    fn envelope() -> SuggestionEnvelope {
        SuggestionEnvelope::request(
            SuggestionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            scope(),
            SuggestionCapability::SalesQuoteCompose,
            SuggestionSource {
                record_kind: "quote".into(),
                record_id: Uuid::from_u128(3),
                record_version: 7,
            },
            2,
            "a".repeat(64),
            SuggestionProvenance {
                provider_kind: SuggestionProviderKind::Fixture,
                provider_ref: "fixture-ai-v1".into(),
                model_ref: "deterministic".into(),
                prompt_digest: "b".repeat(64),
                invocation_evidence_ref: "assignment:fixture-1".into(),
            },
            "human:sales-maker".into(),
            true,
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    fn field_patch() -> RecordFieldPatch {
        let record = StableRecordRef::try_new(
            RecordId::try_from_uuid(Uuid::from_u128(31)).unwrap(),
            ContextId::Sales,
            RecordTypeId::try_new("sales.quote").unwrap(),
        )
        .unwrap();
        RecordFieldPatch {
            source: erpx_integration_contracts::RecordVersionRef::try_new(record.clone(), 7)
                .unwrap(),
            schema_version: 1,
            changes: vec![SuggestedFieldChange {
                field: StableFieldRef::try_new(record, "quote.notes", 1).unwrap(),
                previous_value_digest: "d".repeat(64),
                proposed_value: SuggestedFieldValue::Text("Prepared for review".into()),
                confidence_basis_points: 8_800,
                evidence: Vec::new(),
            }],
        }
    }

    fn field_envelope() -> SuggestionEnvelope {
        SuggestionEnvelope::request(
            SuggestionId::try_from_uuid(Uuid::from_u128(30)).unwrap(),
            scope(),
            SuggestionCapability::RecordFieldPatch {
                owner: ContextId::Sales,
            },
            SuggestionSource {
                record_kind: "sales.quote".into(),
                record_id: Uuid::from_u128(31),
                record_version: 7,
            },
            1,
            "a".repeat(64),
            envelope().provenance().clone(),
            "human:sales-maker".into(),
            true,
            DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn suggestion_never_applies_before_ready_or_without_a_human_command() {
        let mut value = envelope();
        let at = value.created_at();
        let decision_ref = suggestion_apply_decision_ref(value.id());
        assert_eq!(
            value.apply(7, &"a".repeat(64), human_decision(&decision_ref, at)),
            Err(SuggestionError::InvalidTransition)
        );
        value.start(at).unwrap();
        value.complete("c".repeat(64), at).unwrap();
        assert_eq!(
            HumanDecisionProof::try_new(
                ActorAttribution {
                    principal_ref: "agent:suggester".into(),
                    kind: ActorKind::Agent,
                    session_id: Some(
                        SessionId::try_from_uuid(Uuid::from_u128(10)).expect("non-nil session"),
                    ),
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                scope(),
                decision_ref.clone(),
                at,
            ),
            Err(HumanDecisionError::HumanActorRequired)
        );
        assert_eq!(
            value.apply(
                7,
                &"a".repeat(64),
                human_decision(
                    &suggestion_apply_decision_ref(
                        SuggestionId::try_from_uuid(Uuid::from_u128(99)).unwrap(),
                    ),
                    at,
                ),
            ),
            Err(SuggestionError::HumanCommandRequired)
        );
        value
            .apply(7, &"a".repeat(64), human_decision(&decision_ref, at))
            .unwrap();
        assert_eq!(value.status(), SuggestionStatus::Applied);
        assert_eq!(value.applied_actor_kind(), Some(ActorKind::HoldingUser));
        assert_eq!(value.applied_decision_ref(), Some(decision_ref.as_str()));
        assert!(value.applied_session_id().is_some());
    }

    #[test]
    fn stale_source_or_payload_is_rejected() {
        let mut value = envelope();
        let at = value.created_at();
        let decision_ref = suggestion_apply_decision_ref(value.id());
        value.start(at).unwrap();
        value.complete("c".repeat(64), at).unwrap();
        assert_eq!(
            value.apply(8, &"a".repeat(64), human_decision(&decision_ref, at)),
            Err(SuggestionError::StaleSuggestion)
        );
        assert_eq!(
            value.apply(7, &"d".repeat(64), human_decision(&decision_ref, at)),
            Err(SuggestionError::StaleSuggestion)
        );
        assert_eq!(value.status(), SuggestionStatus::Ready);
    }

    #[test]
    fn fixture_provider_fails_closed_outside_local_development() {
        let mut provenance = envelope().provenance().clone();
        assert_eq!(
            provenance.validate(false),
            Err(SuggestionError::FixtureProviderDenied)
        );
        provenance.provider_kind = SuggestionProviderKind::Ollama;
        provenance.validate(false).unwrap();
    }

    #[test]
    fn universal_field_patch_requires_typed_output_and_persists_final_human_editor() {
        let mut value = field_envelope();
        let at = value.created_at();
        value.start(at).unwrap();
        assert_eq!(
            value.complete("c".repeat(64), at),
            Err(SuggestionError::TypedFieldPatchRequired)
        );
        value
            .complete_field_patch(field_patch(), "c".repeat(64), at)
            .unwrap();
        let decision_ref = suggestion_apply_decision_ref(value.id());
        value
            .apply_field_patch(
                field_patch(),
                7,
                &"a".repeat(64),
                Vec::new(),
                human_decision(&decision_ref, at),
            )
            .unwrap();
        assert_eq!(value.status(), SuggestionStatus::Applied);
        let review = value
            .human_field_patch_review()
            .expect("human review evidence");
        assert_eq!(review.reviewer_principal_ref, "human:maker");
        assert!(value.field_patch_conflicts().is_empty());
    }

    #[test]
    fn universal_field_patch_exposes_typed_stale_conflicts_without_applying() {
        let mut value = field_envelope();
        let at = value.created_at();
        value.start(at).unwrap();
        value
            .complete_field_patch(field_patch(), "c".repeat(64), at)
            .unwrap();
        let patch = field_patch();
        let conflict = FieldPatchConflict {
            field: patch.changes[0].field.clone(),
            reason: FieldPatchConflictReason::FieldValueChanged,
            current_record_version: 8,
        };
        value
            .apply_field_patch(
                patch,
                8,
                &"e".repeat(64),
                vec![conflict],
                human_decision(&suggestion_apply_decision_ref(value.id()), at),
            )
            .unwrap();
        assert_eq!(value.status(), SuggestionStatus::Conflicted);
        assert_eq!(value.field_patch_conflicts().len(), 1);
        assert!(value.human_field_patch_review().is_none());
    }
}
