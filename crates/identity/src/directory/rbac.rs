//! Directory-managed RBAC contracts.
//!
//! This packet is intentionally registration-ready rather than self-registering:
//! the coordinator owns `directory.rs`, public re-exports, migrations, server
//! and worker registration, and shared authorization.  The types below are the
//! authoritative policy boundary that those owners must compose.
//!
//! The important distinction is that a directory group observation is not a
//! role assignment by itself.  A connector-scoped, revisioned mapping must be
//! present, the mapping target must be contained by the connector and actor
//! scopes, the identity link must be immutable, and the feed must be fresh.
//! Every resulting grant or revoke is an append-only fact and can be projected
//! or reconciled without mutating source identity history.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

use super::sync::{ConnectorScope, ImmutableIdentityLink, directory_holding_id};

pub const DIRECTORY_RBAC_CONTRACT_VERSION: u16 = 1;
pub const DIRECTORY_RBAC_REVOCATION_SLA: Duration = Duration::minutes(5);
pub const DIRECTORY_RBAC_MFA_WINDOW: Duration = Duration::minutes(5);
pub const MAX_DIRECTORY_GROUP_REFERENCE_BYTES: usize = 1_024;
pub const MAX_DIRECTORY_ROLE_REFERENCE_BYTES: usize = 512;
pub const MAX_DIRECTORY_RBAC_IDEMPOTENCY_BYTES: usize = 512;
pub const MAX_DIRECTORY_RBAC_FACT_CHAIN: usize = 1_000_000;

/// The synchronization observation used by RBAC evaluation.  A revoked or
/// stale observation denies access even when the last observed group set
/// still contains a mapped group.  The constructor is intentionally small so
/// feed adapters can only provide the timestamp and explicit revocation bit;
/// they cannot manufacture an "allowed" state that bypasses freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryFeedFreshness {
    observed_at: DateTime<Utc>,
    revoked: bool,
}

impl DirectoryFeedFreshness {
    pub const fn new(observed_at: DateTime<Utc>) -> Self {
        Self {
            observed_at,
            revoked: false,
        }
    }

    pub const fn revoked(observed_at: DateTime<Utc>) -> Self {
        Self {
            observed_at,
            revoked: true,
        }
    }

    pub const fn observed_at(self) -> DateTime<Utc> {
        self.observed_at
    }

    pub const fn is_revoked(self) -> bool {
        self.revoked
    }

