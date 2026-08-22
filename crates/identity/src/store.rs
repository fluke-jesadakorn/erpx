use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    access::{RiskTier, RoleAssignment},
    contracts::AuthorityScope as Scope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use crate::challenge::{
    ChallengeCeremony, ChallengeLifetime, ChallengeMaterial, WebAuthnChallenge,
};
use crate::directory::sync::{
    ImmutableIdentityLink, directory_holding_id, immutable_identity_database_key,
};
use crate::session::PersistedSession;
use crate::totp::{
    KeyResolver, TotpEnrollmentStart, TotpSecret, TotpSecretEnvelope, validate_enrollment_lifetime,
};
use crate::{
    AccessToken, AccessTokenClaims, AccessTokenIssueWindow, AccessTokenLifetime, AccessTokenSigner,
    ActiveRoleClaim, AuthenticatedAuthority, AuthenticatedPrincipalKind, AuthenticatedRealm,
    DEVELOPMENT_CLUSTER_MARKER, DEVELOPMENT_CONTROLLER_READ_PERMISSIONS, DirectoryRoleTargetScope,
    IdentityError, PasswordService, RecoveryCodeBatch, RecoveryCodeSet, RecoverySession,
    RefreshRotationOutcome, SessionRecord, SessionService, TokenHash, decrypt_totp_secret,
    encrypt_totp_secret, permission_catalog, verify_totp_code,
};

const REFRESH_LIFETIME: Duration = Duration::days(30);

#[derive(Debug, Error)]
pub enum IdentityStoreError {
    #[error("identity database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("identity primitive error: {0}")]
    Primitive(#[from] IdentityError),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("principal is not active")]
    PrincipalInactive,
    #[error("device is revoked")]
    DeviceRevoked,
    #[error("passkey is already registered")]
    PasskeyAlreadyRegistered,
    #[error("passkey was not found")]
    PasskeyNotFound,
    #[error("passkey is revoked")]
    PasskeyRevoked,
    #[error("session is inactive")]
    SessionInactive,
    #[error("active role selection is required")]
    RoleSelectionRequired,
    #[error("active role selection is invalid")]
    RoleSelectionInvalid,
    #[error("security command idempotency key conflicts with another request")]
    SecurityCommandConflict,
    #[error("identity record is invalid: {0}")]
    InvalidRecord(String),
}

/// Safe, non-secret device projection returned to account-management code.
/// Session tokens and credential material are intentionally absent.
///
/// The crate root re-exports this DTO for delivery adapters, while its private
/// fields ensure callers can only access the explicitly safe projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSnapshot {
    id: Uuid,
    principal_ref: String,
    label: String,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

/// Result of a principal-scoped device revocation. Replaying a revocation is
/// successful but is reported separately so delivery adapters can preserve
/// idempotency evidence without exposing session material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceRevocationOutcome {
    replayed: bool,
    occurred_at: DateTime<Utc>,
}

/// Server-derived metadata committed with a high-risk identity command.
/// The digest binds the idempotency key to the complete non-secret request;
/// credential and one-time recovery material must never be included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityCommandContext {
    command_id: Uuid,
    audit_reference: Uuid,
    idempotency_key: String,
    request_digest: [u8; 32],
    purpose: String,
    locale: String,
}

impl SecurityCommandContext {
    pub fn new(
        command_id: Uuid,
        audit_reference: Uuid,
        idempotency_key: String,
        request_digest: [u8; 32],
        purpose: String,
        locale: String,
    ) -> Result<Self, IdentityStoreError> {
        if command_id.is_nil()
            || audit_reference.is_nil()
            || idempotency_key.trim().is_empty()
            || idempotency_key.len() > 128
            || idempotency_key.chars().any(char::is_control)
            || purpose.trim().is_empty()
            || purpose.len() > 256
            || purpose.chars().any(char::is_control)
            || !matches!(locale.as_str(), "en" | "th")
        {
            return Err(IdentityStoreError::InvalidRecord(
                "security command context is invalid".into(),
            ));
        }
        Ok(Self {
            command_id,
            audit_reference,
            idempotency_key,
            request_digest,
            purpose,
            locale,
        })
    }

    pub const fn command_id(&self) -> Uuid {
        self.command_id
    }

    pub const fn audit_reference(&self) -> Uuid {
        self.audit_reference
    }
}

/// Secret-free authoritative summary for the account security center.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityFactorState {
    directory_managed: bool,
    totp_enabled_at: Option<DateTime<Utc>>,
    passkey_count: u16,
    recovery_codes_total: u16,
    recovery_codes_remaining: u16,
    recovery_codes_issued_at: Option<DateTime<Utc>>,
    measured_at: DateTime<Utc>,
}

