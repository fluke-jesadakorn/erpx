//! Tenant-bound Marketing and CRM acquisition rules.
//!
//! This module owns consent and campaign policy only. Channel adapters,
//! templates, journeys, CRM handoff, attribution, and delivery workers remain
//! infrastructure/application concerns.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::TenantScope;

pub const MAX_MARKETING_TEXT: usize = 255;
pub const MAX_MARKETING_POLICY_VERSION: usize = 64;
pub const MAX_MARKETING_PROFILE_FIELDS: usize = 64;
pub const MAX_AUDIENCE_MEMBERSHIP: usize = 100_000;
pub const MAX_MARKETING_PROFILE_ERASURE_REASON: usize = 512;
pub const MARKETING_PROFILE_REDACTED_MARKER: &str = "[REDACTED]";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("marketing scope is invalid or not contained by the tenant")]
    InvalidScope,
    #[error("marketing profile erasure scope is not contained by the authorizing scope")]
    ErasureScopeMismatch,
    #[error("{field} timestamp is outside the representable precision range")]
    InvalidTimestamp { field: &'static str },
    #[error("consent record is not active for the requested purpose and channel")]
    ConsentRequired,
    #[error("marketing profile is suppressed")]
    Suppressed,
    #[error("double opt-in is required before delivery")]
    DoubleOptInRequired,
    #[error("campaign lifecycle transition is not allowed")]
    InvalidCampaignTransition,
    #[error("campaign tenant scope does not match the requested scope")]
    CampaignScopeMismatch,
    #[error("delivery idempotency key has already been used for another message")]
    DeliveryIdempotencyConflict,
    #[error("audience membership exceeds the bounded maximum")]
    AudienceTooLarge,
}

/// Tenant-scoped source/read reconciliation facts for the disposable
/// Marketing/CRM projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MarketingProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    pub payload_mismatch_count: u64,
}

