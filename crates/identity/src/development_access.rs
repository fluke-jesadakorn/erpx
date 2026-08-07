//! Explicitly gated local development access.
//!
//! This module contains the policy vocabulary shared by the server, the
//! development fixture commands, and the console.  It deliberately does not
//! contain passwords or bearer tokens.  A persona context is an opaque value
//! issued by the server; only its SHA-256 digest is persisted.

use std::{collections::BTreeMap, net::IpAddr, str::FromStr};

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use crate::{IdentityError, IdentityStoreError, PasswordService, SqlxIdentityStore};

pub const DEVELOPMENT_PROFILE: &str = "local";
pub const DEVELOPMENT_FRESH_PROFILE: &str = "local-fresh-state";
pub const DEVELOPMENT_PERSONA_MODE: &str = "enabled";
pub const DEVELOPMENT_CLUSTER_MARKER: &str = "local-ucs-lab";
pub const DEVELOPMENT_CONTROLLER_PRINCIPAL: &str = "local:dev-controller";
pub const DEVELOPMENT_CONTROLLER_ROLE: &str = "development_restricted_tenant_full_admin";
pub const DEVELOPMENT_CONTROLLER_ROLE_PROFILE: &str = "restricted_tenant_full_admin";
pub const DEVELOPMENT_CONTROLLER_TARGET_TENANT: &str = "demo-tenant-restricted";
pub const DEVELOPMENT_MFA_BYPASS_PERMISSION: &str = "development.mfa.bypass";
pub const DEVELOPMENT_SOD_BYPASS_PERMISSION: &str = "development.sod.self-approve";
pub const DEVELOPMENT_CONTEXT_HEADER: &str = "x-erpx-dev-persona-context";
pub const DEVELOPMENT_CONTEXT_LIFETIME: Duration = Duration::days(30);

pub const DEVELOPMENT_CONTROLLER_READ_PERMISSIONS: [&str; 10] = [
    "platform.customer.read",
    "platform.tenant.read",
    "platform.infrastructure.read",
    "tenant.infrastructure.read",
    "tenant.directory.read",
    "tenant.brand.read",
    "business-unit.configuration.read",
    "organization.read",
    "tenant.root-access.read",
    "platform.tenant.history.read",
];

pub const DEVELOPMENT_CONTROLLER_MUTATION_PERMISSIONS: [&str; 22] = [
    "platform.tenant.enter",
    "platform.tenant.provision",
    "platform.tenant.backup",
    "platform.tenant.suspend",
    "platform.tenant.recovery",
    "platform.infrastructure.manage",
    "tenant.infrastructure.propose",
    "tenant.infrastructure.approve",
    "tenant.infrastructure.apply",
    "tenant.agent.enroll",
    "tenant.agent.rotate",
    "tenant.root-access.request",
    "tenant.root-access.approve",
    "tenant.root-access.revoke",
    "business-unit.configuration.manage",
    "tenant.directory.manage",
    "tenant.directory.activate",
    "tenant.brand.manage",
    "tenant.brand.publish",
    "tenant.brand.lock",
    "organization.admin",
    "branch.infrastructure.provision",
];

pub fn development_controller_permissions() -> Vec<String> {
    DEVELOPMENT_CONTROLLER_READ_PERMISSIONS
        .into_iter()
        .chain(DEVELOPMENT_CONTROLLER_MUTATION_PERMISSIONS)
        .chain([
            DEVELOPMENT_MFA_BYPASS_PERMISSION,
            DEVELOPMENT_SOD_BYPASS_PERMISSION,
        ])
        .map(str::to_owned)
        .collect()
}

/// The development surface is unavailable unless every boundary is true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentAccessGates {
    pub debug_build: bool,
    pub local_profile: bool,
    pub persona_mode: bool,
    pub loopback_http: bool,
    pub loopback_database: bool,
    pub development_cluster: bool,
}

impl DevelopmentAccessGates {
    pub const fn all_satisfied(&self) -> bool {
        self.debug_build
            && self.local_profile
            && self.persona_mode
            && self.loopback_http
            && self.loopback_database
            && self.development_cluster
    }