/// Recovery rotation returns plaintext only on the first execution. Exact
/// replay receipts intentionally cannot reconstruct one-time codes from their
/// persisted Argon2id digests.
#[derive(Debug)]
pub struct RecoveryCodeRotationOutcome {
    batch: Option<RecoveryCodeBatch>,
    replayed: bool,
    occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotpRemovalOutcome {
    replayed: bool,
    occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitPolicy {
    max_attempts: u32,
    window_seconds: u64,
    block_seconds: u64,
}

impl RateLimitPolicy {
    pub const fn new(max_attempts: u32, window_seconds: u64, block_seconds: u64) -> Self {
        Self {
            max_attempts,
            window_seconds,
            block_seconds,
        }
    }

    fn validate(self) -> Result<(), IdentityStoreError> {
        if self.max_attempts == 0
            || self.window_seconds == 0
            || self.block_seconds == 0
            || self.window_seconds > i64::MAX as u64
            || self.block_seconds > i64::MAX as u64
            || self.window_seconds > 86_400
            || self.block_seconds > 604_800
            || self.max_attempts > i32::MAX as u32
        {
            return Err(IdentityStoreError::InvalidRecord(
                "rate-limit policy is outside bounded limits".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    Allowed,
    Limited,
}

/// Result of consulting the directory-managed authorization boundary.
///
/// `Unmanaged` is the only result that permits the shared authorization
/// caller to evaluate the legacy/local role aggregate. Once a principal has
/// a persisted directory projection, a missing, stale, disabled, or revoked
/// directory grant is an explicit denial rather than an opportunity to fall
/// back to a local assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryRoleAccess {
    Unmanaged,
    Allowed,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleSelectionSource {
    Local,
    Directory,
}

impl RoleSelectionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Directory => "directory",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "directory" => Some(Self::Directory),
            _ => None,
        }
    }
}

/// A credential-free role choice produced exclusively from authoritative
/// server state. `selection_id` is a local assignment UUID or an opaque,
/// deterministic digest for one exact directory mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleOption {
    pub selection_id: String,
    pub source: RoleSelectionSource,
    pub role_key: String,
    pub scope_key: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub requires_mfa: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RoleAssignmentTargetPolicy {
    Unrestricted,
    HoldingAllowlist(Vec<String>),
}

fn role_assignment_target_policy_from_parts(
    kind: &str,
    environment_gate: Option<&str>,
    targets: Vec<String>,
) -> Option<RoleAssignmentTargetPolicy> {
    match kind {
        "unrestricted" if environment_gate.is_none() && targets.is_empty() => {
            Some(RoleAssignmentTargetPolicy::Unrestricted)
        }
        "holding_allowlist"
            if environment_gate == Some(DEVELOPMENT_CLUSTER_MARKER) && !targets.is_empty() =>
        {
            Some(RoleAssignmentTargetPolicy::HoldingAllowlist(targets))
        }
        _ => None,
    }
}

fn role_assignment_target_allows(
    policy: Option<&RoleAssignmentTargetPolicy>,
    required_permission: &str,
    target_holding: Option<&str>,
) -> bool {
    match policy {
        Some(RoleAssignmentTargetPolicy::Unrestricted) => true,
        Some(RoleAssignmentTargetPolicy::HoldingAllowlist(targets)) => {
            DEVELOPMENT_CONTROLLER_READ_PERMISSIONS.contains(&required_permission)
                || target_holding
                    .is_some_and(|holding_id| targets.iter().any(|target| target == holding_id))
        }
        None => false,
    }
}

/// Safe projection used by account drawers. Bearer and credential material
/// cannot be represented by this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSessionSummary {
    pub session_id: Uuid,
    pub principal_ref: String,
    pub active: bool,
    pub selected_role: Option<RoleOption>,
    pub refresh_expires_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
}

impl DeviceSnapshot {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn principal_ref(&self) -> &str {
        &self.principal_ref
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn last_seen_at(&self) -> DateTime<Utc> {
        self.last_seen_at
    }

    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
}

impl DeviceRevocationOutcome {
    pub const fn replayed(self) -> bool {
        self.replayed
    }

    pub const fn occurred_at(self) -> DateTime<Utc> {
        self.occurred_at
    }
}

impl SecurityFactorState {
    pub const fn directory_managed(&self) -> bool {
        self.directory_managed
    }

    pub const fn totp_enabled(&self) -> bool {
        self.totp_enabled_at.is_some()
    }

    pub const fn totp_enabled_at(&self) -> Option<DateTime<Utc>> {
        self.totp_enabled_at
    }

    pub const fn passkey_count(&self) -> u16 {
        self.passkey_count
    }

    pub const fn recovery_codes_total(&self) -> u16 {
        self.recovery_codes_total
    }

    pub const fn recovery_codes_remaining(&self) -> u16 {
        self.recovery_codes_remaining
    }

    pub const fn recovery_codes_issued_at(&self) -> Option<DateTime<Utc>> {
        self.recovery_codes_issued_at
    }

    pub const fn measured_at(&self) -> DateTime<Utc> {
        self.measured_at
    }
}

impl RecoveryCodeRotationOutcome {
    pub fn batch(&self) -> Option<&RecoveryCodeBatch> {
        self.batch.as_ref()
    }

    pub const fn replayed(&self) -> bool {
        self.replayed
    }

    pub const fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

impl TotpRemovalOutcome {
    pub const fn replayed(self) -> bool {
        self.replayed
    }

    pub const fn occurred_at(self) -> DateTime<Utc> {
        self.occurred_at
    }
}

#[derive(Debug, Clone)]
pub struct SqlxIdentityStore {
    pool: PgPool,
}

impl SqlxIdentityStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Lightweight readiness probe used by the server boundary. This checks
    /// the same write-side pool that serves identity mutations and therefore
    /// never presents a read replica as an identity authority.
    pub async fn check_ready(&self) -> Result<(), IdentityStoreError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    /// Evaluate a directory-managed permission from the identity write side.
    ///
    /// The directory projection is authoritative for current group membership;
    /// role facts remain the append-only history and are rebuilt into that
    /// projection. A projection older than the five-minute revocation SLA is
    /// denied. Platform mappings cover descendants, while a holding mapping
    /// covers only that holding. A mapping marked high-risk is usable only
    /// after a recent one-time TOTP verification, whose authoritative
    /// timestamp is updated by `verify_totp` in the same write database.
    pub async fn directory_role_access(
        &self,
        principal_ref: &str,
        required_permission: &str,
        requested_scope_key: &str,
        holding_scope_key: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<DirectoryRoleAccess, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if required_permission.trim().is_empty()
            || required_permission.len() > 512
            || required_permission.chars().any(char::is_control)
            || requested_scope_key.trim().is_empty()
            || requested_scope_key.len() > 512
            || requested_scope_key.chars().any(char::is_control)
            || holding_scope_key.is_some_and(|value| {
                value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control)
            })
        {
            return Err(IdentityStoreError::InvalidRecord(
                "directory authorization scope or permission is invalid".into(),
            ));
        }

        let canonical_requested_scope = normalize_directory_scope_key(requested_scope_key)
            .ok_or_else(|| {
                IdentityStoreError::InvalidRecord(
                    "directory authorization scope or permission is invalid".into(),
                )
            })?;
        let canonical_holding_scope = holding_scope_key
            .map(|value| {
                normalize_directory_scope_key(value).ok_or_else(|| {
                    IdentityStoreError::InvalidRecord(
                        "directory authorization scope or permission is invalid".into(),
                    )
                })
            })
            .transpose()?;

        let managed: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.directory_principal_bindings binding
                 JOIN erpx_identity.principals principal ON principal.id=binding.principal_id
                 WHERE principal.principal_ref=$1 AND binding.active
             )",
        )
        .bind(principal_ref)
        .fetch_one(&self.pool)
        .await?;
        if !managed {
            return Ok(DirectoryRoleAccess::Unmanaged);
        }

        let all_bindings_healthy: bool = sqlx::query_scalar(
            "SELECT COALESCE(bool_and(
                    connector.enabled
                    AND connector.lifecycle='active'
                    AND link.active
                    AND link.revoked_at IS NULL
                    AND projection.identity_link IS NOT NULL
                    AND projection.active
                    AND projection.revoked_at IS NULL
                    AND projection.last_feed_at <= $2
                    AND projection.last_feed_at >= ($2 - INTERVAL '5 minutes')
                    AND NOT EXISTS (
                        SELECT 1 FROM erpx_identity.directory_identity_conflicts conflict
                        WHERE conflict.state='unresolved'
                          AND (conflict.principal_id=binding.principal_id OR (
                              conflict.connector_id=binding.connector_id
                              AND conflict.external_object_id=binding.external_object_id
                          ))
                    )
                ), false)
             FROM erpx_identity.directory_principal_bindings binding
             JOIN erpx_identity.principals principal ON principal.id=binding.principal_id
             JOIN erpx_identity.directory_identity_links link
               ON link.connector_id=binding.connector_id
              AND link.external_object_id=binding.external_object_id
             JOIN erpx_identity.directory_connectors connector ON connector.id=binding.connector_id
             LEFT JOIN erpx_identity.directory_identity_projection projection
               ON projection.identity_link=link.identity_link
             WHERE principal.principal_ref=$1 AND binding.active",
        )
        .bind(principal_ref)
        .bind(at)
        .fetch_one(&self.pool)
        .await?;
        if !all_bindings_healthy {
            return Ok(DirectoryRoleAccess::Denied);
        }

        let rows = sqlx::query(
            "SELECT c.scope_key AS connector_scope_key,
                    m.scope_key, m.requires_mfa
             FROM erpx_identity.directory_principal_bindings binding
             JOIN erpx_identity.principals principal ON principal.id=binding.principal_id
             JOIN erpx_identity.directory_identity_links link
               ON link.connector_id=binding.connector_id
              AND link.external_object_id=binding.external_object_id
             JOIN erpx_identity.directory_identity_projection p
               ON p.identity_link=link.identity_link
             JOIN erpx_identity.directory_connectors c
               ON c.id = p.connector_id
             JOIN erpx_identity.directory_role_mappings m
               ON m.connector_id = p.connector_id
             WHERE principal.principal_ref = $1
               AND binding.active
               AND c.enabled
               AND c.lifecycle='active'
               AND p.active
               AND p.revoked_at IS NULL
               AND p.last_feed_at <= $5
               AND p.last_feed_at >= ($5 - INTERVAL '5 minutes')
               AND m.role_ref = $2
               AND m.active
               AND p.groups_json @> jsonb_build_array(m.group_ref)
               AND (
                   ($3 = 'platform' AND m.scope_key = 'platform')
                   OR ($3 <> 'platform'
                       AND $4 IS NOT NULL
                       AND (m.scope_key = 'platform' OR m.scope_key LIKE 'holding:%'))
               )",
        )
        .bind(principal_ref)
        .bind(required_permission)
        .bind(&canonical_requested_scope)
        .bind(canonical_holding_scope.as_deref())
        .bind(at)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let connector_scope_key: String = row.try_get("connector_scope_key")?;
            let mapping_scope_key: String = row.try_get("scope_key")?;
            let mapping_scope_key = normalize_directory_scope_key(&mapping_scope_key);
            let connector_scope_key = normalize_directory_scope_key(&connector_scope_key);
            let connector_contains_mapping = connector_scope_key
                .as_deref()
                .zip(mapping_scope_key.as_deref())
                .is_some_and(|(connector_scope, mapping_scope)| {
                    directory_connector_contains_mapping(connector_scope, mapping_scope)
                });
            if !connector_contains_mapping {
                continue;
            }
            let in_requested_scope = mapping_scope_key.as_deref().is_some_and(|mapping_scope| {
                directory_scope_matches(
                    &canonical_requested_scope,
                    canonical_holding_scope.as_deref(),
                    mapping_scope,
                )
            });
            if !in_requested_scope {
                continue;
            }
            let requires_mfa: bool = row.try_get("requires_mfa")?;
            if !requires_mfa {
                return Ok(DirectoryRoleAccess::Allowed);
            }
            let mfa_recent: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1
                     FROM erpx_identity.totp_secrets t
                     JOIN erpx_identity.principals p ON p.id = t.principal_id
                     WHERE p.principal_ref = $1
                       AND p.status = 'active'
                       AND t.last_verified_step IS NOT NULL
                       AND t.updated_at <= $2
                       AND t.updated_at >= ($2 - INTERVAL '5 minutes')
                 )",
            )
            .bind(principal_ref)
            .bind(at)
            .fetch_one(&self.pool)
            .await?;
            if mfa_recent {
                return Ok(DirectoryRoleAccess::Allowed);
            }
        }
        Ok(DirectoryRoleAccess::Denied)
    }

    /// Administrative/bootstrap primitive. It is intentionally not exposed
    /// by the public HTTP login surface; provisioning code must call this
    /// from an authenticated control-plane workflow.
    pub async fn create_principal_with_password(
        &self,
        principal_ref: &str,
        password: &str,
        at: DateTime<Utc>,
    ) -> Result<Uuid, IdentityStoreError> {
        self.create_principal_with_password_and_authority(
            principal_ref,
            password,
            AuthenticatedAuthority::legacy_human(),
            at,
        )
        .await
    }

    /// Authority-aware provisioning primitive. New non-human principals must
    /// use this path so their realm and kind are never inferred from a login
    /// method or silently replaced with the legacy human authority.
    pub async fn create_principal_with_password_and_authority(
        &self,
        principal_ref: &str,
        password: &str,
        authority: AuthenticatedAuthority,
        at: DateTime<Utc>,
    ) -> Result<Uuid, IdentityStoreError> {
        if principal_ref.trim().is_empty() {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidIdentityReference,
            ));
        }
        let authority = authority.validate()?;
        let id = Uuid::new_v4();
        let digest = PasswordService::default().hash(password)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_identity.principals
                (id, principal_ref, status, authenticated_realm, principal_kind,
                 created_at, updated_at)
             VALUES ($1, $2, 'active', $3, $4, $5, $5)",
        )
        .bind(id)
        .bind(principal_ref)
        .bind(authority_realm_name(authority.realm))
        .bind(authority_kind_name(authority.principal_kind))
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.password_credentials
                (principal_id, argon2id_digest, changed_at)
             VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(digest.as_str())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

    /// Rotate the local development Platform Root password through the
    /// explicitly guarded break-glass path. This primitive is intentionally
    /// limited to the unjoinable local principal; it updates the credential
    /// atomically and revokes all existing sessions before the caller audits
    /// the administrative action.
    pub async fn rotate_platform_root_password(
        &self,
        new_password: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        validate_password_input(new_password)?;
        let digest = PasswordService::default().hash(new_password)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id: Uuid = sqlx::query_scalar(
            "SELECT id
             FROM erpx_identity.principals
             WHERE principal_ref = 'local:platform-root'
               AND status = 'active'
             FOR UPDATE",
        )
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        sqlx::query(
            "INSERT INTO erpx_identity.password_credentials
                (principal_id, argon2id_digest, changed_at)
             VALUES ($1, $2, $3)
             ON CONFLICT (principal_id) DO UPDATE
             SET argon2id_digest = EXCLUDED.argon2id_digest,
                 changed_at = EXCLUDED.changed_at",
        )
        .bind(principal_id)
        .bind(digest.as_str())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        let revoked = sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at = COALESCE(revoked_at, $2), last_used_at = $2
             WHERE principal_id = $1 AND revoked_at IS NULL",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        transaction.commit().await?;
        Ok(revoked)
    }

    pub async fn login(
        &self,
        signer: &AccessTokenSigner,
        principal_ref: &str,
        password: &str,
        device_label: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::IssuedSession, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT p.id, p.status, p.authenticated_realm, p.principal_kind,
                    pc.argon2id_digest
             FROM erpx_identity.principals p
             JOIN erpx_identity.password_credentials pc ON pc.principal_id = p.id
             WHERE p.principal_ref = $1
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )",
        )
        .bind(principal_ref)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let status: String = row.try_get("status")?;
        if status != "active" {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        let digest =
            crate::PasswordDigest::from_stored(row.try_get::<String, _>("argon2id_digest")?)
                .map_err(|_| IdentityStoreError::InvalidCredentials)?;
        PasswordService::default()
            .verify(password, &digest)
            .map_err(|_| IdentityStoreError::InvalidCredentials)?;
        if device_label.trim().is_empty() || device_label.len() > 200 {
            return Err(IdentityStoreError::InvalidRecord(
                "device label is empty or too long".into(),
            ));
        }
        let principal_id: Uuid = row.try_get("id")?;
        let authority = authority_from_row(&row, "authenticated_realm", "principal_kind")?;
        let device_id = Uuid::new_v4();
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let issued = service.issue_with_authority(
            principal_ref,
            device_id,
            authority,
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $4)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(device_label)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        insert_session_in_transaction(&mut transaction, principal_id, &issued).await?;
        transaction.commit().await?;
        Ok(issued)
    }

    /// Resolve an explicitly bound immutable directory identity and issue a
    /// session for its canonical ERPX principal. Every active binding for the
    /// principal is evaluated in one transaction; one stale, disabled,
    /// revoked, suspended, missing, or conflicting provider denies all login
    /// paths. No principal is created or merged from a username or email.
    pub async fn login_with_directory_link(
        &self,
        signer: &AccessTokenSigner,
        connector_id: Uuid,
        identity_link: &str,
        device_label: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::IssuedSession, IdentityStoreError> {
        if connector_id.is_nil() || identity_link.trim().is_empty() || identity_link.len() > 512 {
            return Err(IdentityStoreError::InvalidCredentials);
        }
        validate_device_label(device_label)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT binding.principal_id, principal.principal_ref, principal.status,
                    principal.authenticated_realm, principal.principal_kind,
                    l.external_object_id, l.active, l.revoked_at,
                    l.immutable_authority, l.immutable_value
             FROM erpx_identity.directory_identity_links l
             JOIN erpx_identity.directory_connectors connector
               ON connector.id = l.connector_id
             JOIN erpx_identity.directory_principal_bindings binding
               ON binding.connector_id=l.connector_id
              AND binding.external_object_id=l.external_object_id
              AND binding.active
             JOIN erpx_identity.principals principal
               ON principal.id=binding.principal_id
             WHERE l.connector_id=$1
               AND l.identity_link=$2
               AND connector.enabled
               AND connector.lifecycle='active'
             FOR UPDATE OF l, binding, principal",
        )
        .bind(connector_id)
        .bind(identity_link)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let active: bool = row.try_get("active")?;
        let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at")?;
        let status: String = row.try_get("status")?;
        if !active || revoked_at.is_some() || status != "active" {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        let principal_id: Uuid = row.try_get("principal_id")?;
        let principal_ref: String = row.try_get("principal_ref")?;
        let authority = authority_from_row(&row, "authenticated_realm", "principal_kind")?;
        let immutable_authority: String = row.try_get("immutable_authority")?;
        let immutable_value: String = row.try_get("immutable_value")?;
        let canonical_link = ImmutableIdentityLink::new(immutable_authority, immutable_value)
            .map_err(|_| {
                IdentityStoreError::InvalidRecord(
                    "directory identity link has invalid immutable authority/value".into(),
                )
            })?;
        if immutable_identity_database_key(&canonical_link) != identity_link {
            return Err(IdentityStoreError::InvalidRecord(
                "directory identity link does not match its immutable authority/value".into(),
            ));
        }
        let health = sqlx::query(
            "SELECT COUNT(*) > 0 AS has_bindings,
                    COALESCE(bool_and(
                        connector.enabled
                        AND connector.lifecycle='active'
                        AND link.active
                        AND link.revoked_at IS NULL
                        AND projection.identity_link IS NOT NULL
                        AND projection.active
                        AND projection.revoked_at IS NULL
                        AND projection.last_feed_at <= $2
                        AND projection.last_feed_at >= ($2 - INTERVAL '5 minutes')
                        AND NOT EXISTS (
                            SELECT 1 FROM erpx_identity.directory_identity_conflicts conflict
                            WHERE conflict.state='unresolved'
                              AND (conflict.principal_id=$1 OR (
                                  conflict.connector_id=binding.connector_id
                                  AND conflict.external_object_id=binding.external_object_id
                              ))
                        )
                    ), false) AS all_healthy
             FROM erpx_identity.directory_principal_bindings binding
             JOIN erpx_identity.directory_identity_links link
               ON link.connector_id=binding.connector_id
              AND link.external_object_id=binding.external_object_id
             JOIN erpx_identity.directory_connectors connector
               ON connector.id=binding.connector_id
             LEFT JOIN erpx_identity.directory_identity_projection projection
               ON projection.identity_link=link.identity_link
             WHERE binding.principal_id=$1 AND binding.active",
        )
        .bind(principal_id)
        .bind(at)
        .fetch_one(&mut *transaction)
        .await?;
        if !health.try_get::<bool, _>("has_bindings")?
            || !health.try_get::<bool, _>("all_healthy")?
        {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        let device_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at)
             VALUES ($1,$2,$3,$4,$4)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(device_label)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let issued = service.issue_with_authority(
            principal_ref.clone(),
            device_id,
            authority,
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        insert_session_in_transaction(&mut transaction, principal_id, &issued).await?;
        let event_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_identity.directory_outbox
                (event_id, aggregate_id, aggregate_type, payload_json, occurred_at,
                 event_type, event_version, aggregate_version, scope_json, idempotency_key)
             VALUES ($1,$2,'directory_authentication',$3,$4,
                     'directory.authentication_succeeded.v1',1,1,'{}'::jsonb,$5)",
        )
        .bind(event_id)
        .bind(connector_id)
        .bind(serde_json::json!({
            "event": "directory.authentication_succeeded.v1",
            "connector_id": connector_id,
            "identity_link": identity_link,
            "principal_ref": principal_ref,
            "session_id": issued.record().id(),
            "authenticated_at": at,
        }))
        .bind(at)
        .bind(issued.record().id().to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(issued)
    }

    /// Issue a new device/session after a browser passkey assertion has been
    /// verified. The assertion verifier owns credential proof; this method
    /// rechecks the principal status and creates the device/session on the
    /// identity write side, without requiring a password or trusting client
    /// claims.
    pub async fn login_with_passkey(
        &self,
        signer: &AccessTokenSigner,
        principal_ref: &str,
        device_label: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::IssuedSession, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_device_label(device_label)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, status, authenticated_realm, principal_kind
             FROM erpx_identity.principals p
             WHERE p.principal_ref = $1
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             FOR UPDATE",
        )
        .bind(principal_ref)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let status: String = row.try_get("status")?;
        if status != "active" {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        let principal_id: Uuid = row.try_get("id")?;
        let authority = authority_from_row(&row, "authenticated_realm", "principal_kind")?;
        let device_id = Uuid::new_v4();
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let issued = service.issue_with_authority(
            principal_ref,
            device_id,
            authority,
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $4)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(device_label)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        insert_session_in_transaction(&mut transaction, principal_id, &issued).await?;
        transaction.commit().await?;
        Ok(issued)
    }

    pub async fn refresh(
        &self,
        signer: &AccessTokenSigner,
        session_id: Uuid,
        presented_refresh_token: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::RotatedSession, IdentityStoreError> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT s.id, p.principal_ref, p.status, s.device_id,
                    s.refresh_token_hash, s.issued_at, s.refresh_expires_at,
                    s.revoked_at, s.last_used_at, s.active_role_source,
                    s.active_role_selection_id, s.authorization_context_version,
                    s.authenticated_realm AS session_authenticated_realm,
                    s.principal_kind AS session_principal_kind,
                    p.authenticated_realm AS principal_authenticated_realm,
                    p.principal_kind AS principal_principal_kind,
                    d.revoked_at AS device_revoked_at
             FROM erpx_identity.sessions s
             JOIN erpx_identity.principals p ON p.id = s.principal_id
             JOIN erpx_identity.devices d ON d.id = s.device_id
             WHERE s.id = $1
             FOR UPDATE",
        )
        .bind(session_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::SessionInactive)?;
        if row.try_get::<String, _>("status")? != "active" {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        if row
            .try_get::<Option<DateTime<Utc>>, _>("device_revoked_at")?
            .is_some()
        {
            return Err(IdentityStoreError::DeviceRevoked);
        }
        let session_authority = authority_from_row(
            &row,
            "session_authenticated_realm",
            "session_principal_kind",
        )?;
        let principal_authority = authority_from_row(
            &row,
            "principal_authenticated_realm",
            "principal_principal_kind",
        )?;
        if session_authority != principal_authority {
            return Err(IdentityStoreError::SessionInactive);
        }
        let record = SessionRecord::from_persisted(PersistedSession {
            id: row.try_get("id")?,
            principal_ref: row.try_get("principal_ref")?,
            device_id: row.try_get("device_id")?,
            refresh_token_hash: TokenHash::from_bytes(
                row.try_get::<Vec<u8>, _>("refresh_token_hash")?.as_slice(),
            )?,
            issued_at: row.try_get("issued_at")?,
            refresh_expires_at: row.try_get("refresh_expires_at")?,
            revoked_at: row.try_get("revoked_at")?,
            last_used_at: row.try_get("last_used_at")?,
            active_role_source: row.try_get("active_role_source")?,
            active_role_selection_id: row.try_get("active_role_selection_id")?,
            authorization_context_version: u64::try_from(
                row.try_get::<i64, _>("authorization_context_version")?,
            )
            .map_err(|_| {
                IdentityStoreError::InvalidRecord("authorization context version is invalid".into())
            })?,
            authority: session_authority,
        })?;
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let outcome = service.rotate_refresh(&record, presented_refresh_token, at)?;
        match outcome {
            RefreshRotationOutcome::Rotated(rotated) => {
                sqlx::query(
                    "UPDATE erpx_identity.sessions
                     SET refresh_token_hash = $2, last_used_at = $3
                     WHERE id = $1",
                )
                .bind(session_id)
                .bind(rotated.record().refresh_token_hash().as_bytes().as_slice())
                .bind(rotated.record().last_used_at())
                .execute(&mut *transaction)
                .await?;
                transaction.commit().await?;
                Ok(rotated)
            }
            RefreshRotationOutcome::ReuseDetected { revoked_session } => {
                sqlx::query(
                    "UPDATE erpx_identity.sessions
                     SET revoked_at = COALESCE(revoked_at, $2), last_used_at = $2
                     WHERE id = $1",
                )
                .bind(session_id)
                .bind(revoked_session.revoked_at().unwrap_or(at))
                .execute(&mut *transaction)
                .await?;
                transaction.commit().await?;
                Err(IdentityStoreError::Primitive(
                    IdentityError::RefreshTokenReuseDetected,
                ))
            }
        }
    }

    pub async fn authorize_access_token(
        &self,
        signer: &AccessTokenSigner,
        token: &str,
        at: DateTime<Utc>,
    ) -> Result<AccessTokenClaims, IdentityStoreError> {
        let claims = signer.verify(token, at)?;
        let row = sqlx::query(
            "SELECT s.active_role_source, s.active_role_selection_id,
                    s.authorization_context_version,
                    s.authenticated_realm AS session_authenticated_realm,
                    s.principal_kind AS session_principal_kind,
                    p.authenticated_realm AS principal_authenticated_realm,
                    p.principal_kind AS principal_principal_kind
             FROM erpx_identity.sessions s
             JOIN erpx_identity.principals p ON p.id = s.principal_id
             JOIN erpx_identity.devices d ON d.id = s.device_id
             WHERE s.id = $1
               AND p.principal_ref = $2
               AND p.status = 'active'
               AND d.revoked_at IS NULL
               AND s.revoked_at IS NULL
               AND s.issued_at <= $3
               AND s.refresh_expires_at > $3",
        )
        .bind(claims.session_id)
        .bind(&claims.principal_ref)
        .bind(at)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityStoreError::SessionInactive)?;
        let source: Option<String> = row.try_get("active_role_source")?;
        let selection_id: Option<String> = row.try_get("active_role_selection_id")?;
        let version = u64::try_from(row.try_get::<i64, _>("authorization_context_version")?)
            .map_err(|_| {
                IdentityStoreError::InvalidRecord("authorization context version is invalid".into())
            })?;
        let session_authority = authority_from_row(
            &row,
            "session_authenticated_realm",
            "session_principal_kind",
        )?;
        let principal_authority = authority_from_row(
            &row,
            "principal_authenticated_realm",
            "principal_principal_kind",
        )?;
        let persisted_role = source.zip(selection_id);
        let claimed_role = claims
            .active_role
            .as_ref()
            .map(|role| (role.source.clone(), role.selection_id.clone()));
        let context_matches = match claims.authorization_context_version {
            Some(claimed_version) => claimed_version == version && claimed_role == persisted_role,
            // A signed token issued before the active-role rollout is accepted
            // only while the migrated session remains unselected. The business
            // boundary rejects its missing claim.
            None => persisted_role.is_none() && claimed_role.is_none(),
        };
        if !context_matches
            || session_authority != principal_authority
            || claims.authority() != session_authority
        {
            return Err(IdentityStoreError::SessionInactive);
        }
        Ok(claims)
    }

    /// Return every currently effective exact role available to a principal.
    /// Local choices are assignment UUIDs with their complete parent chain
    /// validated. Directory choices are opaque mapping digests and require a
    /// fresh authoritative membership projection.
    pub async fn role_options(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<Vec<RoleOption>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let mut options = self.local_role_options(principal_ref, at).await?;
        options.extend(self.directory_role_options(principal_ref, at).await?);
        options.sort_by(|left, right| {
            left.source
                .as_str()
                .cmp(right.source.as_str())
                .then_with(|| left.role_key.cmp(&right.role_key))
                .then_with(|| left.scope_key.cmp(&right.scope_key))
                .then_with(|| left.selection_id.cmp(&right.selection_id))
        });
        options.dedup_by(|left, right| {
            left.source == right.source && left.selection_id == right.selection_id
        });
        Ok(options)
    }

    /// Select or change one role after resolving the supplied opaque choice
    /// back to current server state. The context version update and audit row
    /// are committed atomically; all access tokens from the prior context are
    /// rejected immediately by `authorize_access_token`.
    pub async fn select_active_role(
        &self,
        signer: &AccessTokenSigner,
        principal_ref: &str,
        session_id: Uuid,
        selection_id: &str,
        at: DateTime<Utc>,
    ) -> Result<(RoleOption, AccessToken), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if session_id.is_nil()
            || selection_id.trim().is_empty()
            || selection_id.len() > 512
            || selection_id.chars().any(char::is_control)
        {
            return Err(IdentityStoreError::RoleSelectionInvalid);
        }
        let option = self
            .role_options(principal_ref, at)
            .await?
            .into_iter()
            .find(|option| option.selection_id == selection_id)
            .ok_or(IdentityStoreError::RoleSelectionInvalid)?;

        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT s.active_role_source, s.active_role_selection_id,
                    s.authorization_context_version,
                    s.authenticated_realm AS session_authenticated_realm,
                    s.principal_kind AS session_principal_kind,
                    p.authenticated_realm AS principal_authenticated_realm,
                    p.principal_kind AS principal_principal_kind
             FROM erpx_identity.sessions s
             JOIN erpx_identity.principals p ON p.id=s.principal_id
             JOIN erpx_identity.devices d ON d.id=s.device_id
             WHERE s.id=$1 AND p.principal_ref=$2 AND p.status='active'
               AND d.revoked_at IS NULL AND s.revoked_at IS NULL
               AND s.issued_at <= $3 AND s.refresh_expires_at > $3
             FOR UPDATE OF s",
        )
        .bind(session_id)
        .bind(principal_ref)
        .bind(at)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::SessionInactive)?;
        let previous_source: Option<String> = row.try_get("active_role_source")?;
        let previous_selection: Option<String> = row.try_get("active_role_selection_id")?;
        let current_version: i64 = row.try_get("authorization_context_version")?;
        let session_authority = authority_from_row(
            &row,
            "session_authenticated_realm",
            "session_principal_kind",
        )?;
        let principal_authority = authority_from_row(
            &row,
            "principal_authenticated_realm",
            "principal_principal_kind",
        )?;
        if session_authority != principal_authority {
            return Err(IdentityStoreError::SessionInactive);
        }
        let next_version = current_version.checked_add(1).ok_or_else(|| {
            IdentityStoreError::InvalidRecord("authorization context version overflow".into())
        })?;
        sqlx::query(
            "UPDATE erpx_identity.sessions
             SET active_role_source=$2, active_role_selection_id=$3,
                 authorization_context_version=$4,
                 last_used_at=GREATEST(last_used_at,$5)
             WHERE id=$1",
        )
        .bind(session_id)
        .bind(option.source.as_str())
        .bind(&option.selection_id)
        .bind(next_version)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        let changed = previous_source.as_deref() != Some(option.source.as_str())
            || previous_selection.as_deref() != Some(option.selection_id.as_str());
        sqlx::query(
            "INSERT INTO erpx_identity.session_authorization_audit
                (event_id,session_id,principal_ref,event_type,role_source,
                 role_selection_id,authorization_context_version,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(Uuid::new_v4())
        .bind(session_id)
        .bind(principal_ref)
        .bind(if previous_selection.is_some() && changed {
            "role_changed"
        } else {
            "role_selected"
        })
        .bind(option.source.as_str())
        .bind(&option.selection_id)
        .bind(next_version)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        let version = u64::try_from(next_version).map_err(|_| {
            IdentityStoreError::InvalidRecord("authorization context version is invalid".into())
        })?;
        let token = signer.issue_with_authority(
            principal_ref,
            session_id,
            Some(ActiveRoleClaim {
                source: option.source.as_str().into(),
                selection_id: option.selection_id.clone(),
            }),
            Some(version),
            session_authority,
            AccessTokenIssueWindow::new(at, AccessTokenLifetime::five_minutes()),
        )?;
        Ok((option, token))
    }

    /// Revalidate only the selected role identity and its parent/membership
    /// chain. Permission and requested scope are checked separately for every
    /// business operation by `selected_role_access`.
    pub async fn active_role_is_valid(
        &self,
        principal_ref: &str,
        role: &ActiveRoleClaim,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        let Some(source) = RoleSelectionSource::parse(&role.source) else {
            return Ok(false);
        };
        Ok(self
            .role_options(principal_ref, at)
            .await?
            .into_iter()
            .any(|option| option.source == source && option.selection_id == role.selection_id))
    }

    /// Evaluate one required permission against only the role named in the
    /// access-token claim. No other assignment or directory mapping can
    /// contribute permission or scope.
    pub async fn selected_role_access(
        &self,
        principal_ref: &str,
        role: &ActiveRoleClaim,
        requested_scope: &Scope,
        required_permission: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        self.selected_role_access_for_holding(
            principal_ref,
            role,
            requested_scope,
            required_permission,
            None,
            at,
        )
        .await
    }

    /// Evaluate an exact selected role and an optional server-resolved holding
    /// target. Holding-allowlisted roles may read the explicit global read
    /// catalog across holdings, but every mutation requires an exact target.
    pub async fn selected_role_access_for_holding(
        &self,
        principal_ref: &str,
        role: &ActiveRoleClaim,
        requested_scope: &Scope,
        required_permission: &str,
        target_holding: Option<&str>,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        if required_permission.trim().is_empty() || required_permission.len() > 512 {
            return Ok(false);
        }
        match RoleSelectionSource::parse(&role.source) {
            Some(RoleSelectionSource::Local) => {
                let Ok(selection_id) = Uuid::parse_str(&role.selection_id) else {
                    return Ok(false);
                };
                let assignments = self.local_role_assignments().await?;
                let Some(assignment) = assignments.get(&selection_id) else {
                    return Ok(false);
                };
                if !self
                    .local_role_target_allows(selection_id, required_permission, target_holding)
                    .await?
                {
                    return Ok(false);
                }
                // Local role risk class describes the assurance required by
                // high-risk commands; it must not turn every permission read
                // into an implicit MFA challenge. Command boundaries perform
                // their explicit, one-time MFA step-up after target-aware role
                // authorization. Keeping that separation also preserves the
                // pre-selection authorization contract for ordinary reads.
                Ok(selected_local_assignment_allows(
                    assignment,
                    &assignments,
                    principal_ref,
                    requested_scope,
                    required_permission,
                    at,
                ))
            }
            Some(RoleSelectionSource::Directory) => {
                let Some(option) = self
                    .directory_role_options(principal_ref, at)
                    .await?
                    .into_iter()
                    .find(|option| option.selection_id == role.selection_id)
                else {
                    return Ok(false);
                };
                let Some(target_scope) = directory_role_target_scope(&option.role_key) else {
                    return Ok(false);
                };
                if !directory_role_allows_permission(&option.role_key, required_permission)
                    || !directory_target_scope_allows(target_scope, requested_scope)
                {
                    return Ok(false);
                }
                let requested_key = requested_scope.canonical_key();
                let holding_key = scope_holding_key(requested_scope);
                if !directory_scope_matches(
                    &requested_key,
                    holding_key.as_deref(),
                    &option.scope_key,
                ) {
                    return Ok(false);
                }
                if option.requires_mfa && !self.recent_mfa(principal_ref, at).await? {
                    return Ok(false);
                }
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub async fn selected_local_role_has_any_key(
        &self,
        principal_ref: &str,
        role: &ActiveRoleClaim,
        requested_scope: &Scope,
        role_keys: &[&str],
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        if RoleSelectionSource::parse(&role.source) != Some(RoleSelectionSource::Local) {
            return Ok(true);
        }
        let Ok(selection_id) = Uuid::parse_str(&role.selection_id) else {
            return Ok(false);
        };
        let assignments = self.local_role_assignments().await?;
        let Some(assignment) = assignments.get(&selection_id) else {
            return Ok(false);
        };
        Ok(assignment.principal_ref == principal_ref
            && role_keys.iter().any(|key| *key == assignment.role_key)
            && assignment.scope.contains(requested_scope)
            && effective_local_role_chain(assignment, &assignments, at, &mut Vec::new()))
    }

    /// Clear an invalid selected role and advance the context fence. This is
    /// best-effort at the business boundary: a concurrent role change is not
    /// overwritten because the expected version and selection are matched.
    pub async fn invalidate_active_role(
        &self,
        principal_ref: &str,
        session_id: Uuid,
        role: &ActiveRoleClaim,
        authorization_context_version: u64,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        let expected_version = i64::try_from(authorization_context_version).map_err(|_| {
            IdentityStoreError::InvalidRecord("authorization context version is invalid".into())
        })?;
        let mut transaction = self.pool.begin().await?;
        let next_version = expected_version.checked_add(1).ok_or_else(|| {
            IdentityStoreError::InvalidRecord("authorization context version overflow".into())
        })?;
        let updated = sqlx::query(
            "UPDATE erpx_identity.sessions s
             SET active_role_source=NULL, active_role_selection_id=NULL,
                 authorization_context_version=$6,
                 last_used_at=GREATEST(s.last_used_at,$5)
             FROM erpx_identity.principals p
             WHERE s.id=$1 AND s.principal_id=p.id AND p.principal_ref=$2
               AND s.active_role_source=$3 AND s.active_role_selection_id=$4
               AND s.authorization_context_version=$7 AND s.revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(principal_ref)
        .bind(&role.source)
        .bind(&role.selection_id)
        .bind(at)
        .bind(next_version)
        .bind(expected_version)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
            == 1;
        if updated {
            sqlx::query(
                "INSERT INTO erpx_identity.session_authorization_audit
                    (event_id,session_id,principal_ref,event_type,role_source,
                     role_selection_id,authorization_context_version,occurred_at)
                 VALUES ($1,$2,$3,'role_invalidated',$4,$5,$6,$7)",
            )
            .bind(Uuid::new_v4())
            .bind(session_id)
            .bind(principal_ref)
            .bind(&role.source)
            .bind(&role.selection_id)
            .bind(next_version)
            .bind(at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(updated)
    }

    async fn local_role_assignments(
        &self,
    ) -> Result<HashMap<Uuid, RoleAssignment>, IdentityStoreError> {
        let rows = sqlx::query("SELECT payload FROM erpx_control.role_assignments")
            .fetch_all(&self.pool)
            .await?;
        let mut assignments = HashMap::new();
        for row in rows {
            let payload: serde_json::Value = row.try_get("payload")?;
            if let Ok(assignment) = serde_json::from_value::<RoleAssignment>(payload) {
                assignments.insert(assignment.id, assignment);
            }
        }
        Ok(assignments)
    }

    async fn local_role_options(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<Vec<RoleOption>, IdentityStoreError> {
        let assignments = self.local_role_assignments().await?;
        let mut options = Vec::new();
        for assignment in assignments.values() {
            if assignment.principal_ref != principal_ref
                || !effective_local_role_chain(assignment, &assignments, at, &mut Vec::new())
                || self
                    .local_role_target_policy(assignment.id)
                    .await?
                    .is_none()
            {
                continue;
            }
            options.push(RoleOption {
                selection_id: assignment.id.to_string(),
                source: RoleSelectionSource::Local,
                role_key: assignment.role_key.clone(),
                scope_key: assignment.scope.canonical_key(),
                expires_at: Some(assignment.valid_until),
                requires_mfa: matches!(
                    assignment.risk_tier,
                    RiskTier::PlatformRoot
                        | RiskTier::Administrator
                        | RiskTier::High
                        | RiskTier::Emergency
                ),
            });
        }
        Ok(options)
    }

    async fn local_role_target_policy(
        &self,
        assignment_id: Uuid,
    ) -> Result<Option<RoleAssignmentTargetPolicy>, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT policy.policy_kind, policy.environment_gate,
                    COALESCE(array_agg(target.holding_id ORDER BY target.holding_id)
                        FILTER (WHERE target.holding_id IS NOT NULL), '{}') AS holding_targets
             FROM erpx_control.role_assignment_target_policies policy
             LEFT JOIN erpx_control.role_assignment_holding_targets target
               ON target.assignment_id=policy.assignment_id
             WHERE policy.assignment_id=$1
             GROUP BY policy.policy_kind, policy.environment_gate",
        )
        .bind(assignment_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let kind: String = row.try_get("policy_kind")?;
        let environment_gate: Option<String> = row.try_get("environment_gate")?;
        let targets: Vec<String> = row.try_get("holding_targets")?;
        Ok(role_assignment_target_policy_from_parts(
            &kind,
            environment_gate.as_deref(),
            targets,
        ))
    }

    async fn local_role_target_allows(
        &self,
        assignment_id: Uuid,
        required_permission: &str,
        target_holding: Option<&str>,
    ) -> Result<bool, IdentityStoreError> {
        let policy = self.local_role_target_policy(assignment_id).await?;
        Ok(role_assignment_target_allows(
            policy.as_ref(),
            required_permission,
            target_holding,
        ))
    }

    async fn directory_role_options(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<Vec<RoleOption>, IdentityStoreError> {
        let rows = sqlx::query(
            "SELECT m.connector_id, m.group_ref, m.role_ref, m.scope_key,
                    m.target_scope_key, m.requires_mfa,
                    c.scope_key AS connector_scope_key
             FROM erpx_identity.directory_principal_bindings binding
             JOIN erpx_identity.principals principal ON principal.id=binding.principal_id
             JOIN erpx_identity.directory_identity_links link
               ON link.connector_id=binding.connector_id
              AND link.external_object_id=binding.external_object_id
             JOIN erpx_identity.directory_identity_projection projection
               ON projection.identity_link=link.identity_link
             JOIN erpx_identity.directory_connectors c ON c.id=projection.connector_id
             JOIN erpx_identity.directory_role_mappings m ON m.connector_id=projection.connector_id
             WHERE principal.principal_ref=$1 AND principal.status='active'
               AND binding.active AND link.active AND link.revoked_at IS NULL
               AND c.enabled AND c.lifecycle='active'
               AND projection.active AND projection.revoked_at IS NULL
               AND projection.last_feed_at <= $2
               AND projection.last_feed_at >= ($2 - INTERVAL '5 minutes')
               AND m.active
               AND projection.groups_json @> jsonb_build_array(m.group_ref)
               AND NOT EXISTS (
                   SELECT 1 FROM erpx_identity.directory_identity_conflicts conflict
                   WHERE conflict.state='unresolved'
                     AND (conflict.principal_id=binding.principal_id OR (
                         conflict.connector_id=binding.connector_id
                         AND conflict.external_object_id=binding.external_object_id
                     ))
               )",
        )
        .bind(principal_ref)
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        let mut options = Vec::new();
        for row in rows {
            let connector_id: Uuid = row.try_get("connector_id")?;
            let group_ref: String = row.try_get("group_ref")?;
            let role_ref: String = row.try_get("role_ref")?;
            let target_scope: String = row.try_get("target_scope_key")?;
            let Some(scope_key) =
                normalize_directory_scope_key(&row.try_get::<String, _>("scope_key")?)
            else {
                continue;
            };
            let Some(connector_scope) =
                normalize_directory_scope_key(&row.try_get::<String, _>("connector_scope_key")?)
            else {
                continue;
            };
            if !directory_connector_contains_mapping(&connector_scope, &scope_key)
                || !directory_role_is_allow_listed(&role_ref, &target_scope)
            {
                continue;
            }
            options.push(RoleOption {
                selection_id: directory_selection_id(
                    connector_id,
                    &group_ref,
                    &role_ref,
                    &scope_key,
                ),
                source: RoleSelectionSource::Directory,
                role_key: role_ref,
                scope_key,
                expires_at: None,
                requires_mfa: row.try_get("requires_mfa")?,
            });
        }
        Ok(options)
    }

    async fn recent_mfa(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.totp_secrets t
                 JOIN erpx_identity.principals p ON p.id=t.principal_id
                 WHERE p.principal_ref=$1 AND p.status='active'
                   AND t.last_verified_step IS NOT NULL
                   AND t.updated_at <= $2
                   AND t.updated_at >= ($2 - INTERVAL '5 minutes')
             )",
        )
        .bind(principal_ref)
        .bind(at)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Resolve a named approver from authoritative identity state. This is
    /// intentionally separate from token authorization because approval
    /// references may identify a second active principal rather than the
    /// authenticated requester.
    pub async fn principal_is_active(
        &self,
        principal_ref: &str,
    ) -> Result<bool, IdentityStoreError> {
        if principal_ref.trim().is_empty() || principal_ref.len() > 255 {
            return Ok(false);
        }
        Ok(sqlx::query(
            "SELECT 1
             FROM erpx_identity.principals
             WHERE principal_ref = $1 AND status = 'active'",
        )
        .bind(principal_ref)
        .fetch_optional(&self.pool)
        .await?
        .is_some())
    }

    /// Directory-linked principals are never admitted through the ordinary
    /// password endpoint.  Their directory credential is authoritative; a
    /// local credential may only be used by an explicitly local break-glass
    /// flow at the trusted server boundary.
    pub async fn principal_is_directory_managed(
        &self,
        principal_ref: &str,
    ) -> Result<bool, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1
                 FROM erpx_identity.directory_principal_bindings binding
                 JOIN erpx_identity.principals principal ON principal.id=binding.principal_id
                 WHERE principal.principal_ref=$1 AND binding.active
             )",
        )
        .bind(principal_ref)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Resolve the active device bound to an authenticated session. The
    /// lookup intentionally rechecks principal, device, session, and time
    /// state so capability issuance cannot trust a client-supplied device
    /// label or stale session metadata.
    pub async fn active_session_device_id(
        &self,
        session_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Uuid, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT s.device_id
             FROM erpx_identity.sessions s
             JOIN erpx_identity.principals p ON p.id = s.principal_id
             JOIN erpx_identity.devices d ON d.id = s.device_id
             WHERE s.id = $1
               AND p.status = 'active'
               AND d.revoked_at IS NULL
               AND s.revoked_at IS NULL
               AND s.issued_at <= $2
               AND s.refresh_expires_at > $2",
        )
        .bind(session_id)
        .bind(at)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| row.try_get("device_id"))
            .transpose()?
            .ok_or(IdentityStoreError::SessionInactive)
    }

    pub async fn account_session_summaries(
        &self,
        session_ids: &[Uuid],
        active_session_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Vec<AccountSessionSummary>, IdentityStoreError> {
        if session_ids.len() > 5
            || session_ids.is_empty()
            || session_ids.iter().any(Uuid::is_nil)
            || !session_ids.contains(&active_session_id)
        {
            return Err(IdentityStoreError::InvalidRecord(
                "account session summary request is invalid".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT s.id, p.principal_ref, s.active_role_source,
                    s.active_role_selection_id, s.refresh_expires_at, s.last_used_at
             FROM erpx_identity.sessions s
             JOIN erpx_identity.principals p ON p.id=s.principal_id
             JOIN erpx_identity.devices d ON d.id=s.device_id
             WHERE s.id=ANY($1) AND p.status='active' AND d.revoked_at IS NULL
               AND s.revoked_at IS NULL AND s.issued_at <= $2
               AND s.refresh_expires_at > $2",
        )
        .bind(session_ids)
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        let mut summaries = Vec::new();
        for row in rows {
            let session_id: Uuid = row.try_get("id")?;
            let principal_ref: String = row.try_get("principal_ref")?;
            let selected = row
                .try_get::<Option<String>, _>("active_role_source")?
                .zip(row.try_get::<Option<String>, _>("active_role_selection_id")?);
            let selected_role = if let Some((source, selection_id)) = selected {
                let source = RoleSelectionSource::parse(&source);
                self.role_options(&principal_ref, at)
                    .await?
                    .into_iter()
                    .find(|option| {
                        Some(option.source) == source && option.selection_id == selection_id
                    })
            } else {
                None
            };
            summaries.push(AccountSessionSummary {
                session_id,
                principal_ref,
                active: session_id == active_session_id,
                selected_role,
                refresh_expires_at: row.try_get("refresh_expires_at")?,
                last_used_at: row.try_get("last_used_at")?,
            });
        }
        summaries.sort_by_key(|summary| {
            session_ids
                .iter()
                .position(|id| *id == summary.session_id)
                .unwrap_or(usize::MAX)
        });
        Ok(summaries)
    }

    pub async fn revoke_session(
        &self,
        session_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        let result = sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at = COALESCE(revoked_at, $2), last_used_at = $2
             WHERE id = $1",
        )
        .bind(session_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Revoke one session only when it belongs to the authenticated principal.
    /// A mismatched principal returns `false` without revealing whether the
    /// session exists; the shared server boundary can use this method for
    /// logout and device-management commands without permitting cross-user
    /// session cleanup.
    pub async fn revoke_session_for_principal(
        &self,
        principal_ref: &str,
        session_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if session_id.is_nil() {
            return Err(IdentityStoreError::InvalidRecord(
                "session identifier is nil".into(),
            ));
        }
        let result = sqlx::query(
            "UPDATE erpx_identity.sessions s
             SET revoked_at = COALESCE(s.revoked_at, $3), last_used_at = GREATEST(s.last_used_at, $3)
             FROM erpx_identity.principals p
             WHERE s.id = $1 AND s.principal_id = p.id AND p.principal_ref = $2",
        )
        .bind(session_id)
        .bind(principal_ref)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn revoke_principal_sessions(
        &self,
        principal_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        let result = sqlx::query(
            "UPDATE erpx_identity.sessions s
             SET revoked_at = COALESCE(s.revoked_at, $2), last_used_at = $2
             FROM erpx_identity.principals p
             WHERE s.principal_id = p.id AND p.principal_ref = $1",
        )
        .bind(principal_ref)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Atomically admit an authentication-sensitive operation into a durable
    /// bucket. Only a SHA-256 digest of the namespace and identifier is
    /// persisted; raw principal references and other identifiers never reach
    /// the rate-limit table.
    pub async fn admit_rate_limit(
        &self,
        namespace: &str,
        identifier: &str,
        policy: RateLimitPolicy,
        at: DateTime<Utc>,
    ) -> Result<RateLimitDecision, IdentityStoreError> {
        policy.validate()?;
        if namespace.trim().is_empty()
            || namespace.len() > 64
            || identifier.trim().is_empty()
            || identifier.len() > 512
        {
            return Err(IdentityStoreError::InvalidRecord(
                "rate-limit bucket identifier is invalid".into(),
            ));
        }
        let bucket_digest = rate_limit_digest(namespace, identifier);
        let window = Duration::seconds(policy.window_seconds as i64);
        let block = Duration::seconds(policy.block_seconds as i64);
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_identity.auth_rate_limits
                (bucket_digest, window_started_at, attempts, blocked_until, updated_at)
             VALUES ($1, $2, 0, NULL, $2)
             ON CONFLICT (bucket_digest) DO NOTHING",
        )
        .bind(bucket_digest.as_slice())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        let row = sqlx::query(
            "SELECT window_started_at, attempts, blocked_until
             FROM erpx_identity.auth_rate_limits
             WHERE bucket_digest = $1
             FOR UPDATE",
        )
        .bind(bucket_digest.as_slice())
        .fetch_one(&mut *transaction)
        .await?;
        let stored_window_start: DateTime<Utc> = row.try_get("window_started_at")?;
        if at < stored_window_start {
            return Err(IdentityStoreError::InvalidRecord(
                "rate-limit timestamp moved backwards".into(),
            ));
        }
        let stored_attempts: i32 = row.try_get("attempts")?;
        if stored_attempts < 0 {
            return Err(IdentityStoreError::InvalidRecord(
                "stored rate-limit attempts are invalid".into(),
            ));
        }
        let stored_blocked_until: Option<DateTime<Utc>> = row.try_get("blocked_until")?;
        let window_expired = at
            >= stored_window_start
                .checked_add_signed(window)
                .ok_or_else(|| {
                    IdentityStoreError::InvalidRecord("rate-limit window overflow".into())
                })?;
        let block_expired = stored_blocked_until.is_some_and(|until| at >= until);
        let mut window_started_at = stored_window_start;
        let mut attempts = stored_attempts;
        let mut blocked_until = stored_blocked_until;
        if window_expired || block_expired {
            window_started_at = at;
            attempts = 0;
            blocked_until = None;
        }
        if blocked_until.is_some_and(|until| at < until) {
            transaction.commit().await?;
            return Ok(RateLimitDecision::Limited);
        }
        if attempts >= policy.max_attempts as i32 {
            blocked_until = Some(at.checked_add_signed(block).ok_or_else(|| {
                IdentityStoreError::InvalidRecord("rate-limit block overflow".into())
            })?);
            sqlx::query(
                "UPDATE erpx_identity.auth_rate_limits
                 SET window_started_at = $2, attempts = $3,
                     blocked_until = $4, updated_at = $5
                 WHERE bucket_digest = $1",
            )
            .bind(bucket_digest.as_slice())
            .bind(window_started_at)
            .bind(attempts)
            .bind(blocked_until)
            .bind(at)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            return Ok(RateLimitDecision::Limited);
        }
        attempts = attempts.checked_add(1).ok_or_else(|| {
            IdentityStoreError::InvalidRecord("rate-limit attempts overflow".into())
        })?;
        sqlx::query(
            "UPDATE erpx_identity.auth_rate_limits
             SET window_started_at = $2, attempts = $3,
                 blocked_until = $4, updated_at = $5
             WHERE bucket_digest = $1",
        )
        .bind(bucket_digest.as_slice())
        .bind(window_started_at)
        .bind(attempts)
        .bind(blocked_until)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(RateLimitDecision::Allowed)
    }

    pub async fn clear_rate_limit(
        &self,
        namespace: &str,
        identifier: &str,
    ) -> Result<(), IdentityStoreError> {
        if namespace.trim().is_empty()
            || namespace.len() > 64
            || identifier.trim().is_empty()
            || identifier.len() > 512
        {
            return Err(IdentityStoreError::InvalidRecord(
                "rate-limit bucket identifier is invalid".into(),
            ));
        }
        let bucket_digest = rate_limit_digest(namespace, identifier);
        sqlx::query("DELETE FROM erpx_identity.auth_rate_limits WHERE bucket_digest = $1")
            .bind(bucket_digest.as_slice())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Read the identity owner's secret-free factor posture. Counts are
    /// derived from current write-side rows and never expose credential IDs,
    /// recovery hashes, encrypted TOTP envelopes, or session material.
    pub async fn security_factor_state(
        &self,
        principal_ref: &str,
        measured_at: DateTime<Utc>,
    ) -> Result<SecurityFactorState, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let row = sqlx::query(
            "SELECT
                 EXISTS(
                     SELECT 1 FROM erpx_identity.directory_principal_bindings managed
                     WHERE managed.principal_id=p.id AND managed.active
                 ) AS directory_managed,
                 t.enabled_at AS totp_enabled_at,
                 (SELECT COUNT(*) FROM erpx_identity.passkeys passkey
                  WHERE passkey.principal_id=p.id AND passkey.revoked_at IS NULL) AS passkey_count,
                 (SELECT COUNT(*) FROM erpx_identity.recovery_codes recovery
                  WHERE recovery.principal_id=p.id) AS recovery_codes_total,
                 (SELECT COUNT(*) FROM erpx_identity.recovery_codes recovery
                  WHERE recovery.principal_id=p.id AND recovery.consumed_at IS NULL)
                    AS recovery_codes_remaining,
                 (SELECT MAX(recovery.issued_at)
                  FROM erpx_identity.recovery_codes recovery
                  WHERE recovery.principal_id=p.id) AS recovery_codes_issued_at
             FROM erpx_identity.principals p
             LEFT JOIN erpx_identity.totp_secrets t ON t.principal_id=p.id
             WHERE p.principal_ref=$1 AND p.status='active'",
        )
        .bind(principal_ref)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityStoreError::PrincipalInactive)?;
        let passkey_count = bounded_security_count(row.try_get("passkey_count")?)?;
        let recovery_codes_total = bounded_security_count(row.try_get("recovery_codes_total")?)?;
        let recovery_codes_remaining =
            bounded_security_count(row.try_get("recovery_codes_remaining")?)?;
        if recovery_codes_remaining > recovery_codes_total {
            return Err(IdentityStoreError::InvalidRecord(
                "recovery code counts are inconsistent".into(),
            ));
        }
        Ok(SecurityFactorState {
            directory_managed: row.try_get("directory_managed")?,
            totp_enabled_at: row.try_get("totp_enabled_at")?,
            passkey_count,
            recovery_codes_total,
            recovery_codes_remaining,
            recovery_codes_issued_at: row.try_get("recovery_codes_issued_at")?,
            measured_at,
        })
    }

    /// Rotate all recovery codes only after a current, one-time TOTP proof.
    /// The factor verification, hash replacement, and append-only command
    /// receipt commit atomically. Exact replay returns a receipt without
    /// reconstructing plaintext recovery material.
    pub async fn rotate_recovery_codes_with_totp<K: KeyResolver>(
        &self,
        principal_ref: &str,
        count: usize,
        presented_code: &str,
        keys: &K,
        command: &SecurityCommandContext,
        at: DateTime<Utc>,
    ) -> Result<RecoveryCodeRotationOutcome, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_totp_code_input(presented_code)?;
        let batch = RecoveryCodeSet::issue(principal_ref, count, &PasswordService::default(), at)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        if let Some(occurred_at) = security_command_replay_time(
            &mut transaction,
            principal_id,
            "recovery_codes_rotate",
            None,
            command,
        )
        .await?
        {
            transaction.commit().await?;
            return Ok(RecoveryCodeRotationOutcome {
                batch: None,
                replayed: true,
                occurred_at,
            });
        }
        verify_totp_in_transaction(&mut transaction, principal_ref, presented_code, keys, at)
            .await?;
        sqlx::query("DELETE FROM erpx_identity.recovery_codes WHERE principal_id=$1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await?;
        for record in batch.set().records() {
            sqlx::query(
                "INSERT INTO erpx_identity.recovery_codes
                    (id, principal_id, code_hash, issued_at, consumed_at)
                 VALUES ($1, $2, $3, $4, NULL)",
            )
            .bind(record.id())
            .bind(principal_id)
            .bind(record.hash().as_str())
            .bind(batch.set().issued_at())
            .execute(&mut *transaction)
            .await?;
        }
        insert_security_command_audit(
            &mut transaction,
            principal_id,
            "recovery_codes_rotate",
            None,
            command,
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(RecoveryCodeRotationOutcome {
            batch: Some(batch),
            replayed: false,
            occurred_at: at,
        })
    }

    /// Remove the active TOTP factor only after verifying that same factor in
    /// the command transaction. An exact idempotent replay succeeds without
    /// requiring or accepting another code.
    pub async fn remove_totp_with_proof<K: KeyResolver>(
        &self,
        principal_ref: &str,
        presented_code: &str,
        keys: &K,
        command: &SecurityCommandContext,
        at: DateTime<Utc>,
    ) -> Result<TotpRemovalOutcome, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_totp_code_input(presented_code)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        if let Some(occurred_at) = security_command_replay_time(
            &mut transaction,
            principal_id,
            "totp_remove",
            None,
            command,
        )
        .await?
        {
            transaction.commit().await?;
            return Ok(TotpRemovalOutcome {
                replayed: true,
                occurred_at,
            });
        }
        verify_totp_in_transaction(&mut transaction, principal_ref, presented_code, keys, at)
            .await?;
        let removed = sqlx::query("DELETE FROM erpx_identity.totp_secrets WHERE principal_id=$1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await?;
        if removed.rows_affected() != 1 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::TotpEnrollmentInvalid,
            ));
        }
        sqlx::query(
            "DELETE FROM erpx_identity.totp_enrollments
             WHERE principal_id=$1 AND verified_at IS NULL",
        )
        .bind(principal_id)
        .execute(&mut *transaction)
        .await?;
        insert_security_command_audit(
            &mut transaction,
            principal_id,
            "totp_remove",
            None,
            command,
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(TotpRemovalOutcome {
            replayed: false,
            occurred_at: at,
        })
    }

    /// Issue a new recovery-code batch and atomically replace all previous
    /// hashes for the principal. The returned batch contains the plaintext
    /// codes exactly once; only Argon2id digests are written to PostgreSQL.
    pub async fn issue_recovery_codes(
        &self,
        principal_ref: &str,
        count: usize,
        at: DateTime<Utc>,
    ) -> Result<RecoveryCodeBatch, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let batch = RecoveryCodeSet::issue(principal_ref, count, &PasswordService::default(), at)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_recovery_principal(&mut transaction, principal_ref)
            .await?;

        sqlx::query("DELETE FROM erpx_identity.recovery_codes WHERE principal_id = $1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await?;
        for record in batch.set().records() {
            sqlx::query(
                "INSERT INTO erpx_identity.recovery_codes
                    (id, principal_id, code_hash, issued_at, consumed_at)
                 VALUES ($1, $2, $3, $4, NULL)",
            )
            .bind(record.id())
            .bind(principal_id)
            .bind(record.hash().as_str())
            .bind(batch.set().issued_at())
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(batch)
    }

    /// Atomically consume a recovery code. Candidate rows are locked before
    /// verification, and the final update retains a NULL consumed-at guard so
    /// concurrent requests cannot consume the same code twice.
    pub async fn consume_recovery_code(
        &self,
        principal_ref: &str,
        presented_code: &str,
        at: DateTime<Utc>,
    ) -> Result<Uuid, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if presented_code.trim().is_empty() || presented_code.len() > 512 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidRecoveryCode,
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT rc.id, rc.code_hash, rc.consumed_at
             FROM erpx_identity.recovery_codes rc
             JOIN erpx_identity.principals p ON p.id = rc.principal_id
             WHERE p.principal_ref = $1
               AND p.status IN ('active', 'recovery_required')
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             ORDER BY rc.id
             FOR UPDATE",
        )
        .bind(principal_ref)
        .fetch_all(&mut *transaction)
        .await?;
        if rows.is_empty() {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidRecoveryCode,
            ));
        }

        let verifier = PasswordService::default();
        let mut matching_id = None;
        let mut matching_consumed = false;
        for row in rows {
            let digest_text: String = row.try_get("code_hash")?;
            let digest = crate::PasswordDigest::from_stored(digest_text).map_err(|_| {
                IdentityStoreError::InvalidRecord("stored recovery-code hash is invalid".into())
            })?;
            if verifier.verify(presented_code, &digest).is_ok() && matching_id.is_none() {
                matching_id = Some(row.try_get("id")?);
                matching_consumed = row
                    .try_get::<Option<DateTime<Utc>>, _>("consumed_at")?
                    .is_some();
            }
        }

        let code_id = matching_id.ok_or(IdentityStoreError::Primitive(
            IdentityError::InvalidRecoveryCode,
        ))?;
        if matching_consumed {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoveryCodeAlreadyConsumed,
            ));
        }
        let updated = sqlx::query(
            "UPDATE erpx_identity.recovery_codes
             SET consumed_at = $2
             WHERE id = $1 AND consumed_at IS NULL",
        )
        .bind(code_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoveryCodeAlreadyConsumed,
            ));
        }
        transaction.commit().await?;
        Ok(code_id)
    }

    /// Start a durable TOTP enrollment. The generated secret is returned only
    /// in the ephemeral result; the write-side row contains an AES-GCM
    /// envelope and no plaintext secret. A new pending enrollment replaces
    /// older pending enrollments for the same principal.
    #[allow(clippy::too_many_arguments)]
    pub async fn begin_totp_enrollment<K: KeyResolver>(
        &self,
        principal_ref: &str,
        keys: &K,
        key_reference: &str,
        issuer: &str,
        account: &str,
        at: DateTime<Utc>,
        lifetime: Duration,
    ) -> Result<TotpEnrollmentStart, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_enrollment_lifetime(lifetime)?;
        if issuer.trim().is_empty()
            || issuer.len() > 128
            || account.trim().is_empty()
            || account.len() > 256
        {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidIdentityReference,
            ));
        }
        let secret = TotpSecret::generate()?;
        let id = Uuid::new_v4();
        let expires_at = at
            .checked_add_signed(lifetime)
            .ok_or(IdentityError::InvalidTotpEnrollmentLifetime)?;
        let envelope = encrypt_totp_secret(keys, key_reference, principal_ref, secret.as_bytes())?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        sqlx::query(
            "DELETE FROM erpx_identity.totp_enrollments
             WHERE principal_id = $1 AND verified_at IS NULL",
        )
        .bind(principal_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.totp_enrollments
                (id, principal_id, key_reference, nonce, ciphertext,
                 issued_at, expires_at, verified_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)",
        )
        .bind(id)
        .bind(principal_id)
        .bind(envelope.key_reference())
        .bind(envelope.nonce().as_slice())
        .bind(envelope.ciphertext())
        .bind(at)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        TotpEnrollmentStart::new(
            id,
            principal_ref.to_owned(),
            secret,
            issuer,
            account,
            at,
            expires_at,
        )
        .map_err(IdentityStoreError::Primitive)
    }

    /// Verify a pending enrollment and atomically promote its encrypted
    /// secret to the active TOTP row. The enrollment row is retained as a
    /// non-sensitive audit marker and can never be verified twice.
    pub async fn verify_totp_enrollment<K: KeyResolver>(
        &self,
        principal_ref: &str,
        enrollment_id: Uuid,
        presented_code: &str,
        keys: &K,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_totp_code_input(presented_code)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT e.id, e.key_reference, e.nonce, e.ciphertext,
                    e.issued_at, e.expires_at, e.verified_at,
                    p.principal_ref
             FROM erpx_identity.totp_enrollments e
             JOIN erpx_identity.principals p ON p.id = e.principal_id
             WHERE p.principal_ref = $1
               AND p.status = 'active'
               AND e.id = $2
             FOR UPDATE",
        )
        .bind(principal_ref)
        .bind(enrollment_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::Primitive(
            IdentityError::TotpEnrollmentInvalid,
        ))?;
        let verified_at: Option<DateTime<Utc>> = row.try_get("verified_at")?;
        let issued_at: DateTime<Utc> = row.try_get("issued_at")?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        if verified_at.is_some() || at < issued_at || at >= expires_at {
            return Err(IdentityStoreError::Primitive(
                IdentityError::TotpEnrollmentInvalid,
            ));
        }
        let envelope = totp_envelope_from_parts(
            row.try_get("key_reference")?,
            row.try_get("principal_ref")?,
            row.try_get("nonce")?,
            row.try_get("ciphertext")?,
        )?;
        let secret = decrypt_totp_secret(keys, principal_ref, &envelope)?;
        let step = verify_totp_code(secret.as_bytes(), presented_code, at, None)?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.totp_secrets
                (principal_id, key_reference, nonce, ciphertext, updated_at,
                 algorithm, digits, period_seconds, enabled_at, last_verified_step)
             VALUES ($1, $2, $3, $4, $5, 'SHA256', 6, 30, $5, $6)
             ON CONFLICT (principal_id) DO UPDATE SET
                 key_reference = EXCLUDED.key_reference,
                 nonce = EXCLUDED.nonce,
                 ciphertext = EXCLUDED.ciphertext,
                 updated_at = EXCLUDED.updated_at,
                 algorithm = EXCLUDED.algorithm,
                 digits = EXCLUDED.digits,
                 period_seconds = EXCLUDED.period_seconds,
                 enabled_at = EXCLUDED.enabled_at,
                 last_verified_step = EXCLUDED.last_verified_step",
        )
        .bind(principal_id)
        .bind(envelope.key_reference())
        .bind(envelope.nonce().as_slice())
        .bind(envelope.ciphertext())
        .bind(at)
        .bind(step)
        .execute(&mut *transaction)
        .await?;
        let updated = sqlx::query(
            "UPDATE erpx_identity.totp_enrollments
             SET verified_at = $3
             WHERE id = $1 AND principal_id = $2 AND verified_at IS NULL",
        )
        .bind(enrollment_id)
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::TotpEnrollmentInvalid,
            ));
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Verify an already-enabled TOTP factor against the write-side secret.
    /// The last accepted time step is updated under the same row lock, making
    /// a valid code one-time even when two requests arrive concurrently.
    pub async fn verify_totp<K: KeyResolver>(
        &self,
        principal_ref: &str,
        presented_code: &str,
        keys: &K,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_totp_code_input(presented_code)?;
        let mut transaction = self.pool.begin().await?;
        verify_totp_in_transaction(&mut transaction, principal_ref, presented_code, keys, at)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Consume one recovery code and create a short-lived, one-time opaque
    /// handoff. This is intentionally separate from `consume_recovery_code`
    /// so the code consumption and handoff insertion share one transaction.
    pub async fn initiate_recovery_session(
        &self,
        principal_ref: &str,
        presented_code: &str,
        at: DateTime<Utc>,
        lifetime: Duration,
    ) -> Result<RecoverySession, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_recovery_code_input(presented_code)?;
        let session = RecoverySession::issue(principal_ref, at, lifetime)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_recovery_principal(&mut transaction, principal_ref)
            .await?;
        let rows = sqlx::query(
            "SELECT id, code_hash, consumed_at
             FROM erpx_identity.recovery_codes
             WHERE principal_id = $1
             ORDER BY id
             FOR UPDATE",
        )
        .bind(principal_id)
        .fetch_all(&mut *transaction)
        .await?;
        let verifier = PasswordService::default();
        let mut matching_id = None;
        let mut matching_consumed = false;
        for row in rows {
            let digest_text: String = row.try_get("code_hash")?;
            let digest = crate::PasswordDigest::from_stored(digest_text).map_err(|_| {
                IdentityStoreError::InvalidRecord("stored recovery-code hash is invalid".into())
            })?;
            if verifier.verify(presented_code, &digest).is_ok() && matching_id.is_none() {
                matching_id = Some(row.try_get::<Uuid, _>("id")?);
                matching_consumed = row
                    .try_get::<Option<DateTime<Utc>>, _>("consumed_at")?
                    .is_some();
            }
        }
        let code_id = matching_id.ok_or(IdentityStoreError::Primitive(
            IdentityError::InvalidRecoveryCode,
        ))?;
        if matching_consumed {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoveryCodeAlreadyConsumed,
            ));
        }
        let updated = sqlx::query(
            "UPDATE erpx_identity.recovery_codes
             SET consumed_at = $2
             WHERE id = $1 AND consumed_at IS NULL",
        )
        .bind(code_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoveryCodeAlreadyConsumed,
            ));
        }
        sqlx::query(
            "UPDATE erpx_identity.recovery_sessions
             SET consumed_at = COALESCE(consumed_at, $2)
             WHERE principal_id = $1 AND consumed_at IS NULL",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.recovery_sessions
                (id, principal_id, token_hash, issued_at, expires_at, consumed_at)
             VALUES ($1, $2, $3, $4, $5, NULL)",
        )
        .bind(session.id())
        .bind(principal_id)
        .bind(session.token_hash().as_bytes().as_slice())
        .bind(session.issued_at())
        .bind(session.expires_at())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(session)
    }

    /// Exchange a recovery handoff for a fresh device/session. The handoff
    /// is consumed, all previous sessions are revoked, and the new device and
    /// session are inserted atomically. A recovery-required principal returns
    /// to active status only as part of this successful exchange.
    pub async fn consume_recovery_session(
        &self,
        signer: &AccessTokenSigner,
        presented_token: &str,
        device_label: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::IssuedSession, IdentityStoreError> {
        if presented_token.trim().is_empty() || presented_token.len() > 512 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoverySessionInvalid,
            ));
        }
        validate_device_label(device_label)?;
        let token_hash = TokenHash::of(presented_token);
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT rs.id, rs.issued_at, rs.expires_at, rs.consumed_at,
                    p.id AS principal_id, p.principal_ref, p.status,
                    p.authenticated_realm, p.principal_kind
             FROM erpx_identity.recovery_sessions rs
             JOIN erpx_identity.principals p ON p.id = rs.principal_id
             WHERE rs.token_hash = $1
               AND p.status IN ('active', 'recovery_required')
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             FOR UPDATE",
        )
        .bind(token_hash.as_bytes().as_slice())
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::Primitive(
            IdentityError::RecoverySessionInvalid,
        ))?;
        let issued_at: DateTime<Utc> = row.try_get("issued_at")?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let consumed_at: Option<DateTime<Utc>> = row.try_get("consumed_at")?;
        if consumed_at.is_some() || at < issued_at || at >= expires_at {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoverySessionInvalid,
            ));
        }
        let principal_id: Uuid = row.try_get("principal_id")?;
        let principal_ref: String = row.try_get("principal_ref")?;
        let authority = authority_from_row(&row, "authenticated_realm", "principal_kind")?;
        let device_id = Uuid::new_v4();
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let issued = service.issue_with_authority(
            principal_ref,
            device_id,
            authority,
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at = COALESCE(revoked_at, $2),
                 last_used_at = GREATEST(last_used_at, $2)
             WHERE principal_id = $1 AND revoked_at IS NULL",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at, revoked_at)
             VALUES ($1, $2, $3, $4, $4, NULL)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(device_label)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        insert_session_in_transaction(&mut transaction, principal_id, &issued).await?;
        sqlx::query(
            "UPDATE erpx_identity.recovery_sessions
             SET consumed_at = $2
             WHERE token_hash = $1 AND consumed_at IS NULL",
        )
        .bind(token_hash.as_bytes().as_slice())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_identity.principals
             SET status = 'active', updated_at = $2
             WHERE id = $1 AND status = 'recovery_required'",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(issued)
    }

    /// Replace a principal password through a valid, one-time recovery
    /// handoff. The token is consumed, previous sessions are revoked, and a
    /// fresh device/session is created in one write-side transaction.
    pub async fn reset_password_with_recovery_session(
        &self,
        signer: &AccessTokenSigner,
        presented_token: &str,
        new_password: &str,
        device_label: &str,
        at: DateTime<Utc>,
    ) -> Result<crate::IssuedSession, IdentityStoreError> {
        if presented_token.trim().is_empty() || presented_token.len() > 512 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoverySessionInvalid,
            ));
        }
        validate_password_input(new_password)?;
        validate_device_label(device_label)?;
        let digest = PasswordService::default().hash(new_password)?;
        let token_hash = TokenHash::of(presented_token);
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT rs.issued_at, rs.expires_at, rs.consumed_at,
                    p.id AS principal_id, p.principal_ref, p.status,
                    p.authenticated_realm, p.principal_kind
             FROM erpx_identity.recovery_sessions rs
             JOIN erpx_identity.principals p ON p.id = rs.principal_id
             WHERE rs.token_hash = $1
               AND p.status IN ('active', 'recovery_required')
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             FOR UPDATE",
        )
        .bind(token_hash.as_bytes().as_slice())
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::Primitive(
            IdentityError::RecoverySessionInvalid,
        ))?;
        let issued_at: DateTime<Utc> = row.try_get("issued_at")?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let consumed_at: Option<DateTime<Utc>> = row.try_get("consumed_at")?;
        if consumed_at.is_some() || at < issued_at || at >= expires_at {
            return Err(IdentityStoreError::Primitive(
                IdentityError::RecoverySessionInvalid,
            ));
        }
        let principal_id: Uuid = row.try_get("principal_id")?;
        let principal_ref: String = row.try_get("principal_ref")?;
        let authority = authority_from_row(&row, "authenticated_realm", "principal_kind")?;
        let device_id = Uuid::new_v4();
        let service = SessionService::new(signer.clone(), REFRESH_LIFETIME)?;
        let issued = service.issue_with_authority(
            principal_ref,
            device_id,
            authority,
            at,
            AccessTokenLifetime::five_minutes(),
        )?;
        sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at = COALESCE(revoked_at, $2),
                 last_used_at = GREATEST(last_used_at, $2)
             WHERE principal_id = $1 AND revoked_at IS NULL",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.password_credentials
                (principal_id, argon2id_digest, changed_at)
             VALUES ($1, $2, $3)
             ON CONFLICT (principal_id) DO UPDATE
             SET argon2id_digest = EXCLUDED.argon2id_digest,
                 changed_at = EXCLUDED.changed_at",
        )
        .bind(principal_id)
        .bind(digest.as_str())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.devices
                (id, principal_id, label, created_at, last_seen_at, revoked_at)
             VALUES ($1, $2, $3, $4, $4, NULL)",
        )
        .bind(device_id)
        .bind(principal_id)
        .bind(device_label)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        insert_session_in_transaction(&mut transaction, principal_id, &issued).await?;
        sqlx::query(
            "UPDATE erpx_identity.recovery_sessions
             SET consumed_at = $2
             WHERE token_hash = $1 AND consumed_at IS NULL",
        )
        .bind(token_hash.as_bytes().as_slice())
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_identity.principals
             SET status = 'active', updated_at = $2
             WHERE id = $1 AND status = 'recovery_required'",
        )
        .bind(principal_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(issued)
    }

    /// Issue and persist a WebAuthn challenge atomically with an active
    /// principal check. Only the digest is written; the material is returned
    /// once to the ceremony adapter for the browser challenge response.
    pub async fn issue_webauthn_challenge(
        &self,
        principal_ref: &str,
        ceremony: ChallengeCeremony,
        lifetime: ChallengeLifetime,
        at: DateTime<Utc>,
    ) -> Result<(WebAuthnChallenge, ChallengeMaterial), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let (challenge, material) =
            WebAuthnChallenge::issue(principal_ref, ceremony, at, lifetime)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.webauthn_challenges
                (id, principal_id, ceremony, challenge_digest,
                 issued_at, expires_at, consumed_at)
             VALUES ($1, $2, $3, $4, $5, $6, NULL)",
        )
        .bind(challenge.id)
        .bind(principal_id)
        .bind(challenge_ceremony_name(ceremony))
        .bind(challenge.digest().as_slice())
        .bind(challenge.issued_at)
        .bind(challenge.expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok((challenge, material))
    }

    /// Persist provider ceremony state server-side. Browser clients receive
    /// only provider options; the opaque state is never returned to them.
    pub async fn save_webauthn_provider_state(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        ceremony: ChallengeCeremony,
        provider_state: &[u8],
    ) -> Result<(), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if provider_state.is_empty() || provider_state.len() > 128 * 1024 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidCredential,
            ));
        }
        let updated = sqlx::query(
            "UPDATE erpx_identity.webauthn_challenges c
             SET provider_state = $4
             FROM erpx_identity.principals p
             WHERE c.principal_id = p.id
               AND p.principal_ref = $1
               AND p.status = 'active'
               AND c.id = $2
               AND c.ceremony = $3
               AND c.consumed_at IS NULL",
        )
        .bind(principal_ref)
        .bind(challenge_id)
        .bind(challenge_ceremony_name(ceremony))
        .bind(provider_state)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(IdentityStoreError::InvalidCredentials);
        }
        Ok(())
    }

    pub async fn load_webauthn_provider_state(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        ceremony: ChallengeCeremony,
    ) -> Result<Vec<u8>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let provider_state: Vec<u8> = sqlx::query_scalar(
            "SELECT c.provider_state
             FROM erpx_identity.webauthn_challenges c
             JOIN erpx_identity.principals p ON p.id = c.principal_id
             WHERE p.principal_ref = $1
               AND p.status = 'active'
               AND c.id = $2
               AND c.ceremony = $3
               AND c.consumed_at IS NULL
               AND c.provider_state IS NOT NULL",
        )
        .bind(principal_ref)
        .bind(challenge_id)
        .bind(challenge_ceremony_name(ceremony))
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        if provider_state.is_empty() || provider_state.len() > 128 * 1024 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidCredential,
            ));
        }
        Ok(provider_state)
    }

    /// Load a challenge through its principal and exact ceremony binding.
    /// This does not consume it; the finish operation must call
    /// `consume_webauthn_challenge` after provider verification.
    pub async fn load_webauthn_challenge(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        ceremony: ChallengeCeremony,
    ) -> Result<WebAuthnChallenge, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let row = sqlx::query(
            "SELECT c.id, p.principal_ref, c.ceremony, c.challenge_digest,
                    c.issued_at, c.expires_at, c.consumed_at
             FROM erpx_identity.webauthn_challenges c
             JOIN erpx_identity.principals p ON p.id = c.principal_id
             WHERE p.principal_ref = $1
               AND p.status = 'active'
               AND c.id = $2",
        )
        .bind(principal_ref)
        .bind(challenge_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        challenge_from_row(row, ceremony)
    }

    /// Atomically consume a persisted challenge before provider work. Row
    /// locking makes concurrent finishes resolve to one winner; expiry,
    /// replay, principal, ceremony, and digest checks remain fail-closed
    /// through the shared challenge primitive.
    pub async fn consume_webauthn_challenge(
        &self,
        principal_ref: &str,
        challenge_id: Uuid,
        ceremony: ChallengeCeremony,
        material: &[u8],
        at: DateTime<Utc>,
    ) -> Result<WebAuthnChallenge, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT c.id, p.principal_ref, c.ceremony, c.challenge_digest,
                    c.issued_at, c.expires_at, c.consumed_at
             FROM erpx_identity.webauthn_challenges c
             JOIN erpx_identity.principals p ON p.id = c.principal_id
             WHERE p.principal_ref = $1
               AND p.status = 'active'
               AND c.id = $2
             FOR UPDATE",
        )
        .bind(principal_ref)
        .bind(challenge_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let challenge = challenge_from_row(row, ceremony)?;
        let consumed = challenge.consume_for(principal_ref, ceremony, material, at)?;
        let updated = sqlx::query(
            "UPDATE erpx_identity.webauthn_challenges
             SET consumed_at = $2
             WHERE id = $1 AND consumed_at IS NULL",
        )
        .bind(challenge_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(IdentityStoreError::Primitive(
                IdentityError::ChallengeReplay,
            ));
        }
        transaction.commit().await?;
        Ok(consumed)
    }

    /// Persist a provider-verified passkey while binding it to an active
    /// principal. Credential bytes and public keys are required by the
    /// provider boundary; the credential-ID digest is stored alongside them
    /// for indexed, principal-scoped lookup.
    pub async fn register_passkey(
        &self,
        principal_ref: &str,
        credential: &crate::PasskeyCredential,
    ) -> Result<crate::PasskeyCredential, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if credential.principal_ref() != principal_ref {
            return Err(IdentityStoreError::Primitive(
                IdentityError::InvalidCredential,
            ));
        }
        credential.validate_for_persistence()?;
        let credential_id_hash = passkey_credential_id_hash(credential.credential_id());
        let sign_count = passkey_count_to_i32(credential.sign_count())?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        let result = sqlx::query(
            "INSERT INTO erpx_identity.passkeys
                (id, principal_id, credential_id, credential_id_hash,
                 public_key, rp_id, sign_count, created_at, last_used_at, revoked_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(credential.id())
        .bind(principal_id)
        .bind(credential.credential_id())
        .bind(credential_id_hash.as_slice())
        .bind(credential.public_key())
        .bind(credential.rp_id())
        .bind(sign_count)
        .bind(credential.created_at())
        .bind(credential.last_used_at())
        .bind(credential.revoked_at())
        .execute(&mut *transaction)
        .await;
        match result {
            Ok(_) => {
                transaction.commit().await?;
                Ok(credential.clone())
            }
            Err(error) if is_unique_violation(&error) => {
                Err(IdentityStoreError::PasskeyAlreadyRegistered)
            }
            Err(error) => Err(IdentityStoreError::Database(error)),
        }
    }

    /// Find a passkey only through its principal binding and credential-ID
    /// digest/raw-byte pair. Legacy rows without the new binding columns are
    /// intentionally not accepted by this lookup and therefore fail closed.
    pub async fn find_passkey(
        &self,
        principal_ref: &str,
        credential_id: &[u8],
    ) -> Result<Option<crate::PasskeyCredential>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_passkey_credential_id(credential_id)?;
        let credential_id_hash = passkey_credential_id_hash(credential_id);
        let row = sqlx::query(
            "SELECT p.id, principal.principal_ref, p.credential_id,
                    p.credential_id_hash, p.public_key, p.rp_id,
                    p.sign_count, p.created_at, p.last_used_at, p.revoked_at
             FROM erpx_identity.passkeys p
             JOIN erpx_identity.principals principal
               ON principal.id = p.principal_id
             WHERE principal.principal_ref = $1
               AND p.credential_id_hash = $2
               AND p.credential_id = $3",
        )
        .bind(principal_ref)
        .bind(credential_id_hash.as_slice())
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| passkey_from_row(row, credential_id, &credential_id_hash))
            .transpose()
    }

    /// Resolve the most recently used active passkey for a principal without
    /// disclosing credential identifiers to a pre-authentication client.
    ///
    /// Browser authentication can start from a principal reference alone; the
    /// returned WebAuthn options still bind the ceremony to exactly one stored
    /// credential and the finish step rechecks the browser-selected raw ID.
    pub async fn most_recent_active_passkey(
        &self,
        principal_ref: &str,
    ) -> Result<Option<crate::PasskeyCredential>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let row = sqlx::query(
            "SELECT p.id, principal.principal_ref, p.credential_id,
                    p.credential_id_hash, p.public_key, p.rp_id,
                    p.sign_count, p.created_at, p.last_used_at, p.revoked_at
             FROM erpx_identity.passkeys p
             JOIN erpx_identity.principals principal
               ON principal.id = p.principal_id
             WHERE principal.principal_ref = $1
               AND p.revoked_at IS NULL
               AND p.credential_id_hash IS NOT NULL
               AND p.rp_id IS NOT NULL
             ORDER BY p.last_used_at DESC NULLS LAST, p.created_at DESC, p.id DESC
             LIMIT 1",
        )
        .bind(principal_ref)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let credential_id: Vec<u8> = row.try_get("credential_id")?;
            let credential_id_hash = passkey_credential_id_hash(&credential_id);
            passkey_from_row(row, &credential_id, &credential_id_hash)
        })
        .transpose()
    }

    /// Revoke a passkey idempotently within its principal scope. A revoked
    /// passkey remains queryable for audit but cannot be used for CAS updates.
    pub async fn revoke_passkey(
        &self,
        principal_ref: &str,
        credential_id: &[u8],
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_passkey_credential_id(credential_id)?;
        let credential_id_hash = passkey_credential_id_hash(credential_id);
        let result = sqlx::query(
            "UPDATE erpx_identity.passkeys p
             SET revoked_at = COALESCE(p.revoked_at, $4)
             FROM erpx_identity.principals principal
             WHERE p.principal_id = principal.id
               AND principal.principal_ref = $1
               AND p.credential_id_hash = $2
               AND p.credential_id = $3",
        )
        .bind(principal_ref)
        .bind(credential_id_hash.as_slice())
        .bind(credential_id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Atomically update a passkey counter only when the stored counter still
    /// equals `expected_count`. Concurrent stale writers are rejected rather
    /// than allowed to move a counter backwards or overwrite a newer value.
    pub async fn compare_and_set_passkey_counter(
        &self,
        principal_ref: &str,
        credential_id: &[u8],
        expected_count: u32,
        next_count: u32,
        at: DateTime<Utc>,
    ) -> Result<crate::PasskeyCredential, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        validate_passkey_credential_id(credential_id)?;
        if next_count < expected_count {
            return Err(IdentityStoreError::Primitive(
                IdentityError::CredentialCounterRollback,
            ));
        }
        let expected_count = passkey_count_to_i32(expected_count)?;
        let next_count = passkey_count_to_i32(next_count)?;
        let credential_id_hash = passkey_credential_id_hash(credential_id);
        let row = sqlx::query(
            "UPDATE erpx_identity.passkeys p
             SET sign_count = $4, last_used_at = $5
             FROM erpx_identity.principals principal
             WHERE p.principal_id = principal.id
               AND principal.principal_ref = $1
               AND p.credential_id_hash = $2
               AND p.credential_id = $3
               AND p.revoked_at IS NULL
               AND p.sign_count = $6
             RETURNING p.id, principal.principal_ref, p.credential_id,
                       p.credential_id_hash, p.public_key, p.rp_id,
                       p.sign_count, p.created_at, p.last_used_at, p.revoked_at",
        )
        .bind(principal_ref)
        .bind(credential_id_hash.as_slice())
        .bind(credential_id)
        .bind(next_count)
        .bind(at)
        .bind(expected_count)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(row) = row {
            return passkey_from_row(row, credential_id, &credential_id_hash);
        }

        let current = self
            .find_passkey(principal_ref, credential_id)
            .await?
            .ok_or(IdentityStoreError::PasskeyNotFound)?;
        if current.revoked_at().is_some() {
            return Err(IdentityStoreError::PasskeyRevoked);
        }
        if current.sign_count() > u32::try_from(expected_count).unwrap_or(u32::MAX) {
            return Err(IdentityStoreError::Primitive(
                IdentityError::CredentialCounterRollback,
            ));
        }
        Err(IdentityStoreError::Primitive(
            IdentityError::CredentialCounterRollback,
        ))
    }

    /// List device metadata without exposing refresh-token or credential
    /// material. Results are scoped by principal reference in the query.
    pub async fn list_devices(
        &self,
        principal_ref: &str,
    ) -> Result<Vec<DeviceSnapshot>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let rows = sqlx::query(
            "SELECT d.id, p.principal_ref, d.label, d.created_at,
                    d.last_seen_at, d.revoked_at
             FROM erpx_identity.devices d
             JOIN erpx_identity.principals p ON p.id = d.principal_id
             WHERE p.principal_ref = $1
             ORDER BY d.created_at, d.id",
        )
        .bind(principal_ref)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(DeviceSnapshot {
                    id: row.try_get("id")?,
                    principal_ref: row.try_get("principal_ref")?,
                    label: row.try_get("label")?,
                    created_at: row.try_get("created_at")?,
                    last_seen_at: row.try_get("last_seen_at")?,
                    revoked_at: row.try_get("revoked_at")?,
                })
            })
            .collect()
    }

    /// Return a bounded, principal-scoped page ordered by the stable device
    /// identifier. Fetching one extra row lets callers emit a cursor without
    /// inventing totals or loading the complete device history.
    pub async fn list_devices_page(
        &self,
        principal_ref: &str,
        after_device_id: Option<Uuid>,
        current_device_id: Uuid,
        page_size: u16,
    ) -> Result<(Vec<DeviceSnapshot>, Option<Uuid>), IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        if page_size == 0 || page_size > 100 {
            return Err(IdentityStoreError::InvalidRecord(
                "device page size must be between 1 and 100".into(),
            ));
        }
        let fetch_limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT d.id, p.principal_ref, d.label, d.created_at,
                    d.last_seen_at, d.revoked_at
             FROM erpx_identity.devices d
             JOIN erpx_identity.principals p ON p.id = d.principal_id
             WHERE p.principal_ref = $1
               AND ($2::uuid IS NULL OR (d.id > $2 AND d.id <> $3))
             ORDER BY CASE WHEN $2::uuid IS NULL AND d.id = $3 THEN 0 ELSE 1 END,
                      d.id
             LIMIT $4",
        )
        .bind(principal_ref)
        .bind(after_device_id)
        .bind(current_device_id)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;
        let mut devices = rows
            .into_iter()
            .map(|row| {
                Ok(DeviceSnapshot {
                    id: row.try_get("id")?,
                    principal_ref: row.try_get("principal_ref")?,
                    label: row.try_get("label")?,
                    created_at: row.try_get("created_at")?,
                    last_seen_at: row.try_get("last_seen_at")?,
                    revoked_at: row.try_get("revoked_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let has_more = devices.len() > usize::from(page_size);
        if has_more {
            devices.pop();
        }
        let next_device_id = has_more.then(|| {
            devices
                .last()
                .expect("a page with an extra row retains at least one device")
                .id()
        });
        Ok((devices, next_device_id))
    }

    /// Revoke a device and all sessions attached to it in one transaction.
    /// The principal predicate prevents cross-principal revocation by ID.
    pub async fn revoke_device(
        &self,
        principal_ref: &str,
        device_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        Ok(self
            .revoke_device_with_outcome(principal_ref, device_id, at)
            .await?
            .is_some())
    }

    /// Revoke a device and every attached session while distinguishing a
    /// successful replay from the first state transition.
    pub async fn revoke_device_with_outcome(
        &self,
        principal_ref: &str,
        device_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Option<DeviceRevocationOutcome>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT d.revoked_at
             FROM erpx_identity.devices d
             JOIN erpx_identity.principals p ON p.id = d.principal_id
             WHERE d.id = $1 AND p.principal_ref = $2
             FOR UPDATE",
        )
        .bind(device_id)
        .bind(principal_ref)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(existing) = existing else {
            transaction.commit().await?;
            return Ok(None);
        };
        let existing_revoked_at = existing.try_get::<Option<DateTime<Utc>>, _>("revoked_at")?;
        let replayed = existing_revoked_at.is_some();
        sqlx::query(
            "UPDATE erpx_identity.devices
             SET revoked_at = COALESCE(revoked_at, $2)
             WHERE id = $1",
        )
        .bind(device_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at = COALESCE(revoked_at, $2),
                 last_used_at = GREATEST(last_used_at, $2)
             WHERE device_id = $1 AND revoked_at IS NULL",
        )
        .bind(device_id)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(Some(DeviceRevocationOutcome {
            replayed,
            occurred_at: existing_revoked_at.unwrap_or(at),
        }))
    }

    /// Audited delivery path for device revocation. The device and attached
    /// sessions transition in the same transaction as the append-only
    /// idempotency receipt.
    pub async fn revoke_device_command(
        &self,
        principal_ref: &str,
        device_id: Uuid,
        command: &SecurityCommandContext,
        at: DateTime<Utc>,
    ) -> Result<Option<DeviceRevocationOutcome>, IdentityStoreError> {
        validate_principal_ref(principal_ref)?;
        let mut transaction = self.pool.begin().await?;
        let principal_id = self
            .lock_active_principal(&mut transaction, principal_ref)
            .await?;
        if let Some(occurred_at) = security_command_replay_time(
            &mut transaction,
            principal_id,
            "device_revoke",
            Some(device_id),
            command,
        )
        .await?
        {
            transaction.commit().await?;
            return Ok(Some(DeviceRevocationOutcome {
                replayed: true,
                occurred_at,
            }));
        }
        let existing = sqlx::query(
            "SELECT revoked_at FROM erpx_identity.devices
             WHERE id=$1 AND principal_id=$2 FOR UPDATE",
        )
        .bind(device_id)
        .bind(principal_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(existing) = existing else {
            transaction.commit().await?;
            return Ok(None);
        };
        let existing_revoked_at = existing.try_get::<Option<DateTime<Utc>>, _>("revoked_at")?;
        let replayed = existing_revoked_at.is_some();
        sqlx::query(
            "UPDATE erpx_identity.devices SET revoked_at=COALESCE(revoked_at,$2)
             WHERE id=$1 AND principal_id=$3",
        )
        .bind(device_id)
        .bind(at)
        .bind(principal_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_identity.sessions
             SET revoked_at=COALESCE(revoked_at,$2),
                 last_used_at=GREATEST(last_used_at,$2)
             WHERE device_id=$1 AND principal_id=$3 AND revoked_at IS NULL",
        )
        .bind(device_id)
        .bind(at)
        .bind(principal_id)
        .execute(&mut *transaction)
        .await?;
        insert_security_command_audit(
            &mut transaction,
            principal_id,
            "device_revoke",
            Some(device_id),
            command,
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(Some(DeviceRevocationOutcome {
            replayed,
            occurred_at: existing_revoked_at.unwrap_or(at),
        }))
    }

    async fn lock_recovery_principal(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        principal_ref: &str,
    ) -> Result<Uuid, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT id, status
             FROM erpx_identity.principals p
             WHERE p.principal_ref = $1
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             FOR UPDATE",
        )
        .bind(principal_ref)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let status: String = row.try_get("status")?;
        if !matches!(status.as_str(), "active" | "recovery_required") {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        Ok(row.try_get("id")?)
    }

    async fn lock_active_principal(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        principal_ref: &str,
    ) -> Result<Uuid, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT id, status
             FROM erpx_identity.principals p
             WHERE p.principal_ref = $1
               AND NOT EXISTS (
                   SELECT 1
                   FROM erpx_identity.directory_principal_bindings managed
                   WHERE managed.principal_id = p.id AND managed.active
               )
             FOR UPDATE",
        )
        .bind(principal_ref)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(IdentityStoreError::InvalidCredentials)?;
        let status: String = row.try_get("status")?;
        if status != "active" {
            return Err(IdentityStoreError::PrincipalInactive);
        }
        Ok(row.try_get("id")?)
    }
}

fn bounded_security_count(value: i64) -> Result<u16, IdentityStoreError> {
    u16::try_from(value).map_err(|_| {
        IdentityStoreError::InvalidRecord("security factor count exceeds its bound".into())
    })
}

async fn security_command_replay_time(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal_id: Uuid,
    operation: &str,
    target_id: Option<Uuid>,
    command: &SecurityCommandContext,
) -> Result<Option<DateTime<Utc>>, IdentityStoreError> {
    let row = sqlx::query(
        "SELECT audit_reference, command_id, target_id, request_digest,
                purpose, locale, occurred_at
         FROM erpx_identity.security_command_audit
         WHERE principal_id=$1 AND operation=$2 AND idempotency_key=$3
         FOR SHARE",
    )
    .bind(principal_id)
    .bind(operation)
    .bind(&command.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let digest: Vec<u8> = row.try_get("request_digest")?;
    if row.try_get::<Uuid, _>("audit_reference")? != command.audit_reference
        || row.try_get::<Uuid, _>("command_id")? != command.command_id
        || row.try_get::<Option<Uuid>, _>("target_id")? != target_id
        || digest.as_slice() != command.request_digest
        || row.try_get::<String, _>("purpose")? != command.purpose
        || row.try_get::<String, _>("locale")? != command.locale
    {
        return Err(IdentityStoreError::SecurityCommandConflict);
    }
    Ok(Some(row.try_get("occurred_at")?))
}

async fn insert_security_command_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal_id: Uuid,
    operation: &str,
    target_id: Option<Uuid>,
    command: &SecurityCommandContext,
    occurred_at: DateTime<Utc>,
) -> Result<(), IdentityStoreError> {
    sqlx::query(
        "INSERT INTO erpx_identity.security_command_audit
            (audit_reference, command_id, principal_id, operation, target_id,
             idempotency_key, request_digest, purpose, locale, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(command.audit_reference)
    .bind(command.command_id)
    .bind(principal_id)
    .bind(operation)
    .bind(target_id)
    .bind(&command.idempotency_key)
    .bind(command.request_digest.as_slice())
    .bind(&command.purpose)
    .bind(&command.locale)
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn verify_totp_in_transaction<K: KeyResolver>(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal_ref: &str,
    presented_code: &str,
    keys: &K,
    at: DateTime<Utc>,
) -> Result<(), IdentityStoreError> {
    let row = sqlx::query(
        "SELECT t.key_reference, t.nonce, t.ciphertext,
                t.last_verified_step, p.principal_ref
         FROM erpx_identity.totp_secrets t
         JOIN erpx_identity.principals p ON p.id=t.principal_id
         WHERE p.principal_ref=$1 AND p.status='active'
         FOR UPDATE",
    )
    .bind(principal_ref)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(IdentityStoreError::Primitive(
        IdentityError::TotpEnrollmentInvalid,
    ))?;
    let envelope = totp_envelope_from_parts(
        row.try_get("key_reference")?,
        row.try_get("principal_ref")?,
        row.try_get("nonce")?,
        row.try_get("ciphertext")?,
    )?;
    let secret = decrypt_totp_secret(keys, principal_ref, &envelope)?;
    let last_verified_step: Option<i64> = row.try_get("last_verified_step")?;
    let step = verify_totp_code(secret.as_bytes(), presented_code, at, last_verified_step)?;
    let updated = sqlx::query(
        "UPDATE erpx_identity.totp_secrets t
         SET last_verified_step=$2, updated_at=$3
         FROM erpx_identity.principals p
         WHERE t.principal_id=p.id AND p.principal_ref=$1
           AND (t.last_verified_step IS NULL OR t.last_verified_step < $2)",
    )
    .bind(principal_ref)
    .bind(step)
    .bind(at)
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(IdentityStoreError::Primitive(IdentityError::TotpReplay));
    }
    Ok(())
}

fn validate_principal_ref(principal_ref: &str) -> Result<(), IdentityStoreError> {
    if principal_ref.trim().is_empty() || principal_ref.len() > 256 {
        return Err(IdentityStoreError::Primitive(
            IdentityError::InvalidIdentityReference,
        ));
    }
    Ok(())
}

/// Normalize both current UUID scope keys and legacy opaque holding keys to
/// the immutable holding identity used by holding authorization. Invalid or
/// non-directory scope keys return `None` and therefore cannot authorize a
/// directory-managed permission.
fn normalize_directory_scope_key(value: &str) -> Option<String> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > 512
        || value.chars().any(char::is_control)
    {
        return None;
    }
    if value == "platform" {
        return Some("platform".to_owned());
    }
    let segments = value.split(':').collect::<Vec<_>>();
    match segments.as_slice() {
        ["holding", data_profile_key] if !data_profile_key.is_empty() => Some(format!(
            "holding:{}",
            directory_holding_id(data_profile_key)
        )),
        ["company", data_profile_key, company_id] if !data_profile_key.is_empty() => {
            let company_id = Uuid::parse_str(company_id).ok()?;
            (!company_id.is_nil()).then(|| {
                format!(
                    "company:{}:{company_id}",
                    directory_holding_id(data_profile_key)
                )
            })
        }
        ["branch", data_profile_key, company_id, branch_id] if !data_profile_key.is_empty() => {
            let company_id = Uuid::parse_str(company_id).ok()?;
            let branch_id = Uuid::parse_str(branch_id).ok()?;
            (!company_id.is_nil() && !branch_id.is_nil()).then(|| {
                format!(
                    "branch:{}:{company_id}:{branch_id}",
                    directory_holding_id(data_profile_key)
                )
            })
        }
        _ => None,
    }
}

fn directory_scope_matches(
    requested_scope_key: &str,
    holding_scope_key: Option<&str>,
    mapping_scope_key: &str,
) -> bool {
    let mapping = normalize_directory_scope_key(mapping_scope_key);
    let Some(mapping) = mapping else {
        return false;
    };
    let Some(requested) = normalize_directory_scope_key(requested_scope_key) else {
        return false;
    };
    if mapping == "platform" {
        return true;
    }
    if requested == "platform" {
        return false;
    }
    if mapping.starts_with("holding:") {
        let requested_holding = if requested.starts_with("holding:") {
            Some(requested.clone())
        } else {
            requested
                .split(':')
                .nth(1)
                .map(|holding_id| format!("holding:{holding_id}"))
        };
        let supplied_holding = holding_scope_key.and_then(normalize_directory_scope_key);
        return requested_holding.as_deref() == Some(mapping.as_str())
            && supplied_holding.as_deref() == requested_holding.as_deref();
    }
    if mapping.starts_with("company:") {
        if requested == mapping {
            return true;
        }
        let company_prefix = mapping
            .strip_prefix("company:")
            .map(|value| format!("branch:{value}:"));
        return company_prefix.is_some_and(|prefix| requested.starts_with(&prefix));
    }
    mapping.starts_with("branch:") && requested == mapping
}

fn directory_connector_contains_mapping(
    connector_scope_key: &str,
    mapping_scope_key: &str,
) -> bool {
    if connector_scope_key == "platform" {
        return true;
    }
    if connector_scope_key.starts_with("branch:") {
        return connector_scope_key == mapping_scope_key;
    }
    if connector_scope_key.starts_with("holding:") {
        let holding_id = connector_scope_key.strip_prefix("holding:");
        return holding_id.is_some_and(|holding_id| {
            mapping_scope_key == connector_scope_key
                || mapping_scope_key
                    .strip_prefix("company:")
                    .is_some_and(|value| value.starts_with(&format!("{holding_id}:")))
                || mapping_scope_key
                    .strip_prefix("branch:")
                    .is_some_and(|value| value.starts_with(&format!("{holding_id}:")))
        });
    }
    false
}

fn effective_local_role_chain(
    assignment: &RoleAssignment,
    all: &HashMap<Uuid, RoleAssignment>,
    at: DateTime<Utc>,
    path: &mut Vec<Uuid>,
) -> bool {
    if !assignment.is_active_at(at) || path.contains(&assignment.id) {
        return false;
    }
    let Some(parent_id) = assignment.parent_assignment_id else {
        return true;
    };
    let Some(parent) = all.get(&parent_id) else {
        return false;
    };
    if !parent.is_active_at(at)
        || !parent.scope.contains(&assignment.scope)
        || assignment.authority_level > parent.authority_level
        || assignment.valid_from < parent.valid_from
        || assignment.valid_until > parent.valid_until
        || assignment
            .permissions
            .iter()
            .any(|permission| !parent.permissions.contains(permission))
    {
        return false;
    }
    path.push(assignment.id);
    let effective = effective_local_role_chain(parent, all, at, path);
    path.pop();
    effective
}

fn selected_local_assignment_allows(
    assignment: &RoleAssignment,
    all: &HashMap<Uuid, RoleAssignment>,
    principal_ref: &str,
    requested_scope: &Scope,
    required_permission: &str,
    at: DateTime<Utc>,
) -> bool {
    assignment.principal_ref == principal_ref
        && assignment.scope.contains(requested_scope)
        && assignment
            .permissions
            .iter()
            .any(|permission| permission == required_permission)
        && effective_local_role_chain(assignment, all, at, &mut Vec::new())
}

fn directory_role_target_scope(role_ref: &str) -> Option<DirectoryRoleTargetScope> {
    permission_catalog()
        .into_iter()
        .find(|entry| entry.role_key == role_ref)
        .map(|entry| entry.target_scope)
}

fn directory_role_is_allow_listed(role_ref: &str, target_scope: &str) -> bool {
    directory_role_target_scope(role_ref)
        .is_some_and(|catalog_scope| catalog_scope.as_str() == target_scope)
}

fn directory_role_allows_permission(role_ref: &str, required_permission: &str) -> bool {
    permission_catalog().into_iter().any(|entry| {
        entry.role_key == role_ref
            && entry
                .permissions
                .iter()
                .any(|permission| permission == required_permission)
    })
}

fn directory_target_scope_allows(
    target_scope: DirectoryRoleTargetScope,
    requested_scope: &Scope,
) -> bool {
    match target_scope {
        DirectoryRoleTargetScope::Platform => true,
        DirectoryRoleTargetScope::Holding => !matches!(requested_scope, Scope::Platform),
        DirectoryRoleTargetScope::Company => {
            matches!(
                requested_scope,
                Scope::Company { .. } | Scope::Branch { .. }
            )
        }
        DirectoryRoleTargetScope::Branch => matches!(requested_scope, Scope::Branch { .. }),
    }
}

fn directory_selection_id(
    connector_id: Uuid,
    group_ref: &str,
    role_ref: &str,
    scope_key: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erpx-directory-role-selection\0");
    digest.update(connector_id.as_bytes());
    for value in [group_ref, role_ref, scope_key] {
        digest.update([0]);
        digest.update(value.as_bytes());
    }
    let digest = digest.finalize();
    let mut encoded = String::with_capacity(3 + digest.len() * 2);
    encoded.push_str("d1_");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn scope_holding_key(scope: &Scope) -> Option<String> {
    let holding_id = match scope {
        Scope::Platform => return None,
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => holding_id,
    };
    Some(format!("holding:{holding_id}"))
}

fn totp_envelope_from_parts(
    key_reference: String,
    principal_ref: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
) -> Result<TotpSecretEnvelope, IdentityStoreError> {
    TotpSecretEnvelope::from_parts(key_reference, principal_ref, nonce, ciphertext)
        .map_err(IdentityStoreError::Primitive)
}

fn validate_totp_code_input(code: &str) -> Result<(), IdentityStoreError> {
    if code.len() != 6 || !code.as_bytes().iter().all(u8::is_ascii_digit) {
        return Err(IdentityStoreError::Primitive(
            IdentityError::InvalidTotpCode,
        ));
    }
    Ok(())
}

fn validate_recovery_code_input(code: &str) -> Result<(), IdentityStoreError> {
    if code.trim().is_empty() || code.len() > 512 {
        return Err(IdentityStoreError::Primitive(
            IdentityError::InvalidRecoveryCode,
        ));
    }
    Ok(())
}

fn validate_device_label(label: &str) -> Result<(), IdentityStoreError> {
    if label.trim().is_empty() || label.len() > 200 || label.chars().any(char::is_control) {
        return Err(IdentityStoreError::InvalidRecord(
            "device label is empty, too long, or contains control characters".into(),
        ));
    }
    Ok(())
}

fn validate_password_input(password: &str) -> Result<(), IdentityStoreError> {
    if password.is_empty() || password.len() > 1024 || password.chars().any(char::is_control) {
        return Err(IdentityStoreError::Primitive(
            IdentityError::PasswordHashingFailed,
        ));
    }
    Ok(())
}

fn authority_from_row(
    row: &sqlx::postgres::PgRow,
    realm_column: &str,
    kind_column: &str,
) -> Result<AuthenticatedAuthority, IdentityStoreError> {
    let realm: String = row.try_get(realm_column)?;
    let kind: String = row.try_get(kind_column)?;
    let authority = AuthenticatedAuthority {
        realm: match realm.as_str() {
            "internal_workforce" => AuthenticatedRealm::InternalWorkforce,
            "platform_administration" => AuthenticatedRealm::PlatformAdministration,
            "customer_portal" => AuthenticatedRealm::CustomerPortal,
            "supplier_portal" => AuthenticatedRealm::SupplierPortal,
            "commerce_portal" => AuthenticatedRealm::CommercePortal,
            "candidate_portal" => AuthenticatedRealm::CandidatePortal,
            "signatory_portal" => AuthenticatedRealm::SignatoryPortal,
            _ => {
                return Err(IdentityStoreError::InvalidRecord(
                    "authenticated realm is invalid".into(),
                ));
            }
        },
        principal_kind: match kind.as_str() {
            "human" => AuthenticatedPrincipalKind::Human,
            "agent" => AuthenticatedPrincipalKind::Agent,
            "service" => AuthenticatedPrincipalKind::Service,
            _ => {
                return Err(IdentityStoreError::InvalidRecord(
                    "authenticated principal kind is invalid".into(),
                ));
            }
        },
    };
    authority.validate().map_err(IdentityStoreError::from)
}

const fn authority_realm_name(realm: AuthenticatedRealm) -> &'static str {
    match realm {
        AuthenticatedRealm::InternalWorkforce => "internal_workforce",
        AuthenticatedRealm::PlatformAdministration => "platform_administration",
        AuthenticatedRealm::CustomerPortal => "customer_portal",
        AuthenticatedRealm::SupplierPortal => "supplier_portal",
        AuthenticatedRealm::CommercePortal => "commerce_portal",
        AuthenticatedRealm::CandidatePortal => "candidate_portal",
        AuthenticatedRealm::SignatoryPortal => "signatory_portal",
    }
}

const fn authority_kind_name(kind: AuthenticatedPrincipalKind) -> &'static str {
    match kind {
        AuthenticatedPrincipalKind::Human => "human",
        AuthenticatedPrincipalKind::Agent => "agent",
        AuthenticatedPrincipalKind::Service => "service",
    }
}

async fn insert_session_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    principal_id: Uuid,
    issued: &crate::IssuedSession,
) -> Result<(), IdentityStoreError> {
    sqlx::query(
        "INSERT INTO erpx_identity.sessions
            (id, principal_id, device_id, refresh_token_hash, issued_at,
             refresh_expires_at, revoked_at, last_used_at,
             active_role_source, active_role_selection_id,
             authorization_context_version, authenticated_realm, principal_kind)
         VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, NULL, NULL, $8, $9, $10)",
    )
    .bind(issued.record().id())
    .bind(principal_id)
    .bind(issued.record().device_id())
    .bind(issued.record().refresh_token_hash().as_bytes().as_slice())
    .bind(issued.record().issued_at())
    .bind(issued.record().refresh_expires_at())
    .bind(issued.record().last_used_at())
    .bind(
        i64::try_from(issued.record().authorization_context_version()).map_err(|_| {
            IdentityStoreError::InvalidRecord("authorization context version is too large".into())
        })?,
    )
    .bind(authority_realm_name(issued.record().authority().realm))
    .bind(authority_kind_name(
        issued.record().authority().principal_kind,
    ))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn rate_limit_digest(namespace: &str, identifier: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(namespace.as_bytes());
    hasher.update([0]);
    hasher.update(identifier.as_bytes());
    hasher.finalize().into()
}