impl MarketingProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.source_event_count >= self.source_aggregate_count
            && self.projected_aggregate_count == self.source_aggregate_count
            && self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.payload_mismatch_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingPurpose {
    Newsletter,
    ProductUpdates,
    Promotional,
    Transactional,
    LeadNurture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingChannel {
    Email,
    Sms,
    Social,
    Push,
    Webhook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentState {
    PendingDoubleOptIn,
    Granted,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProfile {
    pub id: Uuid,
    pub scope: TenantScope,
    pub external_ref: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub locale: String,
    pub dedupe_key: String,
    pub attributes: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MarketingProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        external_ref: impl Into<String>,
        email: Option<String>,
        phone: Option<String>,
        locale: impl Into<String>,
        attributes: BTreeMap<String, String>,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingError> {
        let external_ref = external_ref.into();
        let locale = locale.into();
        let dedupe_key = dedupe_key(email.as_deref(), phone.as_deref(), &external_ref);
        let value = Self {
            id,
            scope,
            external_ref,
            email,
            phone,
            locale,
            dedupe_key,
            attributes,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingError> {
        if self.id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "profile_id",
            });
        }
        validate_scope(&self.scope)?;
        bounded(&self.external_ref, "external_ref", MAX_MARKETING_TEXT)?;
        bounded(&self.locale, "locale", 32)?;
        if self.email.is_none() && self.phone.is_none() {
            return Err(MarketingError::Required {
                field: "email_or_phone",
            });
        }
        for value in [&self.email, &self.phone].into_iter().flatten() {
            bounded(value, "contact", MAX_MARKETING_TEXT)?;
        }
        if self.attributes.len() > MAX_MARKETING_PROFILE_FIELDS {
            return Err(MarketingError::TooLong {
                field: "attributes",
                max: MAX_MARKETING_PROFILE_FIELDS,
            });
        }
        for (key, value) in &self.attributes {
            bounded(key, "attribute_key", MAX_MARKETING_TEXT)?;
            bounded(value, "attribute_value", MAX_MARKETING_TEXT)?;
        }
        Ok(())
    }
}

/// A tenant-scoped, contact-free command to redact a Marketing profile.
///
/// The erasure contract intentionally carries neither a profile snapshot nor
/// any contact/attribute values. Infrastructure must use `profile_id` and
/// `scope` to locate the write-side record, then project the stable redacted
/// representation returned by [`Self::redacted_projection`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProfileErasure {
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub erased_at: DateTime<Utc>,
    pub reason: String,
}

impl MarketingProfileErasure {
    pub fn new(
        profile_id: Uuid,
        scope: TenantScope,
        erased_at: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> Result<Self, MarketingError> {
        let value = Self {
            profile_id,
            scope,
            erased_at,
            reason: reason.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingError> {
        if self.profile_id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "profile_id",
            });
        }
        validate_erasure_scope(&self.scope)?;
        if self.erased_at.timestamp_nanos_opt().is_none() {
            return Err(MarketingError::InvalidTimestamp { field: "erased_at" });
        }
        bounded(
            &self.reason,
            "erasure_reason",
            MAX_MARKETING_PROFILE_ERASURE_REASON,
        )?;
        Ok(())
    }

    /// Validates that an actor's scope contains the profile erasure target.
    pub fn validate_for_scope(
        &self,
        authorizing_scope: &TenantScope,
    ) -> Result<(), MarketingError> {
        self.validate()?;
        validate_scope(authorizing_scope)?;
        if authorizing_scope.contains(&self.scope) {
            Ok(())
        } else {
            Err(MarketingError::ErasureScopeMismatch)
        }
    }

    pub fn redacted_projection(&self) -> MarketingProfileRedactedProjection {
        MarketingProfileRedactedProjection {
            profile_id: self.profile_id,
            scope: self.scope.clone(),
            erased_at: self.erased_at,
            marker: MARKETING_PROFILE_REDACTED_MARKER.to_owned(),
        }
    }
}

/// The only profile data emitted after erasure. It contains no contact,
/// external-reference, dedupe, locale, or attribute values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProfileRedactedProjection {
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub erased_at: DateTime<Utc>,
    pub marker: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub purpose: MarketingPurpose,
    pub channel: MarketingChannel,
    pub state: ConsentState,
    pub source: String,
    pub jurisdiction: String,
    pub policy_version: String,
    pub proof_ref: Option<String>,
    pub recorded_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ConsentRecord {
    pub fn validate(&self) -> Result<(), MarketingError> {
        if self.id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "consent_id",
            });
        }
        if self.profile_id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "profile_id",
            });
        }
        validate_scope(&self.scope)?;
        bounded(&self.source, "consent_source", MAX_MARKETING_TEXT)?;
        bounded(&self.jurisdiction, "jurisdiction", 64)?;
        bounded(
            &self.policy_version,
            "policy_version",
            MAX_MARKETING_POLICY_VERSION,
        )?;
        if let Some(proof_ref) = &self.proof_ref {
            bounded(proof_ref, "proof_ref", MAX_MARKETING_TEXT)?;
        }
        if self
            .expires_at
            .is_some_and(|expires_at| expires_at <= self.recorded_at)
        {
            return Err(MarketingError::InvalidScope);
        }
        if self.state == ConsentState::Granted && self.proof_ref.is_none() {
            return Err(MarketingError::Required { field: "proof_ref" });
        }
        Ok(())
    }

    pub fn is_granted_at(
        &self,
        profile_id: Uuid,
        purpose: MarketingPurpose,
        channel: MarketingChannel,
        at: DateTime<Utc>,
    ) -> bool {
        self.validate().is_ok()
            && self.profile_id == profile_id
            && self.purpose == purpose
            && self.channel == channel
            && self.state == ConsentState::Granted
            && self.recorded_at <= at
            && self.expires_at.is_none_or(|expires_at| at < expires_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuppressionRecord {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub reason: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl SuppressionRecord {
    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.created_at <= at && self.expires_at.is_none_or(|expires_at| at < expires_at)
    }

    pub fn validate(&self) -> Result<(), MarketingError> {
        if self.id.is_nil() || self.profile_id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "suppression_id",
            });
        }
        validate_scope(&self.scope)?;
        bounded(&self.reason, "suppression_reason", MAX_MARKETING_TEXT)?;
        bounded(&self.source, "suppression_source", MAX_MARKETING_TEXT)?;
        if self
            .expires_at
            .is_some_and(|expires_at| expires_at <= self.created_at)
        {
            return Err(MarketingError::InvalidScope);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudienceMembership {
    pub audience_id: Uuid,
    pub scope: TenantScope,
    pub profile_ids: Vec<Uuid>,
    pub materialized_at: DateTime<Utc>,
}

impl AudienceMembership {
    pub fn validate(&self) -> Result<(), MarketingError> {
        if self.audience_id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "audience_id",
            });
        }
        validate_scope(&self.scope)?;
        if self.profile_ids.len() > MAX_AUDIENCE_MEMBERSHIP {
            return Err(MarketingError::AudienceTooLarge);
        }
        let mut unique = BTreeSet::new();
        for profile_id in &self.profile_ids {
            if profile_id.is_nil() || !unique.insert(*profile_id) {
                return Err(MarketingError::NilIdentifier {
                    field: "profile_id_or_duplicate",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Draft,
    Review,
    Approved,
    Scheduled,
    Running,
    Paused,
    Completed,
    Archived,
}

impl CampaignStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Review)
                | (Self::Review, Self::Approved | Self::Draft)
                | (Self::Approved, Self::Scheduled | Self::Draft)
                | (Self::Scheduled, Self::Running | Self::Draft)
                | (Self::Running, Self::Paused | Self::Completed)
                | (Self::Paused, Self::Running | Self::Completed)
                | (Self::Completed, Self::Archived)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Campaign {
    pub id: Uuid,
    pub scope: TenantScope,
    pub name: String,
    pub status: CampaignStatus,
    pub audience_id: Uuid,
    pub purpose: MarketingPurpose,
    pub channel: MarketingChannel,
    pub updated_at: DateTime<Utc>,
}

impl Campaign {
    pub fn transition(
        &mut self,
        next: CampaignStatus,
        scope: &TenantScope,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingError> {
        if !scope.contains(&self.scope) || !self.status.can_transition_to(next) {
            return Err(if !scope.contains(&self.scope) {
                MarketingError::CampaignScopeMismatch
            } else {
                MarketingError::InvalidCampaignTransition
            });
        }
        self.status = next;
        self.updated_at = at;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeliveryLedger {
    messages: BTreeMap<String, (Uuid, DateTime<Utc>)>,
}

impl DeliveryLedger {
    #[allow(clippy::too_many_arguments)]
    pub fn authorize(
        &mut self,
        profile: &MarketingProfile,
        consents: &[ConsentRecord],
        suppressions: &[SuppressionRecord],
        campaign: &Campaign,
        message_id: Uuid,
        idempotency_key: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, MarketingError> {
        profile.validate()?;
        if message_id.is_nil() {
            return Err(MarketingError::NilIdentifier {
                field: "message_id",
            });
        }
        bounded(
            idempotency_key,
            "delivery_idempotency_key",
            MAX_MARKETING_TEXT,
        )?;
        if !campaign.scope.contains(&profile.scope) {
            return Err(MarketingError::CampaignScopeMismatch);
        }
        if campaign.status != CampaignStatus::Running {
            return Err(MarketingError::InvalidCampaignTransition);
        }
        if suppressions
            .iter()
            .any(|suppression| suppression.profile_id == profile.id && suppression.is_active_at(at))
        {
            return Err(MarketingError::Suppressed);
        }
        if !consents.iter().any(|consent| {
            consent.is_granted_at(profile.id, campaign.purpose, campaign.channel, at)
        }) {
            if consents.iter().any(|consent| {
                consent.profile_id == profile.id
                    && consent.purpose == campaign.purpose
                    && consent.channel == campaign.channel
                    && consent.state == ConsentState::PendingDoubleOptIn
            }) {
                return Err(MarketingError::DoubleOptInRequired);
            }
            return Err(MarketingError::ConsentRequired);
        }
        if let Some((existing_message, _)) = self.messages.get(idempotency_key) {
            return if *existing_message == message_id {
                Ok(true)
            } else {
                Err(MarketingError::DeliveryIdempotencyConflict)
            };
        }
        self.messages
            .insert(idempotency_key.to_owned(), (message_id, at));
        Ok(false)
    }
}

pub fn dedupe_key(email: Option<&str>, phone: Option<&str>, external_ref: &str) -> String {
    let canonical = format!(
        "{}|{}|{}",
        email.unwrap_or_default().trim().to_ascii_lowercase(),
        phone.unwrap_or_default().trim(),
        external_ref.trim().to_ascii_lowercase()
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn validate_scope(scope: &TenantScope) -> Result<(), MarketingError> {
    match scope {
        TenantScope::Holding { holding_id }
        | TenantScope::Company { holding_id, .. }
        | TenantScope::Branch { holding_id, .. }
        | TenantScope::Subtree { holding_id, .. }
            if holding_id.is_nil() =>
        {
            Err(MarketingError::InvalidScope)
        }
        TenantScope::Company { company_id, .. } if company_id.is_nil() => {
            Err(MarketingError::InvalidScope)
        }
        TenantScope::Branch {
            company_id,
            branch_id,
            ..
        } if company_id.is_nil() || branch_id.is_nil() => Err(MarketingError::InvalidScope),
        TenantScope::Subtree {
            company_ids,
            branch_ids,
            ..
        } if company_ids.iter().any(Uuid::is_nil) || branch_ids.iter().any(Uuid::is_nil) => {
            Err(MarketingError::InvalidScope)
        }
        _ => Ok(()),
    }
}

fn validate_erasure_scope(scope: &TenantScope) -> Result<(), MarketingError> {
    validate_scope(scope)?;
    match scope {
        TenantScope::Platform => Err(MarketingError::InvalidScope),
        TenantScope::Subtree {
            company_ids,
            branch_ids,
            ..
        } if company_ids.is_empty() && branch_ids.is_empty() => Err(MarketingError::InvalidScope),
        _ => Ok(()),
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), MarketingError> {
    if value.trim().is_empty() {
        return Err(MarketingError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind, AuditContext, CommandContext};
    use chrono::Duration;

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn profile() -> MarketingProfile {
        MarketingProfile::new(
            Uuid::from_u128(10),
            scope(),
            "crm-10",
            Some("person@example.test".into()),
            None,
            "th-TH",
            BTreeMap::new(),
            Utc::now(),
        )
        .expect("profile")
    }

    fn erasure(at: DateTime<Utc>, reason: impl Into<String>) -> MarketingProfileErasure {
        MarketingProfileErasure::new(Uuid::from_u128(10), scope(), at, reason)
            .expect("profile erasure")
    }

    fn campaign() -> Campaign {
        Campaign {
            id: Uuid::from_u128(20),
            scope: scope(),
            name: "July newsletter".into(),
            status: CampaignStatus::Running,
            audience_id: Uuid::from_u128(21),
            purpose: MarketingPurpose::Newsletter,
            channel: MarketingChannel::Email,
            updated_at: Utc::now(),
        }
    }

    fn consent(state: ConsentState) -> ConsentRecord {
        ConsentRecord {
            id: Uuid::from_u128(30),
            profile_id: Uuid::from_u128(10),
            scope: scope(),
            purpose: MarketingPurpose::Newsletter,
            channel: MarketingChannel::Email,
            state,
            source: "double_opt_in".into(),
            jurisdiction: "TH".into(),
            policy_version: "2026-01".into(),
            proof_ref: Some("proof-30".into()),
            recorded_at: Utc::now() - Duration::minutes(1),
            expires_at: None,
        }
    }

    #[test]
    fn consent_and_suppression_are_checked_at_send_time() {
        let mut ledger = DeliveryLedger::default();
        let now = Utc::now();
        assert_eq!(
            ledger.authorize(
                &profile(),
                &[consent(ConsentState::Granted)],
                &[],
                &campaign(),
                Uuid::from_u128(40),
                "delivery-40",
                now
            ),
            Ok(false)
        );
        let suppression = SuppressionRecord {
            id: Uuid::from_u128(41),
            profile_id: Uuid::from_u128(10),
            scope: scope(),
            reason: "unsubscribe".into(),
            source: "customer".into(),
            created_at: now,
            expires_at: None,
        };
        assert_eq!(
            ledger.authorize(
                &profile(),
                &[consent(ConsentState::Granted)],
                &[suppression],
                &campaign(),
                Uuid::from_u128(42),
                "delivery-42",
                now
            ),
            Err(MarketingError::Suppressed)
        );
    }

    #[test]
    fn pending_double_opt_in_and_revoked_consent_fail_closed() {
        let now = Utc::now();
        let mut ledger = DeliveryLedger::default();
        assert_eq!(
            ledger.authorize(
                &profile(),
                &[consent(ConsentState::PendingDoubleOptIn)],
                &[],
                &campaign(),
                Uuid::from_u128(50),
                "delivery-50",
                now
            ),
            Err(MarketingError::DoubleOptInRequired)
        );
        assert_eq!(
            ledger.authorize(
                &profile(),
                &[consent(ConsentState::Revoked)],
                &[],
                &campaign(),
                Uuid::from_u128(51),
                "delivery-51",
                now
            ),
            Err(MarketingError::ConsentRequired)
        );
    }

    #[test]
    fn invalid_granted_proof_cannot_authorize_delivery() {
        let now = Utc::now();
        let mut invalid = consent(ConsentState::Granted);
        invalid.proof_ref = Some(String::new());

        assert!(!invalid.is_granted_at(
            Uuid::from_u128(10),
            MarketingPurpose::Newsletter,
            MarketingChannel::Email,
            now,
        ));
        assert_eq!(
            DeliveryLedger::default().authorize(
                &profile(),
                &[invalid],
                &[],
                &campaign(),
                Uuid::from_u128(52),
                "delivery-52",
                now,
            ),
            Err(MarketingError::ConsentRequired)
        );
    }

    #[test]
    fn campaign_lifecycle_and_delivery_replay_are_deterministic() {
        let mut campaign = campaign();
        campaign.status = CampaignStatus::Draft;
        campaign
            .transition(CampaignStatus::Review, &scope(), Utc::now())
            .expect("review");
        assert!(
            campaign
                .transition(CampaignStatus::Completed, &scope(), Utc::now())
                .is_err()
        );
        campaign.status = CampaignStatus::Running;
        let mut ledger = DeliveryLedger::default();
        let now = Utc::now();
        assert!(
            !ledger
                .authorize(
                    &profile(),
                    &[consent(ConsentState::Granted)],
                    &[],
                    &campaign,
                    Uuid::from_u128(60),
                    "delivery-60",
                    now
                )
                .expect("first delivery")
        );
        assert!(
            ledger
                .authorize(
                    &profile(),
                    &[consent(ConsentState::Granted)],
                    &[],
                    &campaign,
                    Uuid::from_u128(60),
                    "delivery-60",
                    now
                )
                .expect("replay")
        );
        assert_eq!(
            ledger.authorize(
                &profile(),
                &[consent(ConsentState::Granted)],
                &[],
                &campaign,
                Uuid::from_u128(61),
                "delivery-60",
                now
            ),
            Err(MarketingError::DeliveryIdempotencyConflict)
        );
    }

    #[test]
    fn scope_and_dedupe_are_tenant_bound() {
        let first = dedupe_key(Some("Person@Example.test"), None, "crm-10");
        let second = dedupe_key(Some("person@example.test"), None, "crm-10");
        assert_eq!(first, second);
        let actor = ActorAttribution {
            principal_ref: "marketing".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let context = CommandContext::try_new("marketing-test", actor, scope(), "marketing test")
            .expect("context");
        assert!(context.tenant_scope.contains(&TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }));
        let _ = AuditContext {
            purpose: "marketing test".into(),
            customer_visible: false,
            module: Some("marketing".into()),
            correlation_id: context.correlation_id,
        };
    }

    #[test]
    fn profile_erasure_reason_and_timestamp_boundaries_are_explicit() {
        let exact_reason = "r".repeat(MAX_MARKETING_PROFILE_ERASURE_REASON);
        assert!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                scope(),
                DateTime::<Utc>::from_timestamp_nanos(i64::MAX),
                exact_reason,
            )
            .is_ok()
        );

        assert_eq!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                scope(),
                Utc::now(),
                "r".repeat(MAX_MARKETING_PROFILE_ERASURE_REASON + 1),
            ),
            Err(MarketingError::TooLong {
                field: "erasure_reason",
                max: MAX_MARKETING_PROFILE_ERASURE_REASON,
            })
        );
        assert_eq!(
            MarketingProfileErasure::new(Uuid::from_u128(10), scope(), Utc::now(), " \t"),
            Err(MarketingError::Required {
                field: "erasure_reason",
            })
        );
        assert_eq!(
            MarketingProfileErasure::new(Uuid::from_u128(10), scope(), Utc::now(), "reason\n"),
            Err(MarketingError::ControlCharacter {
                field: "erasure_reason",
            })
        );
        assert_eq!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                scope(),
                DateTime::<Utc>::MAX_UTC,
                "customer request",
            ),
            Err(MarketingError::InvalidTimestamp { field: "erased_at" })
        );
    }

    #[test]
    fn profile_erasure_rejects_invalid_scopes_and_enforces_containment() {
        let target = MarketingProfileErasure::new(
            Uuid::from_u128(10),
            TenantScope::Branch {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(2),
                branch_id: Uuid::from_u128(3),
            },
            Utc::now(),
            "customer request",
        )
        .expect("branch erasure");

        assert!(
            target
                .validate_for_scope(&TenantScope::Holding {
                    holding_id: Uuid::from_u128(1),
                })
                .is_ok()
        );
        assert_eq!(
            target.validate_for_scope(&TenantScope::Company {
                holding_id: Uuid::from_u128(1),
                company_id: Uuid::from_u128(9),
            }),
            Err(MarketingError::ErasureScopeMismatch)
        );
        assert_eq!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                TenantScope::Platform,
                Utc::now(),
                "customer request",
            ),
            Err(MarketingError::InvalidScope)
        );
        assert_eq!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                TenantScope::Subtree {
                    holding_id: Uuid::from_u128(1),
                    company_ids: Vec::new(),
                    branch_ids: Vec::new(),
                },
                Utc::now(),
                "customer request",
            ),
            Err(MarketingError::InvalidScope)
        );
        assert_eq!(
            MarketingProfileErasure::new(
                Uuid::from_u128(10),
                TenantScope::Branch {
                    holding_id: Uuid::from_u128(1),
                    company_id: Uuid::nil(),
                    branch_id: Uuid::from_u128(3),
                },
                Utc::now(),
                "customer request",
            ),
            Err(MarketingError::InvalidScope)
        );
    }

    #[test]
    fn profile_erasure_payload_and_projection_never_contain_contact_fields() {
        let erasure = erasure(Utc::now(), "customer privacy request");
        let payload = serde_json::to_string(&erasure).expect("serialize erasure");
        assert!(!payload.contains("email"));
        assert!(!payload.contains("phone"));
        assert!(!payload.contains("attributes"));
        assert!(!payload.contains("person@example.test"));

        let projection = erasure.redacted_projection();
        assert_eq!(projection.marker, MARKETING_PROFILE_REDACTED_MARKER);
        let projection_payload = serde_json::to_string(&projection).expect("serialize projection");
        assert!(!projection_payload.contains("email"));
        assert!(!projection_payload.contains("phone"));
        assert!(!projection_payload.contains("attributes"));
        assert!(!projection_payload.contains("person@example.test"));
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(
            MarketingProjectionReconciliation {
                source_event_count: 1,
                source_aggregate_count: 1,
                projected_aggregate_count: 1,
                ..MarketingProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(MarketingProjectionReconciliation::default().is_consistent());
        assert!(
            !MarketingProjectionReconciliation {
                source_event_count: 0,
                source_aggregate_count: 1,
                projected_aggregate_count: 1,
                ..MarketingProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !MarketingProjectionReconciliation {
                source_event_count: 1,
                source_aggregate_count: 1,
                projected_aggregate_count: 1,
                missing_aggregate_count: 1,
                ..MarketingProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !MarketingProjectionReconciliation {
                source_event_count: 1,
                source_aggregate_count: 1,
                projected_aggregate_count: 1,
                stale_aggregate_count: 1,
                ..MarketingProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !MarketingProjectionReconciliation {
                source_event_count: 1,
                source_aggregate_count: 1,
                projected_aggregate_count: 1,
                payload_mismatch_count: 1,
                ..MarketingProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }
}