    pub fn missing(&self) -> Vec<&'static str> {
        [
            (self.debug_build, "debug_build"),
            (
                self.local_profile,
                "ERPX_DEVELOPMENT_PROFILE=local|local-fresh-state",
            ),
            (self.persona_mode, "ERPX_DEV_PERSONA_MODE=enabled"),
            (self.loopback_http, "loopback_http"),
            (self.loopback_database, "loopback_database"),
            (
                self.development_cluster,
                "ERPX_DEVELOPMENT_CLUSTER=local-ucs-lab",
            ),
        ]
        .into_iter()
        .filter_map(|(satisfied, name)| (!satisfied).then_some(name))
        .collect()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DevelopmentAccessError {
    #[error("development persona mode is unavailable: {0}")]
    Gates(String),
    #[error("development persona context is invalid")]
    InvalidContext,
    #[error("development persona context is expired or revoked")]
    ContextInactive,
    #[error("development persona is not active")]
    PersonaInactive,
    #[error("development controller authorization configuration is invalid: {0}")]
    InvalidConfiguration(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentControllerRoleConfig {
    pub target_tenant: String,
    pub mfa_bypass: bool,
    pub sod_bypass: bool,
}

/// Parse the four controller flags as one indivisible local-development
/// capability. An absent set disables the role; partial, misspelled, or
/// non-local configurations are startup errors rather than silent fallback.
pub fn development_controller_role_config_from_env(
    debug_build: bool,
    http_addr: &str,
    database_urls: &[String],
    environment: &BTreeMap<String, String>,
) -> Result<Option<DevelopmentControllerRoleConfig>, DevelopmentAccessError> {
    const KEYS: [&str; 4] = [
        "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
        "ERPX_DEV_CONTROLLER_TARGET_TENANT",
        "ERPX_DEV_CONTROLLER_MFA_BYPASS",
        "ERPX_DEV_CONTROLLER_SOD_BYPASS",
    ];
    if KEYS.iter().all(|key| !environment.contains_key(*key)) {
        return Ok(None);
    }
    let expected = [
        (KEYS[0], DEVELOPMENT_CONTROLLER_ROLE_PROFILE),
        (KEYS[2], "enabled"),
        (KEYS[3], "enabled"),
    ];
    for (key, value) in expected {
        if environment.get(key).map(String::as_str) != Some(value) {
            return Err(DevelopmentAccessError::InvalidConfiguration(format!(
                "{key} must be exactly {value}"
            )));
        }
    }
    let target_tenant = environment
        .get(KEYS[1])
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            DevelopmentAccessError::InvalidConfiguration(format!("{} is required", KEYS[1]))
        })?;
    match environment
        .get("ERPX_DEVELOPMENT_PROFILE")
        .map(String::as_str)
    {
        Some(DEVELOPMENT_PROFILE) if target_tenant == DEVELOPMENT_CONTROLLER_TARGET_TENANT => {}
        Some(DEVELOPMENT_FRESH_PROFILE)
            if Uuid::parse_str(target_tenant).is_ok_and(|value| {
                !value.is_nil() && value.to_string().as_str() == target_tenant.as_str()
            }) => {}
        Some(DEVELOPMENT_PROFILE) => {
            return Err(DevelopmentAccessError::InvalidConfiguration(format!(
                "{} must be exactly {}",
                KEYS[1], DEVELOPMENT_CONTROLLER_TARGET_TENANT
            )));
        }
        Some(DEVELOPMENT_FRESH_PROFILE) => {
            return Err(DevelopmentAccessError::InvalidConfiguration(format!(
                "{} must be a canonical non-nil tenant UUID",
                KEYS[1]
            )));
        }
        _ => {}
    }
    require_development_gates(debug_build, http_addr, database_urls, environment)?;
    Ok(Some(DevelopmentControllerRoleConfig {
        target_tenant: target_tenant.clone(),
        mfa_bypass: true,
        sod_bypass: true,
    }))
}

/// Validate all runtime gates.  The caller supplies `debug_build` so release
/// binaries can hard-code `false` and cannot turn the mode on with env vars.
pub fn development_gates_from_env(
    debug_build: bool,
    http_addr: &str,
    database_urls: &[String],
    environment: &BTreeMap<String, String>,
) -> DevelopmentAccessGates {
    let loopback_http = http_addr
        .rsplit_once(':')
        .map(|(host, _)| host.trim_matches(['[', ']']))
        .is_some_and(|host| {
            host == "localhost" || IpAddr::from_str(host).is_ok_and(|ip| ip.is_loopback())
        });
    let loopback_database = !database_urls.is_empty()
        && database_urls
            .iter()
            .all(|url| database_url_is_loopback(url));
    DevelopmentAccessGates {
        debug_build,
        local_profile: matches!(
            environment
                .get("ERPX_DEVELOPMENT_PROFILE")
                .map(String::as_str),
            Some(DEVELOPMENT_PROFILE | DEVELOPMENT_FRESH_PROFILE)
        ),
        persona_mode: environment.get("ERPX_DEV_PERSONA_MODE").map(String::as_str)
            == Some(DEVELOPMENT_PERSONA_MODE),
        loopback_http,
        loopback_database,
        development_cluster: environment
            .get("ERPX_DEVELOPMENT_CLUSTER")
            .map(String::as_str)
            == Some(DEVELOPMENT_CLUSTER_MARKER),
    }
}

pub fn require_development_gates(
    debug_build: bool,
    http_addr: &str,
    database_urls: &[String],
    environment: &BTreeMap<String, String>,
) -> Result<DevelopmentAccessGates, DevelopmentAccessError> {
    let gates = development_gates_from_env(debug_build, http_addr, database_urls, environment);
    if gates.all_satisfied() {
        Ok(gates)
    } else {
        Err(DevelopmentAccessError::Gates(gates.missing().join(", ")))
    }
}

fn database_url_is_loopback(url: &str) -> bool {
    let Some(authority) = url.split_once("://").map(|(_, rest)| rest) else {
        return false;
    };
    let authority = authority.split('/').next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches(['[', ']']);
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum DirectoryRoleTargetScope {
    Platform,
    Holding,
    Company,
    Branch,
}

impl DirectoryRoleTargetScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Platform => "platform",
            Self::Holding => "holding",
            Self::Company => "company",
            Self::Branch => "branch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum BusinessUnitRoleLevel {
    Viewer,
    Operator,
    Approver,
    Administrator,
    Auditor,
}

impl BusinessUnitRoleLevel {
    pub const ALL: [Self; 5] = [
        Self::Viewer,
        Self::Operator,
        Self::Approver,
        Self::Administrator,
        Self::Auditor,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Approver => "approver",
            Self::Administrator => "administrator",
            Self::Auditor => "auditor",
        }
    }
}