fn validate_passkey_credential_id(credential_id: &[u8]) -> Result<(), IdentityStoreError> {
    if credential_id.is_empty() || credential_id.len() > 1024 {
        return Err(IdentityStoreError::Primitive(
            IdentityError::InvalidCredential,
        ));
    }
    Ok(())
}

fn passkey_count_to_i32(count: u32) -> Result<i32, IdentityStoreError> {
    i32::try_from(count).map_err(|_| {
        IdentityStoreError::InvalidRecord("passkey counter exceeds database range".into())
    })
}

fn passkey_credential_id_hash(credential_id: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx-passkey-credential-id");
    hasher.update([0]);
    hasher.update(credential_id);
    hasher.finalize().into()
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database_error) if database_error.code().as_deref() == Some("23505")
    )
}

fn passkey_from_row(
    row: sqlx::postgres::PgRow,
    expected_credential_id: &[u8],
    expected_credential_id_hash: &[u8],
) -> Result<crate::PasskeyCredential, IdentityStoreError> {
    let credential_id: Vec<u8> = row.try_get("credential_id")?;
    let stored_hash: Option<Vec<u8>> = row.try_get("credential_id_hash")?;
    if credential_id != expected_credential_id
        || stored_hash.as_deref() != Some(expected_credential_id_hash)
    {
        return Err(IdentityStoreError::InvalidRecord(
            "stored passkey credential binding is invalid".into(),
        ));
    }
    let sign_count = u32::try_from(row.try_get::<i32, _>("sign_count")?).map_err(|_| {
        IdentityStoreError::InvalidRecord("stored passkey counter is invalid".into())
    })?;
    let principal_ref: String = row.try_get("principal_ref")?;
    let rp_id: Option<String> = row.try_get("rp_id")?;
    let rp_id = rp_id.ok_or_else(|| {
        IdentityStoreError::InvalidRecord("stored passkey RP ID is missing".into())
    })?;
    crate::PasskeyCredential::from_persisted(crate::webauthn::PersistedPasskey {
        id: row.try_get("id")?,
        principal_ref,
        rp_id,
        credential_id,
        public_key: row.try_get("public_key")?,
        sign_count,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
    .map_err(IdentityStoreError::Primitive)
}

fn challenge_ceremony_name(ceremony: ChallengeCeremony) -> &'static str {
    match ceremony {
        ChallengeCeremony::Registration => "registration",
        ChallengeCeremony::Assertion => "assertion",
    }
}

fn challenge_ceremony(value: &str) -> Result<ChallengeCeremony, IdentityStoreError> {
    match value {
        "registration" => Ok(ChallengeCeremony::Registration),
        "assertion" => Ok(ChallengeCeremony::Assertion),
        _ => Err(IdentityStoreError::InvalidRecord(
            "stored WebAuthn ceremony is invalid".into(),
        )),
    }
}

fn challenge_from_row(
    row: sqlx::postgres::PgRow,
    expected_ceremony: ChallengeCeremony,
) -> Result<WebAuthnChallenge, IdentityStoreError> {
    let stored_ceremony = challenge_ceremony(row.try_get::<String, _>("ceremony")?.as_str())?;
    if stored_ceremony != expected_ceremony {
        return Err(IdentityStoreError::Primitive(
            IdentityError::CeremonyMismatch,
        ));
    }
    let digest = row.try_get::<Vec<u8>, _>("challenge_digest")?;
    let digest: [u8; 32] = digest.try_into().map_err(|_| {
        IdentityStoreError::InvalidRecord("stored WebAuthn challenge digest is invalid".into())
    })?;
    WebAuthnChallenge::from_persisted(
        row.try_get("id")?,
        row.try_get("principal_ref")?,
        stored_ceremony,
        digest,
        row.try_get("issued_at")?,
        row.try_get("expires_at")?,
        row.try_get::<Option<DateTime<Utc>>, _>("consumed_at")?
            .is_some(),
    )
    .map_err(IdentityStoreError::Primitive)
}

#[cfg(test)]
mod rate_limit_tests {
    use super::*;
    use crate::DEVELOPMENT_CONTROLLER_TARGET_HOLDING;

    #[test]
    fn rate_limit_policy_is_bounded_and_bucket_digest_hides_identifier() {
        assert!(RateLimitPolicy::new(0, 300, 900).validate().is_err());
        assert!(RateLimitPolicy::new(5, 86_401, 900).validate().is_err());
        let digest = rate_limit_digest("login", "principal@example.test");
        assert_eq!(digest.len(), 32);
        assert!(!String::from_utf8_lossy(&digest).contains("principal@example.test"));
    }

    #[test]
    fn directory_scope_normalization_binds_opaque_refs_to_immutable_ids() {
        let opaque = normalize_directory_scope_key("holding:holding-ref-42")
            .expect("opaque holding scope is a valid directory scope");
        let stable_id = directory_holding_id("holding-ref-42");
        assert_eq!(opaque, format!("holding:{stable_id}"));
        assert_eq!(
            normalize_directory_scope_key(&format!("holding:{stable_id}")),
            Some(format!("holding:{stable_id}"))
        );
        assert_eq!(normalize_directory_scope_key("holding:"), None);
        assert_eq!(normalize_directory_scope_key("company:ignored"), None);
    }

    #[test]
    fn directory_scope_authorization_binds_requested_holding_to_authorized_holding() {
        let requested = "holding:requested-holding";
        let authorized = "holding:authorized-holding";
        assert!(!directory_scope_matches(
            requested,
            Some(authorized),
            "holding:authorized-holding"
        ));
        assert!(directory_scope_matches(
            requested,
            Some(requested),
            "holding:requested-holding"
        ));
        assert!(directory_scope_matches(
            requested,
            Some(requested),
            "platform"
        ));
        assert!(!directory_scope_matches(
            "platform",
            Some(requested),
            requested
        ));
        assert!(directory_connector_contains_mapping("platform", requested));
        assert!(!directory_connector_contains_mapping(
            "holding:authorized-holding",
            "platform"
        ));
        assert!(!directory_connector_contains_mapping(
            "holding:authorized-holding",
            requested
        ));
    }

    #[test]
    fn directory_branch_scope_is_exact_and_connector_bounded() {
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        let other_branch = Uuid::new_v4();
        let holding_scope = format!("holding:{holding}");
        let branch_scope = format!("branch:{holding}:{company}:{branch}");
        let other_scope = format!("branch:{holding}:{company}:{other_branch}");
        assert_eq!(
            normalize_directory_scope_key(&branch_scope),
            Some(branch_scope.clone())
        );
        assert!(directory_connector_contains_mapping(
            &holding_scope,
            &branch_scope
        ));
        assert!(directory_scope_matches(
            &branch_scope,
            Some(&holding_scope),
            &branch_scope
        ));
        assert!(!directory_scope_matches(
            &other_scope,
            Some(&holding_scope),
            &branch_scope
        ));
    }

    #[test]
    fn directory_role_catalog_never_combines_permissions_from_other_roles() {
        assert!(directory_role_is_allow_listed("finance_viewer", "branch"));
        assert!(directory_role_allows_permission(
            "finance_viewer",
            "finance.read"
        ));
        assert!(!directory_role_allows_permission(
            "finance_viewer",
            "finance.approve"
        ));
        assert!(!directory_role_is_allow_listed("finance.read", "branch"));
        assert!(!directory_target_scope_allows(
            DirectoryRoleTargetScope::Branch,
            &Scope::Holding {
                holding_id: Uuid::new_v4(),
            }
        ));
    }

    #[test]
    fn role_assignment_target_policy_is_fail_closed_and_target_exact() {
        assert!(
            role_assignment_target_policy_from_parts("unrestricted", None, Vec::new()).is_some()
        );
        assert!(
            role_assignment_target_policy_from_parts(
                "unrestricted",
                None,
                vec!["holding-a".into()]
            )
            .is_none()
        );
        assert!(
            role_assignment_target_policy_from_parts(
                "holding_allowlist",
                Some("production"),
                vec!["holding-a".into()]
            )
            .is_none()
        );
        assert!(
            role_assignment_target_policy_from_parts(
                "holding_allowlist",
                Some(DEVELOPMENT_CLUSTER_MARKER),
                Vec::new()
            )
            .is_none()
        );

        let policy = role_assignment_target_policy_from_parts(
            "holding_allowlist",
            Some(DEVELOPMENT_CLUSTER_MARKER),
            vec![DEVELOPMENT_CONTROLLER_TARGET_HOLDING.into()],
        )
        .expect("valid local target policy");
        assert!(role_assignment_target_allows(
            Some(&policy),
            "platform.holding.read",
            Some("holding-other")
        ));
        assert!(role_assignment_target_allows(
            Some(&policy),
            "platform.holding.suspend",
            Some(DEVELOPMENT_CONTROLLER_TARGET_HOLDING)
        ));
        assert!(!role_assignment_target_allows(
            Some(&policy),
            "platform.holding.suspend",
            Some("holding-other")
        ));
        assert!(!role_assignment_target_allows(
            None,
            "platform.holding.read",
            Some(DEVELOPMENT_CONTROLLER_TARGET_HOLDING)
        ));
    }

    #[test]
    fn platform_root_reads_do_not_gain_an_implicit_mfa_requirement() {
        let at = DateTime::parse_from_rfc3339("2026-08-04T12:00:00Z")
            .expect("fixed time")
            .with_timezone(&Utc);
        let assignment = RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: "local:platform-root".into(),
            role_key: "platform_root".into(),
            permissions: vec!["platform.directory.read".into()],
            functional_domain: "platform".into(),
            authority_level: 255,
            approval_limit_minor: u64::MAX,
            scope: Scope::Platform,
            valid_from: at - Duration::hours(1),
            valid_until: at + Duration::hours(1),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::PlatformRoot,
            grant_reason: "authorization regression fixture".into(),
            ticket_reference: Some("test:platform-root-read".into()),
            grantor: "test:grantor".into(),
            approver: Some("test:approver".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        };
        let assignments = HashMap::from([(assignment.id, assignment.clone())]);

        assert!(selected_local_assignment_allows(
            &assignment,
            &assignments,
            "local:platform-root",
            &Scope::Platform,
            "platform.directory.read",
            at,
        ));
        assert!(!selected_local_assignment_allows(
            &assignment,
            &assignments,
            "local:platform-root",
            &Scope::Platform,
            "platform.directory.manage",
            at,
        ));
    }

    #[test]
    fn development_holding_allowlist_preserves_platform_directory_reads() {
        let policy = RoleAssignmentTargetPolicy::HoldingAllowlist(vec![
            DEVELOPMENT_CONTROLLER_TARGET_HOLDING.into(),
        ]);

        assert!(role_assignment_target_allows(
            Some(&policy),
            "platform.directory.read",
            None,
        ));
        assert!(!role_assignment_target_allows(
            Some(&policy),
            "platform.directory.manage",
            None,
        ));
    }
}