    pub fn gate(self, now: DateTime<Utc>) -> RevocationGate {
        if self.observed_at > now {
            RevocationGate::FutureDated
        } else if self.revoked {
            RevocationGate::Revoked
        } else if now.signed_duration_since(self.observed_at) > DIRECTORY_RBAC_REVOCATION_SLA {
            RevocationGate::Stale
        } else {
            RevocationGate::Allowed
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationGate {
    Allowed,
    Revoked,
    Stale,
    FutureDated,
}

/// A directory group reference must be the provider's stable group key.  A
/// DN, UPN, login name, or display name is not an acceptable identity link;
/// those mutable attributes belong in the directory observation instead.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DirectoryGroupRef(String);

impl DirectoryGroupRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DirectoryRbacError> {
        let value = bounded_text(
            value.into(),
            "group_ref",
            MAX_DIRECTORY_GROUP_REFERENCE_BYTES,
        )?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// ERPX role references are opaque to the directory adapter.  The shared
/// authorization owner resolves the reference to a permission set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DirectoryRoleRef(String);

impl DirectoryRoleRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DirectoryRbacError> {
        let value = bounded_text(value.into(), "role_ref", MAX_DIRECTORY_ROLE_REFERENCE_BYTES)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The only identity input accepted by directory-managed RBAC.  It retains
/// the immutable authority/value pair from the synchronization contract and
/// exposes a stable hashed database key.  There is deliberately no setter or
/// conversion from a username or DN.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DirectoryRoleIdentity {
    link: ImmutableIdentityLink,
}

impl DirectoryRoleIdentity {
    pub fn from_immutable_link(link: ImmutableIdentityLink) -> Self {
        Self { link }
    }

    pub fn link(&self) -> &ImmutableIdentityLink {
        &self.link
    }

    /// A stable, non-secret value suitable for `identity_link` columns and
    /// event payloads.  The source authority/value never changes in place.
    pub fn database_key(&self) -> String {
        hex_lower(&self.link.fingerprint())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleMappingKey {
    group_ref: DirectoryGroupRef,
    role_ref: DirectoryRoleRef,
    scope: ConnectorScope,
}

impl DirectoryRoleMappingKey {
    pub fn group_ref(&self) -> &DirectoryGroupRef {
        &self.group_ref
    }

    pub fn role_ref(&self) -> &DirectoryRoleRef {
        &self.role_ref
    }

    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    fn storage_key(&self) -> String {
        format!(
            "{}:{}|{}:{}|{}",
            self.group_ref.as_str().len(),
            self.group_ref.as_str(),
            self.role_ref.as_str().len(),
            self.role_ref.as_str(),
            scope_key(&self.scope)
        )
    }
}

/// Authoritative group-to-role mapping.  The key fields are immutable in
/// place; changing a group, role, or scope is represented as a new mapping
/// key and a compensating revoke, never as an identity or role mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryManagedRoleMapping {
    connector_id: Uuid,
    key: DirectoryRoleMappingKey,
    requires_mfa: bool,
    active: bool,
    revision: u64,
    updated_at: DateTime<Utc>,
}

impl DirectoryManagedRoleMapping {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        connector_id: Uuid,
        group_ref: DirectoryGroupRef,
        role_ref: DirectoryRoleRef,
        scope: ConnectorScope,
        requires_mfa: bool,
        active: bool,
        revision: u64,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, DirectoryRbacError> {
        if connector_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier {
                field: "connector_id",
            });
        }
        validate_scope(&scope)?;
        if revision == 0 {
            return Err(DirectoryRbacError::InvalidRevision);
        }
        Ok(Self {
            connector_id,
            key: DirectoryRoleMappingKey {
                group_ref,
                role_ref,
                scope,
            },
            requires_mfa,
            active,
            revision,
            updated_at,
        })
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn key(&self) -> &DirectoryRoleMappingKey {
        &self.key
    }

    pub fn group_ref(&self) -> &DirectoryGroupRef {
        &self.key.group_ref
    }

    pub fn role_ref(&self) -> &DirectoryRoleRef {
        &self.key.role_ref
    }

    pub fn scope(&self) -> &ConnectorScope {
        &self.key.scope
    }

    pub const fn requires_mfa(&self) -> bool {
        self.requires_mfa
    }

    pub const fn active(&self) -> bool {
        self.active
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn validate_for_connector(
        &self,
        connector_id: Uuid,
        connector_scope: &ConnectorScope,
    ) -> Result<(), DirectoryRbacError> {
        validate_scope(connector_scope)?;
        validate_scope(self.scope())?;
        if self.connector_id != connector_id {
            return Err(DirectoryRbacError::ConnectorMismatch);
        }
        if !connector_scope.permits(self.scope()) {
            return Err(DirectoryRbacError::MappingScopeOutsideConnector);
        }
        Ok(())
    }

    pub fn event_id(&self) -> Uuid {
        let value = format!(
            "directory-role-mapping/{}/{}/{}",
            self.connector_id,
            self.key.storage_key(),
            self.revision
        );
        Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes())
    }
}

/// A validated set prevents duplicate keys and makes scope containment a
/// property of the collection, not an optional server-side convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRoleMappingSet {
    connector_id: Uuid,
    connector_scope: ConnectorScope,
    mappings: BTreeMap<String, DirectoryManagedRoleMapping>,
}

impl DirectoryRoleMappingSet {
    pub fn try_new(
        connector_id: Uuid,
        connector_scope: ConnectorScope,
        mappings: impl IntoIterator<Item = DirectoryManagedRoleMapping>,
    ) -> Result<Self, DirectoryRbacError> {
        if connector_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier {
                field: "connector_id",
            });
        }
        validate_scope(&connector_scope)?;
        let mut values = BTreeMap::new();
        for mapping in mappings {
            mapping.validate_for_connector(connector_id, &connector_scope)?;
            if values
                .insert(mapping.key().storage_key(), mapping)
                .is_some()
            {
                return Err(DirectoryRbacError::DuplicateMapping);
            }
        }
        Ok(Self {
            connector_id,
            connector_scope,
            mappings: values,
        })
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn connector_scope(&self) -> &ConnectorScope {
        &self.connector_scope
    }

    pub fn mappings(&self) -> impl Iterator<Item = &DirectoryManagedRoleMapping> {
        self.mappings.values()
    }

    pub fn get(&self, key: &DirectoryRoleMappingKey) -> Option<&DirectoryManagedRoleMapping> {
        self.mappings.get(&key.storage_key())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRbacAction {
    ReadMapping,
    WriteMapping,
    Reconcile,
    Rollback,
    ApplyRoleFact,
}

impl DirectoryRbacAction {
    pub const fn high_risk(self) -> bool {
        matches!(
            self,
            Self::WriteMapping | Self::Reconcile | Self::Rollback | Self::ApplyRoleFact
        )
    }
}

/// Coordinator-facing authorization hook.  It deliberately takes only
/// immutable scope and MFA evidence; the shared authorization owner remains
/// responsible for resolving the actor's permission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRbacAuthorizationContext {
    actor_ref: String,
    authorized_scope: ConnectorScope,
    mfa_verified_at: Option<DateTime<Utc>>,
    local_break_glass: bool,
    request_is_localhost: bool,
}

impl DirectoryRbacAuthorizationContext {
    pub fn try_new(
        actor_ref: impl Into<String>,
        authorized_scope: ConnectorScope,
        mfa_verified_at: Option<DateTime<Utc>>,
        local_break_glass: bool,
        request_is_localhost: bool,
    ) -> Result<Self, DirectoryRbacError> {
        let actor_ref = bounded_text(actor_ref.into(), "actor_ref", 256)?;
        validate_scope(&authorized_scope)?;
        if local_break_glass && !request_is_localhost {
            return Err(DirectoryRbacError::BreakGlassMustBeLocal);
        }
        Ok(Self {
            actor_ref,
            authorized_scope,
            mfa_verified_at,
            local_break_glass,
            request_is_localhost,
        })
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn authorize(
        &self,
        action: DirectoryRbacAction,
        connector_scope: &ConnectorScope,
        mapping_scope: &ConnectorScope,
        mapping_requires_mfa: bool,
        now: DateTime<Utc>,
    ) -> Result<(), DirectoryRbacError> {
        validate_scope(&self.authorized_scope)?;
        validate_scope(connector_scope)?;
        validate_scope(mapping_scope)?;
        if !self.authorized_scope.permits(connector_scope)
            || !connector_scope.permits(mapping_scope)
        {
            return Err(DirectoryRbacError::UnauthorizedScope);
        }
        // A local break-glass identity is an explicitly selected fallback
        // principal. It must never become a directory-managed RBAC principal
        // merely because the request originated on localhost. The shared
        // authorization owner may use `is_local_break_glass()` to authorize
        // the separate fallback path; this boundary rejects all directory
        // mapping/fact operations for that principal.
        if self.local_break_glass {
            return Err(DirectoryRbacError::BreakGlassFallbackOnly);
        }
        let needs_mfa = action.high_risk() || mapping_requires_mfa;
        if needs_mfa && !mfa_is_fresh(self.mfa_verified_at, now) {
            return Err(DirectoryRbacError::MfaRequired);
        }
        if self.local_break_glass && !self.request_is_localhost {
            return Err(DirectoryRbacError::BreakGlassMustBeLocal);
        }
        Ok(())
    }

    pub const fn is_local_break_glass(&self) -> bool {
        self.local_break_glass
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveDirectoryRole {
    role_ref: DirectoryRoleRef,
    scope: ConnectorScope,
    requires_mfa: bool,
}

impl EffectiveDirectoryRole {
    pub fn role_ref(&self) -> &DirectoryRoleRef {
        &self.role_ref
    }

    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    pub const fn requires_mfa(&self) -> bool {
        self.requires_mfa
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRoleDenyReason {
    IdentityInactive,
    FeedStale,
    FeedFromFuture,
    NoManagedMapping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleEvaluation {
    identity: DirectoryRoleIdentity,
    connector_id: Uuid,
    observed_at: DateTime<Utc>,
    granted: Vec<EffectiveDirectoryRole>,
    mfa_required: Vec<EffectiveDirectoryRole>,
    denied: Option<DirectoryRoleDenyReason>,
}

impl DirectoryRoleEvaluation {
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        identity: DirectoryRoleIdentity,
        connector_id: Uuid,
        connector_scope: &ConnectorScope,
        groups: &BTreeSet<DirectoryGroupRef>,
        active: bool,
        feed: DirectoryFeedFreshness,
        mappings: &DirectoryRoleMappingSet,
        mfa_verified_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Self, DirectoryRbacError> {
        if connector_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier {
                field: "connector_id",
            });
        }
        validate_scope(connector_scope)?;
        if mappings.connector_id() != connector_id || mappings.connector_scope() != connector_scope
        {
            return Err(DirectoryRbacError::ConnectorMismatch);
        }
        let denied = match feed.gate(now) {
            RevocationGate::Allowed => {
                (!active).then_some(DirectoryRoleDenyReason::IdentityInactive)
            }
            RevocationGate::Revoked => Some(DirectoryRoleDenyReason::IdentityInactive),
            RevocationGate::Stale => Some(DirectoryRoleDenyReason::FeedStale),
            RevocationGate::FutureDated => Some(DirectoryRoleDenyReason::FeedFromFuture),
        };
        if let Some(denied) = denied {
            return Ok(Self {
                identity,
                connector_id,
                observed_at: feed.observed_at(),
                granted: Vec::new(),
                mfa_required: Vec::new(),
                denied: Some(denied),
            });
        }

        let mut granted = Vec::new();
        let mut mfa_required = Vec::new();
        for mapping in mappings.mappings().filter(|mapping| mapping.active()) {
            if !groups.contains(mapping.group_ref()) {
                continue;
            }
            if !connector_scope.permits(mapping.scope()) {
                return Err(DirectoryRbacError::MappingScopeOutsideConnector);
            }
            let role = EffectiveDirectoryRole {
                role_ref: mapping.role_ref().clone(),
                scope: mapping.scope().clone(),
                requires_mfa: mapping.requires_mfa(),
            };
            let role_ref = role.role_ref().clone();
            let role_scope = role.scope().clone();
            let same_role = |candidate: &EffectiveDirectoryRole| {
                candidate.role_ref() == &role_ref && candidate.scope() == &role_scope
            };
            if mapping.requires_mfa() && !mfa_is_fresh(mfa_verified_at, now) {
                if !granted.iter().any(same_role) && !mfa_required.iter().any(same_role) {
                    mfa_required.push(role);
                }
            } else {
                if !granted.iter().any(same_role) {
                    granted.push(role);
                }
                // A second mapping for the same effective role that does not
                // require MFA is sufficient to authorize it. Do not leave a
                // stale MFA-only duplicate in the evaluation result.
                mfa_required.retain(|candidate| !same_role(candidate));
            }
        }
        let denied = if granted.is_empty() && mfa_required.is_empty() {
            Some(DirectoryRoleDenyReason::NoManagedMapping)
        } else {
            None
        };
        Ok(Self {
            identity,
            connector_id,
            observed_at: feed.observed_at(),
            granted,
            mfa_required,
            denied,
        })
    }

    pub fn identity(&self) -> &DirectoryRoleIdentity {
        &self.identity
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }

    pub fn granted(&self) -> &[EffectiveDirectoryRole] {
        &self.granted
    }

    pub fn mfa_required(&self) -> &[EffectiveDirectoryRole] {
        &self.mfa_required
    }

    pub const fn denied(&self) -> Option<DirectoryRoleDenyReason> {
        self.denied
    }

    pub fn allows(&self, role_ref: &DirectoryRoleRef, scope: &ConnectorScope) -> bool {
        self.denied.is_none()
            && self
                .granted
                .iter()
                .any(|role| role.role_ref() == role_ref && role.scope() == scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRoleFactAction {
    Grant,
    Revoke,
}

impl DirectoryRoleFactAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Revoke => "revoke",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRoleFactReason {
    GroupMatched,
    GroupRemoved,
    MappingDeactivated,
    IdentityDisabled,
    IdentityDeleted,
    FeedStale,
    Reconciliation,
}

impl DirectoryRoleFactReason {
    const fn permits_action(self, action: DirectoryRoleFactAction) -> bool {
        matches!(
            (self, action),
            (Self::GroupMatched, DirectoryRoleFactAction::Grant)
                | (Self::GroupRemoved, DirectoryRoleFactAction::Revoke)
                | (Self::MappingDeactivated, DirectoryRoleFactAction::Revoke)
                | (Self::IdentityDisabled, DirectoryRoleFactAction::Revoke)
                | (Self::IdentityDeleted, DirectoryRoleFactAction::Revoke)
                | (Self::FeedStale, DirectoryRoleFactAction::Revoke)
                | (Self::Reconciliation, _)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleFactInput {
    pub identity: DirectoryRoleIdentity,
    pub connector_id: Uuid,
    pub group_ref: DirectoryGroupRef,
    pub role_ref: DirectoryRoleRef,
    pub scope: ConnectorScope,
    pub action: DirectoryRoleFactAction,
    pub reason: DirectoryRoleFactReason,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleFact {
    id: Uuid,
    connector_id: Uuid,
    identity: DirectoryRoleIdentity,
    group_ref: DirectoryGroupRef,
    role_ref: DirectoryRoleRef,
    scope: ConnectorScope,
    action: DirectoryRoleFactAction,
    reason: DirectoryRoleFactReason,
    sequence: u64,
    previous_hash: Option<String>,
    fact_hash: String,
    idempotency_key: String,
    occurred_at: DateTime<Utc>,
}

impl DirectoryRoleFact {
    pub(crate) fn from_input(
        input: DirectoryRoleFactInput,
        sequence: u64,
        previous_hash: Option<String>,
    ) -> Result<Self, DirectoryRbacError> {
        if input.connector_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier {
                field: "connector_id",
            });
        }
        if sequence == 0 {
            return Err(DirectoryRbacError::InvalidFactSequence);
        }
        if sequence > MAX_DIRECTORY_RBAC_FACT_CHAIN as u64 {
            return Err(DirectoryRbacError::FactChainTooLong);
        }
        validate_scope(&input.scope)?;
        if !input.reason.permits_action(input.action) {
            return Err(DirectoryRbacError::InvalidFactReason);
        }
        let idempotency_key = bounded_text(
            input.idempotency_key,
            "idempotency_key",
            MAX_DIRECTORY_RBAC_IDEMPOTENCY_BYTES,
        )?;
        if let Some(previous_hash) = previous_hash.as_deref() {
            validate_hash_reference(previous_hash)?;
        }
        let mut digest = Sha256::new();
        digest.update(b"erpx-directory-role-fact:v1\0");
        digest.update(input.connector_id.as_bytes());
        digest.update(input.identity.database_key().as_bytes());
        digest.update(input.group_ref.as_str().as_bytes());
        digest.update(input.role_ref.as_str().as_bytes());
        digest.update(scope_key(&input.scope).as_bytes());
        digest.update(input.action.as_str().as_bytes());
        digest.update([input.reason as u8]);
        digest.update(sequence.to_be_bytes());
        digest.update(idempotency_key.as_bytes());
        if let Some(previous_hash) = previous_hash.as_deref() {
            digest.update(previous_hash.as_bytes());
        }
        digest.update(
            input
                .occurred_at
                .timestamp_nanos_opt()
                .unwrap_or_default()
                .to_be_bytes(),
        );
        let fact_hash = hex_lower(&digest.finalize());
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes());
        Ok(Self {
            id,
            connector_id: input.connector_id,
            identity: input.identity,
            group_ref: input.group_ref,
            role_ref: input.role_ref,
            scope: input.scope,
            action: input.action,
            reason: input.reason,
            sequence,
            previous_hash,
            fact_hash,
            idempotency_key,
            occurred_at: input.occurred_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn connector_id(&self) -> Uuid {
        self.connector_id
    }

    pub fn identity(&self) -> &DirectoryRoleIdentity {
        &self.identity
    }

    pub fn group_ref(&self) -> &DirectoryGroupRef {
        &self.group_ref
    }

    pub fn role_ref(&self) -> &DirectoryRoleRef {
        &self.role_ref
    }

    pub fn scope(&self) -> &ConnectorScope {
        &self.scope
    }

    pub const fn action(&self) -> DirectoryRoleFactAction {
        self.action
    }

    pub const fn reason(&self) -> DirectoryRoleFactReason {
        self.reason
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn previous_hash(&self) -> Option<&str> {
        self.previous_hash.as_deref()
    }

    pub fn fact_hash(&self) -> &str {
        &self.fact_hash
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub const fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }

    pub fn chain_key(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.connector_id,
            self.identity.database_key(),
            self.role_ref().as_str(),
            scope_key(self.scope())
        )
    }

    pub fn verify_hash(&self) -> bool {
        let input = DirectoryRoleFactInput {
            identity: self.identity.clone(),
            connector_id: self.connector_id,
            group_ref: self.group_ref.clone(),
            role_ref: self.role_ref.clone(),
            scope: self.scope.clone(),
            action: self.action,
            reason: self.reason,
            idempotency_key: self.idempotency_key.clone(),
            occurred_at: self.occurred_at,
        };
        Self::from_input(input, self.sequence, self.previous_hash.clone())
            .is_ok_and(|candidate| candidate.fact_hash == self.fact_hash && candidate.id == self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRoleFactAppendOutcome {
    Applied,
    AlreadyApplied,
}

/// In-memory authoritative append-only contract used by workers/projectors
/// and by the SQLx adapter's focused tests.  It is not a substitute for the
/// SQLx store; the coordinator must persist every applied fact.
#[derive(Debug, Default, Clone)]
pub struct DirectoryRoleFactLedger {
    facts: Vec<DirectoryRoleFact>,
    by_idempotency: HashMap<String, usize>,
}

impl DirectoryRoleFactLedger {
    pub fn append(
        &mut self,
        input: DirectoryRoleFactInput,
    ) -> Result<(DirectoryRoleFactAppendOutcome, DirectoryRoleFact), DirectoryRbacError> {
        let idempotency_key = bounded_text(
            input.idempotency_key.clone(),
            "idempotency_key",
            MAX_DIRECTORY_RBAC_IDEMPOTENCY_BYTES,
        )?;
        if let Some(index) = self.by_idempotency.get(&idempotency_key) {
            let existing = &self.facts[*index];
            let candidate = DirectoryRoleFact::from_input(
                input.clone(),
                existing.sequence(),
                existing.previous_hash.clone(),
            )?;
            if candidate.fact_hash() != existing.fact_hash() || candidate.id() != existing.id() {
                return Err(DirectoryRbacError::ImmutableConflict);
            }
            return Ok((
                DirectoryRoleFactAppendOutcome::AlreadyApplied,
                existing.clone(),
            ));
        }
        if self.facts.len() >= MAX_DIRECTORY_RBAC_FACT_CHAIN {
            return Err(DirectoryRbacError::FactChainTooLong);
        }
        let chain_key = format!(
            "{}/{}/{}/{}",
            input.connector_id,
            input.identity.database_key(),
            input.role_ref.as_str(),
            scope_key(&input.scope)
        );
        let (sequence, previous_hash) = self
            .facts
            .iter()
            .filter(|fact| fact.chain_key() == chain_key)
            .max_by_key(|fact| fact.sequence())
            .map_or(Ok((1, None)), |fact| {
                fact.sequence
                    .checked_add(1)
                    .map(|sequence| (sequence, Some(fact.fact_hash.clone())))
                    .ok_or(DirectoryRbacError::FactChainTooLong)
            })?;
        let fact = DirectoryRoleFact::from_input(input, sequence, previous_hash)?;
        self.by_idempotency
            .insert(fact.idempotency_key.clone(), self.facts.len());
        self.facts.push(fact.clone());
        Ok((DirectoryRoleFactAppendOutcome::Applied, fact))
    }

    pub fn facts(&self) -> &[DirectoryRoleFact] {
        &self.facts
    }

    pub fn verify(&self) -> Result<(), DirectoryRbacError> {
        let mut chains: HashMap<String, (&DirectoryRoleFact, u64)> = HashMap::new();
        for fact in &self.facts {
            if !fact.verify_hash() {
                return Err(DirectoryRbacError::FactHashInvalid);
            }
            let entry = chains.entry(fact.chain_key()).or_insert((fact, 0));
            if entry.1 == 0 && fact.previous_hash().is_some() {
                return Err(DirectoryRbacError::FactHashChainBroken);
            }
            if entry.1.checked_add(1) != Some(fact.sequence()) {
                return Err(DirectoryRbacError::FactSequenceGap);
            }
            if entry.1 > 0 && fact.previous_hash() != Some(entry.0.fact_hash()) {
                return Err(DirectoryRbacError::FactHashChainBroken);
            }
            *entry = (fact, fact.sequence());
        }
        Ok(())
    }
}

/// Rebuildable read model.  It contains only effective roles and applied
/// event IDs; the facts remain the source of truth.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DirectoryRoleReadModel {
    roles: BTreeMap<(String, String), BTreeSet<(DirectoryRoleRef, String)>>,
    applied_events: BTreeSet<Uuid>,
    latest_facts: BTreeMap<String, (u64, String)>,
    group_states: BTreeMap<(String, String, String, String, String), DirectoryRoleFactAction>,
}

impl DirectoryRoleReadModel {
    pub fn apply(&mut self, fact: &DirectoryRoleFact) -> Result<(), DirectoryRbacError> {
        if self.applied_events.contains(&fact.id()) {
            return Ok(());
        }
        if !fact.verify_hash() {
            return Err(DirectoryRbacError::FactHashInvalid);
        }
        let chain_key = fact.chain_key();
        if let Some((latest_sequence, latest_hash)) = self.latest_facts.get(&chain_key) {
            if fact.sequence() < *latest_sequence {
                self.applied_events.insert(fact.id());
                return Ok(());
            }
            if fact.sequence() == *latest_sequence {
                if fact.fact_hash() != latest_hash {
                    return Err(DirectoryRbacError::FactHashChainBroken);
                }
                self.applied_events.insert(fact.id());
                return Ok(());
            }
            if fact.sequence() != latest_sequence.saturating_add(1) {
                return Err(DirectoryRbacError::FactSequenceGap);
            }
            if fact.previous_hash() != Some(latest_hash.as_str()) {
                return Err(DirectoryRbacError::FactHashChainBroken);
            }
        } else if fact.sequence() != 1 || fact.previous_hash().is_some() {
            return Err(DirectoryRbacError::FactSequenceGap);
        }
        let key = (
            fact.identity().database_key(),
            fact.connector_id.to_string(),
        );
        // A role can be mapped from more than one directory group. Track the
        // latest fact for each group, then derive the effective role from the
        // union of those group states. A revoke for group A must not remove a
        // role still granted by group B.
        self.group_states.insert(
            (
                fact.identity().database_key(),
                fact.connector_id.to_string(),
                fact.role_ref().as_str().to_owned(),
                scope_key(fact.scope()),
                fact.group_ref().as_str().to_owned(),
            ),
            fact.action(),
        );
        let role = (fact.role_ref().clone(), scope_key(fact.scope()));
        let role_is_active = self.group_states.iter().any(
            |((identity_link, connector_id, role_ref, role_scope, _), action)| {
                identity_link == &key.0
                    && connector_id == &key.1
                    && role_ref == role.0.as_str()
                    && role_scope == &role.1
                    && *action == DirectoryRoleFactAction::Grant
            },
        );
        let roles = self.roles.entry(key.clone()).or_default();
        if role_is_active {
            roles.insert(role);
        } else {
            roles.remove(&role);
        }
        if roles.is_empty() {
            self.roles.remove(&key);
        }
        self.latest_facts
            .insert(chain_key, (fact.sequence(), fact.fact_hash().to_owned()));
        self.applied_events.insert(fact.id());
        Ok(())
    }

    pub fn roles_for(
        &self,
        identity: &DirectoryRoleIdentity,
        connector_id: Uuid,
    ) -> BTreeSet<(DirectoryRoleRef, String)> {
        self.roles
            .get(&(identity.database_key(), connector_id.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn applied_event_count(&self) -> usize {
        self.applied_events.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleOutboxEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub connector_id: Uuid,
    pub scope: ConnectorScope,
    pub idempotency_key: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

impl DirectoryRoleOutboxEvent {
    pub fn from_fact(fact: &DirectoryRoleFact) -> Result<Self, DirectoryRbacError> {
        if !fact.verify_hash() {
            return Err(DirectoryRbacError::FactHashInvalid);
        }
        let aggregate_key = format!("{}/{}", fact.connector_id(), fact.identity().database_key());
        let aggregate_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, aggregate_key.as_bytes());
        Ok(Self {
            event_id: fact.id(),
            event_type: match fact.action() {
                DirectoryRoleFactAction::Grant => "directory.role_granted.v1".to_owned(),
                DirectoryRoleFactAction::Revoke => "directory.role_revoked.v1".to_owned(),
            },
            aggregate_type: "directory_role".to_owned(),
            aggregate_id,
            aggregate_version: fact.sequence(),
            connector_id: fact.connector_id(),
            scope: fact.scope().clone(),
            idempotency_key: fact.idempotency_key().to_owned(),
            payload: serde_json::to_value(fact)
                .map_err(|_| DirectoryRbacError::EventEncodingFailed)?,
            occurred_at: fact.occurred_at(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRbacReconciliationReport {
    pub contract_version: u16,
    pub expected_role_count: u64,
    pub projected_role_count: u64,
    pub fact_count: u64,
    pub outbox_event_count: u64,
    pub source_stable: bool,
    pub complete: bool,
    pub issues: Vec<DirectoryRbacReconciliationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRbacReconciliationIssue {
    EffectiveRoleMismatch,
    MissingRoleFact,
    MissingOutboxEvent,
    InvalidFactChain,
    StaleFeed,
    ImmutableLinkMismatch,
}

impl DirectoryRbacReconciliationReport {
    pub fn is_reconciled(&self) -> bool {
        self.complete && self.source_stable && self.issues.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryRbacRollbackMode {
    StagedImport { import_id: Uuid },
    ForwardOnlyDisable { connector_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRbacRollbackPlan {
    mode: DirectoryRbacRollbackMode,
    reason: String,
    preserve_role_facts: bool,
    preserve_immutable_links: bool,
    rebuild_read_model: bool,
}

impl DirectoryRbacRollbackPlan {
    pub fn staged_import(
        import_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, DirectoryRbacError> {
        if import_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier { field: "import_id" });
        }
        Ok(Self {
            mode: DirectoryRbacRollbackMode::StagedImport { import_id },
            reason: bounded_text(reason.into(), "rollback_reason", 1_024)?,
            preserve_role_facts: true,
            preserve_immutable_links: true,
            rebuild_read_model: false,
        })
    }

    pub fn finalized(
        connector_id: Uuid,
        reason: impl Into<String>,
    ) -> Result<Self, DirectoryRbacError> {
        if connector_id.is_nil() {
            return Err(DirectoryRbacError::NilIdentifier {
                field: "connector_id",
            });
        }
        Ok(Self {
            mode: DirectoryRbacRollbackMode::ForwardOnlyDisable { connector_id },
            reason: bounded_text(reason.into(), "rollback_reason", 1_024)?,
            preserve_role_facts: true,
            preserve_immutable_links: true,
            rebuild_read_model: true,
        })
    }

    pub fn mode(&self) -> &DirectoryRbacRollbackMode {
        &self.mode
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub const fn preserves_history(&self) -> bool {
        self.preserve_role_facts && self.preserve_immutable_links
    }

    pub const fn rebuilds_read_model(&self) -> bool {
        self.rebuild_read_model
    }
}

#[derive(Debug, Error)]
pub enum DirectoryRbacError {
    #[error("{field} is empty, too long, or contains a control character")]
    InvalidReference { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("directory role mapping revision must be positive")]
    InvalidRevision,
    #[error("directory role mapping connector does not match the request")]
    ConnectorMismatch,
    #[error("directory connector was not found")]
    ConnectorNotFound,
    #[error("caller connector scope does not match the authoritative persisted scope")]
    ConnectorScopeMismatch,
    #[error("directory role mapping scope is outside the connector scope")]
    MappingScopeOutsideConnector,
    #[error("directory role grant requires an active managed mapping")]
    ManagedMappingNotActive,
    #[error("directory role mapping key is duplicated")]
    DuplicateMapping,
    #[error("actor or mapping scope is not authorized")]
    UnauthorizedScope,
    #[error("directory role operation requires recent MFA")]
    MfaRequired,
    #[error("local break-glass must be a localhost-only path")]
    BreakGlassMustBeLocal,
    #[error("local break-glass is restricted to the explicit fallback path")]
    BreakGlassFallbackOnly,
    #[error("directory role fact sequence is invalid")]
    InvalidFactSequence,
    #[error("directory role fact action and reason are inconsistent")]
    InvalidFactReason,
    #[error("directory role fact sequence contains a gap")]
    FactSequenceGap,
    #[error("directory role fact hash is invalid")]
    FactHashInvalid,
    #[error("directory role fact hash chain is broken")]
    FactHashChainBroken,
    #[error("directory role fact chain exceeded its safety bound")]
    FactChainTooLong,
    #[error("directory role outbox event could not be encoded")]
    EventEncodingFailed,
    #[error("directory role outbox event could not be decoded")]
    EventDecodingFailed,
    #[error("directory role persistence failed: {0}")]
    Persistence(#[from] sqlx::Error),
    #[error("directory role persistence encountered an immutable conflict")]
    ImmutableConflict,
    #[error("directory immutable identity link is not persisted")]
    IdentityNotFound,
    #[error("directory role grant requires an active immutable identity link")]
    IdentityInactive,
}

impl PartialEq for DirectoryRbacError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::InvalidReference { field: left }, Self::InvalidReference { field: right })
            | (Self::NilIdentifier { field: left }, Self::NilIdentifier { field: right }) => {
                left == right
            }
            (Self::Persistence(_), Self::Persistence(_)) => true,
            (left, right) => std::mem::discriminant(left) == std::mem::discriminant(right),
        }
    }
}

impl Eq for DirectoryRbacError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleProjectionReport {
    pub examined: u64,
    pub projected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryProjectedRole {
    pub connector_id: Uuid,
    pub identity_link: String,
    pub role_ref: String,
    pub scope_key: String,
    pub group_ref: String,
    pub active: bool,
    pub last_sequence: u64,
    pub last_fact_hash: String,
    pub last_event_id: Uuid,
    pub updated_at: DateTime<Utc>,
}

/// Append compensating RBAC revocations for a mapping that was deactivated.
/// This runs in the caller's mapping transaction so the policy change and its
/// role-history correction cannot become separately visible.
pub(crate) async fn append_mapping_deactivation_revokes(
    tx: &mut Transaction<'_, Postgres>,
    connector_id: Uuid,
    group_ref: &DirectoryGroupRef,
    role_ref: &DirectoryRoleRef,
    scope: &ConnectorScope,
    mapping_revision: u64,
    occurred_at: DateTime<Utc>,
) -> Result<u64, DirectoryRbacError> {
    let scope_key_value = scope_key(scope);
    let rows = sqlx::query(
        "SELECT p.identity_link, l.immutable_authority, l.immutable_value
         FROM erpx_identity.directory_identity_projection AS p
         JOIN erpx_identity.directory_identity_links AS l
           ON l.connector_id=p.connector_id AND l.identity_link=p.identity_link
         WHERE p.connector_id=$1
           AND l.immutable_authority IS NOT NULL
           AND l.immutable_value IS NOT NULL
           AND (
               SELECT f.action
               FROM erpx_identity.directory_role_facts AS f
               JOIN erpx_identity.directory_outbox AS o ON o.event_id=f.id
               WHERE f.connector_id=p.connector_id
                 AND f.identity_link=p.identity_link
                 AND f.role_ref=$3
                 AND f.scope_key=$4
                 AND o.aggregate_type='directory_role'
                 AND o.payload_json->>'group_ref'=$2
               ORDER BY f.sequence DESC, f.occurred_at DESC
               LIMIT 1
           ) = 'grant'
         ORDER BY p.identity_link",
    )
    .bind(connector_id)
    .bind(group_ref.as_str())
    .bind(role_ref.as_str())
    .bind(&scope_key_value)
    .fetch_all(&mut **tx)
    .await?;

    let mut appended = 0_u64;
    for row in rows {
        let identity_link: String = row.try_get("identity_link")?;
        let chain_key = format!(
            "directory-role/{connector_id}/{identity_link}/{}/{scope_key_value}",
            role_ref.as_str()
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(chain_key)
            .execute(&mut **tx)
            .await?;

        let tail = sqlx::query(
            "SELECT f.action, f.sequence, f.fact_hash
             FROM erpx_identity.directory_role_facts AS f
             JOIN erpx_identity.directory_outbox AS o ON o.event_id=f.id
             WHERE f.connector_id=$1 AND f.identity_link=$2 AND f.role_ref=$3
               AND f.scope_key=$4 AND o.aggregate_type='directory_role'
             ORDER BY f.sequence DESC, f.occurred_at DESC
             LIMIT 1
             FOR UPDATE OF f",
        )
        .bind(connector_id)
        .bind(&identity_link)
        .bind(role_ref.as_str())
        .bind(&scope_key_value)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(tail) = tail else {
            continue;
        };
        if tail.try_get::<String, _>("action")? != "grant" {
            continue;
        }
        let sequence = u64::try_from(tail.try_get::<i64, _>("sequence")?)
            .map_err(|_| DirectoryRbacError::InvalidFactSequence)?
            .checked_add(1)
            .ok_or(DirectoryRbacError::FactChainTooLong)?;
        if sequence > MAX_DIRECTORY_RBAC_FACT_CHAIN as u64 {
            return Err(DirectoryRbacError::FactChainTooLong);
        }
        let immutable_link = ImmutableIdentityLink::new(
            row.try_get::<String, _>("immutable_authority")?,
            row.try_get::<String, _>("immutable_value")?,
        )
        .map_err(|_| DirectoryRbacError::IdentityNotFound)?;
        let input = DirectoryRoleFactInput {
            identity: DirectoryRoleIdentity::from_immutable_link(immutable_link),
            connector_id,
            group_ref: group_ref.clone(),
            role_ref: role_ref.clone(),
            scope: scope.clone(),
            action: DirectoryRoleFactAction::Revoke,
            reason: DirectoryRoleFactReason::MappingDeactivated,
            idempotency_key: format!(
                "directory-role-mapping-revoke/{connector_id}/{identity_link}/{}/{}",
                role_ref.as_str(),
                mapping_revision
            ),
            occurred_at,
        };
        let fact = DirectoryRoleFact::from_input(
            input,
            sequence,
            Some(tail.try_get::<String, _>("fact_hash")?),
        )?;
        if insert_role_fact_and_event(tx, &fact).await? {
            appended = appended.saturating_add(1);
        }
    }
    Ok(appended)
}

async fn insert_role_fact_and_event(
    tx: &mut Transaction<'_, Postgres>,
    fact: &DirectoryRoleFact,
) -> Result<bool, DirectoryRbacError> {
    if !fact.verify_hash() {
        return Err(DirectoryRbacError::FactHashInvalid);
    }
    let result = sqlx::query(
        "INSERT INTO erpx_identity.directory_role_facts
            (id, connector_id, identity_link, role_ref, scope_key, action,
             sequence, previous_hash, fact_hash, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (fact_hash) DO NOTHING",
    )
    .bind(fact.id())
    .bind(fact.connector_id())
    .bind(fact.identity().database_key())
    .bind(fact.role_ref().as_str())
    .bind(scope_key(fact.scope()))
    .bind(fact.action().as_str())
    .bind(i64::try_from(fact.sequence()).map_err(|_| DirectoryRbacError::InvalidFactSequence)?)
    .bind(fact.previous_hash())
    .bind(fact.fact_hash())
    .bind(fact.occurred_at())
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() == 0 {
        let existing = sqlx::query(
            "SELECT id, connector_id, identity_link, role_ref, scope_key,
                    action, sequence, previous_hash, fact_hash, occurred_at
             FROM erpx_identity.directory_role_facts
             WHERE fact_hash=$1",
        )
        .bind(fact.fact_hash())
        .fetch_optional(&mut **tx)
        .await?;
        if existing.is_none_or(|row| {
            row.try_get::<Uuid, _>("id").ok() != Some(fact.id())
                || row.try_get::<Uuid, _>("connector_id").ok() != Some(fact.connector_id())
                || row.try_get::<String, _>("identity_link").ok()
                    != Some(fact.identity().database_key())
                || row.try_get::<String, _>("role_ref").ok()
                    != Some(fact.role_ref().as_str().to_owned())
                || row.try_get::<String, _>("scope_key").ok() != Some(scope_key(fact.scope()))
                || row.try_get::<String, _>("action").ok()
                    != Some(fact.action().as_str().to_owned())
                || row.try_get::<i64, _>("sequence").ok() != i64::try_from(fact.sequence()).ok()
                || row.try_get::<Option<String>, _>("previous_hash").ok()
                    != Some(fact.previous_hash().map(ToOwned::to_owned))
                || row.try_get::<String, _>("fact_hash").ok() != Some(fact.fact_hash().to_owned())
                || row.try_get::<DateTime<Utc>, _>("occurred_at").ok() != Some(fact.occurred_at())
        }) {
            return Err(DirectoryRbacError::ImmutableConflict);
        }
    }
    let event = DirectoryRoleOutboxEvent::from_fact(fact)?;
    let expected_scope =
        serde_json::to_value(&event.scope).map_err(|_| DirectoryRbacError::EventEncodingFailed)?;
    let result = sqlx::query(
        "INSERT INTO erpx_identity.directory_outbox
            (event_id, aggregate_id, aggregate_type, event_type,
             event_version, connector_id, aggregate_version, scope_json,
             idempotency_key, payload_json, occurred_at)
         VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event.event_id)
    .bind(event.aggregate_id)
    .bind(&event.aggregate_type)
    .bind(&event.event_type)
    .bind(event.connector_id)
    .bind(
        i64::try_from(event.aggregate_version)
            .map_err(|_| DirectoryRbacError::InvalidFactSequence)?,
    )
    .bind(&expected_scope)
    .bind(&event.idempotency_key)
    .bind(&event.payload)
    .bind(event.occurred_at)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() == 0 {
        let same = sqlx::query(
            "SELECT aggregate_id, aggregate_type, event_type, connector_id,
                    aggregate_version, scope_json, idempotency_key,
                    payload_json, occurred_at
             FROM erpx_identity.directory_outbox WHERE event_id=$1",
        )
        .bind(event.event_id)
        .fetch_optional(&mut **tx)
        .await?
        .is_some_and(|row| {
            row.try_get::<Uuid, _>("aggregate_id").ok() == Some(event.aggregate_id)
                && row.try_get::<String, _>("aggregate_type").ok()
                    == Some(event.aggregate_type.clone())
                && row.try_get::<String, _>("event_type").ok() == Some(event.event_type.clone())
                && row.try_get::<Uuid, _>("connector_id").ok() == Some(event.connector_id)
                && row.try_get::<i64, _>("aggregate_version").ok()
                    == i64::try_from(event.aggregate_version).ok()
                && row.try_get::<Value, _>("scope_json").ok() == Some(expected_scope.clone())
                && row.try_get::<String, _>("idempotency_key").ok()
                    == Some(event.idempotency_key.clone())
                && row.try_get::<Value, _>("payload_json").ok() == Some(event.payload.clone())
                && row.try_get::<DateTime<Utc>, _>("occurred_at").ok() == Some(event.occurred_at)
        });
        if !same {
            return Err(DirectoryRbacError::ImmutableConflict);
        }
    }
    Ok(true)
}

/// SQLx adapter for the existing identity write-side tables.  Registration,
/// migration ownership, and worker scheduling remain coordinator hooks.
#[derive(Debug, Clone)]
pub struct SqlxDirectoryRbacStore {
    pool: PgPool,
}

impl SqlxDirectoryRbacStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Project a bounded batch of durable role events into the SQLx read model.
    /// Source facts and their outbox envelopes are validated before the read
    /// row is changed; the event is marked projected in the same transaction,
    /// so a crash leaves the event safely replayable.
    pub async fn project_role_outbox(
        &self,
        limit: u32,
        projected_at: DateTime<Utc>,
    ) -> Result<DirectoryRoleProjectionReport, DirectoryRbacError> {
        if limit == 0 {
            return Ok(DirectoryRoleProjectionReport {
                examined: 0,
                projected: 0,
            });
        }
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT o.event_id, o.aggregate_id, o.aggregate_type, o.event_type,
                    o.connector_id, o.aggregate_version, o.scope_json,
                    o.idempotency_key, o.payload_json, o.occurred_at,
                    c.scope_key AS connector_scope_key,
                    f.id AS fact_id, f.connector_id AS fact_connector_id,
                    f.identity_link AS fact_identity_link, f.role_ref AS fact_role_ref,
                    f.scope_key AS fact_scope_key, f.action AS fact_action,
                    f.sequence AS fact_sequence, f.previous_hash AS fact_previous_hash,
                    f.fact_hash AS fact_hash,
                    f.occurred_at AS fact_occurred_at
             FROM erpx_identity.directory_outbox AS o
             LEFT JOIN erpx_identity.directory_connectors AS c ON c.id=o.connector_id
             LEFT JOIN erpx_identity.directory_role_facts AS f ON f.id=o.event_id
             WHERE o.aggregate_type='directory_role' AND o.projected_at IS NULL
             ORDER BY o.occurred_at, o.aggregate_id, o.aggregate_version, o.event_id
             LIMIT $1
             FOR UPDATE OF o SKIP LOCKED",
        )
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let mut report = DirectoryRoleProjectionReport {
            examined: 0,
            projected: 0,
        };
        for row in rows {
            report.examined = report.examined.saturating_add(1);
            let fact: DirectoryRoleFact = serde_json::from_value(row.try_get("payload_json")?)
                .map_err(|_| DirectoryRbacError::EventDecodingFailed)?;
            let event = DirectoryRoleOutboxEvent::from_fact(&fact)?;
            let connector_scope = validate_role_outbox_row(&row, &fact, &event)?;
            if !connector_scope.permits(fact.scope()) {
                return Err(DirectoryRbacError::MappingScopeOutsideConnector);
            }
            let identity_link_present: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1
                     FROM erpx_identity.directory_identity_links
                     WHERE connector_id=$1
                       AND immutable_authority=$2
                       AND immutable_value=$3
                       AND (identity_link IS NULL OR identity_link=$4)
                 )",
            )
            .bind(fact.connector_id())
            .bind(fact.identity().link().authority())
            .bind(fact.identity().link().value())
            .bind(fact.identity().database_key())
            .fetch_one(&mut *tx)
            .await?;
            if !identity_link_present {
                return Err(DirectoryRbacError::IdentityNotFound);
            }
            let read_tail = sqlx::query(
                "SELECT last_sequence, last_fact_hash
                 FROM erpx_identity.directory_role_read_model
                 WHERE connector_id=$1 AND identity_link=$2 AND role_ref=$3
                   AND scope_key=$4
                 FOR UPDATE",
            )
            .bind(fact.connector_id())
            .bind(fact.identity().database_key())
            .bind(fact.role_ref().as_str())
            .bind(scope_key(fact.scope()))
            .fetch_optional(&mut *tx)
            .await?;
            let mut apply = true;
            if let Some(tail) = read_tail {
                let last_sequence = u64::try_from(tail.try_get::<i64, _>("last_sequence")?)
                    .map_err(|_| DirectoryRbacError::InvalidFactSequence)?;
                let last_hash: String = tail.try_get("last_fact_hash")?;
                if fact.sequence() < last_sequence {
                    apply = false;
                } else if fact.sequence() == last_sequence {
                    if fact.fact_hash() != last_hash {
                        return Err(DirectoryRbacError::FactHashChainBroken);
                    }
                    apply = false;
                } else {
                    if fact.sequence() != last_sequence.saturating_add(1) {
                        return Err(DirectoryRbacError::FactSequenceGap);
                    }
                    if fact.previous_hash() != Some(last_hash.as_str()) {
                        return Err(DirectoryRbacError::FactHashChainBroken);
                    }
                }
            } else if fact.sequence() != 1 || fact.previous_hash().is_some() {
                return Err(DirectoryRbacError::FactSequenceGap);
            }
            if apply {
                let active: bool = sqlx::query_scalar(
                    "WITH latest_group AS (
                         SELECT DISTINCT ON (o.payload_json->>'group_ref')
                                o.payload_json->>'group_ref' AS group_ref, f.action
                         FROM erpx_identity.directory_role_facts AS f
                         JOIN erpx_identity.directory_outbox AS o ON o.event_id=f.id
                         WHERE f.connector_id=$1 AND f.identity_link=$2
                           AND f.role_ref=$3 AND f.scope_key=$4
                           AND (o.projected_at IS NOT NULL OR o.event_id=$5)
                         ORDER BY o.payload_json->>'group_ref',
                                  f.sequence DESC, f.occurred_at DESC, f.id DESC
                     )
                     SELECT EXISTS(
                         SELECT 1 FROM latest_group WHERE action='grant'
                     )",
                )
                .bind(fact.connector_id())
                .bind(fact.identity().database_key())
                .bind(fact.role_ref().as_str())
                .bind(scope_key(fact.scope()))
                .bind(fact.id())
                .fetch_one(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO erpx_identity.directory_role_read_model
                        (connector_id, identity_link, role_ref, scope_key, group_ref,
                         active, last_sequence, last_fact_hash, last_event_id, updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                     ON CONFLICT (connector_id, identity_link, role_ref, scope_key)
                     DO UPDATE SET group_ref=EXCLUDED.group_ref,
                         active=EXCLUDED.active,
                         last_sequence=EXCLUDED.last_sequence,
                         last_fact_hash=EXCLUDED.last_fact_hash,
                         last_event_id=EXCLUDED.last_event_id,
                         updated_at=EXCLUDED.updated_at",
                )
                .bind(fact.connector_id())
                .bind(fact.identity().database_key())
                .bind(fact.role_ref().as_str())
                .bind(scope_key(fact.scope()))
                .bind(fact.group_ref().as_str())
                .bind(active)
                .bind(
                    i64::try_from(fact.sequence())
                        .map_err(|_| DirectoryRbacError::InvalidFactSequence)?,
                )
                .bind(fact.fact_hash())
                .bind(fact.id())
                .bind(projected_at)
                .execute(&mut *tx)
                .await?;
            }
            let result = sqlx::query(
                "UPDATE erpx_identity.directory_outbox
                 SET projected_at=$2, projection_attempts=projection_attempts + 1
                 WHERE event_id=$1 AND projected_at IS NULL",
            )
            .bind(fact.id())
            .bind(projected_at)
            .execute(&mut *tx)
            .await?;
            report.projected = report.projected.saturating_add(result.rows_affected());
        }
        tx.commit().await?;
        Ok(report)
    }

    pub async fn projected_roles_for(
        &self,
        identity: &DirectoryRoleIdentity,
        connector_id: Uuid,
    ) -> Result<BTreeSet<(DirectoryRoleRef, String)>, DirectoryRbacError> {
        let rows = sqlx::query(
            "SELECT role_ref, scope_key
             FROM erpx_identity.directory_role_read_model
             WHERE connector_id=$1 AND identity_link=$2 AND active
             ORDER BY role_ref, scope_key",
        )
        .bind(connector_id)
        .bind(identity.database_key())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let role_ref = DirectoryRoleRef::try_new(row.try_get::<String, _>("role_ref")?)
                    .map_err(|_| DirectoryRbacError::InvalidReference { field: "role_ref" })?;
                let scope_key = row.try_get::<String, _>("scope_key")?;
                parse_scope_key(&scope_key)?;
                Ok((role_ref, scope_key))
            })
            .collect()
    }

    pub async fn save_mapping(
        &self,
        mapping: &DirectoryManagedRoleMapping,
        connector_scope: &ConnectorScope,
        authorization: &DirectoryRbacAuthorizationContext,
        now: DateTime<Utc>,
    ) -> Result<DirectoryMappingWriteOutcome, DirectoryRbacError> {
        mapping.validate_for_connector(mapping.connector_id(), connector_scope)?;
        let mut tx = self.pool.begin().await?;
        // The caller-provided scope is an authorization hint, not authority.
        // Reload the connector scope under a share lock before authorizing or
        // writing the mapping; otherwise a forged Platform scope could widen a
        // holding connector's role-management boundary.
        let persisted_scope_key = sqlx::query(
            "SELECT scope_key
             FROM erpx_identity.directory_connectors
             WHERE id=$1
             FOR SHARE",
        )
        .bind(mapping.connector_id())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DirectoryRbacError::ConnectorNotFound)?
        .try_get::<String, _>("scope_key")?;
        let persisted_scope = parse_scope_key(&persisted_scope_key)?;
        if persisted_scope != *connector_scope {
            tx.rollback().await?;
            return Err(DirectoryRbacError::ConnectorScopeMismatch);
        }
        mapping.validate_for_connector(mapping.connector_id(), &persisted_scope)?;
        authorization.authorize(
            DirectoryRbacAction::WriteMapping,
            &persisted_scope,
            mapping.scope(),
            mapping.requires_mfa(),
            now,
        )?;
        let scope_key = scope_key(mapping.scope());
        let mapping_key = sqlx::query(
            "SELECT requires_mfa, active, revision, updated_at
             FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1 AND group_ref=$2 AND role_ref=$3 AND scope_key=$4
             FOR UPDATE",
        )
        .bind(mapping.connector_id())
        .bind(mapping.group_ref().as_str())
        .bind(mapping.role_ref().as_str())
        .bind(&scope_key)
        .fetch_optional(&mut *tx)
        .await?;
        let was_active = mapping_key
            .as_ref()
            .map(|row| row.try_get::<bool, _>("active"))
            .transpose()?
            .unwrap_or(false);
        if let Some(row) = mapping_key {
            let current_revision = u64::try_from(row.try_get::<i64, _>("revision")?)
                .map_err(|_| DirectoryRbacError::InvalidRevision)?;
            if current_revision >= mapping.revision() {
                let same_revision = current_revision == mapping.revision()
                    && row.try_get::<bool, _>("requires_mfa")? == mapping.requires_mfa()
                    && row.try_get::<bool, _>("active")? == mapping.active()
                    && row.try_get::<DateTime<Utc>, _>("updated_at")? == mapping.updated_at();
                tx.rollback().await?;
                return if same_revision {
                    Ok(DirectoryMappingWriteOutcome::AlreadyCurrent)
                } else if current_revision == mapping.revision() {
                    Err(DirectoryRbacError::ImmutableConflict)
                } else {
                    Ok(DirectoryMappingWriteOutcome::AlreadyCurrent)
                };
            }
        }
        let result = sqlx::query(
            "INSERT INTO erpx_identity.directory_role_mappings
                (connector_id, group_ref, role_ref, scope_key, requires_mfa,
                 active, revision, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (connector_id, group_ref, role_ref, scope_key) DO UPDATE SET
                requires_mfa=EXCLUDED.requires_mfa, active=EXCLUDED.active,
                revision=EXCLUDED.revision, updated_at=EXCLUDED.updated_at
             WHERE erpx_identity.directory_role_mappings.revision < EXCLUDED.revision",
        )
        .bind(mapping.connector_id())
        .bind(mapping.group_ref().as_str())
        .bind(mapping.role_ref().as_str())
        .bind(&scope_key)
        .bind(mapping.requires_mfa())
        .bind(mapping.active())
        .bind(i64::try_from(mapping.revision()).map_err(|_| DirectoryRbacError::InvalidRevision)?)
        .bind(mapping.updated_at())
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            let current = sqlx::query(
                "SELECT requires_mfa, active, revision, updated_at
                 FROM erpx_identity.directory_role_mappings
                 WHERE connector_id=$1 AND group_ref=$2 AND role_ref=$3 AND scope_key=$4
                 FOR UPDATE",
            )
            .bind(mapping.connector_id())
            .bind(mapping.group_ref().as_str())
            .bind(mapping.role_ref().as_str())
            .bind(&scope_key)
            .fetch_optional(&mut *tx)
            .await?;
            let conflicting_same_revision = if let Some(row) = current {
                let current_revision = u64::try_from(row.try_get::<i64, _>("revision")?)
                    .map_err(|_| DirectoryRbacError::InvalidRevision)?;
                current_revision == mapping.revision()
                    && (row.try_get::<bool, _>("requires_mfa")? != mapping.requires_mfa()
                        || row.try_get::<bool, _>("active")? != mapping.active()
                        || row.try_get::<DateTime<Utc>, _>("updated_at")? != mapping.updated_at())
            } else {
                false
            };
            tx.rollback().await?;
            return if conflicting_same_revision {
                Err(DirectoryRbacError::ImmutableConflict)
            } else {
                Ok(DirectoryMappingWriteOutcome::AlreadyCurrent)
            };
        }
        let event_id = mapping.event_id();
        let payload = serde_json::json!({
            "contract_version": DIRECTORY_RBAC_CONTRACT_VERSION,
            "event": "directory.role_mapping_saved",
            "connector_id": mapping.connector_id(),
            "group_ref": mapping.group_ref().as_str(),
            "role_ref": mapping.role_ref().as_str(),
            "scope": scope_key,
            "requires_mfa": mapping.requires_mfa(),
            "active": mapping.active(),
            "revision": mapping.revision(),
            "actor_ref": authorization.actor_ref(),
        });
        let expected_scope = serde_json::to_value(mapping.scope())
            .map_err(|_| DirectoryRbacError::EventEncodingFailed)?;
        let outbox_result = sqlx::query(
            "INSERT INTO erpx_identity.directory_outbox
                (event_id, aggregate_id, aggregate_type, event_type,
                 event_version, connector_id, aggregate_version, scope_json,
                 idempotency_key, payload_json, occurred_at)
             VALUES ($1,$2,'directory_role_mapping',
                     'directory.role_mapping_saved.v1',1,$2,$5,$6,$7,$3,$4)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(mapping.connector_id())
        .bind(&payload)
        .bind(now)
        .bind(i64::try_from(mapping.revision()).map_err(|_| DirectoryRbacError::InvalidRevision)?)
        .bind(&expected_scope)
        .bind(event_id.to_string())
        .execute(&mut *tx)
        .await?;
        if outbox_result.rows_affected() == 0 {
            let existing_payload = sqlx::query(
                "SELECT aggregate_id, aggregate_type, event_type, event_version,
                        connector_id, aggregate_version, scope_json, idempotency_key,
                        payload_json, occurred_at
                 FROM erpx_identity.directory_outbox WHERE event_id=$1",
            )
            .bind(event_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| {
                Ok::<bool, sqlx::Error>(
                    row.try_get::<Uuid, _>("aggregate_id")? == mapping.connector_id()
                        && row.try_get::<String, _>("aggregate_type")? == "directory_role_mapping"
                        && row.try_get::<String, _>("event_type")?
                            == "directory.role_mapping_saved.v1"
                        && row.try_get::<i32, _>("event_version")? == 1
                        && row.try_get::<Uuid, _>("connector_id")? == mapping.connector_id()
                        && row.try_get::<i64, _>("aggregate_version")?
                            == i64::try_from(mapping.revision()).unwrap_or_default()
                        && row.try_get::<Value, _>("scope_json")? == expected_scope
                        && row.try_get::<String, _>("idempotency_key")? == event_id.to_string()
                        && row.try_get::<Value, _>("payload_json")? == payload
                        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == now,
                )
            })
            .transpose()?;
            if existing_payload != Some(true) {
                tx.rollback().await?;
                return Err(DirectoryRbacError::ImmutableConflict);
            }
        }
        if was_active && !mapping.active() {
            append_mapping_deactivation_revokes(
                &mut tx,
                mapping.connector_id(),
                mapping.group_ref(),
                mapping.role_ref(),
                mapping.scope(),
                mapping.revision(),
                now,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(DirectoryMappingWriteOutcome::Applied { event_id })
    }

    /// Rebuild the disposable effective-role read model from the durable role
    /// facts and their matching outbox events. Every row must agree with its
    /// typed fact and the immutable fact table before it is applied; a missing
    /// or forged side of the source boundary fails closed.
    pub async fn reconstruct_read_model(
        &self,
    ) -> Result<DirectoryRoleReadModel, DirectoryRbacError> {
        // A role fact is authoritative only when its corresponding outbox
        // event is still present. The event-driven query below detects an
        // event whose fact is missing, but a deleted/missing event would
        // otherwise make a durable fact silently disappear from a rebuild.
        let orphan_fact: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM erpx_identity.directory_role_facts AS f
                 WHERE NOT EXISTS (
                     SELECT 1
                     FROM erpx_identity.directory_outbox AS o
                     WHERE o.event_id = f.id
                       AND o.aggregate_type = 'directory_role'
                 )
             )",
        )
        .fetch_one(&self.pool)
        .await?;
        if orphan_fact {
            return Err(DirectoryRbacError::ImmutableConflict);
        }

        let rows = sqlx::query(
            "SELECT o.event_id, o.aggregate_id, o.aggregate_type, o.event_type,
                    o.connector_id, o.aggregate_version, o.scope_json,
                    o.idempotency_key, o.payload_json, o.occurred_at,
                    c.scope_key AS connector_scope_key,
                    f.id AS fact_id, f.connector_id AS fact_connector_id,
                    f.identity_link AS fact_identity_link, f.role_ref AS fact_role_ref,
                    f.scope_key AS fact_scope_key, f.action AS fact_action,
                    f.sequence AS fact_sequence, f.previous_hash AS fact_previous_hash,
                    f.fact_hash AS fact_hash,
                    f.occurred_at AS fact_occurred_at
             FROM erpx_identity.directory_outbox AS o
             LEFT JOIN erpx_identity.directory_connectors AS c ON c.id=o.connector_id
             LEFT JOIN erpx_identity.directory_role_facts AS f ON f.id=o.event_id
             WHERE o.aggregate_type='directory_role'
             ORDER BY o.aggregate_id, o.aggregate_version, o.event_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut read_model = DirectoryRoleReadModel::default();
        for row in rows {
            let payload: Value = row.try_get("payload_json")?;
            let fact: DirectoryRoleFact = serde_json::from_value(payload)
                .map_err(|_| DirectoryRbacError::EventDecodingFailed)?;
            let event = DirectoryRoleOutboxEvent::from_fact(&fact)?;
            let connector_scope = row
                .try_get::<Option<String>, _>("connector_scope_key")?
                .ok_or(DirectoryRbacError::ConnectorNotFound)
                .and_then(|scope| parse_scope_key(&scope))?;
            if !connector_scope.permits(fact.scope()) {
                return Err(DirectoryRbacError::MappingScopeOutsideConnector);
            }
            let identity_link_present: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1
                     FROM erpx_identity.directory_identity_links
                     WHERE connector_id=$1
                       AND immutable_authority=$2
                       AND immutable_value=$3
                       AND (identity_link IS NULL OR identity_link=$4)
                 )",
            )
            .bind(fact.connector_id())
            .bind(fact.identity().link().authority())
            .bind(fact.identity().link().value())
            .bind(fact.identity().database_key())
            .fetch_one(&self.pool)
            .await?;
            if !identity_link_present {
                return Err(DirectoryRbacError::IdentityNotFound);
            }
            let expected_scope = serde_json::to_value(&event.scope)
                .map_err(|_| DirectoryRbacError::EventEncodingFailed)?;
            let outbox_matches = row.try_get::<Uuid, _>("event_id")? == event.event_id
                && row.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
                && row.try_get::<String, _>("aggregate_type")? == event.aggregate_type
                && row.try_get::<String, _>("event_type")? == event.event_type
                && row.try_get::<Uuid, _>("connector_id")? == event.connector_id
                && u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
                    .map_err(|_| DirectoryRbacError::InvalidFactSequence)?
                    == event.aggregate_version
                && row.try_get::<Value, _>("scope_json")? == expected_scope
                && row.try_get::<String, _>("idempotency_key")? == event.idempotency_key
                && row.try_get::<Value, _>("payload_json")? == event.payload
                && row.try_get::<DateTime<Utc>, _>("occurred_at")? == event.occurred_at;
            let fact_matches = row.try_get::<Option<Uuid>, _>("fact_id")? == Some(fact.id())
                && row.try_get::<Option<Uuid>, _>("fact_connector_id")?
                    == Some(fact.connector_id())
                && row
                    .try_get::<Option<String>, _>("fact_identity_link")?
                    .as_deref()
                    == Some(fact.identity().database_key().as_str())
                && row
                    .try_get::<Option<String>, _>("fact_role_ref")?
                    .as_deref()
                    == Some(fact.role_ref().as_str())
                && row
                    .try_get::<Option<String>, _>("fact_scope_key")?
                    .as_deref()
                    == Some(scope_key(fact.scope()).as_str())
                && row.try_get::<Option<String>, _>("fact_action")?.as_deref()
                    == Some(fact.action().as_str())
                && row.try_get::<Option<i64>, _>("fact_sequence")?
                    == Some(
                        i64::try_from(fact.sequence())
                            .map_err(|_| DirectoryRbacError::InvalidFactSequence)?,
                    )
                && row
                    .try_get::<Option<String>, _>("fact_previous_hash")?
                    .as_deref()
                    == fact.previous_hash()
                && row.try_get::<Option<String>, _>("fact_hash")?.as_deref()
                    == Some(fact.fact_hash())
                && row.try_get::<Option<DateTime<Utc>>, _>("fact_occurred_at")?
                    == Some(fact.occurred_at());
            if !outbox_matches || !fact_matches {
                return Err(DirectoryRbacError::ImmutableConflict);
            }
            read_model.apply(&fact)?;
        }
        Ok(read_model)
    }

    pub async fn append_fact(
        &self,
        fact: &DirectoryRoleFact,
    ) -> Result<DirectoryRoleFactAppendOutcome, DirectoryRbacError> {
        if !fact.verify_hash() {
            return Err(DirectoryRbacError::FactHashInvalid);
        }
        if fact.sequence() > MAX_DIRECTORY_RBAC_FACT_CHAIN as u64 {
            return Err(DirectoryRbacError::FactChainTooLong);
        }
        let mut tx = self.pool.begin().await?;
        // Serialize appends for one immutable identity/role/scope chain. A
        // row-level tail lock alone cannot protect the first concurrent
        // append, because both transactions could observe an empty chain.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(fact.chain_key())
            .execute(&mut *tx)
            .await?;
        let existing = sqlx::query(
            "SELECT fact_hash FROM erpx_identity.directory_role_facts WHERE id=$1 FOR UPDATE",
        )
        .bind(fact.id())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = existing {
            let existing_hash: String = row.try_get("fact_hash")?;
            let existing_payload = sqlx::query(
                "SELECT payload_json FROM erpx_identity.directory_outbox WHERE event_id=$1",
            )
            .bind(fact.id())
            .fetch_optional(&mut *tx)
            .await?
            .and_then(|row| row.try_get::<Value, _>("payload_json").ok())
            .and_then(|payload| serde_json::from_value::<DirectoryRoleFact>(payload).ok());
            tx.rollback().await?;
            if existing_hash == fact.fact_hash() && existing_payload.as_ref() == Some(fact) {
                return Ok(DirectoryRoleFactAppendOutcome::AlreadyApplied);
            }
            return Err(DirectoryRbacError::ImmutableConflict);
        }
        // The authority/value pair is canonical. `identity_link` is a legacy
        // derived hash and may be NULL, but a populated hash must agree rather
        // than allowing either representation to authenticate the fact.
        let identity_link = sqlx::query(
            "SELECT active, revoked_at
             FROM erpx_identity.directory_identity_links
             WHERE connector_id=$1
               AND immutable_authority=$3
               AND immutable_value=$4
               AND (identity_link IS NULL OR identity_link=$2)
             FOR SHARE",
        )
        .bind(fact.connector_id())
        .bind(fact.identity().database_key())
        .bind(fact.identity().link().authority())
        .bind(fact.identity().link().value())
        .fetch_optional(&mut *tx)
        .await?;
        let identity_link = identity_link.ok_or(DirectoryRbacError::IdentityNotFound)?;
        if fact.action() == DirectoryRoleFactAction::Grant
            && (!identity_link.try_get::<bool, _>("active")?
                || identity_link
                    .try_get::<Option<DateTime<Utc>>, _>("revoked_at")?
                    .is_some())
        {
            tx.rollback().await?;
            return Err(DirectoryRbacError::IdentityInactive);
        }
        let scope = scope_key(fact.scope());
        let connector_scope_key: String = sqlx::query_scalar(
            "SELECT scope_key
               FROM erpx_identity.directory_connectors
              WHERE id=$1
              FOR SHARE",
        )
        .bind(fact.connector_id())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DirectoryRbacError::ConnectorNotFound)?;
        let connector_scope = parse_scope_key(&connector_scope_key)?;
        if !connector_scope.permits(fact.scope()) {
            tx.rollback().await?;
            return Err(DirectoryRbacError::MappingScopeOutsideConnector);
        }
        if fact.action() == DirectoryRoleFactAction::Grant {
            let mapping_active: Option<bool> = sqlx::query_scalar(
                "SELECT active
                   FROM erpx_identity.directory_role_mappings
                  WHERE connector_id=$1 AND group_ref=$2 AND role_ref=$3
                    AND scope_key=$4
                  FOR SHARE",
            )
            .bind(fact.connector_id())
            .bind(fact.group_ref().as_str())
            .bind(fact.role_ref().as_str())
            .bind(&scope)
            .fetch_optional(&mut *tx)
            .await?;
            if mapping_active != Some(true) {
                tx.rollback().await?;
                return Err(DirectoryRbacError::ManagedMappingNotActive);
            }
        }
        let tail = sqlx::query(
            "SELECT sequence, fact_hash
             FROM erpx_identity.directory_role_facts
             WHERE connector_id=$1 AND identity_link=$2 AND role_ref=$3 AND scope_key=$4
             ORDER BY sequence DESC
             LIMIT 1
             FOR UPDATE",
        )
        .bind(fact.connector_id())
        .bind(fact.identity().database_key())
        .bind(fact.role_ref().as_str())
        .bind(&scope)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = tail {
            let tail_sequence = u64::try_from(row.try_get::<i64, _>("sequence")?)
                .map_err(|_| DirectoryRbacError::InvalidFactSequence)?;
            let expected_sequence = tail_sequence
                .checked_add(1)
                .ok_or(DirectoryRbacError::FactChainTooLong)?;
            if fact.sequence() != expected_sequence {
                return Err(DirectoryRbacError::FactSequenceGap);
            }
            let tail_hash: String = row.try_get("fact_hash")?;
            if fact.previous_hash() != Some(tail_hash.as_str()) {
                return Err(DirectoryRbacError::FactHashChainBroken);
            }
        } else if fact.sequence() != 1 || fact.previous_hash().is_some() {
            return Err(DirectoryRbacError::FactSequenceGap);
        }
        sqlx::query(
            "INSERT INTO erpx_identity.directory_role_facts
                (id, connector_id, identity_link, role_ref, scope_key, action,
                 sequence, previous_hash, fact_hash, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(fact.id())
        .bind(fact.connector_id())
        .bind(fact.identity().database_key())
        .bind(fact.role_ref().as_str())
        .bind(&scope)
        .bind(fact.action().as_str())
        .bind(i64::try_from(fact.sequence()).map_err(|_| DirectoryRbacError::InvalidFactSequence)?)
        .bind(fact.previous_hash())
        .bind(fact.fact_hash())
        .bind(fact.occurred_at())
        .execute(&mut *tx)
        .await?;
        let event = DirectoryRoleOutboxEvent::from_fact(fact)?;
        let expected_scope = serde_json::to_value(&event.scope)
            .map_err(|_| DirectoryRbacError::EventEncodingFailed)?;
        let outbox_result = sqlx::query(
            "INSERT INTO erpx_identity.directory_outbox
                (event_id, aggregate_id, aggregate_type, event_type,
                 event_version, connector_id, aggregate_version, scope_json,
                 idempotency_key, payload_json, occurred_at)
             VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event.event_id)
        .bind(event.aggregate_id)
        .bind(&event.aggregate_type)
        .bind(&event.event_type)
        .bind(event.connector_id)
        .bind(
            i64::try_from(event.aggregate_version)
                .map_err(|_| DirectoryRbacError::InvalidFactSequence)?,
        )
        .bind(&expected_scope)
        .bind(&event.idempotency_key)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .execute(&mut *tx)
        .await?;
        if outbox_result.rows_affected() == 0 {
            let existing_payload = sqlx::query(
                "SELECT aggregate_id, aggregate_type, event_type, event_version,
                        connector_id, aggregate_version, scope_json, idempotency_key,
                        payload_json, occurred_at
                 FROM erpx_identity.directory_outbox WHERE event_id=$1",
            )
            .bind(event.event_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| {
                Ok::<bool, sqlx::Error>(
                    row.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
                        && row.try_get::<String, _>("aggregate_type")? == event.aggregate_type
                        && row.try_get::<String, _>("event_type")? == event.event_type
                        && row.try_get::<i32, _>("event_version")? == 1
                        && row.try_get::<Uuid, _>("connector_id")? == event.connector_id
                        && row.try_get::<i64, _>("aggregate_version")?
                            == i64::try_from(event.aggregate_version).unwrap_or_default()
                        && row.try_get::<Value, _>("scope_json")? == expected_scope
                        && row.try_get::<String, _>("idempotency_key")? == event.idempotency_key
                        && row.try_get::<Value, _>("payload_json")? == event.payload
                        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == event.occurred_at,
                )
            })
            .transpose()?;
            if existing_payload != Some(true) {
                tx.rollback().await?;
                return Err(DirectoryRbacError::ImmutableConflict);
            }
        }
        tx.commit().await?;
        Ok(DirectoryRoleFactAppendOutcome::Applied)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryMappingWriteOutcome {
    Applied { event_id: Uuid },
    AlreadyCurrent,
}

fn mfa_is_fresh(verified_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    verified_at.is_some_and(|verified_at| {
        now >= verified_at && now.signed_duration_since(verified_at) <= DIRECTORY_RBAC_MFA_WINDOW
    })
}

fn bounded_text(
    value: String,
    field: &'static str,
    max: usize,
) -> Result<String, DirectoryRbacError> {
    let value = value.trim().to_owned();
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(DirectoryRbacError::InvalidReference { field });
    }
    Ok(value)
}

fn scope_key(scope: &ConnectorScope) -> String {
    match scope {
        ConnectorScope::Platform => "platform".to_owned(),
        ConnectorScope::Holding(holding_id) => format!("holding:{holding_id}"),
        ConnectorScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
    }
}

fn validate_role_outbox_row(
    row: &sqlx::postgres::PgRow,
    fact: &DirectoryRoleFact,
    event: &DirectoryRoleOutboxEvent,
) -> Result<ConnectorScope, DirectoryRbacError> {
    let connector_scope = row
        .try_get::<Option<String>, _>("connector_scope_key")?
        .ok_or(DirectoryRbacError::ConnectorNotFound)
        .and_then(|scope| parse_scope_key(&scope))?;
    let expected_scope =
        serde_json::to_value(&event.scope).map_err(|_| DirectoryRbacError::EventEncodingFailed)?;
    let outbox_matches = row.try_get::<Uuid, _>("event_id")? == event.event_id
        && row.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
        && row.try_get::<String, _>("aggregate_type")? == event.aggregate_type
        && row.try_get::<String, _>("event_type")? == event.event_type
        && row.try_get::<Uuid, _>("connector_id")? == event.connector_id
        && u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
            .map_err(|_| DirectoryRbacError::InvalidFactSequence)?
            == event.aggregate_version
        && row.try_get::<Value, _>("scope_json")? == expected_scope
        && row.try_get::<String, _>("idempotency_key")? == event.idempotency_key
        && row.try_get::<Value, _>("payload_json")? == event.payload
        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == event.occurred_at;
    let fact_matches = row.try_get::<Option<Uuid>, _>("fact_id")? == Some(fact.id())
        && row.try_get::<Option<Uuid>, _>("fact_connector_id")? == Some(fact.connector_id())
        && row
            .try_get::<Option<String>, _>("fact_identity_link")?
            .as_deref()
            == Some(fact.identity().database_key().as_str())
        && row
            .try_get::<Option<String>, _>("fact_role_ref")?
            .as_deref()
            == Some(fact.role_ref().as_str())
        && row
            .try_get::<Option<String>, _>("fact_scope_key")?
            .as_deref()
            == Some(scope_key(fact.scope()).as_str())
        && row.try_get::<Option<String>, _>("fact_action")?.as_deref()
            == Some(fact.action().as_str())
        && row.try_get::<Option<i64>, _>("fact_sequence")?
            == Some(
                i64::try_from(fact.sequence())
                    .map_err(|_| DirectoryRbacError::InvalidFactSequence)?,
            )
        && row
            .try_get::<Option<String>, _>("fact_previous_hash")?
            .as_deref()
            == fact.previous_hash()
        && row.try_get::<Option<String>, _>("fact_hash")?.as_deref() == Some(fact.fact_hash())
        && row.try_get::<Option<DateTime<Utc>>, _>("fact_occurred_at")? == Some(fact.occurred_at());
    if !outbox_matches || !fact_matches || !fact.verify_hash() {
        return Err(DirectoryRbacError::ImmutableConflict);
    }
    Ok(connector_scope)
}

fn parse_scope_key(value: &str) -> Result<ConnectorScope, DirectoryRbacError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > 512
        || value.chars().any(char::is_control)
    {
        return Err(DirectoryRbacError::InvalidReference {
            field: "connector_scope",
        });
    }
    if value == "platform" {
        return Ok(ConnectorScope::Platform);
    }
    let holding_ref = value
        .strip_prefix("holding:")
        .filter(|reference| !reference.is_empty())
        .ok_or(DirectoryRbacError::InvalidReference {
            field: "connector_scope",
        })?;
    let holding_id = directory_holding_id(holding_ref);
    validate_scope(&ConnectorScope::Holding(holding_id))?;
    Ok(ConnectorScope::Holding(holding_id))
}

fn validate_scope(scope: &ConnectorScope) -> Result<(), DirectoryRbacError> {
    if matches!(scope, ConnectorScope::Holding(holding_id) if holding_id.is_nil()) {
        return Err(DirectoryRbacError::NilIdentifier {
            field: "holding_id",
        });
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_hash_reference(value: &str) -> Result<(), DirectoryRbacError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DirectoryRbacError::InvalidReference {
            field: "previous_hash",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 27, 12, 0, 0)
            .single()
            .expect("fixed timestamp")
    }

    fn link() -> DirectoryRoleIdentity {
        DirectoryRoleIdentity::from_immutable_link(
            ImmutableIdentityLink::new("ad:object_guid", "guid-1").expect("link"),
        )
    }

    fn mapping(
        connector_id: Uuid,
        connector_scope: ConnectorScope,
        requires_mfa: bool,
        revision: u64,
    ) -> DirectoryManagedRoleMapping {
        DirectoryManagedRoleMapping::try_new(
            connector_id,
            DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            DirectoryRoleRef::try_new("finance.approve").expect("role"),
            connector_scope,
            requires_mfa,
            true,
            revision,
            at(),
        )
        .expect("mapping")
    }

    #[test]
    fn mapping_requires_connector_and_actor_scope_containment() {
        let connector = Uuid::from_u128(1);
        let holding = Uuid::from_u128(2);
        let holding_mapping = mapping(connector, ConnectorScope::Holding(holding), false, 1);
        let set = DirectoryRoleMappingSet::try_new(
            connector,
            ConnectorScope::Platform,
            [holding_mapping.clone()],
        )
        .expect("platform connector can contain a holding mapping");
        let denied_actor = DirectoryRbacAuthorizationContext::try_new(
            "actor",
            ConnectorScope::Holding(Uuid::from_u128(3)),
            None,
            false,
            false,
        )
        .expect("context");
        assert_eq!(
            denied_actor.authorize(
                DirectoryRbacAction::WriteMapping,
                set.connector_scope(),
                holding_mapping.scope(),
                false,
                at(),
            ),
            Err(DirectoryRbacError::UnauthorizedScope)
        );
        let holding_connector = mapping(connector, ConnectorScope::Holding(holding), false, 1);
        assert!(
            DirectoryRoleMappingSet::try_new(
                connector,
                ConnectorScope::Holding(Uuid::from_u128(3)),
                [holding_connector]
            )
            .is_err()
        );
    }

    #[test]
    fn mapping_keys_do_not_collide_when_references_contain_delimiters() {
        let connector = Uuid::from_u128(1);
        let first = DirectoryManagedRoleMapping::try_new(
            connector,
            DirectoryGroupRef::try_new("group/a").expect("group"),
            DirectoryRoleRef::try_new("role").expect("role"),
            ConnectorScope::Platform,
            false,
            true,
            1,
            at(),
        )
        .expect("mapping");
        let second = DirectoryManagedRoleMapping::try_new(
            connector,
            DirectoryGroupRef::try_new("group").expect("group"),
            DirectoryRoleRef::try_new("a/role").expect("role"),
            ConnectorScope::Platform,
            false,
            true,
            1,
            at(),
        )
        .expect("mapping");
        let set = DirectoryRoleMappingSet::try_new(
            connector,
            ConnectorScope::Platform,
            [first.clone(), second.clone()],
        )
        .expect("distinct mappings");
        assert_eq!(set.mappings().count(), 2);
        assert_ne!(first.event_id(), second.event_id());
    }

    #[test]
    fn stale_feed_denies_roles_even_when_group_matches() {
        let connector = Uuid::from_u128(1);
        let group = DirectoryGroupRef::try_new("group:finance-admin").expect("group");
        let mapping = mapping(connector, ConnectorScope::Platform, false, 1);
        let set = DirectoryRoleMappingSet::try_new(connector, ConnectorScope::Platform, [mapping])
            .expect("set");
        let result = DirectoryRoleEvaluation::evaluate(
            link(),
            connector,
            &ConnectorScope::Platform,
            &BTreeSet::from([group]),
            true,
            DirectoryFeedFreshness::new(at() - Duration::minutes(5) - Duration::seconds(1)),
            &set,
            Some(at()),
            at(),
        )
        .expect("evaluation");
        assert_eq!(result.denied(), Some(DirectoryRoleDenyReason::FeedStale));
        assert!(result.granted().is_empty());
    }

    #[test]
    fn explicit_revocation_denies_roles_before_the_five_minute_deadline() {
        let connector = Uuid::from_u128(1);
        let mapping = mapping(connector, ConnectorScope::Platform, false, 1);
        let set = DirectoryRoleMappingSet::try_new(connector, ConnectorScope::Platform, [mapping])
            .expect("set");
        let result = DirectoryRoleEvaluation::evaluate(
            link(),
            connector,
            &ConnectorScope::Platform,
            &BTreeSet::from([DirectoryGroupRef::try_new("group:finance-admin").expect("group")]),
            true,
            DirectoryFeedFreshness::revoked(at()),
            &set,
            Some(at()),
            at() + Duration::seconds(1),
        )
        .expect("evaluation");
        assert_eq!(
            result.denied(),
            Some(DirectoryRoleDenyReason::IdentityInactive)
        );
        assert!(result.granted().is_empty());
    }

    #[test]
    fn high_risk_and_mfa_mapping_fail_closed_but_fresh_mfa_allows() {
        let connector = Uuid::from_u128(1);
        let mapping = mapping(connector, ConnectorScope::Platform, true, 1);
        let set = DirectoryRoleMappingSet::try_new(connector, ConnectorScope::Platform, [mapping])
            .expect("set");
        let no_mfa = DirectoryRbacAuthorizationContext::try_new(
            "actor",
            ConnectorScope::Platform,
            None,
            false,
            false,
        )
        .expect("context");
        assert_eq!(
            no_mfa.authorize(
                DirectoryRbacAction::ApplyRoleFact,
                &ConnectorScope::Platform,
                &ConnectorScope::Platform,
                true,
                at(),
            ),
            Err(DirectoryRbacError::MfaRequired)
        );
        let with_mfa = DirectoryRbacAuthorizationContext::try_new(
            "actor",
            ConnectorScope::Platform,
            Some(at() - Duration::minutes(4)),
            false,
            false,
        )
        .expect("context");
        assert!(
            with_mfa
                .authorize(
                    DirectoryRbacAction::ApplyRoleFact,
                    &ConnectorScope::Platform,
                    &ConnectorScope::Platform,
                    true,
                    at(),
                )
                .is_ok()
        );
        let evaluation = DirectoryRoleEvaluation::evaluate(
            link(),
            connector,
            &ConnectorScope::Platform,
            &BTreeSet::from([DirectoryGroupRef::try_new("group:finance-admin").expect("group")]),
            true,
            DirectoryFeedFreshness::new(at()),
            &set,
            None,
            at(),
        )
        .expect("evaluation");
        assert!(evaluation.granted().is_empty());
        assert_eq!(evaluation.mfa_required().len(), 1);

        let stale_mfa = DirectoryRoleEvaluation::evaluate(
            link(),
            connector,
            &ConnectorScope::Platform,
            &BTreeSet::from([DirectoryGroupRef::try_new("group:finance-admin").expect("group")]),
            true,
            DirectoryFeedFreshness::new(at()),
            &set,
            Some(at() - DIRECTORY_RBAC_MFA_WINDOW - Duration::nanoseconds(1)),
            at(),
        )
        .expect("evaluation");
        assert!(stale_mfa.granted().is_empty());
        assert_eq!(stale_mfa.mfa_required().len(), 1);
    }

    #[test]
    fn local_break_glass_is_local_only_and_fact_chain_is_append_only() {
        assert_eq!(
            DirectoryRbacAuthorizationContext::try_new(
                "actor",
                ConnectorScope::Platform,
                Some(at()),
                true,
                false,
            ),
            Err(DirectoryRbacError::BreakGlassMustBeLocal)
        );
        let mut ledger = DirectoryRoleFactLedger::default();
        let input = || DirectoryRoleFactInput {
            identity: link(),
            connector_id: Uuid::from_u128(1),
            group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
            scope: ConnectorScope::Platform,
            action: DirectoryRoleFactAction::Revoke,
            reason: DirectoryRoleFactReason::IdentityDisabled,
            idempotency_key: "directory-revoke/1".to_owned(),
            occurred_at: at(),
        };
        let (first_outcome, first) = ledger.append(input()).expect("first fact");
        assert_eq!(first_outcome, DirectoryRoleFactAppendOutcome::Applied);
        let (replay_outcome, replay) = ledger.append(input()).expect("replay");
        assert_eq!(
            replay_outcome,
            DirectoryRoleFactAppendOutcome::AlreadyApplied
        );
        assert_eq!(first.id(), replay.id());
        assert!(ledger.verify().is_ok());
        assert_eq!(ledger.facts().len(), 1);
    }

    #[test]
    fn local_break_glass_cannot_authorize_directory_managed_rbac() {
        let context = DirectoryRbacAuthorizationContext::try_new(
            "break-glass-actor",
            ConnectorScope::Platform,
            Some(at()),
            true,
            true,
        )
        .expect("localhost-only break-glass context");
        assert_eq!(
            context.authorize(
                DirectoryRbacAction::ReadMapping,
                &ConnectorScope::Platform,
                &ConnectorScope::Platform,
                false,
                at(),
            ),
            Err(DirectoryRbacError::BreakGlassFallbackOnly)
        );
        assert!(context.is_local_break_glass());
    }

    #[test]
    fn projector_is_idempotent_and_rollback_preserves_history() {
        let mut ledger = DirectoryRoleFactLedger::default();
        let input = DirectoryRoleFactInput {
            identity: link(),
            connector_id: Uuid::from_u128(1),
            group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
            scope: ConnectorScope::Platform,
            action: DirectoryRoleFactAction::Grant,
            reason: DirectoryRoleFactReason::GroupMatched,
            idempotency_key: "directory-grant/1".to_owned(),
            occurred_at: at(),
        };
        let (_, fact) = ledger.append(input).expect("fact");
        let mut read = DirectoryRoleReadModel::default();
        read.apply(&fact).expect("project");
        read.apply(&fact).expect("duplicate event is a no-op");
        assert_eq!(read.applied_event_count(), 1);
        assert_eq!(read.roles_for(&link(), Uuid::from_u128(1)).len(), 1);
        let rollback = DirectoryRbacRollbackPlan::finalized(
            Uuid::from_u128(1),
            "source snapshot was inconsistent",
        )
        .expect("rollback");
        assert!(rollback.preserves_history());
        assert!(rollback.rebuilds_read_model());
    }

    #[test]
    fn a_group_revoke_does_not_remove_a_role_granted_by_another_group() {
        let identity = link();
        let connector_id = Uuid::from_u128(1);
        let role = DirectoryRoleRef::try_new("finance.approve").expect("role");
        let scope = ConnectorScope::Platform;
        let mut ledger = DirectoryRoleFactLedger::default();
        let fact = |group: &str,
                    action: DirectoryRoleFactAction,
                    reason: DirectoryRoleFactReason,
                    key: &str| DirectoryRoleFactInput {
            identity: identity.clone(),
            connector_id,
            group_ref: DirectoryGroupRef::try_new(group).expect("group"),
            role_ref: role.clone(),
            scope: scope.clone(),
            action,
            reason,
            idempotency_key: key.to_owned(),
            occurred_at: at(),
        };
        let (_, first_grant) = ledger
            .append(fact(
                "group:finance-approvers",
                DirectoryRoleFactAction::Grant,
                DirectoryRoleFactReason::GroupMatched,
                "two-group-grant-a",
            ))
            .expect("first group grant");
        let (_, second_grant) = ledger
            .append(fact(
                "group:finance-auditors",
                DirectoryRoleFactAction::Grant,
                DirectoryRoleFactReason::GroupMatched,
                "two-group-grant-b",
            ))
            .expect("second group grant");
        let (_, revoke_a) = ledger
            .append(fact(
                "group:finance-approvers",
                DirectoryRoleFactAction::Revoke,
                DirectoryRoleFactReason::GroupRemoved,
                "two-group-revoke-a",
            ))
            .expect("first group revoke");

        let mut read_model = DirectoryRoleReadModel::default();
        for event in [&first_grant, &second_grant, &revoke_a] {
            read_model.apply(event).expect("project role event");
        }
        assert_eq!(
            read_model.roles_for(&identity, connector_id),
            BTreeSet::from([(role, "platform".to_owned())])
        );
    }

    #[test]
    fn role_fact_rejects_contradictory_action_and_reason_pairs() {
        let build = |action, reason| DirectoryRoleFactInput {
            identity: link(),
            connector_id: Uuid::from_u128(1),
            group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
            scope: ConnectorScope::Platform,
            action,
            reason,
            idempotency_key: "directory-role-invalid-semantics".to_owned(),
            occurred_at: at(),
        };
        let mut ledger = DirectoryRoleFactLedger::default();
        for reason in [
            DirectoryRoleFactReason::GroupRemoved,
            DirectoryRoleFactReason::MappingDeactivated,
            DirectoryRoleFactReason::IdentityDisabled,
            DirectoryRoleFactReason::IdentityDeleted,
            DirectoryRoleFactReason::FeedStale,
        ] {
            assert_eq!(
                ledger.append(build(DirectoryRoleFactAction::Grant, reason)),
                Err(DirectoryRbacError::InvalidFactReason)
            );
        }
        assert_eq!(
            ledger.append(build(
                DirectoryRoleFactAction::Revoke,
                DirectoryRoleFactReason::GroupMatched,
            )),
            Err(DirectoryRbacError::InvalidFactReason)
        );
        assert!(
            ledger
                .append(build(
                    DirectoryRoleFactAction::Grant,
                    DirectoryRoleFactReason::Reconciliation,
                ))
                .is_ok()
        );
    }

    #[test]
    fn role_fact_conflicts_and_projection_gaps_fail_closed() {
        let base = DirectoryRoleFactInput {
            identity: link(),
            connector_id: Uuid::from_u128(1),
            group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
            scope: ConnectorScope::Platform,
            action: DirectoryRoleFactAction::Grant,
            reason: DirectoryRoleFactReason::GroupMatched,
            idempotency_key: "directory-role-conflict".to_owned(),
            occurred_at: at(),
        };
        let mut ledger = DirectoryRoleFactLedger::default();
        let (_, first) = ledger.append(base.clone()).expect("fact");
        let mut conflict = base.clone();
        conflict.reason = DirectoryRoleFactReason::Reconciliation;
        assert_eq!(
            ledger.append(conflict),
            Err(DirectoryRbacError::ImmutableConflict)
        );

        let mut second_input = base;
        second_input.action = DirectoryRoleFactAction::Revoke;
        second_input.reason = DirectoryRoleFactReason::GroupRemoved;
        second_input.idempotency_key = "directory-role-next".to_owned();
        let (_, second) = ledger.append(second_input).expect("second fact");
        let mut read = DirectoryRoleReadModel::default();
        assert_eq!(
            read.apply(&second),
            Err(DirectoryRbacError::FactSequenceGap)
        );
        read.apply(&first).expect("first projection");
        read.apply(&second).expect("second projection");
        assert!(read.roles_for(&link(), Uuid::from_u128(1)).is_empty());
    }

    #[test]
    fn role_fact_idempotency_cannot_rebind_an_immutable_identity_link() {
        let mut ledger = DirectoryRoleFactLedger::default();
        let base = DirectoryRoleFactInput {
            identity: link(),
            connector_id: Uuid::from_u128(1),
            group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
            role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
            scope: ConnectorScope::Platform,
            action: DirectoryRoleFactAction::Revoke,
            reason: DirectoryRoleFactReason::IdentityDisabled,
            idempotency_key: "directory-link-rebind".to_owned(),
            occurred_at: at(),
        };
        ledger.append(base.clone()).expect("fact");
        let mut rebound = base;
        rebound.identity = DirectoryRoleIdentity::from_immutable_link(
            ImmutableIdentityLink::new("ad:object_guid", "guid-2").expect("link"),
        );
        assert_eq!(
            ledger.append(rebound),
            Err(DirectoryRbacError::ImmutableConflict)
        );
    }

    #[test]
    fn outbox_aggregate_is_connector_scoped_for_one_immutable_identity() {
        let build_fact = |connector_id: Uuid, idempotency_key: &str| {
            DirectoryRoleFact::from_input(
                DirectoryRoleFactInput {
                    identity: link(),
                    connector_id,
                    group_ref: DirectoryGroupRef::try_new("group:finance-admin").expect("group"),
                    role_ref: DirectoryRoleRef::try_new("finance.approve").expect("role"),
                    scope: ConnectorScope::Platform,
                    action: DirectoryRoleFactAction::Grant,
                    reason: DirectoryRoleFactReason::GroupMatched,
                    idempotency_key: idempotency_key.to_owned(),
                    occurred_at: at(),
                },
                1,
                None,
            )
            .expect("fact")
        };
        let first = DirectoryRoleOutboxEvent::from_fact(&build_fact(
            Uuid::from_u128(1),
            "directory-grant/connector-1",
        ))
        .expect("event");
        let second = DirectoryRoleOutboxEvent::from_fact(&build_fact(
            Uuid::from_u128(2),
            "directory-grant/connector-2",
        ))
        .expect("event");
        assert_ne!(first.aggregate_id, second.aggregate_id);
    }

    #[test]
    fn opaque_scope_normalization_rejects_empty_or_malformed_holding_keys() {
        assert!(parse_scope_key("holding:").is_err());
        assert!(parse_scope_key(" holding:abc").is_err());
        assert!(parse_scope_key("holding:abc\n").is_err());
        assert_eq!(
            parse_scope_key("holding:legacy-ref").expect("opaque holding scope"),
            ConnectorScope::Holding(directory_holding_id("legacy-ref"))
        );
    }
}