pub const BUSINESS_UNITS: [&str; 12] = [
    "operations",
    "finance",
    "sales",
    "procurement",
    "inventory",
    "assets",
    "maintenance",
    "quality",
    "marketing-crm",
    "hr-payroll",
    "point-of-sale",
    "legal",
];

pub const PLATFORM_DIRECTORY_ROLES: [&str; 8] = [
    "platform_customer_operations",
    "platform_directory_administrator",
    "platform_support_agent",
    "platform_billing_administrator",
    "platform_operations_administrator",
    "platform_auditor",
    "platform_role_reviewer",
    "platform_tenant_administrator",
];

pub const TENANT_ADMINISTRATION_ROLES: [&str; 5] = [
    "holding_owner",
    "holding_admin",
    "company_admin",
    "branch_admin",
    "customer_security_head",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionCatalogEntry {
    pub role_key: String,
    pub target_scope: DirectoryRoleTargetScope,
    pub permissions: Vec<String>,
}

/// Concrete, allow-listed permissions for the complete 60-unit role matrix.
/// No wildcard permission is emitted by this function.
pub fn permission_catalog() -> Vec<PermissionCatalogEntry> {
    let mut entries = Vec::with_capacity(8 + 5 + 60);
    for role in PLATFORM_DIRECTORY_ROLES {
        let permissions = match role {
            "platform_customer_operations" => &[
                "platform.customer.read",
                "platform.customer.access_audit.read",
                "platform.customer.notes.write",
                "platform.tenant.read",
                "platform.tenant.enter",
            ][..],
            "platform_directory_administrator" => &[
                "platform.directory.read",
                "platform.directory.manage",
                "platform.directory.activate",
            ],
            "platform_support_agent" => &[
                "platform.customer.read",
                "platform.tenant.read",
                "support.diagnostics.read",
                "support.incident.create",
                "support.incident.read",
                "support.incident.update",
                "support.session.request",
                "support.session.revoke",
                "support.ticket.create",
                "support.ticket.list",
                "support.ticket.note",
                "support.ticket.read",
                "support.ticket.reply",
                "support.ticket.update",
            ],
            "platform_billing_administrator" => &[
                "platform.customer.read",
                "platform.tenant.read",
                "commercial.billing.read",
                "commercial.billing.manage",
                "commercial.billing.grace.extend",
                "commercial.entitlement.allocate",
                "commercial.entitlement.activate",
            ],
            "platform_operations_administrator" => &[
                "platform.tenant.read",
                "platform.tenant.enter",
                "platform.tenant.backup",
                "platform.projection.rebuild",
                "platform.outbox.replay",
                "platform.infrastructure.read",
                "platform.brand.read",
                "platform.brand.manage",
                "platform.brand.publish",
                "platform.brand.lock",
                "platform.migration.read",
                "migration.execute",
            ],
            "platform_auditor" => &[
                "platform.customer.read",
                "platform.customer.access_audit.read",
                "platform.tenant.read",
                "platform.directory.read",
                "commercial.billing.read",
                "role.assignment.review",
                "support.incident.read",
                "support.ticket.read",
            ],
            "platform_role_reviewer" => &[
                "platform.tenant.read",
                "role.assignment.read",
                "role.assignment.review",
                "role.assignment.approve",
                "role.assignment.renew",
                "role.assignment.recover",
            ],
            "platform_tenant_administrator" => &[
                "platform.customer.read",
                "platform.tenant.read",
                "platform.tenant.enter",
                "platform.tenant.provision",
                "platform.tenant.backup",
                "platform.tenant.suspend",
                "platform.tenant.recovery",
                "platform.infrastructure.read",
                "business-unit.configuration.read",
                "tenant.root-access.request",
            ],
            _ => unreachable!("the platform role catalog is closed"),
        };
        entries.push(PermissionCatalogEntry {
            role_key: role.into(),
            target_scope: DirectoryRoleTargetScope::Platform,
            permissions: permissions
                .iter()
                .map(|permission| (*permission).into())
                .collect(),
        });
    }
    for (role, scope, permissions) in [
        (
            "holding_owner",
            DirectoryRoleTargetScope::Holding,
            vec![
                "directory.activation",
                "security.approval",
                "security.recovery",
                "role.high-risk.approve",
                "organization.read",
                "organization.write",
            ],
        ),
        (
            "holding_admin",
            DirectoryRoleTargetScope::Holding,
            vec![
                "branch.provision",
                "organization.read",
                "organization.write",
            ],
        ),
        (
            "company_admin",
            DirectoryRoleTargetScope::Company,
            vec!["organization.read", "organization.write"],
        ),
        (
            "branch_admin",
            DirectoryRoleTargetScope::Branch,
            vec!["organization.read", "organization.write"],
        ),
        (
            "customer_security_head",
            DirectoryRoleTargetScope::Holding,
            vec!["security.read", "security.approve"],
        ),
    ] {
        entries.push(PermissionCatalogEntry {
            role_key: role.into(),
            target_scope: scope,
            permissions: permissions.into_iter().map(str::to_owned).collect(),
        });
    }
    for unit in BUSINESS_UNITS {
        for level in BusinessUnitRoleLevel::ALL {
            let permissions = match level {
                BusinessUnitRoleLevel::Viewer => vec![format!("{unit}.read")],
                BusinessUnitRoleLevel::Operator => {
                    vec![format!("{unit}.read"), format!("{unit}.write")]
                }
                BusinessUnitRoleLevel::Approver => {
                    vec![format!("{unit}.read"), format!("{unit}.approve")]
                }
                BusinessUnitRoleLevel::Administrator => vec![
                    format!("{unit}.read"),
                    format!("{unit}.write"),
                    format!("{unit}.admin"),
                ],
                BusinessUnitRoleLevel::Auditor => {
                    vec![format!("{unit}.read"), "audit.read".into()]
                }
            };
            entries.push(PermissionCatalogEntry {
                role_key: format!("{unit}_{}", level.as_str()),
                target_scope: DirectoryRoleTargetScope::Branch,
                permissions,
            });
        }
    }
    entries
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentPersona {
    pub persona_key: String,
    pub principal_ref: String,
    pub directory_username: String,
    pub tenant_id: Option<String>,
    pub organization_scope: String,
    pub role_key: String,
    pub feature: String,
    pub permissions: Vec<String>,
    pub entitled: bool,
    pub directory_enabled: bool,
    pub requires_totp_for_high_risk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelopmentPersonaContext {
    pub id: Uuid,
    pub controller_principal: String,
    pub controller_session_id: Uuid,
    pub controller_device_id: Uuid,
    pub persona: DevelopmentPersona,
    pub reason: String,
    pub correlation_id: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl DevelopmentPersonaContext {
    pub fn active_at(&self, at: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && at >= self.issued_at && at < self.expires_at
    }

    pub fn validate(&self, at: DateTime<Utc>) -> Result<(), DevelopmentAccessError> {
        if self.controller_principal != DEVELOPMENT_CONTROLLER_PRINCIPAL
            || self.controller_session_id.is_nil()
            || self.controller_device_id.is_nil()
            || self.id.is_nil()
            || self.persona.principal_ref.is_empty()
            || !self.active_at(at)
        {
            return Err(DevelopmentAccessError::ContextInactive);
        }
        Ok(())
    }
}

/// Issue a high-entropy opaque context value. The return value is only for a
/// response header/body and must never be logged or stored.
pub fn issue_context_token() -> String {
    format!("{}.{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn context_token_hash(token: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"erpx-development-persona-context\0");
    digest.update(token.as_bytes());
    digest.finalize().into()
}

pub fn context_expiry(issued_at: DateTime<Utc>) -> DateTime<Utc> {
    issued_at + DEVELOPMENT_CONTEXT_LIFETIME
}

pub fn demo_personas(tenant_id: &str, entitled_units: &[&str]) -> Vec<DevelopmentPersona> {
    let catalog = permission_catalog();
    let mut personas = Vec::with_capacity(65 + 4);
    for entry in catalog
        .iter()
        .filter(|entry| entry.target_scope != DirectoryRoleTargetScope::Platform)
    {
        let feature = entry
            .role_key
            .split_once('_')
            .map_or_else(|| "platform".to_owned(), |(unit, _)| unit.to_owned());
        let entitled = feature == "platform" || entitled_units.contains(&feature.as_str());
        let key = format!("{tenant_id}:{}", entry.role_key);
        personas.push(DevelopmentPersona {
            persona_key: key.clone(),
            principal_ref: format!("dev:{tenant_id}:{}", entry.role_key),
            directory_username: format!("{}@{tenant_id}.ucs.test", entry.role_key),
            tenant_id: Some(tenant_id.into()),
            organization_scope: match entry.target_scope {
                DirectoryRoleTargetScope::Platform => "platform".into(),
                DirectoryRoleTargetScope::Holding => "holding".into(),
                DirectoryRoleTargetScope::Company => "company:all".into(),
                DirectoryRoleTargetScope::Branch => "branch:all".into(),
            },
            role_key: entry.role_key.clone(),
            feature,
            permissions: entry.permissions.clone(),
            entitled,
            directory_enabled: true,
            requires_totp_for_high_risk: entry.role_key.contains("admin")
                || entry.role_key.contains("owner")
                || entry.role_key.contains("security"),
        });
    }
    personas.extend([
        persona_fixture(tenant_id, "limited", "finance_viewer", false),
        persona_fixture(tenant_id, "multi-role", "operations_operator", true),
        persona_fixture(tenant_id, "collision", "finance_operator", true),
        persona_fixture(tenant_id, "deny-wins", "finance_operator", false),
    ]);
    personas
}

pub fn platform_personas() -> Vec<DevelopmentPersona> {
    permission_catalog()
        .into_iter()
        .filter(|entry| entry.target_scope == DirectoryRoleTargetScope::Platform)
        .map(|entry| DevelopmentPersona {
            persona_key: format!("platform:{}", entry.role_key),
            principal_ref: format!("dev:platform:{}", entry.role_key),
            directory_username: format!("{}@platform.ucs.test", entry.role_key),
            tenant_id: None,
            organization_scope: "platform".into(),
            feature: "platform".into(),
            role_key: entry.role_key,
            permissions: entry.permissions,
            entitled: true,
            directory_enabled: true,
            requires_totp_for_high_risk: true,
        })
        .collect()
}

impl SqlxIdentityStore {
    /// Rotate the explicitly local development controller credential. The
    /// database credential, active sessions, and persona contexts are changed
    /// in one transaction; callers should replace their private file only
    /// after this succeeds.
    pub async fn rotate_development_controller_password(
        &self,
        new_password: &str,
        at: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        if new_password.is_empty()
            || new_password.len() > 1024
            || new_password.chars().any(char::is_control)
        {
            return Err(IdentityStoreError::Primitive(
                IdentityError::PasswordHashingFailed,
            ));
        }
        let digest = PasswordService::default().hash(new_password)?;
        let mut transaction = self.pool().begin().await?;
        let principal_id: Uuid = sqlx::query_scalar(
            "SELECT id
             FROM erpx_identity.principals
             WHERE principal_ref = $1 AND status = 'active'
             FOR UPDATE",
        )
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
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
        sqlx::query(
            "UPDATE erpx_identity.development_persona_contexts
             SET revoked_at = $2, revoked_reason = 'controller_password_rotated'
             WHERE controller_principal_ref = $1 AND revoked_at IS NULL",
        )
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(revoked)
    }

    pub async fn reset_development_controller(
        &self,
        at: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        let mut transaction = self.pool().begin().await?;
        let principal_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id
             FROM erpx_identity.principals
             WHERE principal_ref = $1
             FOR UPDATE",
        )
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(principal_id) = principal_id else {
            transaction.commit().await?;
            return Ok(0);
        };
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
        sqlx::query(
            "UPDATE erpx_identity.development_persona_contexts
             SET revoked_at = $2, revoked_reason = 'development_access_reset'
             WHERE controller_principal_ref = $1 AND revoked_at IS NULL",
        )
        .bind(DEVELOPMENT_CONTROLLER_PRINCIPAL)
        .bind(at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM erpx_identity.password_credentials WHERE principal_id = $1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(revoked)
    }

    /// Idempotently publish secret-free persona manifests.  Passwords and
    /// directory bind material are owned by the local UCS authority instead.
    pub async fn reconcile_development_personas(
        &self,
        personas: &[DevelopmentPersona],
        now: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        let mut transaction = self.pool().begin().await?;
        for persona in personas {
            sqlx::query(
                "INSERT INTO erpx_identity.development_persona_manifests
                    (persona_key, principal_ref, directory_username, tenant_id,
                     organization_scope, role_key, feature, permissions, entitled,
                     directory_enabled, requires_totp_for_high_risk, manifest_version, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12)
                 ON CONFLICT (persona_key) DO UPDATE SET
                    principal_ref=EXCLUDED.principal_ref,
                    directory_username=EXCLUDED.directory_username,
                    tenant_id=EXCLUDED.tenant_id,
                    organization_scope=EXCLUDED.organization_scope,
                    role_key=EXCLUDED.role_key,
                    feature=EXCLUDED.feature,
                    permissions=EXCLUDED.permissions,
                    entitled=EXCLUDED.entitled,
                    directory_enabled=EXCLUDED.directory_enabled,
                    requires_totp_for_high_risk=EXCLUDED.requires_totp_for_high_risk,
                    manifest_version=erpx_identity.development_persona_manifests.manifest_version + 1,
                    updated_at=EXCLUDED.updated_at",
            )
            .bind(&persona.persona_key)
            .bind(&persona.principal_ref)
            .bind(&persona.directory_username)
            .bind(&persona.tenant_id)
            .bind(&persona.organization_scope)
            .bind(&persona.role_key)
            .bind(&persona.feature)
            .bind(&persona.permissions)
            .bind(persona.entitled)
            .bind(persona.directory_enabled)
            .bind(persona.requires_totp_for_high_risk)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(personas.len() as u64)
    }

    pub async fn list_development_personas(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<Vec<DevelopmentPersona>, IdentityStoreError> {
        let rows = sqlx::query(
            "SELECT persona_key, principal_ref, directory_username, tenant_id,
                    organization_scope, role_key, feature, permissions, entitled,
                    directory_enabled, requires_totp_for_high_risk
             FROM erpx_identity.development_persona_manifests
             WHERE ($1::text IS NULL OR tenant_id = $1)
             ORDER BY tenant_id NULLS FIRST, persona_key",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(persona_from_row).collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn begin_development_persona_context(
        &self,
        token: &str,
        controller_principal: &str,
        session_id: Uuid,
        device_id: Uuid,
        persona_key: &str,
        feature: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<DevelopmentPersonaContext, IdentityStoreError> {
        if token.len() < 32
            || token.len() > 256
            || token.chars().any(char::is_control)
            || controller_principal != DEVELOPMENT_CONTROLLER_PRINCIPAL
            || session_id.is_nil()
            || device_id.is_nil()
            || persona_key.trim().is_empty()
            || feature.trim().is_empty()
            || reason.trim().is_empty()
            || reason.len() > 512
            || reason.chars().any(char::is_control)
        {
            return Err(IdentityStoreError::InvalidRecord(
                "development context input is invalid".into(),
            ));
        }
        let token_hash = context_token_hash(token);
        let context_id = Uuid::new_v4();
        let correlation_id = Uuid::new_v4();
        let expires_at = context_expiry(now);
        let mut transaction = self.pool().begin().await?;
        let row = sqlx::query(
            "SELECT persona_key, principal_ref, directory_username, tenant_id,
                    organization_scope, role_key, feature, permissions, entitled,
                    directory_enabled, requires_totp_for_high_risk
             FROM erpx_identity.development_persona_manifests
             WHERE persona_key = $1 AND feature = $2 AND directory_enabled",
        )
        .bind(persona_key)
        .bind(feature)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| IdentityStoreError::InvalidRecord("development persona not found".into()))?;
        let persona = persona_from_row(row)?;
        sqlx::query(
            "UPDATE erpx_identity.development_persona_contexts
             SET revoked_at = $2, revoked_reason = 'switched'
             WHERE controller_session_id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_identity.development_persona_contexts
                (id, token_hash, controller_principal_ref, controller_session_id,
                 controller_device_id, persona_key, role_assignment_ref, tenant_id,
                 organization_scope, feature, reason, correlation_id, issued_at, expires_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(context_id)
        .bind(token_hash.as_slice())
        .bind(controller_principal)
        .bind(session_id)
        .bind(device_id)
        .bind(&persona.persona_key)
        .bind(&persona.role_key)
        .bind(&persona.tenant_id)
        .bind(&persona.organization_scope)
        .bind(&persona.feature)
        .bind(reason)
        .bind(correlation_id)
        .bind(now)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await?;
        append_development_audit(
            &mut transaction,
            controller_principal,
            &persona,
            reason,
            correlation_id,
            "issued",
            now,
        )
        .await?;
        transaction.commit().await?;
        Ok(DevelopmentPersonaContext {
            id: context_id,
            controller_principal: controller_principal.into(),
            controller_session_id: session_id,
            controller_device_id: device_id,
            persona,
            reason: reason.into(),
            correlation_id,
            issued_at: now,
            expires_at,
            revoked_at: None,
        })
    }

    pub async fn get_active_development_persona_context_for_session(
        &self,
        session_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<DevelopmentPersonaContext>, IdentityStoreError> {
        let row = sqlx::query(
            "SELECT c.id, c.controller_principal_ref, c.controller_session_id,
                    c.controller_device_id, c.persona_key, c.tenant_id,
                    c.organization_scope, c.feature, c.reason, c.correlation_id,
                    c.issued_at, c.expires_at, c.revoked_at,
                    p.principal_ref, p.directory_username, p.role_key,
                    p.permissions, p.entitled, p.directory_enabled,
                    p.requires_totp_for_high_risk
             FROM erpx_identity.development_persona_contexts c
             JOIN erpx_identity.development_persona_manifests p ON p.persona_key = c.persona_key
             WHERE c.controller_session_id = $1
               AND c.revoked_at IS NULL
               AND c.expires_at > $2
             ORDER BY c.issued_at DESC
             LIMIT 1",
        )
        .bind(session_id)
        .bind(now)
        .fetch_optional(self.pool())
        .await?;
        row.map(context_from_row).transpose()
    }

    pub async fn get_development_persona_context(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<DevelopmentPersonaContext, IdentityStoreError> {
        if token.len() < 32 || token.len() > 256 || token.chars().any(char::is_control) {
            return Err(IdentityStoreError::InvalidRecord(
                "development context token is invalid".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT c.id, c.controller_principal_ref, c.controller_session_id,
                    c.controller_device_id, c.persona_key, c.tenant_id,
                    c.organization_scope, c.feature, c.reason, c.correlation_id,
                    c.issued_at, c.expires_at, c.revoked_at,
                    p.principal_ref, p.directory_username, p.role_key,
                    p.permissions, p.entitled, p.directory_enabled,
                    p.requires_totp_for_high_risk
             FROM erpx_identity.development_persona_contexts c
             JOIN erpx_identity.development_persona_manifests p ON p.persona_key = c.persona_key
             WHERE c.token_hash = $1",
        )
        .bind(context_token_hash(token).as_slice())
        .fetch_optional(self.pool())
        .await?
        .ok_or(IdentityStoreError::InvalidRecord(
            "development context not found".into(),
        ))?;
        let context = context_from_row(row)?;
        if !context.active_at(now) {
            return Err(IdentityStoreError::InvalidRecord(
                "development context expired or revoked".into(),
            ));
        }
        Ok(context)
    }

    pub async fn end_development_persona_context(
        &self,
        token: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, IdentityStoreError> {
        if reason.trim().is_empty() || reason.len() > 512 || reason.chars().any(char::is_control) {
            return Err(IdentityStoreError::InvalidRecord(
                "development context end reason is invalid".into(),
            ));
        }
        let mut transaction = self.pool().begin().await?;
        let row = sqlx::query(
            "SELECT c.id, c.controller_principal_ref, c.controller_session_id,
                    c.controller_device_id, c.persona_key, c.tenant_id,
                    c.organization_scope, c.feature, c.reason, c.correlation_id,
                    c.issued_at, c.expires_at, c.revoked_at,
                    p.principal_ref, p.directory_username, p.role_key,
                    p.permissions, p.entitled, p.directory_enabled,
                    p.requires_totp_for_high_risk
             FROM erpx_identity.development_persona_contexts c
             JOIN erpx_identity.development_persona_manifests p ON p.persona_key = c.persona_key
             WHERE c.token_hash = $1 FOR UPDATE",
        )
        .bind(context_token_hash(token).as_slice())
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(false);
        };
        let context = context_from_row(row)?;
        if context.revoked_at.is_some() {
            transaction.commit().await?;
            return Ok(false);
        }
        sqlx::query(
            "UPDATE erpx_identity.development_persona_contexts
             SET revoked_at = $2, revoked_reason = $3 WHERE id = $1",
        )
        .bind(context.id)
        .bind(now)
        .bind(reason)
        .execute(&mut *transaction)
        .await?;
        append_development_audit(
            &mut transaction,
            &context.controller_principal,
            &context.persona,
            reason,
            context.correlation_id,
            "ended",
            now,
        )
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn revoke_development_contexts_for_session(
        &self,
        session_id: Uuid,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<u64, IdentityStoreError> {
        let result = sqlx::query(
            "UPDATE erpx_identity.development_persona_contexts
             SET revoked_at = $2, revoked_reason = $3
             WHERE controller_session_id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(now)
        .bind(reason)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
}

fn persona_from_row(row: sqlx::postgres::PgRow) -> Result<DevelopmentPersona, IdentityStoreError> {
    Ok(DevelopmentPersona {
        persona_key: row.try_get("persona_key")?,
        principal_ref: row.try_get("principal_ref")?,
        directory_username: row.try_get("directory_username")?,
        tenant_id: row.try_get("tenant_id")?,
        organization_scope: row.try_get("organization_scope")?,
        role_key: row.try_get("role_key")?,
        feature: row.try_get("feature")?,
        permissions: row.try_get("permissions")?,
        entitled: row.try_get("entitled")?,
        directory_enabled: row.try_get("directory_enabled")?,
        requires_totp_for_high_risk: row.try_get("requires_totp_for_high_risk")?,
    })
}

fn context_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<DevelopmentPersonaContext, IdentityStoreError> {
    let persona = DevelopmentPersona {
        persona_key: row.try_get("persona_key")?,
        principal_ref: row.try_get("principal_ref")?,
        directory_username: row.try_get("directory_username")?,
        tenant_id: row.try_get("tenant_id")?,
        organization_scope: row.try_get("organization_scope")?,
        role_key: row.try_get("role_key")?,
        feature: row.try_get("feature")?,
        permissions: row.try_get("permissions")?,
        entitled: row.try_get("entitled")?,
        directory_enabled: row.try_get("directory_enabled")?,
        requires_totp_for_high_risk: row.try_get("requires_totp_for_high_risk")?,
    };
    Ok(DevelopmentPersonaContext {
        id: row.try_get("id")?,
        controller_principal: row.try_get("controller_principal_ref")?,
        controller_session_id: row.try_get("controller_session_id")?,
        controller_device_id: row.try_get("controller_device_id")?,
        persona,
        reason: row.try_get("reason")?,
        correlation_id: row.try_get("correlation_id")?,
        issued_at: row.try_get("issued_at")?,
        expires_at: row.try_get("expires_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

async fn append_development_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    controller_principal: &str,
    persona: &DevelopmentPersona,
    details: &str,
    correlation_id: Uuid,
    outcome: &str,
    recorded_at: DateTime<Utc>,
) -> Result<(), IdentityStoreError> {
    sqlx::query(
        "INSERT INTO erpx_identity.development_audit
            (id, controller_principal_ref, persona_principal_ref, persona_key,
             role_key, tenant_id, organization_scope, feature, correlation_id,
             outcome, recorded_at, details)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(Uuid::new_v4())
    .bind(controller_principal)
    .bind(&persona.principal_ref)
    .bind(&persona.persona_key)
    .bind(&persona.role_key)
    .bind(&persona.tenant_id)
    .bind(&persona.organization_scope)
    .bind(&persona.feature)
    .bind(correlation_id)
    .bind(outcome)
    .bind(recorded_at)
    .bind(serde_json::json!({ "reason": details }))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn persona_fixture(
    tenant_id: &str,
    key: &str,
    role_key: &str,
    entitled: bool,
) -> DevelopmentPersona {
    let feature = role_key.split('_').next().unwrap_or("operations");
    DevelopmentPersona {
        persona_key: format!("{tenant_id}:{key}"),
        principal_ref: format!("dev:{tenant_id}:{key}"),
        directory_username: format!("{key}@{tenant_id}.ucs.test"),
        tenant_id: Some(tenant_id.into()),
        organization_scope: "branch:all".into(),
        role_key: role_key.into(),
        feature: feature.into(),
        permissions: vec![format!("{feature}.read")],
        entitled,
        directory_enabled: true,
        requires_totp_for_high_risk: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect()
    }

    #[test]
    fn every_gate_is_required_and_release_builds_cannot_enable_it() {
        let environment = env(&[
            ("ERPX_DEVELOPMENT_PROFILE", "local"),
            ("ERPX_DEV_PERSONA_MODE", "enabled"),
            ("ERPX_DEVELOPMENT_CLUSTER", "local-ucs-lab"),
        ]);
        let urls = vec!["postgres://user:pass@127.0.0.1:5432/erpx".into()];
        assert!(require_development_gates(true, "127.0.0.1:3004", &urls, &environment).is_ok());
        assert!(require_development_gates(false, "127.0.0.1:3004", &urls, &environment).is_err());
        assert!(require_development_gates(true, "0.0.0.0:3004", &urls, &environment).is_err());
        assert!(
            require_development_gates(
                true,
                "127.0.0.1:3004",
                &["postgres://db.internal/erpx".into()],
                &environment
            )
            .is_err()
        );
    }

    #[test]
    fn context_tokens_are_hashed_and_catalog_has_exact_role_counts() {
        let token = issue_context_token();
        assert_ne!(token.as_bytes(), context_token_hash(&token).as_slice());
        let catalog = permission_catalog();
        assert_eq!(catalog.len(), 73);
        assert_eq!(demo_personas("demo-tenant", &BUSINESS_UNITS).len(), 69);
        assert!(
            catalog
                .iter()
                .flat_map(|entry| entry.permissions.iter())
                .all(|permission| !permission.contains('*'))
        );
        let customer_operations = catalog
            .iter()
            .find(|entry| entry.role_key == "platform_customer_operations")
            .expect("platform customer operations role");
        for permission in [
            "platform.customer.read",
            "platform.tenant.read",
            "platform.tenant.enter",
        ] {
            assert!(
                customer_operations
                    .permissions
                    .iter()
                    .any(|candidate| candidate == permission),
                "missing {permission}"
            );
        }
    }

    #[test]
    fn controller_configuration_and_permissions_are_closed_and_targeted() {
        let mut environment = env(&[
            ("ERPX_DEVELOPMENT_PROFILE", "local"),
            ("ERPX_DEV_PERSONA_MODE", "enabled"),
            ("ERPX_DEVELOPMENT_CLUSTER", "local-ucs-lab"),
            (
                "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
                "restricted_tenant_full_admin",
            ),
            (
                "ERPX_DEV_CONTROLLER_TARGET_TENANT",
                "demo-tenant-restricted",
            ),
            ("ERPX_DEV_CONTROLLER_MFA_BYPASS", "enabled"),
            ("ERPX_DEV_CONTROLLER_SOD_BYPASS", "enabled"),
        ]);
        let urls = vec!["postgres://user:pass@127.0.0.1:5432/erpx".into()];
        let config = development_controller_role_config_from_env(
            true,
            "127.0.0.1:3004",
            &urls,
            &environment,
        )
        .expect("valid controller environment")
        .expect("enabled controller role");
        assert_eq!(config.target_tenant, DEVELOPMENT_CONTROLLER_TARGET_TENANT);

        let fresh_tenant = "4017ec64-6620-50e2-bfdc-14d0a9b3c414";
        let mut fresh_environment = environment.clone();
        fresh_environment.insert(
            "ERPX_DEVELOPMENT_PROFILE".into(),
            DEVELOPMENT_FRESH_PROFILE.into(),
        );
        fresh_environment.insert(
            "ERPX_DEV_CONTROLLER_TARGET_TENANT".into(),
            fresh_tenant.into(),
        );
        let fresh_config = development_controller_role_config_from_env(
            true,
            "127.0.0.1:3004",
            &urls,
            &fresh_environment,
        )
        .expect("valid fresh controller environment")
        .expect("enabled fresh controller role");
        assert_eq!(fresh_config.target_tenant, fresh_tenant);
        fresh_environment.insert(
            "ERPX_DEV_CONTROLLER_TARGET_TENANT".into(),
            "demo-tenant-restricted".into(),
        );
        assert!(
            development_controller_role_config_from_env(
                true,
                "127.0.0.1:3004",
                &urls,
                &fresh_environment,
            )
            .is_err()
        );

        let permissions = development_controller_permissions();
        assert!(permissions.contains(&DEVELOPMENT_MFA_BYPASS_PERMISSION.into()));
        assert!(permissions.contains(&DEVELOPMENT_SOD_BYPASS_PERMISSION.into()));
        assert!(
            permissions
                .iter()
                .all(|permission| !permission.contains('*'))
        );
        assert!(platform_personas().iter().all(|persona| {
            !persona.role_key.contains("superuser")
                && persona.role_key != DEVELOPMENT_CONTROLLER_ROLE
                && !persona.permissions.iter().any(|permission| {
                    permission == DEVELOPMENT_MFA_BYPASS_PERMISSION
                        || permission == DEVELOPMENT_SOD_BYPASS_PERMISSION
                })
        }));

        environment.insert("ERPX_DEV_CONTROLLER_MFA_BYPASS".into(), "true".into());
        assert!(
            development_controller_role_config_from_env(
                true,
                "127.0.0.1:3004",
                &urls,
                &environment,
            )
            .is_err()
        );
        environment.insert("ERPX_DEV_CONTROLLER_MFA_BYPASS".into(), "enabled".into());
        assert!(
            development_controller_role_config_from_env(
                false,
                "127.0.0.1:3004",
                &urls,
                &environment,
            )
            .is_err()
        );
        assert!(
            development_controller_role_config_from_env(true, "0.0.0.0:3004", &urls, &environment,)
                .is_err()
        );
        assert!(
            development_controller_role_config_from_env(
                true,
                "127.0.0.1:3004",
                &["postgres://user:pass@database.internal:5432/erpx".into()],
                &environment,
            )
            .is_err()
        );
        environment.insert("ERPX_DEVELOPMENT_CLUSTER".into(), "production".into());
        assert!(
            development_controller_role_config_from_env(
                true,
                "127.0.0.1:3004",
                &urls,
                &environment,
            )
            .is_err()
        );
        environment.insert("ERPX_DEVELOPMENT_CLUSTER".into(), "local-ucs-lab".into());
        environment.remove("ERPX_DEV_CONTROLLER_SOD_BYPASS");
        assert!(
            development_controller_role_config_from_env(
                true,
                "127.0.0.1:3004",
                &urls,
                &environment,
            )
            .is_err()
        );
    }

    #[test]
    fn context_lifetime_is_bounded_to_thirty_days() {
        let issued = Utc::now();
        assert_eq!(context_expiry(issued), issued + Duration::days(30));
    }
}
