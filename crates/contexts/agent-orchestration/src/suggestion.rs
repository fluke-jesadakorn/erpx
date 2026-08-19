//! Human-controlled AI suggestion envelopes.
//!
//! The envelope deliberately stores only digests and provenance. Typed Sales
//! or Finance payloads stay in their owning context and can be copied into a
//! draft only after the owner validates the source version and payload digest.

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionOwner {
    Sales,
    Finance,
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
}

impl SuggestionCapability {
    pub const fn owner(self) -> SuggestionOwner {
        match self {
            Self::SalesQuoteCompose | Self::SalesRevenueArrangement => SuggestionOwner::Sales,
            Self::FinanceAccountingPlan | Self::FinancePostingReview => SuggestionOwner::Finance,
        }
    }

    pub const fn token(self) -> &'static str {
        match self {
            Self::SalesQuoteCompose => "sales.quote.compose.v1",
            Self::SalesRevenueArrangement => "sales.revenue-arrangement.v1",
            Self::FinanceAccountingPlan => "finance.accounting-plan.v1",
            Self::FinancePostingReview => "finance.posting-review.v1",
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
    provenance: SuggestionProvenance,
    status: SuggestionStatus,
    requested_by: String,
    applied_by: Option<String>,
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
            provenance,
            status: SuggestionStatus::Queued,
            requested_by,
            applied_by: None,
            failure_code: None,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), SuggestionError> {
        self.transition(SuggestionStatus::Queued, SuggestionStatus::Running, at)
    }

    pub fn complete(
        &mut self,
        output_digest: String,
        at: DateTime<Utc>,
    ) -> Result<(), SuggestionError> {
        if !valid_digest(&output_digest) {
            return Err(SuggestionError::InvalidDigest);
        }
        self.transition(SuggestionStatus::Running, SuggestionStatus::Ready, at)?;
        self.output_digest = Some(output_digest);
        Ok(())
    }

    /// Records a human Apply command. The typed payload is copied by the owner
    /// only after this guard succeeds; this method never mutates ERP data.
    pub fn apply(
        &mut self,
        current_source_version: u64,
        current_payload_digest: &str,
        applied_by: String,
        at: DateTime<Utc>,
    ) -> Result<(), SuggestionError> {
        if current_source_version != self.source.record_version
            || current_payload_digest != self.payload_digest
        {
            return Err(SuggestionError::StaleSuggestion);
        }
        if !valid_text(&applied_by, 256) {
            return Err(SuggestionError::HumanCommandRequired);
        }
        self.transition(SuggestionStatus::Ready, SuggestionStatus::Applied, at)?;
        self.applied_by = Some(applied_by);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> SuggestionEnvelope {
        SuggestionEnvelope::request(
            SuggestionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            AuthorityScope::Holding {
                holding_id: Uuid::from_u128(2),
            },
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

    #[test]
    fn suggestion_never_applies_before_ready_or_without_a_human_command() {
        let mut value = envelope();
        let at = value.created_at();
        assert_eq!(
            value.apply(7, &"a".repeat(64), "human:maker".into(), at),
            Err(SuggestionError::InvalidTransition)
        );
        value.start(at).unwrap();
        value.complete("c".repeat(64), at).unwrap();
        assert_eq!(
            value.apply(7, &"a".repeat(64), "".into(), at),
            Err(SuggestionError::HumanCommandRequired)
        );
        value
            .apply(7, &"a".repeat(64), "human:maker".into(), at)
            .unwrap();
        assert_eq!(value.status(), SuggestionStatus::Applied);
    }

    #[test]
    fn stale_source_or_payload_is_rejected() {
        let mut value = envelope();
        let at = value.created_at();
        value.start(at).unwrap();
        value.complete("c".repeat(64), at).unwrap();
        assert_eq!(
            value.apply(8, &"a".repeat(64), "human:maker".into(), at),
            Err(SuggestionError::StaleSuggestion)
        );
        assert_eq!(
            value.apply(7, &"d".repeat(64), "human:maker".into(), at),
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
}
