//! Directory-backed identity, synchronization, and role-mapping contracts.
//!
//! This module is deliberately protocol-neutral at the wire boundary.  LDAP,
//! Active Directory DirSync, RFC 4533, and UCS Provisioning Service adapters
//! feed the same bounded state machine; credentials and directory payloads do
//! not cross the transport or read-model boundary.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use super::development_access::DirectoryRoleTargetScope;
use feeds::FeedCursor;
use rbac::{
    DirectoryGroupRef, DirectoryRoleFact as RbacRoleFact,
    DirectoryRoleFactAction as RbacRoleFactAction, DirectoryRoleFactInput, DirectoryRoleFactReason,
    DirectoryRoleIdentity, DirectoryRoleOutboxEvent, DirectoryRoleRef,
    append_mapping_deactivation_revokes,
};
use sync::ConnectorScope as RbacConnectorScope;

pub mod auth;
pub mod feeds;
pub mod live;
pub mod rbac;
pub mod revocation;
pub mod secrets;
pub mod sync;
pub mod tls;

pub const MAX_DIRECTORY_IMPORT_USERS: u64 = 1_000_000;
pub const MAX_DIRECTORY_PAGE_SIZE: u32 = 10_000;
pub const MAX_DIRECTORY_BATCH_SIZE: usize = 2_000;
pub const DIRECTORY_REVOCATION_SLA: Duration = Duration::minutes(5);
pub const DIRECTORY_PREFLIGHT_MAX_AGE: Duration = Duration::minutes(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryConnectorLifecycle {
    Draft,
    Verified,
    Importing,
    Ready,
    Active,
    Suspended,
    Failed,
}

impl DirectoryConnectorLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Verified => "verified",
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Failed => "failed",
        }
    }

    pub const fn permits_authentication(self) -> bool {
        matches!(self, Self::Active)
    }

    pub fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Draft, Self::Verified)
                | (Self::Verified, Self::Importing)
                | (Self::Importing, Self::Ready)
                | (Self::Ready, Self::Active)
                | (Self::Active, Self::Suspended)
                | (Self::Suspended, Self::Verified | Self::Active)
                | (Self::Failed, Self::Verified)
        ) || target == Self::Failed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryProvider {
    GenericLdap,
    ActiveDirectoryDomainServices,
    UniventionCorporateServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryTlsMode {
    Ldaps,
    StartTls,
    Https,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryFeedKind {
    Polling,
    AdDirSync,
    Rfc4533,
    UcsProvisioningService,
}

impl DirectoryFeedKind {
    pub const fn supported_by(self, provider: DirectoryProvider) -> bool {
        matches!(
            (provider, self),
            (DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling)
                | (DirectoryProvider::GenericLdap, DirectoryFeedKind::Rfc4533)
                | (
                    DirectoryProvider::ActiveDirectoryDomainServices,
                    DirectoryFeedKind::AdDirSync
                )
                | (
                    DirectoryProvider::UniventionCorporateServer,
                    DirectoryFeedKind::UcsProvisioningService,
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryScope {
    Platform,
    Holding {
        holding_ref: String,
    },
    Branch {
        holding_ref: String,
        company_id: Uuid,
        branch_id: Uuid,
    },
}

impl DirectoryScope {
    pub fn canonical_key(&self) -> Result<String, DirectoryError> {
        match self {
            Self::Platform => Ok("platform".to_owned()),
            Self::Holding { holding_ref } => Ok(format!(
                "holding:{}",
                bounded_ref(holding_ref, "holding_ref")?
            )),
            Self::Branch {
                holding_ref,
                company_id,
                branch_id,
            } => {
                let holding_ref = bounded_ref(holding_ref, "holding_ref")?;
                if company_id.is_nil() || branch_id.is_nil() {
                    return Err(DirectoryError::InvalidReference(
                        "branch directory scope requires non-nil company and branch IDs".into(),
                    ));
                }
                Ok(format!("branch:{holding_ref}:{company_id}:{branch_id}"))
            }
        }
    }

    /// Returns the stable holding identity used by worker-side connector
    /// synchronization. Holding references are intentionally opaque at the
    /// storage boundary; UUID references remain backwards compatible while
    /// other bounded references use the same namespace-derived identity as
    /// the native directory runner.
    pub fn holding_id(&self) -> Option<Uuid> {
        match self {
            Self::Platform => None,
            Self::Holding { holding_ref } | Self::Branch { holding_ref, .. } => {
                Some(sync::directory_holding_id(holding_ref))
            }
        }
    }

    /// Returns the stable scope key used by role authorization and role
    /// facts.  The externally supplied holding reference remains available in
    /// connector configuration, but mappings must authorize against the same
    /// immutable identity used by the tenant scope boundary.
    pub fn authorization_key(&self) -> Result<String, DirectoryError> {
        self.canonical_key()?;
        match self {
            Self::Platform => Ok("platform".to_owned()),
            Self::Holding { .. } => Ok(format!(
                "holding:{}",
                self.holding_id()
                    .expect("holding scope has a stable holding identity")
            )),
            Self::Branch {
                company_id,
                branch_id,
                ..
            } => Ok(format!(
                "branch:{}:{company_id}:{branch_id}",
                self.holding_id()
                    .expect("branch scope has a stable holding identity")
            )),
        }
    }

    /// A platform connector may administer platform and holding mappings;
    /// holding connectors are restricted to their own holding.  This is kept
    /// on the identity domain type so SQLx callers cannot accidentally widen a
    /// connector's role-management scope.
    pub fn permits(&self, requested: &Self) -> bool {
        match (self, requested) {
            (Self::Platform, Self::Platform | Self::Holding { .. } | Self::Branch { .. }) => true,
            (Self::Holding { .. }, Self::Holding { .. }) => {
                self.holding_id() == requested.holding_id()
            }
            (Self::Holding { .. }, Self::Branch { .. }) => {
                self.holding_id() == requested.holding_id()
            }
            (
                Self::Branch {
                    company_id: expected_company,
                    branch_id: expected_branch,
                    ..
                },
                Self::Branch {
                    company_id: requested_company,
                    branch_id: requested_branch,
                    ..
                },
            ) => {
                self.holding_id() == requested.holding_id()
                    && expected_company == requested_company
                    && expected_branch == requested_branch
            }
            (Self::Holding { .. }, Self::Platform) => false,
            (Self::Branch { .. }, Self::Platform | Self::Holding { .. }) => false,
        }
    }

    pub const fn company_id(&self) -> Option<Uuid> {
        match self {
            Self::Branch { company_id, .. } => Some(*company_id),
            Self::Platform | Self::Holding { .. } => None,
        }
    }

    pub const fn branch_id(&self) -> Option<Uuid> {
        match self {
            Self::Branch { branch_id, .. } => Some(*branch_id),
            Self::Platform | Self::Holding { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryTlsPolicy {
    pub mode: DirectoryTlsMode,
    pub ca_certificate_sha256: String,
    pub server_name: String,
    pub minimum_tls_version: String,
}

impl DirectoryTlsPolicy {
    pub fn validate(&self) -> Result<(), DirectoryError> {
        let fingerprint = self.ca_certificate_sha256.trim();
        if fingerprint.len() != 64 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DirectoryError::InvalidTlsPolicy(
                "CA certificate SHA-256 fingerprint must be 64 hexadecimal characters".into(),
            ));
        }
        if self.server_name.trim().is_empty()
            || self.server_name.len() > 253
            || self.server_name.chars().any(char::is_control)
        {
            return Err(DirectoryError::InvalidTlsPolicy(
                "TLS server name is empty or invalid".into(),
            ));
        }
        if self.minimum_tls_version != "1.2" && self.minimum_tls_version != "1.3" {
            return Err(DirectoryError::InvalidTlsPolicy(
                "directory TLS requires TLS 1.2 or TLS 1.3".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryConnector {
    pub id: Uuid,
    pub provider: DirectoryProvider,
    pub scope: DirectoryScope,
    pub endpoint: String,
    pub base_dn: String,
    pub tls: DirectoryTlsPolicy,
    pub feed: DirectoryFeedKind,
    pub credential_ref: String,
    /// Approved server/agent-side resolver key. It is never a bind password.
    pub credential_profile_key: String,
    /// Branch views must reference their holding connector.
    pub parent_connector_id: Option<Uuid>,
    /// Verified directory/forest identity. Draft connectors may omit it until
    /// preflight; activation requires it for branch views.
    pub directory_authority_fingerprint: Option<String>,
    pub fallback_to_holding: bool,
    pub enabled: bool,
    pub max_import_users: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryConnectorSnapshot {
    pub id: Uuid,
    pub provider: DirectoryProvider,
    pub scope_key: String,
    pub endpoint: String,
    pub base_dn: String,
    pub tls_mode: DirectoryTlsMode,
    pub feed: DirectoryFeedKind,
    pub enabled: bool,
    pub lifecycle: DirectoryConnectorLifecycle,
    pub max_import_users: u64,
    pub revision: u64,
    pub status: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_preflight_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub unresolved_conflicts: u32,
    pub credential_profile_key: String,
    pub parent_connector_id: Option<Uuid>,
    pub directory_authority_fingerprint: Option<String>,
    pub fallback_to_holding: bool,
    pub holding_ref: Option<String>,
    pub company_id: Option<Uuid>,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryConnectorReadinessSnapshot {
    pub connector_id: Uuid,
    pub lifecycle: DirectoryConnectorLifecycle,
    pub last_preflight_at: Option<DateTime<Utc>>,
    pub last_import_completed_at: Option<DateTime<Utc>>,
    pub last_reconciled_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub unresolved_conflicts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryConnectorRuntimeSettings {
    pub user_filter: String,
    pub page_size: u32,
    pub attribute_mapping: serde_json::Value,
    pub ca_reference: String,
    pub feed_configuration: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryIdentityConflictSnapshot {
    pub conflict_id: Uuid,
    pub connector_id: Uuid,
    pub local_principal_ref: String,
    pub match_reasons: Vec<String>,
    pub state: String,
    pub proposal_state: Option<String>,
    pub proposed_by: Option<String>,
    pub conflict_version: u64,
    pub detected_at: DateTime<Utc>,
    pub proposed_at: Option<DateTime<Utc>>,
}

impl DirectoryConnectorReadinessSnapshot {
    pub fn readiness(&self, at: DateTime<Utc>) -> &'static str {
        if self.lifecycle == DirectoryConnectorLifecycle::Active
            && self.unresolved_conflicts == 0
            && self.last_preflight_at.is_some_and(|value| {
                value <= at && at.signed_duration_since(value) <= DIRECTORY_PREFLIGHT_MAX_AGE
            })
            && self.last_import_completed_at.is_some()
            && self.last_reconciled_at.is_some()
            && self.last_heartbeat_at.is_some_and(|value| {
                value <= at && at.signed_duration_since(value) <= DIRECTORY_REVOCATION_SLA
            })
        {
            "ready"
        } else if self.lifecycle == DirectoryConnectorLifecycle::Draft {
            "unconfigured"
        } else {
            "not_ready"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryPrincipalBindingSnapshot {
    pub binding_id: Uuid,
    pub connector_id: Uuid,
    pub external_object_id: String,
    pub principal_id: Uuid,
    pub principal_ref: String,
    pub active: bool,
    pub revision: u64,
    pub linked_at: DateTime<Utc>,
    pub unlinked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectorySupervisorClaim {
    pub connector_id: Uuid,
    pub owner_ref: Uuid,
    pub lease_expires_at: DateTime<Utc>,
    pub consecutive_failures: u32,
}

/// Bounded read-side representation of one persisted immutable directory
/// identity link.  This is deliberately separate from the mutable directory
/// projection: the link and its source revision are read from the
/// authoritative write-side relation, while callers can still paginate large
/// imports without loading a million-row connector into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryIdentityLinkSnapshot {
    pub connector_id: Uuid,
    pub external_object_id: String,
    pub identity_link: Option<String>,
    pub principal_ref: Option<String>,
    pub identity_id: Option<Uuid>,
    pub active: bool,
    pub source_revision: u64,
    pub observed_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub immutable_authority: Option<String>,
    pub immutable_value: Option<String>,
}

/// Secret-free progress read model for one authoritative full-import row.
/// The cursor itself is reduced to its protocol kind; opaque cookies and
/// source payloads never cross the identity/API boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryImportStatus {
    pub import_id: Uuid,
    pub connector_id: Uuid,
    pub state: String,
    pub max_users: u64,
    pub users_seen: u64,
    pub pages_applied: u64,
    pub page_count: u64,
    pub cursor_kind: String,
    pub started_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub failure_category: Option<String>,
}

impl DirectoryConnector {
    pub fn validate(&self) -> Result<(), DirectoryError> {
        if self.id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "connector id is nil".into(),
            ));
        }
        self.scope.canonical_key()?;
        self.tls.validate()?;
        let endpoint_matches_tls = match (self.provider, self.tls.mode) {
            (DirectoryProvider::UniventionCorporateServer, DirectoryTlsMode::Https) => {
                self.endpoint.starts_with("https://")
            }
            (_, DirectoryTlsMode::Ldaps) => self.endpoint.starts_with("ldaps://"),
            // StartTLS begins on an LDAP URI but the adapter must complete the
            // TLS upgrade before sending any bind credentials.
            (_, DirectoryTlsMode::StartTls) => self.endpoint.starts_with("ldap://"),
            _ => false,
        };
        if !endpoint_matches_tls {
            return Err(DirectoryError::TlsRequired);
        }
        validate_endpoint_server_name(&self.endpoint, self.tls.mode, &self.tls.server_name)?;
        if self.base_dn.trim().is_empty() || self.base_dn.len() > 2048 {
            return Err(DirectoryError::InvalidReference(
                "base_dn is empty or too long".into(),
            ));
        }
        bounded_ref(&self.credential_ref, "credential_ref")?;
        bounded_ref(&self.credential_profile_key, "credential_profile_key")?;
        if !self
            .credential_profile_key
            .bytes()
            .enumerate()
            .all(|(index, value)| match value {
                b'a'..=b'z' => true,
                b'0'..=b'9' | b'_' | b'-' => index > 0,
                _ => false,
            })
            || self.credential_profile_key.len() > 64
        {
            return Err(DirectoryError::InvalidReference(
                "directory credential profile key is invalid".into(),
            ));
        }
        match &self.scope {
            DirectoryScope::Branch { .. } => {
                if self.parent_connector_id.is_none() {
                    return Err(DirectoryError::InvalidPolicy(
                        "branch directory view requires a holding connector".into(),
                    ));
                }
                if self.enabled && self.directory_authority_fingerprint.is_none() {
                    return Err(DirectoryError::InvalidPolicy(
                        "active branch directory view requires a verified authority fingerprint"
                            .into(),
                    ));
                }
            }
            DirectoryScope::Platform | DirectoryScope::Holding { .. } => {
                if self.parent_connector_id.is_some() || self.fallback_to_holding {
                    return Err(DirectoryError::InvalidPolicy(
                        "only branch directory views may reference holding fallback".into(),
                    ));
                }
            }
        }
        if let Some(fingerprint) = &self.directory_authority_fingerprint
            && (fingerprint.len() != 64
                || !fingerprint.chars().all(|value| value.is_ascii_hexdigit()))
        {
            return Err(DirectoryError::InvalidPolicy(
                "directory authority fingerprint must be SHA-256 hexadecimal".into(),
            ));
        }
        if self.max_import_users == 0 || self.max_import_users > MAX_DIRECTORY_IMPORT_USERS {
            return Err(DirectoryError::ImportLimitExceeded {
                requested: self.max_import_users,
            });
        }
        if !self.feed.supported_by(self.provider) {
            return Err(DirectoryError::UnsupportedFeed {
                provider: self.provider,
                feed: self.feed,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryAuthPolicy {
    pub require_mfa_for_high_risk: bool,
    pub allow_local_break_glass_only: bool,
    pub access_token_lifetime: Duration,
}

impl Default for DirectoryAuthPolicy {
    fn default() -> Self {
        Self {
            require_mfa_for_high_risk: true,
            allow_local_break_glass_only: true,
            access_token_lifetime: Duration::minutes(5),
        }
    }
}

impl DirectoryAuthPolicy {
    pub fn validate(&self) -> Result<(), DirectoryError> {
        if self.access_token_lifetime <= Duration::zero()
            || self.access_token_lifetime > DIRECTORY_REVOCATION_SLA
        {
            return Err(DirectoryError::InvalidPolicy(
                "directory access tokens must be positive and no longer than five minutes".into(),
            ));
        }
        if !self.allow_local_break_glass_only {
            return Err(DirectoryError::InvalidPolicy(
                "local fallback must remain break-glass-only".into(),
            ));
        }
        Ok(())
    }

    pub fn decide(
        &self,
        connector: &DirectoryConnector,
        request: &DirectoryAuthRequest,
        state: &DirectoryRevocationState,
    ) -> Result<DirectoryAuthDecision, DirectoryError> {
        self.validate()?;
        connector.validate()?;
        if request.connector_id != connector.id {
            return Err(DirectoryError::InvalidReference(
                "authentication connector does not match the configured connector".into(),
            ));
        }
        let identity_link = immutable_identity_link(connector.id, &request.external_object_id)?;
        if state.identity_link != identity_link {
            return Err(DirectoryError::InvalidReference(
                "directory revocation state does not match the requested identity".into(),
            ));
        }
        if !connector.enabled || !state.authorization_allowed(request.at) {
            return Ok(DirectoryAuthDecision::Revoked);
        }
        if matches!(request.risk, DirectoryRisk::High)
            && self.require_mfa_for_high_risk
            && !request.mfa_satisfied
        {
            return Ok(DirectoryAuthDecision::RequireMfa);
        }
        Ok(DirectoryAuthDecision::Allow {
            identity_link,
            expires_at: request.at + self.access_token_lifetime,
        })
    }

    /// Local credentials are an emergency recovery path only. Callers must
    /// present an explicitly marked break-glass request and a separate MFA
    /// proof; ordinary directory outage recovery cannot silently downgrade to
    /// a local password.
    pub fn allow_local_break_glass(
        &self,
        explicitly_marked: bool,
        mfa_satisfied: bool,
    ) -> Result<(), DirectoryError> {
        self.validate()?;
        if !explicitly_marked || !mfa_satisfied {
            return Err(DirectoryError::BreakGlassRejected);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRisk {
    Standard,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryAuthRequest {
    pub connector_id: Uuid,
    pub external_object_id: String,
    pub risk: DirectoryRisk,
    pub mfa_satisfied: bool,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectoryAuthDecision {
    Allow {
        identity_link: String,
        expires_at: DateTime<Utc>,
    },
    RequireMfa,
    BreakGlassOnly,
    Revoked,
}

pub fn immutable_identity_link(
    connector_id: Uuid,
    external_object_id: &str,
) -> Result<String, DirectoryError> {
    let external_object_id = bounded_ref(external_object_id, "external_object_id")?;
    let mut digest = Sha256::new();
    digest.update(b"erpx-directory-identity-v1:");
    digest.update(connector_id.as_bytes());
    digest.update([0]);
    digest.update(external_object_id.as_bytes());
    Ok(hex_lower(&digest.finalize()))
}

fn directory_identity_id(directory_authority: &str, external_object_id: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("directory-identity/{directory_authority}/{external_object_id}").as_bytes(),
    )
}

pub fn directory_authority_key(connector: &DirectoryConnector) -> Result<String, DirectoryError> {
    connector.validate()?;
    let provider = provider_key(connector.provider);
    let authority = connector
        .directory_authority_fingerprint
        .as_ref()
        .map(|fingerprint| fingerprint.to_ascii_lowercase())
        .unwrap_or_else(|| format!("connector:{}", connector.id));
    bounded_ref(&authority, "directory_authority")?;
    Ok(format!("{provider}:{authority}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryImportPlan {
    pub connector_id: Uuid,
    pub total_users: u64,
    pub page_size: u32,
    pub max_in_flight_pages: u16,
}

impl DirectoryImportPlan {
    pub fn new(
        connector: &DirectoryConnector,
        total_users: u64,
        page_size: u32,
    ) -> Result<Self, DirectoryError> {
        Self::with_tuning(
            connector,
            total_users,
            page_size,
            sync::DEFAULT_MAX_IN_FLIGHT_PAGES,
        )
    }

    pub fn with_tuning(
        connector: &DirectoryConnector,
        total_users: u64,
        page_size: u32,
        max_in_flight_pages: u16,
    ) -> Result<Self, DirectoryError> {
        connector.validate()?;
        if total_users == 0
            || total_users > connector.max_import_users
            || total_users > MAX_DIRECTORY_IMPORT_USERS
        {
            return Err(DirectoryError::ImportLimitExceeded {
                requested: total_users,
            });
        }
        if page_size == 0 || page_size > sync::MAX_DIRECTORY_PAGE_SIZE {
            return Err(DirectoryError::InvalidBatch(
                "page_size is outside the native worker safe bound".into(),
            ));
        }
        if max_in_flight_pages == 0 || max_in_flight_pages > sync::MAX_IN_FLIGHT_PAGES {
            return Err(DirectoryError::InvalidBatch(
                "max_in_flight_pages is outside the native worker safe bound".into(),
            ));
        }
        Ok(Self {
            connector_id: connector.id,
            total_users,
            page_size,
            max_in_flight_pages,
        })
    }

    pub const fn page_count(&self) -> u64 {
        self.total_users.div_ceil(self.page_size as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectorySyncCursor {
    pub connector_id: Uuid,
    pub feed: DirectoryFeedKind,
    pub sequence: u64,
    pub opaque_cookie: Option<String>,
    pub observed_at: DateTime<Utc>,
}

impl DirectorySyncCursor {
    pub fn validate(&self, provider: DirectoryProvider) -> Result<(), DirectoryError> {
        if self.connector_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "sync cursor connector ID is nil".into(),
            ));
        }
        if self.sequence == 0 {
            return Err(DirectoryError::InvalidBatch(
                "sync cursor sequence must be positive".into(),
            ));
        }
        if self.observed_at > Utc::now() {
            return Err(DirectoryError::InvalidBatch(
                "sync cursor cannot be dated in the future".into(),
            ));
        }
        if !self.feed.supported_by(provider) {
            return Err(DirectoryError::UnsupportedFeed {
                provider,
                feed: self.feed,
            });
        }
        if let Some(cookie) = self.opaque_cookie.as_deref() {
            bounded_ref(cookie, "opaque_cookie")?;
        }
        Ok(())
    }

    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now.signed_duration_since(self.observed_at) > DIRECTORY_REVOCATION_SLA
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUser {
    pub external_object_id: String,
    pub principal_ref: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub groups: BTreeSet<String>,
    pub active: bool,
    pub source_revision: u64,
}

impl DirectoryUser {
    pub fn validate(&self) -> Result<(), DirectoryError> {
        bounded_ref(&self.external_object_id, "external_object_id")?;
        bounded_ref(&self.principal_ref, "principal_ref")?;
        if let Some(email) = self.email.as_deref() {
            bounded_ref(email, "email")?;
        }
        for group in &self.groups {
            bounded_ref(group, "group")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectoryChange {
    Upsert(DirectoryUser),
    Disable {
        external_object_id: String,
        source_revision: u64,
    },
    Delete {
        external_object_id: String,
        source_revision: u64,
    },
}

impl DirectoryChange {
    fn external_object_id(&self) -> &str {
        match self {
            Self::Upsert(user) => &user.external_object_id,
            Self::Disable {
                external_object_id, ..
            }
            | Self::Delete {
                external_object_id, ..
            } => external_object_id,
        }
    }

    fn source_revision(&self) -> u64 {
        match self {
            Self::Upsert(user) => user.source_revision,
            Self::Disable {
                source_revision, ..
            }
            | Self::Delete {
                source_revision, ..
            } => *source_revision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectorySyncPage {
    pub connector_id: Uuid,
    pub cursor: DirectorySyncCursor,
    pub changes: Vec<DirectoryChange>,
}

impl DirectorySyncPage {
    pub fn validate(
        &self,
        provider: DirectoryProvider,
        previous: Option<&DirectorySyncCursor>,
    ) -> Result<(), DirectoryError> {
        if self.connector_id.is_nil() || self.cursor.connector_id != self.connector_id {
            return Err(DirectoryError::ConnectorMismatch);
        }
        self.cursor.validate(provider)?;
        if self.changes.len() > MAX_DIRECTORY_BATCH_SIZE {
            return Err(DirectoryError::InvalidBatch(
                "sync page exceeds the bounded batch size".into(),
            ));
        }
        if let Some(previous) = previous {
            previous.validate(provider)?;
            if previous.connector_id != self.connector_id {
                return Err(DirectoryError::ConnectorMismatch);
            }
            if self.cursor.feed != previous.feed {
                return Err(DirectoryError::FeedMismatch);
            }
            if self.cursor.sequence <= previous.sequence {
                return Err(DirectoryError::CursorRegressed);
            }
            if self.cursor.observed_at < previous.observed_at {
                return Err(DirectoryError::InvalidBatch(
                    "directory cursor observation time regressed".into(),
                ));
            }
        }
        let mut ids = BTreeSet::new();
        for change in &self.changes {
            bounded_ref(change.external_object_id(), "external_object_id")?;
            if change.source_revision() == 0 {
                return Err(DirectoryError::InvalidBatch(
                    "directory change source revision must be positive".into(),
                ));
            }
            if !ids.insert(change.external_object_id().to_owned()) {
                return Err(DirectoryError::DuplicateChange(
                    change.external_object_id().into(),
                ));
            }
            if change.source_revision() > self.cursor.sequence {
                return Err(DirectoryError::ChangeAheadOfCursor);
            }
            if let DirectoryChange::Upsert(user) = change {
                user.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleMapping {
    pub connector_id: Uuid,
    pub group_ref: String,
    pub role_ref: String,
    pub scope: DirectoryScope,
    /// Preferred role-assignment target scope. `scope` is retained as a
    /// deprecated compatibility input for legacy connector mappings.
    pub target_scope: DirectoryRoleTargetScope,
    pub requires_mfa: bool,
    pub active: bool,
    pub revision: u64,
}

impl DirectoryRoleMapping {
    pub fn validate(&self) -> Result<(), DirectoryError> {
        if self.connector_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "role mapping connector ID is nil".into(),
            ));
        }
        bounded_ref(&self.group_ref, "group_ref")?;
        bounded_ref(&self.role_ref, "role_ref")?;
        self.scope.canonical_key()?;
        if self.revision == 0 {
            return Err(DirectoryError::InvalidReference(
                "role mapping revision must be positive".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_for_connector(
        &self,
        connector_id: Uuid,
        connector_scope: &DirectoryScope,
    ) -> Result<(), DirectoryError> {
        self.validate()?;
        if self.connector_id != connector_id {
            return Err(DirectoryError::ConnectorMismatch);
        }
        if !connector_scope.permits(&self.scope) {
            return Err(DirectoryError::InvalidReference(
                "role mapping scope is outside the connector scope".into(),
            ));
        }
        let target_allowed = matches!(
            (connector_scope, self.target_scope),
            (DirectoryScope::Platform, _)
                | (
                    DirectoryScope::Holding { .. },
                    DirectoryRoleTargetScope::Holding
                )
                | (
                    DirectoryScope::Holding { .. },
                    DirectoryRoleTargetScope::Company
                )
                | (
                    DirectoryScope::Holding { .. },
                    DirectoryRoleTargetScope::Branch
                )
                | (
                    DirectoryScope::Branch { .. },
                    DirectoryRoleTargetScope::Branch
                )
        );
        if !target_allowed {
            return Err(DirectoryError::InvalidReference(
                "role mapping target scope is outside the connector authority".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryRoleAction {
    Grant,
    Revoke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRoleFact {
    pub id: Uuid,
    pub connector_id: Uuid,
    pub identity_link: String,
    pub role_ref: String,
    pub scope_key: String,
    pub action: DirectoryRoleAction,
    pub sequence: u64,
    pub previous_hash: Option<String>,
    pub fact_hash: String,
    pub occurred_at: DateTime<Utc>,
}

impl DirectoryRoleFact {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connector_id: Uuid,
        identity_link: String,
        role_ref: String,
        scope_key: String,
        action: DirectoryRoleAction,
        sequence: u64,
        previous_hash: Option<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, DirectoryError> {
        if connector_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "role fact connector ID is nil".into(),
            ));
        }
        if sequence == 0 {
            return Err(DirectoryError::InvalidBatch(
                "role fact sequence must be positive".into(),
            ));
        }
        bounded_ref(&identity_link, "identity_link")?;
        bounded_ref(&role_ref, "role_ref")?;
        bounded_ref(&scope_key, "scope_key")?;
        let mut digest = Sha256::new();
        digest.update(b"erpx-directory-role-fact-v1:");
        digest.update(connector_id.as_bytes());
        digest.update(identity_link.as_bytes());
        digest.update(role_ref.as_bytes());
        digest.update(scope_key.as_bytes());
        digest.update([action as u8]);
        digest.update(sequence.to_be_bytes());
        if let Some(previous_hash) = previous_hash.as_deref() {
            digest.update(previous_hash.as_bytes());
        }
        digest.update(
            occurred_at
                .timestamp_nanos_opt()
                .unwrap_or_default()
                .to_be_bytes(),
        );
        Ok(Self {
            id: Uuid::new_v4(),
            connector_id,
            identity_link,
            role_ref,
            scope_key,
            action,
            sequence,
            previous_hash,
            fact_hash: hex_lower(&digest.finalize()),
            occurred_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRevocationState {
    /// The immutable identity link for which this state was read. Keeping it
    /// in the authorization input prevents an active, fresh projection for a
    /// different directory object from being reused for this request.
    pub identity_link: String,
    pub active: bool,
    pub last_feed_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl DirectoryRevocationState {
    pub fn authorization_allowed(&self, now: DateTime<Utc>) -> bool {
        self.active
            && self.revoked_at.is_none()
            && self.last_feed_at <= now
            && now.signed_duration_since(self.last_feed_at) <= DIRECTORY_REVOCATION_SLA
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DirectoryError {
    #[error("directory reference is invalid: {0}")]
    InvalidReference(String),
    #[error("directory endpoint is invalid")]
    InvalidEndpoint,
    #[error("directory communication must be TLS-only")]
    TlsRequired,
    #[error("invalid directory TLS policy: {0}")]
    InvalidTlsPolicy(String),
    #[error("invalid directory policy: {0}")]
    InvalidPolicy(String),
    #[error("directory import limit exceeded: {requested}")]
    ImportLimitExceeded { requested: u64 },
    #[error("invalid directory batch: {0}")]
    InvalidBatch(String),
    #[error("directory feed {feed:?} is unsupported for {provider:?}")]
    UnsupportedFeed {
        provider: DirectoryProvider,
        feed: DirectoryFeedKind,
    },
    #[error("directory sync cursor regressed")]
    CursorRegressed,
    #[error("directory page and cursor connector IDs do not match")]
    ConnectorMismatch,
    #[error("directory page feed does not match the persisted connector feed")]
    FeedMismatch,
    #[error("directory change is ahead of its cursor")]
    ChangeAheadOfCursor,
    #[error("directory page contains duplicate change {0}")]
    DuplicateChange(String),
    #[error("directory write failed: {0}")]
    Persistence(String),
    #[error("directory outbox payload failed to encode")]
    OutboxEncoding,
    #[error("local credentials are permitted only for an explicitly marked MFA break-glass flow")]
    BreakGlassRejected,
}

#[derive(Debug, Clone)]
pub struct SqlxDirectoryStore {
    pool: PgPool,
}

impl SqlxDirectoryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Verify that a branch connector is a narrowed view of its holding
    /// connector, never an unrelated directory authority. Draft saves require
    /// structural compatibility; preflight/activation additionally require an
    /// identical immutable authority fingerprint.
    pub async fn validate_branch_parent_authority(
        &self,
        connector: &DirectoryConnector,
        require_verified_fingerprint: bool,
    ) -> Result<(), DirectoryError> {
        let DirectoryScope::Branch { holding_ref, .. } = &connector.scope else {
            return Ok(());
        };
        let parent_id = connector.parent_connector_id.ok_or_else(|| {
            DirectoryError::InvalidReference("branch connector parent is required".into())
        })?;
        if parent_id == connector.id {
            return Err(DirectoryError::InvalidReference(
                "branch connector cannot parent itself".into(),
            ));
        }
        let parent = self.connector(parent_id).await?;
        let DirectoryScope::Holding {
            holding_ref: parent_holding,
        } = &parent.scope
        else {
            return Err(DirectoryError::InvalidReference(
                "branch connector parent must be holding-scoped".into(),
            ));
        };
        let naming_context_compatible = connector
            .base_dn
            .to_ascii_lowercase()
            .ends_with(&parent.base_dn.to_ascii_lowercase());
        if parent_holding != holding_ref
            || parent.provider != connector.provider
            || !naming_context_compatible
            || parent.tls.server_name != connector.tls.server_name
            || parent.tls.ca_certificate_sha256 != connector.tls.ca_certificate_sha256
        {
            return Err(DirectoryError::InvalidReference(
                "branch connector does not match its holding directory authority".into(),
            ));
        }
        if require_verified_fingerprint
            && (parent.directory_authority_fingerprint.is_none()
                || connector.directory_authority_fingerprint.is_none()
                || parent.directory_authority_fingerprint
                    != connector.directory_authority_fingerprint)
        {
            return Err(DirectoryError::InvalidReference(
                "branch and holding directory authority fingerprints do not match".into(),
            ));
        }
        Ok(())
    }

    pub async fn bind_verified_hostname(
        &self,
        hostname: &str,
        connector_id: Uuid,
        verified_by: &str,
        verified_at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        let hostname = normalize_directory_hostname(hostname)?;
        if verified_by.trim().is_empty()
            || verified_by.len() > 255
            || verified_by.chars().any(char::is_control)
            || connector_id.is_nil()
            || verified_at > Utc::now()
        {
            return Err(DirectoryError::InvalidReference(
                "verified directory hostname binding is invalid".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_identity.directory_connector_hostnames
                (hostname, connector_id, verified_by, verified_at, revoked_at)
             VALUES ($1,$2,$3,$4,NULL)
             ON CONFLICT (hostname) DO UPDATE
             SET connector_id=EXCLUDED.connector_id, verified_by=EXCLUDED.verified_by,
                 verified_at=EXCLUDED.verified_at, revoked_at=NULL",
        )
        .bind(hostname)
        .bind(connector_id)
        .bind(verified_by)
        .bind(verified_at)
        .execute(&self.pool)
        .await
        .map_err(sqlx_error)?;
        Ok(())
    }

    pub async fn connector_for_verified_hostname(
        &self,
        hostname: &str,
    ) -> Result<Option<DirectoryConnector>, DirectoryError> {
        let hostname = normalize_directory_hostname(hostname)?;
        let connector_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT h.connector_id
             FROM erpx_identity.directory_connector_hostnames h
             JOIN erpx_identity.directory_connectors c ON c.id=h.connector_id
             WHERE h.hostname=$1 AND h.revoked_at IS NULL
               AND c.enabled AND c.lifecycle='active'",
        )
        .bind(hostname)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?;
        match connector_id {
            Some(connector_id) => self.connector(connector_id).await.map(Some),
            None => Ok(None),
        }
    }

    pub async fn record_holding_fallback(
        &self,
        branch: &DirectoryConnector,
        holding_connector_id: Uuid,
        accepted: bool,
        failure_category: Option<&str>,
        correlation_id: Uuid,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        let DirectoryScope::Branch { branch_id, .. } = branch.scope else {
            return Err(DirectoryError::InvalidReference(
                "directory fallback requires branch scope".into(),
            ));
        };
        let fingerprint = branch
            .directory_authority_fingerprint
            .as_deref()
            .ok_or_else(|| {
                DirectoryError::InvalidReference(
                    "directory fallback authority fingerprint is unavailable".into(),
                )
            })?;
        if holding_connector_id.is_nil()
            || correlation_id.is_nil()
            || accepted == failure_category.is_some()
        {
            return Err(DirectoryError::InvalidReference(
                "directory fallback audit shape is invalid".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_identity.directory_fallback_audit
                (id, branch_connector_id, holding_connector_id, branch_id,
                 authority_fingerprint, outcome, failure_category,
                 correlation_id, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(Uuid::new_v4())
        .bind(branch.id)
        .bind(holding_connector_id)
        .bind(branch_id)
        .bind(fingerprint)
        .bind(if accepted { "accepted" } else { "rejected" })
        .bind(failure_category)
        .bind(correlation_id)
        .bind(occurred_at)
        .execute(&self.pool)
        .await
        .map_err(sqlx_error)?;
        Ok(())
    }

    pub async fn list_identity_conflicts(
        &self,
        connector_id: Uuid,
        page_size: u32,
    ) -> Result<Vec<DirectoryIdentityConflictSnapshot>, DirectoryError> {
        if connector_id.is_nil() || !(1..=100).contains(&page_size) {
            return Err(DirectoryError::InvalidReference(
                "directory conflict query is invalid".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT c.id, c.connector_id, p.principal_ref, c.match_reasons,
                    c.state, approval.state AS proposal_state, c.proposed_by,
                    c.conflict_version, c.detected_at, c.proposed_at
             FROM erpx_identity.directory_identity_conflicts c
             JOIN erpx_identity.principals p ON p.id=c.principal_id
             LEFT JOIN erpx_identity.directory_identity_replacement_approvals approval
               ON approval.conflict_id=c.id
             WHERE c.connector_id=$1
             ORDER BY c.detected_at DESC, c.id DESC LIMIT $2",
        )
        .bind(connector_id)
        .bind(i64::from(page_size))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        rows.iter().map(conflict_from_row).collect()
    }

    pub async fn propose_identity_replacement(
        &self,
        conflict_id: Uuid,
        expected_version: u64,
        initiator_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryIdentityConflictSnapshot, DirectoryError> {
        validate_replacement_request(conflict_id, expected_version, initiator_ref, correlation_id)?;
        let version = as_conflict_version(expected_version)?;
        let next_version =
            as_conflict_version(expected_version.checked_add(1).ok_or_else(|| {
                DirectoryError::InvalidReference("conflict version overflow".into())
            })?)?;
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let row = sqlx::query(
            "SELECT c.principal_id, p.principal_ref,
                    EXISTS(SELECT 1 FROM erpx_identity.protected_local_principals protected
                           WHERE protected.principal_id=c.principal_id) AS protected
             FROM erpx_identity.directory_identity_conflicts c
             JOIN erpx_identity.principals p ON p.id=c.principal_id
             WHERE c.id=$1 AND c.state='unresolved'
               AND c.conflict_kind='local_identity_collision'
               AND c.conflict_version=$2 FOR UPDATE OF c, p",
        )
        .bind(conflict_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DirectoryError::InvalidPolicy("directory conflict proposal is stale".into())
        })?;
        if row.try_get::<bool, _>("protected").map_err(sqlx_error)? {
            return Err(DirectoryError::InvalidPolicy(
                "protected local principal cannot enter replacement workflow".into(),
            ));
        }
        let local_ref: String = row.try_get("principal_ref").map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_conflicts
             SET proposed_by=$2, proposed_at=$3, proposed_principal_ref=$4,
                 correlation_id=$5, conflict_version=$6 WHERE id=$1",
        )
        .bind(conflict_id)
        .bind(initiator_ref)
        .bind(at)
        .bind(local_ref)
        .bind(correlation_id)
        .bind(next_version)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_identity_replacement_approvals
                (conflict_id, initiator_ref, state, expected_conflict_version,
                 initiated_at, correlation_id)
             VALUES ($1,$2,'awaiting_approval',$3,$4,$5)
             ON CONFLICT (conflict_id) DO UPDATE SET
                 initiator_ref=EXCLUDED.initiator_ref, approver_ref=NULL,
                 state='awaiting_approval',
                 expected_conflict_version=EXCLUDED.expected_conflict_version,
                 initiated_at=EXCLUDED.initiated_at, approved_at=NULL,
                 rejected_at=NULL, correlation_id=EXCLUDED.correlation_id",
        )
        .bind(conflict_id)
        .bind(initiator_ref)
        .bind(next_version)
        .bind(at)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        self.identity_conflict(conflict_id).await
    }

    pub async fn reject_identity_replacement(
        &self,
        conflict_id: Uuid,
        expected_version: u64,
        approver_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryIdentityConflictSnapshot, DirectoryError> {
        validate_replacement_request(conflict_id, expected_version, approver_ref, correlation_id)?;
        let result = sqlx::query(
            "UPDATE erpx_identity.directory_identity_replacement_approvals approval
             SET state='rejected', approver_ref=$3, rejected_at=$4,
                 correlation_id=$5
             FROM erpx_identity.directory_identity_conflicts conflict
             WHERE approval.conflict_id=$1 AND conflict.id=approval.conflict_id
               AND conflict.conflict_version=$2
               AND approval.expected_conflict_version=$2
               AND approval.state='awaiting_approval'
               AND approval.initiator_ref<>$3",
        )
        .bind(conflict_id)
        .bind(as_conflict_version(expected_version)?)
        .bind(approver_ref)
        .bind(at)
        .bind(correlation_id)
        .execute(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(DirectoryError::InvalidPolicy(
                "directory replacement rejection is stale or not independent".into(),
            ));
        }
        self.identity_conflict(conflict_id).await
    }

    pub async fn approve_identity_replacement(
        &self,
        conflict_id: Uuid,
        expected_version: u64,
        approver_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryIdentityConflictSnapshot, DirectoryError> {
        validate_replacement_request(conflict_id, expected_version, approver_ref, correlation_id)?;
        let version = as_conflict_version(expected_version)?;
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let row = sqlx::query(
            "SELECT c.connector_id, c.external_object_id, c.principal_id,
                    approval.initiator_ref,
                    EXISTS(SELECT 1 FROM erpx_identity.protected_local_principals protected
                           WHERE protected.principal_id=c.principal_id) AS protected
             FROM erpx_identity.directory_identity_conflicts c
             JOIN erpx_identity.directory_identity_replacement_approvals approval
               ON approval.conflict_id=c.id
             WHERE c.id=$1 AND c.state='unresolved' AND c.conflict_version=$2
               AND approval.state='awaiting_approval'
               AND approval.expected_conflict_version=$2
             FOR UPDATE OF c, approval",
        )
        .bind(conflict_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DirectoryError::InvalidPolicy("directory replacement approval is stale".into())
        })?;
        let initiator_ref: String = row.try_get("initiator_ref").map_err(sqlx_error)?;
        if initiator_ref == approver_ref {
            return Err(DirectoryError::InvalidPolicy(
                "directory replacement requires a distinct approver".into(),
            ));
        }
        if row.try_get::<bool, _>("protected").map_err(sqlx_error)? {
            return Err(DirectoryError::InvalidPolicy(
                "protected local principal cannot be replaced".into(),
            ));
        }
        let connector_id: Uuid = row.try_get("connector_id").map_err(sqlx_error)?;
        let external_object_id: String = row.try_get("external_object_id").map_err(sqlx_error)?;
        let local_id: Uuid = row.try_get("principal_id").map_err(sqlx_error)?;
        let identity_link: String = sqlx::query_scalar(
            "SELECT identity_link FROM erpx_identity.directory_identity_links
             WHERE connector_id=$1 AND external_object_id=$2 AND active FOR UPDATE",
        )
        .bind(connector_id)
        .bind(&external_object_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DirectoryError::InvalidPolicy("directory immutable identity is unavailable".into())
        })?;
        let directory_principal_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:directory-principal:v1:{identity_link}").as_bytes(),
        );
        let directory_principal_ref = format!(
            "directory:{}:{}",
            &connector_id.simple().to_string()[..12],
            &directory_principal_id.simple().to_string()[..20]
        );
        sqlx::query(
            "INSERT INTO erpx_identity.principals
                (id, principal_ref, status, created_at, updated_at)
             VALUES ($1,$2,'active',$3,$3) ON CONFLICT (id) DO NOTHING",
        )
        .bind(directory_principal_id)
        .bind(&directory_principal_ref)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.principals
             SET status='suspended', updated_at=$2 WHERE id=$1",
        )
        .bind(local_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        for statement in [
            "DELETE FROM erpx_identity.password_credentials WHERE principal_id=$1",
            "DELETE FROM erpx_identity.recovery_codes WHERE principal_id=$1",
            "DELETE FROM erpx_identity.totp_secrets WHERE principal_id=$1",
            "DELETE FROM erpx_identity.totp_enrollments WHERE principal_id=$1",
            "DELETE FROM erpx_identity.recovery_sessions WHERE principal_id=$1",
            "DELETE FROM erpx_identity.webauthn_challenges WHERE principal_id=$1",
        ] {
            sqlx::query(statement)
                .bind(local_id)
                .execute(&mut *tx)
                .await
                .map_err(sqlx_error)?;
        }
        sqlx::query(
            "UPDATE erpx_identity.sessions SET revoked_at=$2
             WHERE principal_id=$1 AND revoked_at IS NULL",
        )
        .bind(local_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.passkeys SET revoked_at=$2
             WHERE principal_id=$1 AND revoked_at IS NULL",
        )
        .bind(local_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.devices SET revoked_at=$2
             WHERE principal_id=$1 AND revoked_at IS NULL",
        )
        .bind(local_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_principal_bindings
                (id, connector_id, external_object_id, principal_id, active,
                 revision, linked_by, linked_at, correlation_id)
             VALUES ($1,$2,$3,$4,TRUE,1,$5,$6,$7)
             ON CONFLICT (connector_id, external_object_id) DO UPDATE SET
                 principal_id=EXCLUDED.principal_id, active=TRUE,
                 revision=erpx_identity.directory_principal_bindings.revision+1,
                 linked_by=EXCLUDED.linked_by, linked_at=EXCLUDED.linked_at,
                 unlinked_by=NULL, unlinked_at=NULL,
                 correlation_id=EXCLUDED.correlation_id",
        )
        .bind(Uuid::new_v4())
        .bind(connector_id)
        .bind(&external_object_id)
        .bind(directory_principal_id)
        .bind(approver_ref)
        .bind(at)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_links SET principal_ref=$3
             WHERE connector_id=$1 AND external_object_id=$2",
        )
        .bind(connector_id)
        .bind(&external_object_id)
        .bind(&directory_principal_ref)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_projection
             SET principal_ref=$2, updated_at=$3 WHERE identity_link=$1",
        )
        .bind(&identity_link)
        .bind(&directory_principal_ref)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "INSERT INTO erpx_identity.principal_supersession_aliases
                (local_principal_id, directory_principal_id, conflict_id,
                 approved_by, superseded_at, correlation_id)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(local_id)
        .bind(directory_principal_id)
        .bind(conflict_id)
        .bind(approver_ref)
        .bind(at)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_replacement_approvals
             SET state='approved', approver_ref=$2, approved_at=$3
             WHERE conflict_id=$1",
        )
        .bind(conflict_id)
        .bind(approver_ref)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_conflicts
             SET state='resolved', resolved_at=$2, resolution_ref=$3,
                 replacement_principal_ref=$4, conflict_version=conflict_version+1
             WHERE id=$1",
        )
        .bind(conflict_id)
        .bind(at)
        .bind(format!("replacement-approved:{correlation_id}"))
        .bind(directory_principal_ref)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        self.identity_conflict(conflict_id).await
    }

    pub async fn identity_conflict(
        &self,
        conflict_id: Uuid,
    ) -> Result<DirectoryIdentityConflictSnapshot, DirectoryError> {
        let row = sqlx::query(
            "SELECT c.id, c.connector_id, p.principal_ref, c.match_reasons,
                    c.state, approval.state AS proposal_state, c.proposed_by,
                    c.conflict_version, c.detected_at, c.proposed_at
             FROM erpx_identity.directory_identity_conflicts c
             JOIN erpx_identity.principals p ON p.id=c.principal_id
             LEFT JOIN erpx_identity.directory_identity_replacement_approvals approval
               ON approval.conflict_id=c.id WHERE c.id=$1",
        )
        .bind(conflict_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory conflict not found".into()))?;
        conflict_from_row(&row)
    }

    pub async fn list_connectors(&self) -> Result<Vec<DirectoryConnectorSnapshot>, DirectoryError> {
        let rows = sqlx::query(
            "SELECT c.id, c.provider, c.scope_key, c.endpoint, c.base_dn,
                    c.tls_mode, c.feed_kind, c.enabled, c.lifecycle, c.max_import_users,
                    c.revision, COALESCE(s.status, 'never_synced') AS status,
                    s.last_success_at, c.last_preflight_at, c.last_heartbeat_at,
                    c.credential_profile_key, c.parent_connector_id,
                    c.directory_authority_fingerprint, c.fallback_to_holding,
                    c.holding_ref, c.company_id, c.branch_id,
                    (SELECT COUNT(*) FROM erpx_identity.directory_identity_conflicts x
                     WHERE x.connector_id=c.id AND x.state='unresolved') AS unresolved_conflicts
             FROM erpx_identity.directory_connectors c
             LEFT JOIN erpx_identity.directory_sync_checkpoints s ON s.connector_id=c.id
             ORDER BY c.scope_key, c.id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(DirectoryConnectorSnapshot {
                    id: row.try_get("id").map_err(sqlx_error)?,
                    provider: parse_provider(
                        &row.try_get::<String, _>("provider").map_err(sqlx_error)?,
                    )?,
                    scope_key: row.try_get("scope_key").map_err(sqlx_error)?,
                    endpoint: row.try_get("endpoint").map_err(sqlx_error)?,
                    base_dn: row.try_get("base_dn").map_err(sqlx_error)?,
                    tls_mode: parse_tls_mode(
                        &row.try_get::<String, _>("tls_mode").map_err(sqlx_error)?,
                    )?,
                    feed: parse_feed(&row.try_get::<String, _>("feed_kind").map_err(sqlx_error)?)?,
                    enabled: row.try_get("enabled").map_err(sqlx_error)?,
                    lifecycle: parse_connector_lifecycle(
                        &row.try_get::<String, _>("lifecycle").map_err(sqlx_error)?,
                    )?,
                    max_import_users: u64::try_from(
                        row.try_get::<i64, _>("max_import_users")
                            .map_err(sqlx_error)?,
                    )
                    .map_err(|_| {
                        DirectoryError::InvalidReference("stored import limit overflow".into())
                    })?,
                    revision: u64::try_from(row.try_get::<i64, _>("revision").map_err(sqlx_error)?)
                        .map_err(|_| {
                            DirectoryError::InvalidReference("stored revision overflow".into())
                        })?,
                    status: row.try_get("status").map_err(sqlx_error)?,
                    last_success_at: row.try_get("last_success_at").map_err(sqlx_error)?,
                    last_preflight_at: row.try_get("last_preflight_at").map_err(sqlx_error)?,
                    last_heartbeat_at: row.try_get("last_heartbeat_at").map_err(sqlx_error)?,
                    unresolved_conflicts: u32::try_from(
                        row.try_get::<i64, _>("unresolved_conflicts")
                            .map_err(sqlx_error)?,
                    )
                    .map_err(|_| {
                        DirectoryError::Persistence(
                            "stored directory conflict count is invalid".into(),
                        )
                    })?,
                    credential_profile_key: row
                        .try_get("credential_profile_key")
                        .map_err(sqlx_error)?,
                    parent_connector_id: row.try_get("parent_connector_id").map_err(sqlx_error)?,
                    directory_authority_fingerprint: row
                        .try_get("directory_authority_fingerprint")
                        .map_err(sqlx_error)?,
                    fallback_to_holding: row.try_get("fallback_to_holding").map_err(sqlx_error)?,
                    holding_ref: row.try_get("holding_ref").map_err(sqlx_error)?,
                    company_id: row.try_get("company_id").map_err(sqlx_error)?,
                    branch_id: row.try_get("branch_id").map_err(sqlx_error)?,
                })
            })
            .collect()
    }

    pub async fn connector_readiness(
        &self,
        connector_id: Uuid,
    ) -> Result<DirectoryConnectorReadinessSnapshot, DirectoryError> {
        let row = sqlx::query(
            "SELECT c.id, c.lifecycle, c.last_preflight_at,
                    c.last_import_completed_at, c.last_reconciled_at,
                    c.last_heartbeat_at,
                    (SELECT COUNT(*) FROM erpx_identity.directory_identity_conflicts x
                     WHERE x.connector_id=c.id AND x.state='unresolved') AS unresolved_conflicts
             FROM erpx_identity.directory_connectors c WHERE c.id=$1",
        )
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        Ok(DirectoryConnectorReadinessSnapshot {
            connector_id: row.try_get("id").map_err(sqlx_error)?,
            lifecycle: parse_connector_lifecycle(
                &row.try_get::<String, _>("lifecycle").map_err(sqlx_error)?,
            )?,
            last_preflight_at: row.try_get("last_preflight_at").map_err(sqlx_error)?,
            last_import_completed_at: row
                .try_get("last_import_completed_at")
                .map_err(sqlx_error)?,
            last_reconciled_at: row.try_get("last_reconciled_at").map_err(sqlx_error)?,
            last_heartbeat_at: row.try_get("last_heartbeat_at").map_err(sqlx_error)?,
            unresolved_conflicts: u32::try_from(
                row.try_get::<i64, _>("unresolved_conflicts")
                    .map_err(sqlx_error)?,
            )
            .map_err(|_| {
                DirectoryError::Persistence("stored directory conflict count is invalid".into())
            })?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_connector_runtime_settings(
        &self,
        connector_id: Uuid,
        user_filter: &str,
        page_size: u32,
        attribute_mapping: &serde_json::Value,
        ca_reference: &str,
        feed_configuration: &serde_json::Value,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        if connector_id.is_nil()
            || user_filter.trim().is_empty()
            || user_filter.len() > 2_048
            || user_filter.chars().any(char::is_control)
            || page_size == 0
            || page_size > 5_000
            || !attribute_mapping.is_object()
            || !feed_configuration.is_object()
        {
            return Err(DirectoryError::InvalidPolicy(
                "directory connector runtime settings are invalid".into(),
            ));
        }
        bounded_ref(ca_reference, "ca_reference")?;
        let page_size = i32::try_from(page_size).map_err(|_| {
            DirectoryError::InvalidBatch("directory connector page size is invalid".into())
        })?;
        let result = sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET user_filter=$2, page_size=$3, attribute_mapping=$4,
                 ca_reference=$5, feed_configuration=$6, updated_at=$7
             WHERE id=$1 AND lifecycle IN ('draft','verified','failed','suspended')",
        )
        .bind(connector_id)
        .bind(user_filter)
        .bind(page_size)
        .bind(attribute_mapping)
        .bind(ca_reference)
        .bind(feed_configuration)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(DirectoryError::InvalidPolicy(
                "active connector settings cannot be edited".into(),
            ));
        }
        Ok(())
    }

    pub async fn connector_runtime_settings(
        &self,
        connector_id: Uuid,
    ) -> Result<DirectoryConnectorRuntimeSettings, DirectoryError> {
        let row = sqlx::query(
            "SELECT user_filter, page_size, attribute_mapping,
                    ca_reference, feed_configuration
             FROM erpx_identity.directory_connectors WHERE id=$1",
        )
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        let page_size = u32::try_from(row.try_get::<i32, _>("page_size").map_err(sqlx_error)?)
            .map_err(|_| {
                DirectoryError::Persistence("stored directory page size is invalid".into())
            })?;
        Ok(DirectoryConnectorRuntimeSettings {
            user_filter: row.try_get("user_filter").map_err(sqlx_error)?,
            page_size,
            attribute_mapping: row.try_get("attribute_mapping").map_err(sqlx_error)?,
            ca_reference: row.try_get("ca_reference").map_err(sqlx_error)?,
            feed_configuration: row.try_get("feed_configuration").map_err(sqlx_error)?,
        })
    }

    pub async fn connector(&self, id: Uuid) -> Result<DirectoryConnector, DirectoryError> {
        let row = sqlx::query(
            "SELECT id, provider, scope_key, holding_ref, endpoint, base_dn,
                    tls_mode, ca_certificate_sha256, server_name, minimum_tls_version,
                    feed_kind, credential_ref, credential_profile_key,
                    parent_connector_id, directory_authority_fingerprint,
                    fallback_to_holding, enabled, max_import_users, revision
             FROM erpx_identity.directory_connectors WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        let scope_key: String = row.try_get("scope_key").map_err(sqlx_error)?;
        let scope = parse_scope(&scope_key)?;
        let connector = DirectoryConnector {
            id: row.try_get("id").map_err(sqlx_error)?,
            provider: parse_provider(&row.try_get::<String, _>("provider").map_err(sqlx_error)?)?,
            scope,
            endpoint: row.try_get("endpoint").map_err(sqlx_error)?,
            base_dn: row.try_get("base_dn").map_err(sqlx_error)?,
            tls: DirectoryTlsPolicy {
                mode: parse_tls_mode(&row.try_get::<String, _>("tls_mode").map_err(sqlx_error)?)?,
                ca_certificate_sha256: row.try_get("ca_certificate_sha256").map_err(sqlx_error)?,
                server_name: row.try_get("server_name").map_err(sqlx_error)?,
                minimum_tls_version: row.try_get("minimum_tls_version").map_err(sqlx_error)?,
            },
            feed: parse_feed(&row.try_get::<String, _>("feed_kind").map_err(sqlx_error)?)?,
            credential_ref: row.try_get("credential_ref").map_err(sqlx_error)?,
            credential_profile_key: row.try_get("credential_profile_key").map_err(sqlx_error)?,
            parent_connector_id: row.try_get("parent_connector_id").map_err(sqlx_error)?,
            directory_authority_fingerprint: row
                .try_get("directory_authority_fingerprint")
                .map_err(sqlx_error)?,
            fallback_to_holding: row.try_get("fallback_to_holding").map_err(sqlx_error)?,
            enabled: row.try_get("enabled").map_err(sqlx_error)?,
            max_import_users: u64::try_from(
                row.try_get::<i64, _>("max_import_users")
                    .map_err(sqlx_error)?,
            )
            .map_err(|_| DirectoryError::InvalidReference("stored import limit overflow".into()))?,
            revision: u64::try_from(row.try_get::<i64, _>("revision").map_err(sqlx_error)?)
                .map_err(|_| DirectoryError::InvalidReference("stored revision overflow".into()))?,
        };
        connector.validate()?;
        Ok(connector)
    }

    pub async fn list_role_mappings(
        &self,
        connector_id: Uuid,
    ) -> Result<Vec<DirectoryRoleMapping>, DirectoryError> {
        let rows = sqlx::query(
            "SELECT connector_id, group_ref, role_ref, scope_key, target_scope_key, requires_mfa,
                    active, revision
             FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1 ORDER BY group_ref, role_ref, scope_key",
        )
        .bind(connector_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(DirectoryRoleMapping {
                    connector_id: row.try_get("connector_id").map_err(sqlx_error)?,
                    group_ref: row.try_get("group_ref").map_err(sqlx_error)?,
                    role_ref: row.try_get("role_ref").map_err(sqlx_error)?,
                    scope: parse_scope(
                        &row.try_get::<String, _>("scope_key").map_err(sqlx_error)?,
                    )?,
                    target_scope: parse_role_target_scope(
                        &row.try_get::<String, _>("target_scope_key")
                            .map_err(sqlx_error)?,
                    )?,
                    requires_mfa: row.try_get("requires_mfa").map_err(sqlx_error)?,
                    active: row.try_get("active").map_err(sqlx_error)?,
                    revision: u64::try_from(row.try_get::<i64, _>("revision").map_err(sqlx_error)?)
                        .map_err(|_| {
                            DirectoryError::InvalidReference(
                                "stored role mapping revision overflow".into(),
                            )
                        })?,
                })
            })
            .collect()
    }

    /// Read a bounded page of immutable identity links from the authoritative
    /// relation.  The cursor is the canonical identity-link key, so replay or
    /// mutable directory names cannot reorder or rebind the page.  This method
    /// never returns credential material or directory transport payloads.
    pub async fn list_identity_links(
        &self,
        connector_id: Uuid,
        page_size: u32,
        after_identity_link: Option<&str>,
    ) -> Result<(Vec<DirectoryIdentityLinkSnapshot>, bool), DirectoryError> {
        if connector_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "identity-link connector id is nil".into(),
            ));
        }
        if page_size == 0 || page_size > MAX_DIRECTORY_PAGE_SIZE {
            return Err(DirectoryError::InvalidBatch(
                "identity-link page size is outside its bound".into(),
            ));
        }
        if after_identity_link.is_some_and(|value| {
            value.is_empty() || value.len() > 512 || value.chars().any(char::is_control)
        }) {
            return Err(DirectoryError::InvalidReference(
                "identity-link cursor is invalid".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT connector_id, external_object_id, identity_link, principal_ref,
                    identity_id, active, source_revision, observed_at, revoked_at,
                    immutable_authority, immutable_value
             FROM erpx_identity.directory_identity_links
             WHERE connector_id=$1
               AND identity_link IS NOT NULL
               AND ($2::TEXT IS NULL OR identity_link > $2)
             ORDER BY identity_link NULLS LAST, external_object_id
             LIMIT $3",
        )
        .bind(connector_id)
        .bind(after_identity_link)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let has_more = rows.len()
            > usize::try_from(page_size).map_err(|_| {
                DirectoryError::InvalidReference("identity-link page size overflow".into())
            })?;
        let rows = rows.into_iter().take(page_size as usize);
        rows.map(|row| {
            Ok(DirectoryIdentityLinkSnapshot {
                connector_id: row.try_get("connector_id").map_err(sqlx_error)?,
                external_object_id: row.try_get("external_object_id").map_err(sqlx_error)?,
                identity_link: row.try_get("identity_link").map_err(sqlx_error)?,
                principal_ref: row.try_get("principal_ref").map_err(sqlx_error)?,
                identity_id: row.try_get("identity_id").map_err(sqlx_error)?,
                active: row.try_get("active").map_err(sqlx_error)?,
                source_revision: u64::try_from(
                    row.try_get::<i64, _>("source_revision")
                        .map_err(sqlx_error)?,
                )
                .map_err(|_| {
                    DirectoryError::InvalidReference("stored source revision overflow".into())
                })?,
                observed_at: row.try_get("observed_at").map_err(sqlx_error)?,
                revoked_at: row.try_get("revoked_at").map_err(sqlx_error)?,
                immutable_authority: row.try_get("immutable_authority").map_err(sqlx_error)?,
                immutable_value: row.try_get("immutable_value").map_err(sqlx_error)?,
            })
        })
        .collect::<Result<Vec<_>, DirectoryError>>()
        .map(|links| (links, has_more))
    }

    /// Return the last durably committed native-feed cursor. A live adapter
    /// must use this value as its input cursor; environment state is never
    /// treated as authoritative progress.
    pub async fn latest_cursor(
        &self,
        connector_id: Uuid,
    ) -> Result<Option<DirectorySyncCursor>, DirectoryError> {
        let row = sqlx::query(
            "SELECT feed_kind, sequence, opaque_cookie, last_success_at
             FROM erpx_identity.directory_sync_checkpoints
             WHERE connector_id=$1",
        )
        .bind(connector_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?;
        row.map(|row| {
            Ok(DirectorySyncCursor {
                connector_id,
                feed: parse_feed(&row.try_get::<String, _>("feed_kind").map_err(sqlx_error)?)?,
                sequence: u64::try_from(row.try_get::<i64, _>("sequence").map_err(sqlx_error)?)
                    .map_err(|_| {
                        DirectoryError::InvalidReference("stored sequence overflow".into())
                    })?,
                opaque_cookie: row.try_get("opaque_cookie").map_err(sqlx_error)?,
                observed_at: row.try_get("last_success_at").map_err(sqlx_error)?,
            })
        })
        .transpose()
    }

    /// Persist the native-feed cursor reached by a durable full import.
    ///
    /// Full-import staging owns its bounded snapshot transactions and keeps
    /// this ordinary connector checkpoint unchanged until canonical
    /// finalization succeeds. A terminal RFC 4533 response does not carry a
    /// new cookie; callers pass the last non-terminal cookie in
    /// `opaque_cookie` so the next incremental run resumes the native stream
    /// instead of silently restarting from the beginning.
    pub async fn save_sync_checkpoint(
        &self,
        cursor: &DirectorySyncCursor,
    ) -> Result<(), DirectoryError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let connector_row = sqlx::query(
            "SELECT provider, feed_kind, enabled
             FROM erpx_identity.directory_connectors
             WHERE id=$1 FOR SHARE",
        )
        .bind(cursor.connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        let provider = parse_provider(
            &connector_row
                .try_get::<String, _>("provider")
                .map_err(sqlx_error)?,
        )?;
        cursor.validate(provider)?;
        if !connector_row
            .try_get::<bool, _>("enabled")
            .map_err(sqlx_error)?
        {
            return Err(DirectoryError::InvalidPolicy(
                "a disabled directory connector cannot save a sync checkpoint".into(),
            ));
        }
        let stored_feed = parse_feed(
            &connector_row
                .try_get::<String, _>("feed_kind")
                .map_err(sqlx_error)?,
        )?;
        if stored_feed != cursor.feed {
            return Err(DirectoryError::FeedMismatch);
        }
        let sequence = i64::try_from(cursor.sequence)
            .map_err(|_| DirectoryError::InvalidReference("cursor sequence overflow".into()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(cursor.connector_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        if let Some(existing) = sqlx::query(
            "SELECT feed_kind, sequence, opaque_cookie
             FROM erpx_identity.directory_sync_checkpoints
             WHERE connector_id=$1 FOR UPDATE",
        )
        .bind(cursor.connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        {
            let existing_feed = parse_feed(
                &existing
                    .try_get::<String, _>("feed_kind")
                    .map_err(sqlx_error)?,
            )?;
            if existing_feed != cursor.feed {
                return Err(DirectoryError::FeedMismatch);
            }
            let existing_sequence: i64 = existing.try_get("sequence").map_err(sqlx_error)?;
            if existing_sequence > sequence {
                return Err(DirectoryError::CursorRegressed);
            }
            let existing_cookie: Option<String> =
                existing.try_get("opaque_cookie").map_err(sqlx_error)?;
            if existing_sequence == sequence
                && cursor.opaque_cookie.is_some()
                && existing_cookie != cursor.opaque_cookie
            {
                return Err(DirectoryError::InvalidBatch(
                    "a checkpoint sequence is bound to different native source state".into(),
                ));
            }
        }
        let checkpoint = sqlx::query(
            "INSERT INTO erpx_identity.directory_sync_checkpoints
                (connector_id, feed_kind, sequence, opaque_cookie, last_success_at, status)
             VALUES ($1,$2,$3,$4,$5,'healthy')
             ON CONFLICT (connector_id) DO UPDATE SET
                feed_kind=EXCLUDED.feed_kind, sequence=EXCLUDED.sequence,
                opaque_cookie=COALESCE(EXCLUDED.opaque_cookie,
                    erpx_identity.directory_sync_checkpoints.opaque_cookie),
                last_success_at=EXCLUDED.last_success_at,
                status='healthy', last_error_redacted=NULL
             WHERE erpx_identity.directory_sync_checkpoints.sequence < EXCLUDED.sequence",
        )
        .bind(cursor.connector_id)
        .bind(feed_key(cursor.feed))
        .bind(sequence)
        .bind(cursor.opaque_cookie.as_deref())
        .bind(cursor.observed_at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if checkpoint.rows_affected() == 1 {
            // A successfully acknowledged native-feed cursor is positive
            // freshness evidence even when the page contains no object
            // changes. Without this connector-scoped refresh, an unchanged
            // healthy directory would deny every linked principal five
            // minutes after its last mutation.
            sqlx::query(
                "UPDATE erpx_identity.directory_identity_projection
                 SET last_feed_at=$2, updated_at=GREATEST(updated_at,$2)
                 WHERE connector_id=$1 AND source_revision <= $3",
            )
            .bind(cursor.connector_id)
            .bind(cursor.observed_at)
            .bind(sequence)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        }
        tx.commit().await.map_err(sqlx_error)
    }

    /// Persists one connector-scoped, revisioned group-to-role mapping and
    /// emits the corresponding durable outbox event in the same transaction.
    /// Mapping keys are immutable: changing a group, role, or scope creates a
    /// separate key and a later feed observation appends the compensating fact.
    pub async fn save_role_mapping(
        &self,
        mapping: &DirectoryRoleMapping,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        mapping.validate()?;
        if mapping.connector_id.is_nil() || mapping.revision == 0 {
            return Err(DirectoryError::InvalidReference(
                "directory role mapping identifier or revision is invalid".into(),
            ));
        }
        let scope_key = mapping.scope.authorization_key()?;
        let target_scope_key = mapping.target_scope.as_str();
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let connector_scope = sqlx::query(
            "SELECT scope_key
             FROM erpx_identity.directory_connectors
             WHERE id=$1
             FOR SHARE",
        )
        .bind(mapping.connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?
        .try_get::<String, _>("scope_key")
        .map_err(sqlx_error)?;
        let connector_scope = parse_scope(&connector_scope)?;
        mapping.validate_for_connector(mapping.connector_id, &connector_scope)?;
        let previous_mapping = sqlx::query(
            "SELECT active, target_scope_key
             FROM erpx_identity.directory_role_mappings
             WHERE connector_id=$1 AND group_ref=$2 AND role_ref=$3 AND scope_key=$4
             FOR UPDATE",
        )
        .bind(mapping.connector_id)
        .bind(&mapping.group_ref)
        .bind(&mapping.role_ref)
        .bind(&scope_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        let was_active = previous_mapping
            .as_ref()
            .map(|row| row.try_get::<bool, _>("active"))
            .transpose()
            .map_err(sqlx_error)?
            .unwrap_or(false);
        let result = sqlx::query(
            "INSERT INTO erpx_identity.directory_role_mappings
                (connector_id, group_ref, role_ref, scope_key, target_scope_key,
                 requires_mfa, active, revision, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (connector_id, group_ref, role_ref, scope_key) DO UPDATE SET
                target_scope_key=EXCLUDED.target_scope_key,
                requires_mfa=EXCLUDED.requires_mfa, active=EXCLUDED.active,
                revision=EXCLUDED.revision, updated_at=EXCLUDED.updated_at
             WHERE erpx_identity.directory_role_mappings.revision < EXCLUDED.revision",
        )
        .bind(mapping.connector_id)
        .bind(&mapping.group_ref)
        .bind(&mapping.role_ref)
        .bind(&scope_key)
        .bind(target_scope_key)
        .bind(mapping.requires_mfa)
        .bind(mapping.active)
        .bind(i64::try_from(mapping.revision).map_err(|_| {
            DirectoryError::InvalidReference("directory role mapping revision overflow".into())
        })?)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(sqlx_error)?;
            return Err(DirectoryError::InvalidReference(
                "directory role mapping revision is stale".into(),
            ));
        }
        append_outbox(
            &mut tx,
            mapping.connector_id,
            "directory_role_mapping",
            serde_json::json!({
                "event": "directory.role_mapping_saved",
                "connector_id": mapping.connector_id,
                "group_ref": mapping.group_ref,
                "role_ref": mapping.role_ref,
                "scope": scope_key,
                "target_scope": target_scope_key,
                "requires_mfa": mapping.requires_mfa,
                "active": mapping.active,
                "revision": mapping.revision,
            }),
            at,
        )
        .await?;
        if was_active && !mapping.active {
            let role_scope = match &mapping.scope {
                DirectoryScope::Platform => RbacConnectorScope::Platform,
                DirectoryScope::Holding { .. } | DirectoryScope::Branch { .. } => {
                    RbacConnectorScope::Holding(mapping.scope.holding_id().ok_or_else(|| {
                        DirectoryError::InvalidReference(
                            "holding role mapping has no stable holding identity".into(),
                        )
                    })?)
                }
            };
            append_mapping_deactivation_revokes(
                &mut tx,
                mapping.connector_id,
                &DirectoryGroupRef::try_new(mapping.group_ref.clone())
                    .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?,
                &DirectoryRoleRef::try_new(mapping.role_ref.clone())
                    .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?,
                &role_scope,
                mapping.revision,
                at,
            )
            .await
            .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?;
        }
        if mapping.active {
            sqlx::query(
                "UPDATE erpx_identity.directory_connectors c
                 SET lifecycle='ready', updated_at=$2
                 WHERE c.id=$1 AND c.lifecycle='importing'
                   AND c.last_import_completed_at IS NOT NULL
                   AND c.last_reconciled_at IS NOT NULL
                   AND NOT EXISTS (
                       SELECT 1 FROM erpx_identity.directory_identity_conflicts conflict
                       WHERE conflict.connector_id=c.id AND conflict.state='unresolved'
                   )",
            )
            .bind(mapping.connector_id)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        }
        tx.commit().await.map_err(sqlx_error)?;
        Ok(())
    }

    pub async fn start_import(
        &self,
        connector: &DirectoryConnector,
        total_users: u64,
        page_size: u32,
        at: DateTime<Utc>,
    ) -> Result<DirectoryImportPlan, DirectoryError> {
        let plan = DirectoryImportPlan::new(connector, total_users, page_size)?;
        validate_import_connector(connector)?;
        if total_users == 0 {
            return Err(DirectoryError::InvalidBatch(
                "directory full-import total_users must be positive".into(),
            ));
        }
        // The native full-import state machine persists pages in batches of at
        // most 5,000 records.  Reject a command that the worker cannot resume
        // with the exact requested page contract instead of creating a legacy
        // queue row that can never be claimed.
        if page_size > 5_000 {
            return Err(DirectoryError::InvalidBatch(
                "directory full-import page_size exceeds the native worker bound".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(connector.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        let lifecycle: String = sqlx::query_scalar(
            "SELECT lifecycle FROM erpx_identity.directory_connectors
             WHERE id=$1 FOR UPDATE",
        )
        .bind(connector.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        if parse_connector_lifecycle(&lifecycle)? != DirectoryConnectorLifecycle::Verified {
            return Err(DirectoryError::InvalidPolicy(
                "directory import requires a verified connector".into(),
            ));
        }
        let active_import = sqlx::query_scalar::<_, Uuid>(
            "SELECT id
             FROM erpx_identity.directory_imports
             WHERE connector_id=$1 AND state IN ('running', 'finalizing')
             ORDER BY updated_at DESC, id DESC
             LIMIT 1
             FOR UPDATE",
        )
        .bind(connector.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if active_import.is_some() {
            return Err(DirectoryError::InvalidBatch(
                "a directory full import is already active for this connector".into(),
            ));
        }
        let import_id = Uuid::new_v4();
        let cursor =
            serde_json::to_value(FeedCursor::Start).map_err(|_| DirectoryError::OutboxEncoding)?;
        let (scope_kind, scope_id) = match &connector.scope {
            DirectoryScope::Platform => ("platform", None),
            DirectoryScope::Holding { .. } => ("holding", scope_id_for(&connector.scope)),
            DirectoryScope::Branch { .. } => ("branch", scope_id_for(&connector.scope)),
        };
        sqlx::query(
            "INSERT INTO erpx_identity.directory_imports
                (id, connector_id, scope_kind, scope_id, state, page_size,
                 max_users, cursor, users_seen, pages_applied, snapshot_digest,
                 started_at, updated_at)
             VALUES ($1,$2,$3,$4,'running',$5,$6,$7,0,0,$8,$9,$9)",
        )
        .bind(import_id)
        .bind(connector.id)
        .bind(scope_kind)
        .bind(scope_id)
        .bind(i32::try_from(page_size).map_err(|_| {
            DirectoryError::InvalidBatch("directory page_size is not representable".into())
        })?)
        .bind(
            i64::try_from(total_users).map_err(|_| DirectoryError::ImportLimitExceeded {
                requested: total_users,
            })?,
        )
        .bind(cursor)
        .bind(Vec::<u8>::from([0_u8; 32]))
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET lifecycle='importing', enabled=true, updated_at=$2
             WHERE id=$1 AND lifecycle='verified'",
        )
        .bind(connector.id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        append_outbox(
            &mut tx,
            connector.id,
            "directory_import",
            serde_json::json!({
                "event": "directory.import_started",
                "import_id": import_id,
                "total_users": plan.total_users,
                "page_size": plan.page_size,
                "page_count": plan.page_count(),
                "max_in_flight_pages": plan.max_in_flight_pages,
                "state_table": "directory_imports"
            }),
            at,
        )
        .await?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(plan)
    }

    /// Read the latest (or explicitly identified) authoritative import
    /// checkpoint. This is a bounded operational read: it exposes progress
    /// counters and the cursor protocol kind, never the opaque cursor,
    /// failure text, source records, or credentials.
    pub async fn import_status(
        &self,
        connector_id: Uuid,
        import_id: Option<Uuid>,
    ) -> Result<Option<DirectoryImportStatus>, DirectoryError> {
        if connector_id.is_nil() || import_id.is_some_and(|id| id.is_nil()) {
            return Err(DirectoryError::InvalidReference(
                "directory import connector or import ID is nil".into(),
            ));
        }
        let row = if let Some(import_id) = import_id {
            sqlx::query(
                "SELECT id, connector_id, state, max_users, page_size, users_seen,
                        pages_applied, cursor, started_at,
                        finalized_at, updated_at
                   FROM erpx_identity.directory_imports
                  WHERE connector_id=$1 AND id=$2
                  LIMIT 1",
            )
            .bind(connector_id)
            .bind(import_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_error)?
        } else {
            sqlx::query(
                "SELECT id, connector_id, state, max_users, page_size, users_seen,
                        pages_applied, cursor, started_at,
                        finalized_at, updated_at
                   FROM erpx_identity.directory_imports
                  WHERE connector_id=$1
                  ORDER BY updated_at DESC, id DESC
                  LIMIT 1",
            )
            .bind(connector_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_error)?
        };
        row.map(|row| {
            let state: String = row.try_get("state").map_err(sqlx_error)?;
            if !matches!(
                state.as_str(),
                "running" | "finalizing" | "completed" | "failed" | "rolled_back"
            ) {
                return Err(DirectoryError::Persistence(
                    "directory import state is invalid".into(),
                ));
            }
            let max_users: i64 = row.try_get("max_users").map_err(sqlx_error)?;
            let max_users = non_negative_u64(max_users)?;
            let page_size: i32 = row.try_get("page_size").map_err(sqlx_error)?;
            let page_size = non_negative_u64(i64::from(page_size))?;
            if page_size == 0 {
                return Err(DirectoryError::Persistence(
                    "directory import page size is invalid".into(),
                ));
            }
            let cursor_value: serde_json::Value = row.try_get("cursor").map_err(sqlx_error)?;
            let cursor: FeedCursor = serde_json::from_value(cursor_value).map_err(|_| {
                DirectoryError::Persistence("directory import cursor is invalid".into())
            })?;
            Ok(DirectoryImportStatus {
                import_id: row.try_get("id").map_err(sqlx_error)?,
                connector_id: row.try_get("connector_id").map_err(sqlx_error)?,
                state: state.clone(),
                max_users,
                users_seen: non_negative_u64(row.try_get("users_seen").map_err(sqlx_error)?)?,
                pages_applied: non_negative_u64(row.try_get("pages_applied").map_err(sqlx_error)?)?,
                page_count: max_users.div_ceil(page_size),
                cursor_kind: cursor.kind().into(),
                started_at: row.try_get("started_at").map_err(sqlx_error)?,
                finalized_at: row.try_get("finalized_at").map_err(sqlx_error)?,
                updated_at: row.try_get("updated_at").map_err(sqlx_error)?,
                failure_category: (state == "failed").then_some("import_failed".into()),
            })
        })
        .transpose()
    }

    /// Persists connector metadata and its audit/outbox event atomically.
    /// Secret values never enter SQL; only the credential-manager reference is
    /// retained.
    pub async fn save_connector(
        &self,
        connector: &DirectoryConnector,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        connector.validate()?;
        let scope_key = connector.scope.canonical_key()?;
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let result = sqlx::query(
            "INSERT INTO erpx_identity.directory_connectors
                (id, provider, vendor, scope_key, scope_kind, scope_id, holding_ref,
                 company_id, branch_id, parent_connector_id,
                 directory_authority_fingerprint, credential_profile_key,
                 fallback_to_holding, endpoint, base_dn,
                 tls_mode, ca_certificate_sha256, server_name, minimum_tls_version,
                 feed_kind, security, page_size, max_in_flight_pages,
                 credential_ref, enabled, max_import_users, revision,
                 created_at, updated_at)
             VALUES ($1,$2,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                     $15,$16,$17,$18,$19,$20,500,4,$21,$22,$23,$24,$25,$25)
             ON CONFLICT (id) DO UPDATE SET
                provider=EXCLUDED.provider, vendor=EXCLUDED.vendor,
                scope_key=EXCLUDED.scope_key, scope_kind=EXCLUDED.scope_kind,
                scope_id=EXCLUDED.scope_id,
                holding_ref=EXCLUDED.holding_ref,
                company_id=EXCLUDED.company_id, branch_id=EXCLUDED.branch_id,
                parent_connector_id=EXCLUDED.parent_connector_id,
                directory_authority_fingerprint=EXCLUDED.directory_authority_fingerprint,
                credential_profile_key=EXCLUDED.credential_profile_key,
                fallback_to_holding=EXCLUDED.fallback_to_holding,
                endpoint=EXCLUDED.endpoint,
                base_dn=EXCLUDED.base_dn, tls_mode=EXCLUDED.tls_mode,
                ca_certificate_sha256=EXCLUDED.ca_certificate_sha256,
                server_name=EXCLUDED.server_name,
                minimum_tls_version=EXCLUDED.minimum_tls_version,
                feed_kind=EXCLUDED.feed_kind, security=EXCLUDED.security,
                page_size=EXCLUDED.page_size,
                max_in_flight_pages=EXCLUDED.max_in_flight_pages,
                credential_ref=EXCLUDED.credential_ref,
                enabled=EXCLUDED.enabled, max_import_users=EXCLUDED.max_import_users,
                revision=EXCLUDED.revision, updated_at=EXCLUDED.updated_at
             WHERE erpx_identity.directory_connectors.revision < EXCLUDED.revision",
        )
        .bind(connector.id)
        .bind(provider_key(connector.provider))
        .bind(&scope_key)
        .bind(match &connector.scope {
            DirectoryScope::Platform => "platform",
            DirectoryScope::Holding { .. } => "holding",
            DirectoryScope::Branch { .. } => "branch",
        })
        .bind(scope_id_for(&connector.scope))
        .bind(match &connector.scope {
            DirectoryScope::Holding { holding_ref }
            | DirectoryScope::Branch { holding_ref, .. } => Some(holding_ref),
            DirectoryScope::Platform => None::<&String>,
        })
        .bind(connector.scope.company_id())
        .bind(connector.scope.branch_id())
        .bind(connector.parent_connector_id)
        .bind(connector.directory_authority_fingerprint.as_deref())
        .bind(&connector.credential_profile_key)
        .bind(connector.fallback_to_holding)
        .bind(&connector.endpoint)
        .bind(&connector.base_dn)
        .bind(tls_key(connector.tls.mode))
        .bind(&connector.tls.ca_certificate_sha256)
        .bind(&connector.tls.server_name)
        .bind(&connector.tls.minimum_tls_version)
        .bind(feed_key(connector.feed))
        .bind(tls_key(connector.tls.mode))
        .bind(&connector.credential_ref)
        .bind(connector.enabled)
        .bind(i64::try_from(connector.max_import_users).map_err(|_| {
            DirectoryError::ImportLimitExceeded {
                requested: connector.max_import_users,
            }
        })?)
        .bind(
            i64::try_from(connector.revision)
                .map_err(|_| DirectoryError::InvalidReference("revision overflow".into()))?,
        )
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if result.rows_affected() == 0 {
            tx.rollback().await.map_err(sqlx_error)?;
            return Err(DirectoryError::InvalidReference(
                "directory connector revision is stale".into(),
            ));
        }
        let payload = serde_json::json!({
            "event": "directory.connector_saved",
            "connector_id": connector.id,
            "scope": scope_key,
            "revision": connector.revision,
        });
        append_outbox(&mut tx, connector.id, "directory_connector", payload, at).await?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(())
    }

    /// Applies one feed page on the write side. Identity links, cursor, role
    /// facts, projection rows, and an outbox envelope commit together, so a
    /// crash cannot publish a revocation without its authoritative state.
    pub async fn apply_sync_page(
        &self,
        page: &DirectorySyncPage,
        provider: DirectoryProvider,
        previous: Option<&DirectorySyncCursor>,
        role_mappings: &[DirectoryRoleMapping],
    ) -> Result<(), DirectoryError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        // Serialize all cursor commits for one connector. Without this lock,
        // two workers can both validate against an old caller-supplied cursor
        // and apply pages out of order before the monotonic checkpoint update.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(page.connector_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        let connector_row = sqlx::query(
            "SELECT provider, feed_kind, scope_key, enabled,
                    directory_authority_fingerprint
             FROM erpx_identity.directory_connectors
             WHERE id=$1 FOR SHARE",
        )
        .bind(page.connector_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        let stored_provider = parse_provider(
            &connector_row
                .try_get::<String, _>("provider")
                .map_err(sqlx_error)?,
        )?;
        if stored_provider != provider {
            return Err(DirectoryError::InvalidReference(
                "directory provider does not match the persisted connector".into(),
            ));
        }
        let stored_feed = parse_feed(
            &connector_row
                .try_get::<String, _>("feed_kind")
                .map_err(sqlx_error)?,
        )?;
        if stored_feed != page.cursor.feed {
            return Err(DirectoryError::FeedMismatch);
        }
        let connector_scope = parse_scope(
            &connector_row
                .try_get::<String, _>("scope_key")
                .map_err(sqlx_error)?,
        )?;
        if !connector_row
            .try_get::<bool, _>("enabled")
            .map_err(sqlx_error)?
        {
            return Err(DirectoryError::InvalidPolicy(
                "a disabled directory connector cannot apply a feed page".into(),
            ));
        }
        let directory_authority_fingerprint: Option<String> = connector_row
            .try_get("directory_authority_fingerprint")
            .map_err(sqlx_error)?;
        let immutable_authority = format!(
            "{}:{}",
            provider_key(provider),
            directory_authority_fingerprint
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_else(|| format!("connector:{}", page.connector_id))
        );
        bounded_ref(&immutable_authority, "directory_authority")?;
        let persisted_cursor = sqlx::query(
            "SELECT feed_kind, sequence, opaque_cookie, last_success_at
             FROM erpx_identity.directory_sync_checkpoints
             WHERE connector_id=$1
             FOR UPDATE",
        )
        .bind(page.connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .map(|row| {
            Ok::<DirectorySyncCursor, DirectoryError>(DirectorySyncCursor {
                connector_id: page.connector_id,
                feed: parse_feed(&row.try_get::<String, _>("feed_kind").map_err(sqlx_error)?)?,
                sequence: u64::try_from(row.try_get::<i64, _>("sequence").map_err(sqlx_error)?)
                    .map_err(|_| {
                        DirectoryError::InvalidReference(
                            "stored directory cursor sequence overflow".into(),
                        )
                    })?,
                opaque_cookie: row.try_get("opaque_cookie").map_err(sqlx_error)?,
                observed_at: row.try_get("last_success_at").map_err(sqlx_error)?,
            })
        })
        .transpose()?;
        match persisted_cursor {
            Some(persisted) => {
                if persisted.feed != page.cursor.feed {
                    return Err(DirectoryError::FeedMismatch);
                }
                if page.cursor.sequence < persisted.sequence {
                    return Err(DirectoryError::CursorRegressed);
                }
                if page.cursor.sequence == persisted.sequence {
                    if page.cursor != persisted {
                        return Err(DirectoryError::InvalidBatch(
                            "a replayed directory cursor is bound to different source state".into(),
                        ));
                    }
                } else if previous != Some(&persisted) {
                    return Err(DirectoryError::CursorRegressed);
                }
            }
            None if previous.is_some() => {
                return Err(DirectoryError::InvalidBatch(
                    "directory page supplied a predecessor without a persisted checkpoint".into(),
                ));
            }
            None => {}
        }
        page.validate(stored_provider, previous)?;
        for mapping in role_mappings {
            mapping.validate_for_connector(page.connector_id, &connector_scope)?;
        }
        for mapping in role_mappings {
            let scope_key = mapping.scope.authorization_key()?;
            let target_scope_key = mapping.target_scope.as_str();
            let existing_mapping = sqlx::query(
                "SELECT requires_mfa, active, revision, target_scope_key
                 FROM erpx_identity.directory_role_mappings
                 WHERE connector_id=$1 AND group_ref=$2 AND role_ref=$3 AND scope_key=$4
                 FOR UPDATE",
            )
            .bind(mapping.connector_id)
            .bind(&mapping.group_ref)
            .bind(&mapping.role_ref)
            .bind(&scope_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            if let Some(existing_mapping) = &existing_mapping {
                let existing_revision: i64 =
                    existing_mapping.try_get("revision").map_err(sqlx_error)?;
                let incoming_revision = i64::try_from(mapping.revision).map_err(|_| {
                    DirectoryError::InvalidReference("role mapping revision overflow".into())
                })?;
                if existing_revision > incoming_revision {
                    return Err(DirectoryError::InvalidReference(
                        "directory role mapping revision is stale".into(),
                    ));
                }
                if existing_revision == incoming_revision
                    && (existing_mapping
                        .try_get::<bool, _>("requires_mfa")
                        .map_err(sqlx_error)?
                        != mapping.requires_mfa
                        || existing_mapping
                            .try_get::<bool, _>("active")
                            .map_err(sqlx_error)?
                            != mapping.active)
                {
                    return Err(DirectoryError::InvalidBatch(
                        "same-revision directory role mappings contain different policy".into(),
                    ));
                }
            }
            let result = sqlx::query(
                "INSERT INTO erpx_identity.directory_role_mappings
                    (connector_id, group_ref, role_ref, scope_key, target_scope_key,
                     requires_mfa, active, revision, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                 ON CONFLICT (connector_id, group_ref, role_ref, scope_key) DO UPDATE SET
                    target_scope_key=EXCLUDED.target_scope_key,
                    requires_mfa=EXCLUDED.requires_mfa, active=EXCLUDED.active,
                    revision=EXCLUDED.revision, updated_at=EXCLUDED.updated_at
                 WHERE erpx_identity.directory_role_mappings.revision < EXCLUDED.revision",
            )
            .bind(mapping.connector_id)
            .bind(&mapping.group_ref)
            .bind(&mapping.role_ref)
            .bind(&scope_key)
            .bind(target_scope_key)
            .bind(mapping.requires_mfa)
            .bind(mapping.active)
            .bind(i64::try_from(mapping.revision).map_err(|_| {
                DirectoryError::InvalidReference("role mapping revision overflow".into())
            })?)
            .bind(page.cursor.observed_at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            if result.rows_affected() > 0 {
                append_outbox(
                    &mut tx,
                    mapping.connector_id,
                    "directory_role_mapping",
                    serde_json::json!({
                        "event": "directory.role_mapping_reconciled",
                        "connector_id": mapping.connector_id,
                        "group_ref": mapping.group_ref,
                        "role_ref": mapping.role_ref,
                        "scope": scope_key,
                        "target_scope": target_scope_key,
                        "requires_mfa": mapping.requires_mfa,
                        "active": mapping.active,
                        "revision": mapping.revision,
                    }),
                    page.cursor.observed_at,
                )
                .await?;
            }
        }
        for change in &page.changes {
            let (principal_ref, active, groups, source_revision) = match change {
                DirectoryChange::Upsert(user) => (
                    Some(user.principal_ref.clone()),
                    user.active,
                    user.groups.clone(),
                    user.source_revision,
                ),
                DirectoryChange::Disable {
                    source_revision, ..
                }
                | DirectoryChange::Delete {
                    source_revision, ..
                } => (None, false, BTreeSet::new(), *source_revision),
            };
            if source_revision == 0 {
                return Err(DirectoryError::InvalidBatch(
                    "directory change source revision must be positive".into(),
                ));
            }
            let source_payload_hash = hex_lower(&Sha256::digest(
                serde_json::to_vec(change).map_err(|_| DirectoryError::OutboxEncoding)?,
            ));
            let existing_identity = sqlx::query(
                "SELECT identity_id, identity_link, immutable_authority, immutable_value,
                        source_revision, source_payload_hash
                 FROM erpx_identity.directory_identity_links
                 WHERE connector_id=$1 AND external_object_id=$2
                 FOR UPDATE",
            )
            .bind(page.connector_id)
            .bind(change.external_object_id())
            .fetch_optional(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            if let Some(existing_identity) = &existing_identity {
                let stored_authority: Option<String> = existing_identity
                    .try_get("immutable_authority")
                    .map_err(sqlx_error)?;
                let stored_value: Option<String> = existing_identity
                    .try_get("immutable_value")
                    .map_err(sqlx_error)?;
                if stored_authority
                    .as_deref()
                    .zip(stored_value.as_deref())
                    .is_some_and(|(authority, value)| {
                        authority != immutable_authority || value != change.external_object_id()
                    })
                {
                    return Err(DirectoryError::InvalidBatch(
                        "directory change attempted to rebind an immutable identity link".into(),
                    ));
                }
                let stored_revision: i64 = existing_identity
                    .try_get("source_revision")
                    .map_err(sqlx_error)?;
                if stored_revision
                    == i64::try_from(source_revision).map_err(|_| {
                        DirectoryError::InvalidReference("source revision overflow".into())
                    })?
                {
                    let stored_hash: Option<String> = existing_identity
                        .try_get("source_payload_hash")
                        .map_err(sqlx_error)?;
                    if stored_hash
                        .as_deref()
                        .is_some_and(|hash| hash != source_payload_hash)
                    {
                        return Err(DirectoryError::InvalidBatch(
                            "same-revision directory observations contain different payloads"
                                .into(),
                        ));
                    }
                    // Older rows predate the digest column.  Backfill the
                    // digest for a replay and keep the replay side-effect
                    // free; future same-revision conflicts are then detected.
                    if stored_hash.is_none() {
                        sqlx::query(
                            "UPDATE erpx_identity.directory_identity_links
                             SET source_payload_hash=$3
                             WHERE connector_id=$1 AND external_object_id=$2",
                        )
                        .bind(page.connector_id)
                        .bind(change.external_object_id())
                        .bind(&source_payload_hash)
                        .execute(&mut *tx)
                        .await
                        .map_err(sqlx_error)?;
                    }
                    continue;
                }
            }
            let identity_id =
                directory_identity_id(&immutable_authority, change.external_object_id());
            if existing_identity
                .as_ref()
                .and_then(|row| row.try_get::<Option<Uuid>, _>("identity_id").ok())
                .flatten()
                .is_some_and(|stored| stored != identity_id)
            {
                return Err(DirectoryError::InvalidBatch(
                    "directory identity link is bound to a different immutable identity".into(),
                ));
            }
            let canonical_identity_link =
                sync::ImmutableIdentityLink::new(&immutable_authority, change.external_object_id())
                    .map(|link| sync::immutable_identity_database_key(&link))
                    .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?;
            let identity_link = existing_identity
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("identity_link").ok())
                .unwrap_or(canonical_identity_link);
            let identity_result = sqlx::query(
                "INSERT INTO erpx_identity.directory_identity_links
                    (connector_id, external_object_id, identity_link, principal_ref, identity_id,
                     active, source_revision, source_payload_hash, observed_at, revoked_at,
                     immutable_authority, immutable_value, linked_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,CASE WHEN $6 THEN NULL ELSE $9 END,$10,$11,$9)
                 ON CONFLICT (connector_id, external_object_id) DO UPDATE SET
                    principal_ref=EXCLUDED.principal_ref, active=EXCLUDED.active,
                    identity_id=COALESCE(erpx_identity.directory_identity_links.identity_id,
                                        EXCLUDED.identity_id),
                    linked_at=COALESCE(erpx_identity.directory_identity_links.linked_at,
                                       EXCLUDED.linked_at),
                    source_revision=EXCLUDED.source_revision,
                    source_payload_hash=EXCLUDED.source_payload_hash,
                    observed_at=EXCLUDED.observed_at, revoked_at=EXCLUDED.revoked_at
                 WHERE erpx_identity.directory_identity_links.source_revision < EXCLUDED.source_revision",
            )
            .bind(page.connector_id)
            .bind(change.external_object_id())
            .bind(&identity_link)
            .bind(&principal_ref)
            .bind(identity_id)
            .bind(active)
            .bind(i64::try_from(source_revision).map_err(|_| DirectoryError::InvalidReference("source revision overflow".into()))?)
            .bind(&source_payload_hash)
            .bind(page.cursor.observed_at)
            .bind(&immutable_authority)
            .bind(change.external_object_id())
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            // A native-feed replay or regressed per-object revision must not
            // manufacture a new role fact, projection update, or outbox event.
            // The source revision predicate is the authoritative idempotency
            // gate for this identity.
            if identity_result.rows_affected() == 0 {
                continue;
            }
            if let DirectoryChange::Upsert(user) = change {
                let local_principals = sqlx::query(
                    "SELECT id, principal_ref
                     FROM erpx_identity.principals
                     WHERE status='active'
                       AND (
                         lower(principal_ref)=lower($1)
                         OR lower(principal_ref)=lower('local:' || $1)
                         OR ($2::TEXT IS NOT NULL AND lower(principal_ref)=lower($2))
                       )
                       AND NOT EXISTS (
                         SELECT 1 FROM erpx_identity.directory_principal_bindings b
                         WHERE b.principal_id=principals.id AND b.active
                       )
                     FOR SHARE",
                )
                .bind(&user.principal_ref)
                .bind(&user.email)
                .fetch_all(&mut *tx)
                .await
                .map_err(sqlx_error)?;
                for local in local_principals {
                    let local_id: Uuid = local.try_get("id").map_err(sqlx_error)?;
                    let local_ref: String = local.try_get("principal_ref").map_err(sqlx_error)?;
                    let mut match_reasons = vec!["normalized_username".to_owned()];
                    if user
                        .email
                        .as_deref()
                        .is_some_and(|email| email.eq_ignore_ascii_case(&local_ref))
                    {
                        match_reasons.push("normalized_email".to_owned());
                    }
                    sqlx::query(
                        "INSERT INTO erpx_identity.directory_identity_conflicts
                            (id, connector_id, external_object_id, principal_id,
                             conflict_kind, state, detected_at, match_reasons,
                             conflict_version)
                         VALUES ($1,$2,$3,$4,'local_identity_collision','unresolved',$5,$6,1)
                         ON CONFLICT (connector_id, external_object_id, principal_id,
                                      conflict_kind) WHERE state='unresolved'
                         DO NOTHING",
                    )
                    .bind(Uuid::new_v4())
                    .bind(page.connector_id)
                    .bind(change.external_object_id())
                    .bind(local_id)
                    .bind(page.cursor.observed_at)
                    .bind(match_reasons)
                    .execute(&mut *tx)
                    .await
                    .map_err(sqlx_error)?;
                }
            }
            let event_type = match change {
                DirectoryChange::Upsert(user) if user.active => "directory.identity_upserted.v1",
                DirectoryChange::Upsert(_) | DirectoryChange::Disable { .. } => {
                    "directory.identity_disabled.v1"
                }
                DirectoryChange::Delete { .. } => "directory.identity_deleted.v1",
            };
            let payload = serde_json::json!({
                "event": event_type,
                "connector_id": page.connector_id,
                "external_id": change.external_object_id(),
                "identity_link": identity_link,
                "change": change,
            });
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identity_projection
                    (identity_link, connector_id, principal_ref, active, groups_json,
                     source_revision, last_feed_at, revoked_at, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,CASE WHEN $4 THEN NULL ELSE $7 END,$7)
                 ON CONFLICT (identity_link) DO UPDATE SET
                    principal_ref=EXCLUDED.principal_ref, active=EXCLUDED.active,
                    groups_json=EXCLUDED.groups_json, source_revision=EXCLUDED.source_revision,
                    last_feed_at=EXCLUDED.last_feed_at, revoked_at=EXCLUDED.revoked_at,
                    updated_at=EXCLUDED.updated_at
                 WHERE erpx_identity.directory_identity_projection.source_revision < EXCLUDED.source_revision",
            )
            .bind(&identity_link)
            .bind(page.connector_id)
            .bind(principal_ref.as_ref().cloned())
            .bind(active)
            .bind(serde_json::to_value(&groups).map_err(|_| DirectoryError::OutboxEncoding)?)
            .bind(i64::try_from(source_revision).map_err(|_| DirectoryError::InvalidReference("source revision overflow".into()))?)
            .bind(page.cursor.observed_at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            for mapping in role_mappings {
                if mapping.connector_id != page.connector_id {
                    continue;
                }
                let scope_key = mapping.scope.authorization_key()?;
                let should_grant = active && groups.contains(&mapping.group_ref) && mapping.active;
                let action = if should_grant {
                    DirectoryRoleAction::Grant
                } else {
                    DirectoryRoleAction::Revoke
                };
                let chain_key = format!(
                    "directory-role/{}/{}/{}/{}",
                    page.connector_id, identity_link, mapping.role_ref, scope_key
                );
                sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                    .bind(chain_key)
                    .execute(&mut *tx)
                    .await
                    .map_err(sqlx_error)?;
                let previous = sqlx::query(
                    "SELECT sequence, fact_hash
                     FROM erpx_identity.directory_role_facts
                     WHERE connector_id=$1 AND identity_link=$2 AND role_ref=$3
                       AND scope_key=$4
                     ORDER BY sequence DESC, occurred_at DESC LIMIT 1",
                )
                .bind(page.connector_id)
                .bind(&identity_link)
                .bind(&mapping.role_ref)
                .bind(&scope_key)
                .fetch_optional(&mut *tx)
                .await
                .map_err(sqlx_error)?;
                let (sequence, previous_hash) = match previous {
                    Some(row) => (
                        u64::try_from(row.try_get::<i64, _>("sequence").map_err(sqlx_error)?)
                            .map_err(|_| {
                                DirectoryError::InvalidReference(
                                    "stored role sequence is negative".into(),
                                )
                            })?
                            .checked_add(1)
                            .ok_or_else(|| {
                                DirectoryError::InvalidReference("role sequence overflow".into())
                            })?,
                        Some(row.try_get::<String, _>("fact_hash").map_err(sqlx_error)?),
                    ),
                    None => (1, None),
                };
                let role_scope = match &mapping.scope {
                    DirectoryScope::Platform => RbacConnectorScope::Platform,
                    DirectoryScope::Holding { .. } | DirectoryScope::Branch { .. } => {
                        RbacConnectorScope::Holding(mapping.scope.holding_id().ok_or_else(
                            || {
                                DirectoryError::InvalidReference(
                                    "holding role mapping has no stable holding identity".into(),
                                )
                            },
                        )?)
                    }
                };
                let role_action = match action {
                    DirectoryRoleAction::Grant => RbacRoleFactAction::Grant,
                    DirectoryRoleAction::Revoke => RbacRoleFactAction::Revoke,
                };
                let reason = match change {
                    DirectoryChange::Delete { .. } => DirectoryRoleFactReason::IdentityDeleted,
                    DirectoryChange::Disable { .. } => DirectoryRoleFactReason::IdentityDisabled,
                    DirectoryChange::Upsert(_) if !mapping.active => {
                        DirectoryRoleFactReason::MappingDeactivated
                    }
                    DirectoryChange::Upsert(_) if groups.contains(&mapping.group_ref) => {
                        DirectoryRoleFactReason::GroupMatched
                    }
                    DirectoryChange::Upsert(_) => DirectoryRoleFactReason::GroupRemoved,
                };
                let immutable_link = sync::ImmutableIdentityLink::new(
                    &immutable_authority,
                    change.external_object_id(),
                )
                .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?;
                let input = DirectoryRoleFactInput {
                    identity: DirectoryRoleIdentity::from_immutable_link(immutable_link),
                    connector_id: page.connector_id,
                    group_ref: DirectoryGroupRef::try_new(mapping.group_ref.clone())
                        .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?,
                    role_ref: DirectoryRoleRef::try_new(mapping.role_ref.clone())
                        .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?,
                    scope: role_scope,
                    action: role_action,
                    reason,
                    idempotency_key: format!(
                        "directory-role-feed/{}/{}/{}/{}/{}/{}",
                        page.connector_id,
                        identity_link,
                        mapping.role_ref,
                        scope_key,
                        role_action.as_str(),
                        page.cursor.sequence
                    ),
                    occurred_at: page.cursor.observed_at,
                };
                let fact = RbacRoleFact::from_input(input, sequence, previous_hash)
                    .map_err(|error| DirectoryError::InvalidReference(error.to_string()))?;
                sqlx::query(
                    "INSERT INTO erpx_identity.directory_role_facts
                        (id, connector_id, identity_link, role_ref, scope_key,
                         action, sequence, previous_hash, fact_hash, occurred_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                     ON CONFLICT (fact_hash) DO NOTHING",
                )
                .bind(fact.id())
                .bind(fact.connector_id())
                .bind(fact.identity().database_key())
                .bind(fact.role_ref().as_str())
                .bind(rbac_scope_key(fact.scope()))
                .bind(fact.action().as_str())
                .bind(i64::try_from(fact.sequence()).map_err(|_| {
                    DirectoryError::InvalidReference("role sequence overflow".into())
                })?)
                .bind(fact.previous_hash())
                .bind(fact.fact_hash())
                .bind(fact.occurred_at())
                .execute(&mut *tx)
                .await
                .map_err(sqlx_error)?;
                let event = DirectoryRoleOutboxEvent::from_fact(&fact)
                    .map_err(|_| DirectoryError::OutboxEncoding)?;
                sqlx::query(
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
                .bind(i64::try_from(event.aggregate_version).map_err(|_| {
                    DirectoryError::InvalidReference("role event version overflow".into())
                })?)
                .bind(
                    serde_json::to_value(&event.scope)
                        .map_err(|_| DirectoryError::OutboxEncoding)?,
                )
                .bind(&event.idempotency_key)
                .bind(&event.payload)
                .bind(event.occurred_at)
                .execute(&mut *tx)
                .await
                .map_err(sqlx_error)?;
            }
            let identity_id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!(
                    "directory-identity/{}/{}",
                    page.connector_id,
                    change.external_object_id()
                )
                .as_bytes(),
            );
            // Legacy direct-feed callers historically aggregate connector
            // events under the connector ID. Preserve that bounded contract
            // when no canonical full-import link exists; once a full import
            // has established the immutable identity, use its stable
            // identity aggregate for all later native-feed events.
            let aggregate_id = if existing_identity.is_some() {
                identity_id
            } else {
                page.connector_id
            };
            let event_id =
                deterministic_outbox_event_id(aggregate_id, "directory_identity", &payload)?;
            sqlx::query(
                "INSERT INTO erpx_identity.directory_outbox
                    (event_id, aggregate_id, aggregate_type, event_type,
                     event_version, connector_id, aggregate_version, scope_json,
                     idempotency_key, payload_json, occurred_at)
                 VALUES ($1,$2,'directory_identity',$3,1,$4,1,$5,$6,$7,$8)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(event_id)
            .bind(aggregate_id)
            .bind(event_type)
            .bind(page.connector_id)
            .bind(serde_json::json!({ "connector_id": page.connector_id }))
            .bind(format!(
                "directory-feed/{}/{}/{}",
                page.connector_id,
                page.cursor.sequence,
                change.external_object_id()
            ))
            .bind(&payload)
            .bind(page.cursor.observed_at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            if !active {
                let reason = if matches!(change, DirectoryChange::Delete { .. }) {
                    "directory_account_deleted"
                } else {
                    "directory_account_disabled"
                };
                sqlx::query(
                    "INSERT INTO erpx_identity.directory_revocation_sla
                        (event_id, connector_id, external_id,
                         immutable_authority, immutable_value,
                         observed_at, deadline, reason, status, applied_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$6 + INTERVAL '5 minutes',$7,
                             CASE WHEN CURRENT_TIMESTAMP <= $6 + INTERVAL '5 minutes'
                                  THEN 'applied' ELSE 'breached' END,
                             CASE WHEN CURRENT_TIMESTAMP <= $6 + INTERVAL '5 minutes'
                                  THEN CURRENT_TIMESTAMP ELSE NULL END)
                     ON CONFLICT (event_id) DO NOTHING",
                )
                .bind(event_id)
                .bind(page.connector_id)
                .bind(change.external_object_id())
                .bind(&immutable_authority)
                .bind(change.external_object_id())
                .bind(page.cursor.observed_at)
                .bind(reason)
                .execute(&mut *tx)
                .await
                .map_err(sqlx_error)?;
            }
        }
        sqlx::query(
            "INSERT INTO erpx_identity.directory_sync_checkpoints
                (connector_id, feed_kind, sequence, opaque_cookie, last_success_at, status)
             VALUES ($1,$2,$3,$4,$5,'healthy')
             ON CONFLICT (connector_id) DO UPDATE SET
                feed_kind=EXCLUDED.feed_kind, sequence=EXCLUDED.sequence,
                opaque_cookie=COALESCE(EXCLUDED.opaque_cookie,
                    erpx_identity.directory_sync_checkpoints.opaque_cookie),
                last_success_at=EXCLUDED.last_success_at,
                status='healthy', last_error_redacted=NULL
             WHERE erpx_identity.directory_sync_checkpoints.sequence < EXCLUDED.sequence",
        )
        .bind(page.connector_id)
        .bind(feed_key(page.cursor.feed))
        .bind(
            i64::try_from(page.cursor.sequence)
                .map_err(|_| DirectoryError::InvalidReference("cursor overflow".into()))?,
        )
        .bind(page.cursor.opaque_cookie.as_deref())
        .bind(page.cursor.observed_at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(())
    }

    pub async fn read_revocation_state(
        &self,
        identity_link: &str,
        now: DateTime<Utc>,
    ) -> Result<DirectoryRevocationState, DirectoryError> {
        let row = sqlx::query(
            "SELECT active, last_feed_at, revoked_at
             FROM erpx_identity.directory_identity_projection
             WHERE identity_link=$1",
        )
        .bind(identity_link)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("identity link not found".into()))?;
        let state = DirectoryRevocationState {
            identity_link: identity_link.to_owned(),
            active: row.try_get("active").map_err(sqlx_error)?,
            last_feed_at: row.try_get("last_feed_at").map_err(sqlx_error)?,
            revoked_at: row.try_get("revoked_at").map_err(sqlx_error)?,
        };
        if state.authorization_allowed(now) {
            Ok(state)
        } else {
            Ok(DirectoryRevocationState {
                active: false,
                ..state
            })
        }
    }

    /// Record a real TLS/bind/feed preflight without persisting transport
    /// evidence, credentials, or directory entries. A failed preflight is a
    /// fail-closed lifecycle transition.
    pub async fn record_preflight(
        &self,
        connector_id: Uuid,
        succeeded: bool,
        failure_category: Option<&str>,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryConnectorLifecycle, DirectoryError> {
        self.record_preflight_evidence(
            connector_id,
            succeeded,
            failure_category,
            None,
            correlation_id,
            at,
        )
        .await
    }

    pub async fn record_preflight_evidence(
        &self,
        connector_id: Uuid,
        succeeded: bool,
        failure_category: Option<&str>,
        authority_fingerprint: Option<&str>,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryConnectorLifecycle, DirectoryError> {
        if connector_id.is_nil() || correlation_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "directory preflight identifier is invalid".into(),
            ));
        }
        if succeeded == failure_category.is_some() {
            return Err(DirectoryError::InvalidPolicy(
                "directory preflight outcome is inconsistent".into(),
            ));
        }
        if let Some(category) = failure_category {
            bounded_ref(category, "failure_category")?;
        }
        if let Some(fingerprint) = authority_fingerprint
            && (fingerprint.len() != 64
                || !fingerprint.chars().all(|value| value.is_ascii_hexdigit()))
        {
            return Err(DirectoryError::InvalidPolicy(
                "observed directory authority fingerprint is invalid".into(),
            ));
        }
        if !succeeded && authority_fingerprint.is_some() {
            return Err(DirectoryError::InvalidPolicy(
                "failed directory preflight cannot persist authority evidence".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let current: String = sqlx::query_scalar(
            "SELECT lifecycle FROM erpx_identity.directory_connectors
             WHERE id=$1 FOR UPDATE",
        )
        .bind(connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        let current = parse_connector_lifecycle(&current)?;
        let next = if succeeded {
            if matches!(
                current,
                DirectoryConnectorLifecycle::Importing
                    | DirectoryConnectorLifecycle::Ready
                    | DirectoryConnectorLifecycle::Active
            ) {
                current
            } else {
                DirectoryConnectorLifecycle::Verified
            }
        } else {
            DirectoryConnectorLifecycle::Failed
        };
        for probe_kind in ["tls", "bind", "feed"] {
            sqlx::query(
                "INSERT INTO erpx_identity.directory_connector_probes
                    (id, connector_id, probe_kind, succeeded, failure_category,
                     observed_at, correlation_id)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(Uuid::new_v4())
            .bind(connector_id)
            .bind(probe_kind)
            .bind(succeeded)
            .bind(failure_category)
            .bind(at)
            .bind(correlation_id)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
        }
        sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET lifecycle=$2, enabled=CASE WHEN $3 THEN enabled ELSE false END,
                 last_preflight_at=CASE WHEN $3 THEN $4 ELSE last_preflight_at END,
                 failure_category=$5,
                 directory_authority_fingerprint=CASE
                     WHEN $3 AND $6::TEXT IS NOT NULL THEN lower($6)
                     ELSE directory_authority_fingerprint
                 END,
                 updated_at=$4
             WHERE id=$1",
        )
        .bind(connector_id)
        .bind(next.as_str())
        .bind(succeeded)
        .bind(at)
        .bind(failure_category)
        .bind(authority_fingerprint)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(next)
    }

    /// Mark a finalized full import and reconciliation ready for explicit
    /// activation. Role mappings and conflict state are checked in the same
    /// transaction so readiness cannot be inferred from an import alone.
    pub async fn mark_import_reconciled(
        &self,
        connector_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let ready: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.directory_connectors c
                 WHERE c.id=$1 AND c.lifecycle='importing'
                   AND EXISTS (
                       SELECT 1 FROM erpx_identity.directory_role_mappings m
                       WHERE m.connector_id=c.id AND m.active
                   )
                   AND NOT EXISTS (
                       SELECT 1 FROM erpx_identity.directory_identity_conflicts x
                       WHERE x.connector_id=c.id AND x.state='unresolved'
                   )
             )",
        )
        .bind(connector_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET lifecycle=CASE WHEN $3 THEN 'ready' ELSE 'importing' END,
                 enabled=CASE WHEN $3 THEN true ELSE enabled END,
                 last_import_completed_at=$2,
                 last_reconciled_at=$2, failure_category=NULL, updated_at=$2
             WHERE id=$1 AND lifecycle='importing'",
        )
        .bind(connector_id)
        .bind(at)
        .bind(ready)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(())
    }

    pub async fn transition_lifecycle(
        &self,
        connector_id: Uuid,
        target: DirectoryConnectorLifecycle,
        actor_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryConnectorLifecycle, DirectoryError> {
        bounded_ref(actor_ref, "actor_ref")?;
        if connector_id.is_nil() || correlation_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "directory lifecycle identifier is invalid".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let row = sqlx::query(
            "SELECT provider, scope_kind, lifecycle, last_preflight_at,
                    last_import_completed_at, last_reconciled_at,
                    directory_authority_fingerprint
             FROM erpx_identity.directory_connectors WHERE id=$1 FOR UPDATE",
        )
        .bind(connector_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("directory connector not found".into()))?;
        let provider = parse_provider(&row.try_get::<String, _>("provider").map_err(sqlx_error)?)?;
        let current =
            parse_connector_lifecycle(&row.try_get::<String, _>("lifecycle").map_err(sqlx_error)?)?;
        if !current.can_transition_to(target) {
            append_rejected_lifecycle_audit(
                &mut tx,
                connector_id,
                target,
                actor_ref,
                correlation_id,
                "invalid_transition",
                at,
            )
            .await?;
            tx.commit().await.map_err(sqlx_error)?;
            return Err(DirectoryError::InvalidPolicy(
                "directory lifecycle transition is invalid".into(),
            ));
        }
        if target == DirectoryConnectorLifecycle::Active {
            let last_preflight: Option<DateTime<Utc>> =
                row.try_get("last_preflight_at").map_err(sqlx_error)?;
            let imported: Option<DateTime<Utc>> = row
                .try_get("last_import_completed_at")
                .map_err(sqlx_error)?;
            let reconciled: Option<DateTime<Utc>> =
                row.try_get("last_reconciled_at").map_err(sqlx_error)?;
            let scope_kind: String = row.try_get("scope_kind").map_err(sqlx_error)?;
            let authority_fingerprint: Option<String> = row
                .try_get("directory_authority_fingerprint")
                .map_err(sqlx_error)?;
            let healthy = last_preflight.is_some_and(|value| {
                value <= at && at.signed_duration_since(value) <= DIRECTORY_PREFLIGHT_MAX_AGE
            }) && imported.is_some()
                && reconciled.is_some()
                && (scope_kind == "platform" || authority_fingerprint.is_some())
                && sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                         SELECT 1 FROM erpx_identity.directory_role_mappings
                         WHERE connector_id=$1 AND active
                     ) AND NOT EXISTS(
                         SELECT 1 FROM erpx_identity.directory_identity_conflicts
                         WHERE connector_id=$1 AND state='unresolved'
                     )",
                )
                .bind(connector_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(sqlx_error)?;
            if !healthy {
                append_rejected_lifecycle_audit(
                    &mut tx,
                    connector_id,
                    target,
                    actor_ref,
                    correlation_id,
                    "activation_gates",
                    at,
                )
                .await?;
                tx.commit().await.map_err(sqlx_error)?;
                return Err(DirectoryError::InvalidPolicy(
                    "directory connector has not satisfied activation gates".into(),
                ));
            }
            if provider == DirectoryProvider::UniventionCorporateServer
                && current != DirectoryConnectorLifecycle::Ready
            {
                append_rejected_lifecycle_audit(
                    &mut tx,
                    connector_id,
                    target,
                    actor_ref,
                    correlation_id,
                    "ucs_unconfigured",
                    at,
                )
                .await?;
                tx.commit().await.map_err(sqlx_error)?;
                return Err(DirectoryError::InvalidPolicy(
                    "UCS is supported but unconfigured until a genuine provisioning service is verified"
                        .into(),
                ));
            }
        }
        sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET lifecycle=$2, enabled=$3,
                 failure_category=CASE WHEN $2='failed' THEN failure_category ELSE NULL END,
                 updated_at=$4
             WHERE id=$1",
        )
        .bind(connector_id)
        .bind(target.as_str())
        .bind(target == DirectoryConnectorLifecycle::Active)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_administration_audit
                (id, connector_id, action, actor_ref, correlation_id, outcome, occurred_at)
             VALUES ($1,$2,$3,$4,$5,'accepted',$6)",
        )
        .bind(Uuid::new_v4())
        .bind(connector_id)
        .bind(format!("lifecycle:{}", target.as_str()))
        .bind(actor_ref)
        .bind(correlation_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(target)
    }

    /// Explicitly join one immutable directory identity to an existing
    /// canonical ERPX principal. No username/email matching or principal
    /// creation occurs in this operation.
    pub async fn link_principal(
        &self,
        connector_id: Uuid,
        external_object_id: &str,
        principal_ref: &str,
        actor_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<DirectoryPrincipalBindingSnapshot, DirectoryError> {
        for (value, field) in [
            (external_object_id, "external_object_id"),
            (principal_ref, "principal_ref"),
            (actor_ref, "actor_ref"),
        ] {
            bounded_ref(value, field)?;
        }
        if connector_id.is_nil() || correlation_id.is_nil() {
            return Err(DirectoryError::InvalidReference(
                "directory principal binding identifier is invalid".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let principal_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM erpx_identity.principals
             WHERE principal_ref=$1 AND status='active' FOR UPDATE",
        )
        .bind(principal_ref)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| DirectoryError::InvalidReference("canonical principal not found".into()))?;
        if principal_ref == "local:platform-root" {
            sqlx::query(
                "INSERT INTO erpx_identity.directory_administration_audit
                    (id, connector_id, principal_id, action, actor_ref, correlation_id,
                     outcome, failure_category, occurred_at)
                 VALUES ($1,$2,$3,'principal_link',$4,$5,'rejected',
                         'platform_root_unjoinable',$6)",
            )
            .bind(Uuid::new_v4())
            .bind(connector_id)
            .bind(principal_id)
            .bind(actor_ref)
            .bind(correlation_id)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            tx.commit().await.map_err(sqlx_error)?;
            return Err(DirectoryError::InvalidPolicy(
                "Platform Root cannot be joined to a directory".into(),
            ));
        }
        let link_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.directory_identity_links
                 WHERE connector_id=$1 AND external_object_id=$2
             )",
        )
        .bind(connector_id)
        .bind(external_object_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if !link_exists {
            return Err(DirectoryError::InvalidReference(
                "imported immutable directory identity not found".into(),
            ));
        }
        let existing = sqlx::query(
            "SELECT id, principal_id, revision FROM erpx_identity.directory_principal_bindings
             WHERE connector_id=$1 AND external_object_id=$2 FOR UPDATE",
        )
        .bind(connector_id)
        .bind(external_object_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if existing.as_ref().is_some_and(|row| {
            row.try_get::<Uuid, _>("principal_id")
                .is_ok_and(|value| value != principal_id)
        }) {
            sqlx::query(
                "INSERT INTO erpx_identity.directory_identity_conflicts
                    (id, connector_id, external_object_id, principal_id,
                     conflict_kind, state, detected_at)
                 VALUES ($1,$2,$3,$4,'canonical_principal','unresolved',$5)",
            )
            .bind(Uuid::new_v4())
            .bind(connector_id)
            .bind(external_object_id)
            .bind(principal_id)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_error)?;
            tx.commit().await.map_err(sqlx_error)?;
            return Err(DirectoryError::InvalidPolicy(
                "directory identity is already bound to another canonical principal".into(),
            ));
        }
        let binding_id = existing
            .as_ref()
            .and_then(|row| row.try_get("id").ok())
            .unwrap_or_else(Uuid::new_v4);
        let revision = existing
            .as_ref()
            .and_then(|row| row.try_get::<i64, _>("revision").ok())
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| DirectoryError::InvalidReference("binding revision overflow".into()))?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_principal_bindings
                (id, connector_id, external_object_id, principal_id, active,
                 revision, linked_by, linked_at, correlation_id)
             VALUES ($1,$2,$3,$4,true,$5,$6,$7,$8)
             ON CONFLICT (connector_id, external_object_id) DO UPDATE SET
                 principal_id=EXCLUDED.principal_id, active=true,
                 revision=EXCLUDED.revision, linked_by=EXCLUDED.linked_by,
                 linked_at=EXCLUDED.linked_at, unlinked_by=NULL, unlinked_at=NULL,
                 correlation_id=EXCLUDED.correlation_id",
        )
        .bind(binding_id)
        .bind(connector_id)
        .bind(external_object_id)
        .bind(principal_id)
        .bind(revision)
        .bind(actor_ref)
        .bind(at)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_links
             SET principal_ref=$3 WHERE connector_id=$1 AND external_object_id=$2",
        )
        .bind(connector_id)
        .bind(external_object_id)
        .bind(principal_ref)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "UPDATE erpx_identity.directory_identity_projection p
             SET principal_ref=$3, updated_at=$4
             FROM erpx_identity.directory_identity_links l
             WHERE l.connector_id=$1 AND l.external_object_id=$2
               AND p.identity_link=l.identity_link",
        )
        .bind(connector_id)
        .bind(external_object_id)
        .bind(principal_ref)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_administration_audit
                (id, connector_id, principal_id, action, actor_ref, correlation_id,
                 outcome, occurred_at)
             VALUES ($1,$2,$3,'principal_link',$4,$5,'accepted',$6)",
        )
        .bind(Uuid::new_v4())
        .bind(connector_id)
        .bind(principal_id)
        .bind(actor_ref)
        .bind(correlation_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(DirectoryPrincipalBindingSnapshot {
            binding_id,
            connector_id,
            external_object_id: external_object_id.to_owned(),
            principal_id,
            principal_ref: principal_ref.to_owned(),
            active: true,
            revision: u64::try_from(revision).map_err(|_| {
                DirectoryError::InvalidReference("binding revision overflow".into())
            })?,
            linked_at: at,
            unlinked_at: None,
        })
    }

    pub async fn unlink_principal(
        &self,
        connector_id: Uuid,
        external_object_id: &str,
        actor_ref: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        bounded_ref(external_object_id, "external_object_id")?;
        bounded_ref(actor_ref, "actor_ref")?;
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let principal_id: Uuid = sqlx::query_scalar(
            "UPDATE erpx_identity.directory_principal_bindings
             SET active=false, revision=revision+1, unlinked_by=$3,
                 unlinked_at=$4, correlation_id=$5
             WHERE connector_id=$1 AND external_object_id=$2 AND active
             RETURNING principal_id",
        )
        .bind(connector_id)
        .bind(external_object_id)
        .bind(actor_ref)
        .bind(at)
        .bind(correlation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?
        .ok_or_else(|| {
            DirectoryError::InvalidReference("active principal binding not found".into())
        })?;
        sqlx::query(
            "INSERT INTO erpx_identity.directory_administration_audit
                (id, connector_id, principal_id, action, actor_ref, correlation_id,
                 outcome, occurred_at)
             VALUES ($1,$2,$3,'principal_unlink',$4,$5,'accepted',$6)",
        )
        .bind(Uuid::new_v4())
        .bind(connector_id)
        .bind(principal_id)
        .bind(actor_ref)
        .bind(correlation_id)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)
    }

    /// Claim one due active connector. The row lock and expiring lease prevent
    /// concurrent cursor execution across supervisor processes.
    pub async fn claim_supervisor_connector(
        &self,
        owner_ref: Uuid,
        at: DateTime<Utc>,
        lease_for: Duration,
    ) -> Result<Option<DirectorySupervisorClaim>, DirectoryError> {
        if owner_ref.is_nil() || lease_for <= Duration::zero() || lease_for > Duration::minutes(5) {
            return Err(DirectoryError::InvalidPolicy(
                "directory supervisor lease is invalid".into(),
            ));
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let connector_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT c.id FROM erpx_identity.directory_connectors c
             LEFT JOIN erpx_identity.directory_supervisor_leases l ON l.connector_id=c.id
             WHERE (c.lifecycle='importing' OR (c.lifecycle='active' AND c.enabled))
               AND (l.connector_id IS NULL OR (l.lease_expires_at <= $1 AND l.next_attempt_at <= $1))
             ORDER BY COALESCE(l.next_attempt_at, c.updated_at), c.id
             FOR UPDATE OF c SKIP LOCKED LIMIT 1",
        )
        .bind(at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        let Some(connector_id) = connector_id else {
            tx.commit().await.map_err(sqlx_error)?;
            return Ok(None);
        };
        let lease_expires_at = at + lease_for;
        let failures: i32 = sqlx::query_scalar(
            "INSERT INTO erpx_identity.directory_supervisor_leases
                (connector_id, owner_ref, lease_expires_at, heartbeat_at,
                 consecutive_failures, next_attempt_at)
             VALUES ($1,$2,$3,$4,0,$4)
             ON CONFLICT (connector_id) DO UPDATE SET owner_ref=EXCLUDED.owner_ref,
                 lease_expires_at=EXCLUDED.lease_expires_at,
                 heartbeat_at=EXCLUDED.heartbeat_at
             RETURNING consecutive_failures",
        )
        .bind(connector_id)
        .bind(owner_ref)
        .bind(lease_expires_at)
        .bind(at)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)?;
        Ok(Some(DirectorySupervisorClaim {
            connector_id,
            owner_ref,
            lease_expires_at,
            consecutive_failures: u32::try_from(failures).map_err(|_| {
                DirectoryError::InvalidReference(
                    "stored supervisor failure count is invalid".into(),
                )
            })?,
        }))
    }

    pub async fn complete_supervisor_connector(
        &self,
        claim: &DirectorySupervisorClaim,
        succeeded: bool,
        failure_category: Option<&str>,
        next_attempt_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), DirectoryError> {
        if claim.connector_id.is_nil()
            || claim.owner_ref.is_nil()
            || next_attempt_at <= at
            || next_attempt_at > at + Duration::minutes(5)
            || succeeded == failure_category.is_some()
        {
            return Err(DirectoryError::InvalidPolicy(
                "directory supervisor completion is invalid".into(),
            ));
        }
        if let Some(category) = failure_category {
            bounded_ref(category, "failure_category")?;
        }
        let mut tx = self.pool.begin().await.map_err(sqlx_error)?;
        let result = sqlx::query(
            "UPDATE erpx_identity.directory_supervisor_leases
             SET lease_expires_at=$3 + INTERVAL '1 millisecond', heartbeat_at=$3,
                 consecutive_failures=CASE WHEN $4 THEN 0 ELSE consecutive_failures+1 END,
                 next_attempt_at=$5, last_error_category=$6
             WHERE connector_id=$1 AND owner_ref=$2",
        )
        .bind(claim.connector_id)
        .bind(claim.owner_ref)
        .bind(at)
        .bind(succeeded)
        .bind(next_attempt_at)
        .bind(failure_category)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        if result.rows_affected() != 1 {
            return Err(DirectoryError::InvalidPolicy(
                "directory supervisor lease ownership was lost".into(),
            ));
        }
        sqlx::query(
            "UPDATE erpx_identity.directory_connectors
             SET last_heartbeat_at=CASE WHEN $2 THEN $3 ELSE last_heartbeat_at END,
                 lifecycle=CASE
                     WHEN NOT $2 AND lifecycle='importing' THEN 'failed'
                     ELSE lifecycle
                 END,
                 enabled=CASE
                     WHEN NOT $2 AND lifecycle='importing' THEN false
                     ELSE enabled
                 END,
                 failure_category=$4, updated_at=$3
             WHERE id=$1",
        )
        .bind(claim.connector_id)
        .bind(succeeded)
        .bind(at)
        .bind(failure_category)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_error)?;
        tx.commit().await.map_err(sqlx_error)
    }
}

#[allow(clippy::too_many_arguments)]
async fn append_rejected_lifecycle_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    connector_id: Uuid,
    target: DirectoryConnectorLifecycle,
    actor_ref: &str,
    correlation_id: Uuid,
    failure_category: &str,
    at: DateTime<Utc>,
) -> Result<(), DirectoryError> {
    sqlx::query(
        "INSERT INTO erpx_identity.directory_administration_audit
            (id, connector_id, action, actor_ref, correlation_id,
             outcome, failure_category, occurred_at)
         VALUES ($1,$2,$3,$4,$5,'rejected',$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(connector_id)
    .bind(format!("lifecycle:{}", target.as_str()))
    .bind(actor_ref)
    .bind(correlation_id)
    .bind(failure_category)
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_error)?;
    Ok(())
}

async fn append_outbox(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    aggregate_id: Uuid,
    aggregate_type: &str,
    payload: serde_json::Value,
    at: DateTime<Utc>,
) -> Result<(), DirectoryError> {
    let event_id = deterministic_outbox_event_id(aggregate_id, aggregate_type, &payload)?;
    let event_type = payload
        .get("event")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(aggregate_type)
        .to_owned();
    sqlx::query(
        "INSERT INTO erpx_identity.directory_outbox
            (event_id, aggregate_id, aggregate_type, event_type,
             event_version, connector_id, aggregate_version, scope_json,
             idempotency_key, payload_json, occurred_at)
         VALUES ($1,$2,$3,$4,1,$2,1,'{}'::jsonb,$5,$6,$7)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(aggregate_id)
    .bind(aggregate_type)
    .bind(&event_type)
    .bind(event_id.to_string())
    .bind(payload)
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(sqlx_error)?;
    Ok(())
}

fn deterministic_outbox_event_id(
    aggregate_id: Uuid,
    aggregate_type: &str,
    payload: &serde_json::Value,
) -> Result<Uuid, DirectoryError> {
    let encoded = serde_json::to_vec(payload).map_err(|_| DirectoryError::OutboxEncoding)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx-directory-outbox:v1\0");
    digest.update(aggregate_type.as_bytes());
    digest.update([0]);
    digest.update(aggregate_id.as_bytes());
    digest.update([0]);
    digest.update(encoded);
    Ok(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        hex_lower(&digest.finalize()).as_bytes(),
    ))
}

fn bounded_ref<'a>(value: &'a str, field: &str) -> Result<&'a str, DirectoryError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(DirectoryError::InvalidReference(format!(
            "{field} is empty, too long, or contains control characters"
        )));
    }
    Ok(value)
}

fn validate_endpoint_server_name(
    endpoint: &str,
    mode: DirectoryTlsMode,
    server_name: &str,
) -> Result<(), DirectoryError> {
    let scheme = match mode {
        DirectoryTlsMode::Ldaps => "ldaps://",
        DirectoryTlsMode::StartTls => "ldap://",
        DirectoryTlsMode::Https => "https://",
    };
    let authority_and_path = endpoint
        .strip_prefix(scheme)
        .ok_or(DirectoryError::InvalidEndpoint)?;
    if authority_and_path.is_empty()
        || authority_and_path.chars().any(char::is_control)
        || authority_and_path.matches(':').count() > 1
    {
        return Err(DirectoryError::InvalidEndpoint);
    }
    let (authority, path) = match mode {
        // UCS exposes the Provisioning API behind the HTTPS path
        // `/univention/provisioning/`; LDAP endpoints are host-only because
        // their adapters do not interpret URL paths.
        DirectoryTlsMode::Https => authority_and_path
            .split_once('/')
            .map_or((authority_and_path, ""), |(authority, path)| {
                (authority, path)
            }),
        DirectoryTlsMode::Ldaps | DirectoryTlsMode::StartTls => {
            if authority_and_path.contains(['/', '?', '#', '@']) {
                return Err(DirectoryError::InvalidEndpoint);
            }
            (authority_and_path, "")
        }
    };
    if authority.is_empty()
        || authority.contains(['?', '#', '@'])
        || (mode == DirectoryTlsMode::Https
            && (path.len() > 512 || path.contains(['?', '#', '@', '\\'])))
    {
        return Err(DirectoryError::InvalidEndpoint);
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    if let Some(port) = port
        && port.parse::<u16>().ok().filter(|port| *port != 0).is_none()
    {
        return Err(DirectoryError::InvalidEndpoint);
    }
    if !host.eq_ignore_ascii_case(server_name) {
        return Err(DirectoryError::InvalidEndpoint);
    }
    Ok(())
}

fn validate_import_connector(connector: &DirectoryConnector) -> Result<(), DirectoryError> {
    connector.validate()
}

fn hex_lower(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn rbac_scope_key(scope: &RbacConnectorScope) -> String {
    match scope {
        RbacConnectorScope::Platform => "platform".to_owned(),
        RbacConnectorScope::Holding(holding_id) => format!("holding:{holding_id}"),
        RbacConnectorScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
    }
}

fn provider_key(provider: DirectoryProvider) -> &'static str {
    match provider {
        DirectoryProvider::GenericLdap => "generic_ldap",
        DirectoryProvider::ActiveDirectoryDomainServices => "active_directory_domain_services",
        DirectoryProvider::UniventionCorporateServer => "univention_corporate_server",
    }
}

fn normalize_directory_hostname(value: &str) -> Result<String, DirectoryError> {
    let hostname = value.trim().trim_end_matches('.').to_ascii_lowercase();
    let labels = hostname.split('.').collect::<Vec<_>>();
    if hostname.len() > 253
        || labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return Err(DirectoryError::InvalidReference(
            "directory hostname is invalid".into(),
        ));
    }
    Ok(hostname)
}

fn validate_replacement_request(
    conflict_id: Uuid,
    expected_version: u64,
    actor_ref: &str,
    correlation_id: Uuid,
) -> Result<(), DirectoryError> {
    if conflict_id.is_nil()
        || expected_version == 0
        || correlation_id.is_nil()
        || actor_ref.trim().is_empty()
        || actor_ref.len() > 255
        || actor_ref.chars().any(char::is_control)
    {
        return Err(DirectoryError::InvalidReference(
            "directory replacement request is invalid".into(),
        ));
    }
    Ok(())
}

fn as_conflict_version(value: u64) -> Result<i64, DirectoryError> {
    i64::try_from(value)
        .map_err(|_| DirectoryError::InvalidReference("conflict version overflow".into()))
}

fn conflict_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<DirectoryIdentityConflictSnapshot, DirectoryError> {
    Ok(DirectoryIdentityConflictSnapshot {
        conflict_id: row.try_get("id").map_err(sqlx_error)?,
        connector_id: row.try_get("connector_id").map_err(sqlx_error)?,
        local_principal_ref: row.try_get("principal_ref").map_err(sqlx_error)?,
        match_reasons: row.try_get("match_reasons").map_err(sqlx_error)?,
        state: row.try_get("state").map_err(sqlx_error)?,
        proposal_state: row.try_get("proposal_state").map_err(sqlx_error)?,
        proposed_by: row.try_get("proposed_by").map_err(sqlx_error)?,
        conflict_version: u64::try_from(
            row.try_get::<i64, _>("conflict_version")
                .map_err(sqlx_error)?,
        )
        .map_err(|_| DirectoryError::Persistence("stored conflict version is invalid".into()))?,
        detected_at: row.try_get("detected_at").map_err(sqlx_error)?,
        proposed_at: row.try_get("proposed_at").map_err(sqlx_error)?,
    })
}

fn parse_provider(value: &str) -> Result<DirectoryProvider, DirectoryError> {
    match value {
        "generic_ldap" => Ok(DirectoryProvider::GenericLdap),
        "active_directory_domain_services" => Ok(DirectoryProvider::ActiveDirectoryDomainServices),
        "univention_corporate_server" => Ok(DirectoryProvider::UniventionCorporateServer),
        _ => Err(DirectoryError::InvalidReference(
            "stored directory provider is invalid".into(),
        )),
    }
}

fn parse_connector_lifecycle(value: &str) -> Result<DirectoryConnectorLifecycle, DirectoryError> {
    match value {
        "draft" => Ok(DirectoryConnectorLifecycle::Draft),
        "verified" => Ok(DirectoryConnectorLifecycle::Verified),
        "importing" => Ok(DirectoryConnectorLifecycle::Importing),
        "ready" => Ok(DirectoryConnectorLifecycle::Ready),
        "active" => Ok(DirectoryConnectorLifecycle::Active),
        "suspended" => Ok(DirectoryConnectorLifecycle::Suspended),
        "failed" => Ok(DirectoryConnectorLifecycle::Failed),
        _ => Err(DirectoryError::InvalidReference(
            "stored directory connector lifecycle is invalid".into(),
        )),
    }
}

fn parse_tls_mode(value: &str) -> Result<DirectoryTlsMode, DirectoryError> {
    match value {
        "ldaps" => Ok(DirectoryTlsMode::Ldaps),
        "start_tls" => Ok(DirectoryTlsMode::StartTls),
        "https" => Ok(DirectoryTlsMode::Https),
        _ => Err(DirectoryError::InvalidReference(
            "stored TLS mode is invalid".into(),
        )),
    }
}

fn parse_feed(value: &str) -> Result<DirectoryFeedKind, DirectoryError> {
    match value {
        "polling" => Ok(DirectoryFeedKind::Polling),
        "ad_dirsync" => Ok(DirectoryFeedKind::AdDirSync),
        "rfc4533" => Ok(DirectoryFeedKind::Rfc4533),
        "ucs_provisioning_service" => Ok(DirectoryFeedKind::UcsProvisioningService),
        _ => Err(DirectoryError::InvalidReference(
            "stored directory feed is invalid".into(),
        )),
    }
}

fn parse_scope(value: &str) -> Result<DirectoryScope, DirectoryError> {
    if value == "platform" {
        return Ok(DirectoryScope::Platform);
    }
    if let Some(holding_ref) = value.strip_prefix("holding:") {
        return Ok(DirectoryScope::Holding {
            holding_ref: bounded_ref(holding_ref, "holding_ref")?.to_owned(),
        });
    }
    if let Some(branch_scope) = value.strip_prefix("branch:") {
        let mut parts = branch_scope.rsplitn(3, ':');
        let branch_id = parts
            .next()
            .and_then(|part| Uuid::parse_str(part).ok())
            .ok_or_else(|| {
                DirectoryError::InvalidReference("stored branch directory scope is invalid".into())
            })?;
        let company_id = parts
            .next()
            .and_then(|part| Uuid::parse_str(part).ok())
            .ok_or_else(|| {
                DirectoryError::InvalidReference("stored branch directory scope is invalid".into())
            })?;
        let holding_ref = parts.next().ok_or_else(|| {
            DirectoryError::InvalidReference("stored branch directory scope is invalid".into())
        })?;
        return Ok(DirectoryScope::Branch {
            holding_ref: bounded_ref(holding_ref, "holding_ref")?.to_owned(),
            company_id,
            branch_id,
        });
    }
    Err(DirectoryError::InvalidReference(
        "stored directory scope is invalid".into(),
    ))
}

fn parse_role_target_scope(value: &str) -> Result<DirectoryRoleTargetScope, DirectoryError> {
    match value {
        "platform" => Ok(DirectoryRoleTargetScope::Platform),
        "holding" => Ok(DirectoryRoleTargetScope::Holding),
        "company" => Ok(DirectoryRoleTargetScope::Company),
        "branch" => Ok(DirectoryRoleTargetScope::Branch),
        _ => Err(DirectoryError::InvalidReference(
            "stored directory role target scope is invalid".into(),
        )),
    }
}

fn scope_id_for(scope: &DirectoryScope) -> Option<Uuid> {
    match scope {
        DirectoryScope::Platform => None,
        DirectoryScope::Holding { holding_ref } => Uuid::parse_str(holding_ref)
            .ok()
            .or_else(|| Some(Uuid::new_v5(&Uuid::NAMESPACE_URL, holding_ref.as_bytes()))),
        DirectoryScope::Branch { branch_id, .. } => Some(*branch_id),
    }
}

fn tls_key(mode: DirectoryTlsMode) -> &'static str {
    match mode {
        DirectoryTlsMode::Ldaps => "ldaps",
        DirectoryTlsMode::StartTls => "start_tls",
        DirectoryTlsMode::Https => "https",
    }
}
fn feed_key(feed: DirectoryFeedKind) -> &'static str {
    match feed {
        DirectoryFeedKind::Polling => "polling",
        DirectoryFeedKind::AdDirSync => "ad_dirsync",
        DirectoryFeedKind::Rfc4533 => "rfc4533",
        DirectoryFeedKind::UcsProvisioningService => "ucs_provisioning_service",
    }
}

fn sqlx_error(error: sqlx::Error) -> DirectoryError {
    DirectoryError::Persistence(error.to_string())
}

fn non_negative_u64(value: i64) -> Result<u64, DirectoryError> {
    u64::try_from(value)
        .map_err(|_| DirectoryError::Persistence("directory import count is negative".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tls() -> DirectoryTlsPolicy {
        DirectoryTlsPolicy {
            mode: DirectoryTlsMode::Ldaps,
            ca_certificate_sha256: "a".repeat(64),
            server_name: "directory.example.test".into(),
            minimum_tls_version: "1.3".into(),
        }
    }

    fn connector(provider: DirectoryProvider, feed: DirectoryFeedKind) -> DirectoryConnector {
        DirectoryConnector {
            id: Uuid::from_u128(1),
            provider,
            scope: DirectoryScope::Platform,
            endpoint: "ldaps://directory.example.test:636".into(),
            base_dn: "dc=example,dc=test".into(),
            tls: tls(),
            feed,
            credential_ref: "secret://directory/bind".into(),
            credential_profile_key: "default-directory".into(),
            parent_connector_id: None,
            directory_authority_fingerprint: None,
            fallback_to_holding: false,
            enabled: true,
            max_import_users: MAX_DIRECTORY_IMPORT_USERS,
            revision: 1,
        }
    }

    #[test]
    fn rejects_cleartext_and_invalid_tls() {
        let mut value = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        value.endpoint = "ldap://directory.example.test:389".into();
        assert_eq!(value.validate(), Err(DirectoryError::TlsRequired));
        value.endpoint = "ldaps://directory.example.test:636".into();
        value.tls.minimum_tls_version = "1.1".into();
        assert!(matches!(
            value.validate(),
            Err(DirectoryError::InvalidTlsPolicy(_))
        ));
    }

    #[test]
    fn rejects_endpoint_server_name_mismatch_and_malformed_authority() {
        let mut value = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        value.endpoint = "ldaps://other.example.test:636".into();
        assert_eq!(value.validate(), Err(DirectoryError::InvalidEndpoint));
        value.endpoint = "ldaps://directory.example.test:not-a-port".into();
        assert_eq!(value.validate(), Err(DirectoryError::InvalidEndpoint));
    }

    #[test]
    fn accepts_ucs_provisioning_path_but_keeps_ldap_endpoints_host_only() {
        let mut ucs = connector(
            DirectoryProvider::UniventionCorporateServer,
            DirectoryFeedKind::UcsProvisioningService,
        );
        ucs.endpoint = "https://directory.example.test/univention/provisioning/".into();
        ucs.tls.mode = DirectoryTlsMode::Https;
        assert!(ucs.validate().is_ok());

        ucs.endpoint = "https://directory.example.test/univention/provisioning?next=1".into();
        assert_eq!(ucs.validate(), Err(DirectoryError::InvalidEndpoint));

        let mut ldap = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        ldap.endpoint = "ldaps://directory.example.test/ldap".into();
        assert_eq!(ldap.validate(), Err(DirectoryError::InvalidEndpoint));
    }

    #[test]
    fn supports_native_vendor_feeds_only_for_the_matching_provider() {
        assert!(
            connector(
                DirectoryProvider::ActiveDirectoryDomainServices,
                DirectoryFeedKind::AdDirSync
            )
            .validate()
            .is_ok()
        );
        assert!(
            connector(
                DirectoryProvider::UniventionCorporateServer,
                DirectoryFeedKind::UcsProvisioningService
            )
            .validate()
            .is_ok()
        );
        assert!(matches!(
            connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::AdDirSync).validate(),
            Err(DirectoryError::UnsupportedFeed { .. })
        ));
    }

    #[test]
    fn import_plan_is_bounded_to_one_million_users() {
        let value = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        let plan = DirectoryImportPlan::new(&value, 1_000_000, 5_000).expect("bounded plan");
        assert_eq!(plan.page_count(), 200);
        assert_eq!(plan.max_in_flight_pages, sync::DEFAULT_MAX_IN_FLIGHT_PAGES);
        assert!(matches!(
            DirectoryImportPlan::new(&value, 1_000_001, 100),
            Err(DirectoryError::ImportLimitExceeded { .. })
        ));
        assert!(matches!(
            DirectoryImportPlan::new(&value, 1, 5_001),
            Err(DirectoryError::InvalidBatch(_))
        ));
        assert!(matches!(
            DirectoryImportPlan::with_tuning(&value, 1, 1, sync::MAX_IN_FLIGHT_PAGES + 1),
            Err(DirectoryError::InvalidBatch(_))
        ));
    }

    #[test]
    fn import_admission_is_owned_by_the_persisted_lifecycle() {
        let mut value = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        value.enabled = false;
        assert!(DirectoryImportPlan::new(&value, 1, 1).is_ok());
        // The pure connector cannot prove its persisted lifecycle. The SQLx
        // boundary requires `verified` before it creates durable work.
        assert!(validate_import_connector(&value).is_ok());
    }

    #[test]
    fn connector_lifecycle_is_explicit_and_fail_closed() {
        assert!(
            DirectoryConnectorLifecycle::Draft
                .can_transition_to(DirectoryConnectorLifecycle::Verified)
        );
        assert!(
            DirectoryConnectorLifecycle::Ready
                .can_transition_to(DirectoryConnectorLifecycle::Active)
        );
        assert!(
            !DirectoryConnectorLifecycle::Draft
                .can_transition_to(DirectoryConnectorLifecycle::Active)
        );
        assert!(DirectoryConnectorLifecycle::Active.permits_authentication());
        assert!(!DirectoryConnectorLifecycle::Suspended.permits_authentication());
    }

    #[test]
    fn holding_and_branch_views_share_one_canonical_directory_authority() {
        let fingerprint = "b".repeat(64);
        let holding_ref = Uuid::from_u128(10).to_string();
        let mut holding = connector(
            DirectoryProvider::ActiveDirectoryDomainServices,
            DirectoryFeedKind::AdDirSync,
        );
        holding.scope = DirectoryScope::Holding {
            holding_ref: holding_ref.clone(),
        };
        holding.directory_authority_fingerprint = Some(fingerprint.clone());

        let mut branch = holding.clone();
        branch.id = Uuid::from_u128(2);
        branch.scope = DirectoryScope::Branch {
            holding_ref,
            company_id: Uuid::from_u128(3),
            branch_id: Uuid::from_u128(4),
        };
        branch.parent_connector_id = Some(holding.id);

        assert_eq!(
            directory_authority_key(&holding).expect("holding authority"),
            directory_authority_key(&branch).expect("branch authority")
        );

        branch.directory_authority_fingerprint = Some("c".repeat(64));
        assert_ne!(
            directory_authority_key(&holding).expect("holding authority"),
            directory_authority_key(&branch).expect("drifted branch authority")
        );
    }

    #[test]
    fn role_mapping_and_fact_revisions_cannot_start_at_zero() {
        let connector = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        let mut mapping = DirectoryRoleMapping {
            connector_id: connector.id,
            group_ref: "group:directory-users".into(),
            role_ref: "role:directory-reader".into(),
            scope: connector.scope.clone(),
            target_scope: DirectoryRoleTargetScope::Platform,
            requires_mfa: false,
            active: true,
            revision: 0,
        };
        assert!(mapping.validate().is_err());
        mapping.revision = 1;
        mapping.validate().expect("positive mapping revision");
        assert!(
            DirectoryRoleFact::new(
                connector.id,
                "identity-link".into(),
                "role:directory-reader".into(),
                "platform".into(),
                DirectoryRoleAction::Revoke,
                0,
                None,
                Utc::now(),
            )
            .is_err()
        );
    }

    #[test]
    fn identity_links_are_immutable_and_namespaced() {
        let first = immutable_identity_link(Uuid::from_u128(1), "object-1").expect("link");
        let repeat = immutable_identity_link(Uuid::from_u128(1), "object-1").expect("link");
        let other = immutable_identity_link(Uuid::from_u128(2), "object-1").expect("link");
        assert_eq!(first, repeat);
        assert_ne!(first, other);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn feed_cursor_and_changes_are_ordered_and_duplicates_rejected() {
        let at = Utc::now();
        let page = DirectorySyncPage {
            connector_id: Uuid::from_u128(1),
            cursor: DirectorySyncCursor {
                connector_id: Uuid::from_u128(1),
                feed: DirectoryFeedKind::Polling,
                sequence: 2,
                opaque_cookie: None,
                observed_at: at,
            },
            changes: vec![DirectoryChange::Upsert(DirectoryUser {
                external_object_id: "object-1".into(),
                principal_ref: "principal-1".into(),
                email: None,
                display_name: None,
                groups: BTreeSet::new(),
                active: true,
                source_revision: 2,
            })],
        };
        assert!(page.validate(DirectoryProvider::GenericLdap, None).is_ok());
        let mut duplicate = page.clone();
        duplicate.changes.push(duplicate.changes[0].clone());
        assert!(matches!(
            duplicate.validate(DirectoryProvider::GenericLdap, None),
            Err(DirectoryError::DuplicateChange(_))
        ));
    }

    #[test]
    fn feed_pages_reject_zero_revisions_and_observation_time_regressions() {
        let connector_id = Uuid::from_u128(1);
        let first_at = Utc::now() - Duration::seconds(2);
        let previous = DirectorySyncCursor {
            connector_id,
            feed: DirectoryFeedKind::Polling,
            sequence: 1,
            opaque_cookie: None,
            observed_at: first_at,
        };
        let zero_revision = DirectorySyncPage {
            connector_id,
            cursor: DirectorySyncCursor {
                connector_id,
                feed: DirectoryFeedKind::Polling,
                sequence: 2,
                opaque_cookie: None,
                observed_at: first_at + Duration::seconds(1),
            },
            changes: vec![DirectoryChange::Disable {
                external_object_id: "object-1".into(),
                source_revision: 0,
            }],
        };
        assert!(matches!(
            zero_revision.validate(DirectoryProvider::GenericLdap, Some(&previous)),
            Err(DirectoryError::InvalidBatch(message))
                if message.contains("source revision")
        ));

        let mut regressed_time = zero_revision.clone();
        regressed_time.cursor.observed_at = first_at - Duration::seconds(1);
        regressed_time.changes.clear();
        assert!(matches!(
            regressed_time.validate(DirectoryProvider::GenericLdap, Some(&previous)),
            Err(DirectoryError::InvalidBatch(message))
                if message.contains("observation time")
        ));
    }

    #[test]
    fn stale_or_revoked_directory_state_fails_closed() {
        let now = Utc::now();
        let state = DirectoryRevocationState {
            identity_link: "identity-link-1".into(),
            active: true,
            last_feed_at: now - DIRECTORY_REVOCATION_SLA - Duration::seconds(1),
            revoked_at: None,
        };
        assert!(!state.authorization_allowed(now));
        let future = DirectoryRevocationState {
            identity_link: "identity-link-1".into(),
            active: true,
            last_feed_at: now + Duration::seconds(1),
            revoked_at: None,
        };
        assert!(!future.authorization_allowed(now));
        let state = DirectoryRevocationState {
            identity_link: "identity-link-1".into(),
            active: true,
            last_feed_at: now,
            revoked_at: Some(now),
        };
        assert!(!state.authorization_allowed(now));
    }

    #[test]
    fn authorization_rejects_revocation_state_for_another_identity() {
        let connector = connector(DirectoryProvider::GenericLdap, DirectoryFeedKind::Polling);
        let now = Utc::now();
        let request = DirectoryAuthRequest {
            connector_id: connector.id,
            external_object_id: "object-1".into(),
            risk: DirectoryRisk::Standard,
            mfa_satisfied: false,
            at: now,
        };
        let state = DirectoryRevocationState {
            identity_link: immutable_identity_link(connector.id, "object-2").expect("link"),
            active: true,
            last_feed_at: now,
            revoked_at: None,
        };

        assert!(matches!(
            DirectoryAuthPolicy::default().decide(&connector, &request, &state),
            Err(DirectoryError::InvalidReference(message))
                if message == "directory revocation state does not match the requested identity"
        ));
    }

    #[test]
    fn cursor_identity_and_outbox_replay_keys_are_bound_and_stable() {
        let at = Utc::now();
        let page = DirectorySyncPage {
            connector_id: Uuid::from_u128(1),
            cursor: DirectorySyncCursor {
                connector_id: Uuid::from_u128(2),
                feed: DirectoryFeedKind::Polling,
                sequence: 1,
                opaque_cookie: None,
                observed_at: at,
            },
            changes: Vec::new(),
        };
        assert_eq!(
            page.validate(DirectoryProvider::GenericLdap, None),
            Err(DirectoryError::ConnectorMismatch)
        );
        let payload = serde_json::json!({"source_revision": 7, "state": "disabled"});
        let first = deterministic_outbox_event_id(
            Uuid::from_u128(1),
            "directory_identity_changed",
            &payload,
        )
        .expect("deterministic event ID");
        let replay = deterministic_outbox_event_id(
            Uuid::from_u128(1),
            "directory_identity_changed",
            &payload,
        )
        .expect("replayed event ID");
        assert_eq!(first, replay);
    }

    #[test]
    fn role_fact_hash_chain_is_append_only_material() {
        let first = DirectoryRoleFact::new(
            Uuid::from_u128(1),
            "link".into(),
            "role".into(),
            "platform".into(),
            DirectoryRoleAction::Grant,
            1,
            None,
            Utc::now(),
        )
        .expect("fact");
        let second = DirectoryRoleFact::new(
            Uuid::from_u128(1),
            "link".into(),
            "role".into(),
            "platform".into(),
            DirectoryRoleAction::Revoke,
            2,
            Some(first.fact_hash.clone()),
            Utc::now(),
        )
        .expect("fact");
        assert_eq!(
            second.previous_hash.as_deref(),
            Some(first.fact_hash.as_str())
        );
        assert_ne!(first.fact_hash, second.fact_hash);
    }

    #[test]
    fn holding_scope_identity_is_stable_for_uuid_and_opaque_refs() {
        let holding_id = Uuid::from_u128(42);
        let uuid_scope = DirectoryScope::Holding {
            holding_ref: holding_id.to_string(),
        };
        let opaque_scope = DirectoryScope::Holding {
            holding_ref: "holding-ref-42".into(),
        };
        assert_eq!(uuid_scope.holding_id(), Some(holding_id));
        assert_eq!(
            opaque_scope.holding_id(),
            Some(Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                b"erpx:holding-scope:v1:holding-ref-42"
            ))
        );
        assert_eq!(
            opaque_scope.authorization_key().expect("stable scope key"),
            format!(
                "holding:{}",
                Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    b"erpx:holding-scope:v1:holding-ref-42"
                )
            )
        );
        assert!(!uuid_scope.permits(&opaque_scope));
        assert!(
            opaque_scope.permits(&DirectoryScope::Holding {
                holding_ref: Uuid::new_v5(
                    &Uuid::NAMESPACE_URL,
                    b"erpx:holding-scope:v1:holding-ref-42",
                )
                .to_string(),
            })
        );
        assert_eq!(opaque_scope.holding_id(), opaque_scope.holding_id());
    }
}
