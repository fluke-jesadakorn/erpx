use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt,
    future::Future,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, Instant},
};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_cqrs::{CONTRACT_VERSION, CommandEnvelope, CommandHandler, IdempotencyKey};
use erpx_cqrs_sqlx::WritePool;
use erpx_domain::platform_completion::PitrRecoveryPoint;
use erpx_kernel::{
    ActorAttribution as KernelActorAttribution, ActorKind as KernelActorKind, AuditContext,
    AuthorityScope, CommandId, CorrelationId, FixedClock, HumanDecisionProof,
};
use erpx_migration::{migrate_holding_read, migrate_holding_write};
use erpx_organization::ports::AuthorizationRequest;
use erpx_organization::{
    AuthorizationPort as OrganizationAuthorizationPort, CreateOrganizationNode,
    EntitlementPort as OrganizationEntitlementPort, OrganizationCommandHandler, OrganizationNodeId,
    OrganizationPlacement, PortError as OrganizationPortError,
};
use erpx_organization_sqlx::{
    OrganizationCommandAdapter, OrganizationHierarchyRepository, OrganizationWriteRepository,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    PgPool, Postgres, Row, Transaction,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    BranchDatabaseRoute, BranchProvisioningJob, BranchProvisioningState, HoldingStoreRegistration,
    PersistenceError, SqlxControlPlane, SqlxHoldingStoreRegistry,
    object_storage::{
        MinioObjectStore, MinioObjectStoreConfig, ObjectInventoryEntry, ObjectStorageError,
    },
};

const MAX_HEALTH_DETAILS_BYTES: usize = 16 * 1024;
pub const MAX_HOLDING_PITR_INVENTORY_ENTRIES: u32 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseSide {
    Write,
    Read,
}

impl DatabaseSide {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Read => "read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingDatabaseHealth {
    pub holding_id: String,
    pub side: DatabaseSide,
    pub database_key: String,
    pub schema_version: String,
    pub reachable: bool,
    pub checked_at: DateTime<Utc>,
    pub details: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingStorageHealth {
    pub holding_id: String,
    pub storage_namespace: String,
    pub reachable: bool,
    pub checked_at: DateTime<Utc>,
    pub details: Value,
}

/// Durable, secret-free owner bootstrap metadata. The recovery code itself is
/// generated and hashed by the identity write-side transaction; this model
/// contains only the references needed to bind that transaction to one holding
/// and holding.
#[derive(Clone, PartialEq, Eq)]
pub struct HoldingOwnerSeed {
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub principal_ref: String,
    pub approver_ref: String,
    pub role_assignment_id: Uuid,
    pub state: String,
    pub recovery_issued_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for HoldingOwnerSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingOwnerSeed")
            .field("holding_id", &self.holding_id)
            .field("data_profile_key", &self.data_profile_key)
            .field("holding_id", &self.holding_id)
            .field("principal_ref", &self.principal_ref)
            .field("approver_ref", &self.approver_ref)
            .field("role_assignment_id", &self.role_assignment_id)
            .field("state", &self.state)
            .field("recovery_issued_at", &self.recovery_issued_at)
            .finish()
    }
}

/// Map the opaque registry holding reference to the stable typed scope used
/// by role assignments. The reference is namespaced so it cannot collide with
/// unrelated UUID mappings in other contexts.
pub fn holding_holding_scope_id(data_profile_key: &str) -> Uuid {
    let mut bytes = *Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:holding-scope:{data_profile_key}").as_bytes(),
    )
    .as_bytes();
    // Holding scope IDs are also first-class Organization record IDs. Keep
    // the mapping deterministic while satisfying the opaque UUIDv4-shaped
    // routing contract used by Item Directory and canonical record routes.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn holding_owner_assignment_id(holding_id: &str, principal_ref: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:holding-owner-assignment:{holding_id}:{principal_ref}").as_bytes(),
    )
}

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingBackupRun {
    pub id: Uuid,
    pub holding_id: String,
    pub kind: String,
    pub status: String,
    pub backup_ref: String,
    pub restore_target_ref: Option<String>,
    pub pitr_recovery_point: Option<PitrRecoveryPoint>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub verified: bool,
    pub error: Option<String>,
}

pub const MAX_HOLDING_BACKUP_JOB_PAGE_SIZE: u32 = 100;
pub const MAX_HOLDING_BACKUP_JOB_ATTEMPTS: u16 = 5;
pub const MAX_HOLDING_BACKUP_JOB_PROJECTION_BATCH_SIZE: u32 = 1_000;
pub const MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS: i64 = 30;
pub const MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS: i64 = 900;
pub const HOLDING_BACKUP_JOB_AGGREGATE: &str = "platform.holding_backup_job";
pub const HOLDING_BACKUP_JOB_SCHEDULED_EVENT: &str = "platform.holding_backup_job.scheduled";
pub const HOLDING_BACKUP_JOB_CLAIMED_EVENT: &str = "platform.holding_backup_job.claimed";
pub const HOLDING_BACKUP_JOB_COMPLETED_EVENT: &str = "platform.holding_backup_job.completed";
pub const HOLDING_BACKUP_JOB_FAILED_EVENT: &str = "platform.holding_backup_job.failed";

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingBackupJobRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub kind: String,
    pub backup_ref: String,
    pub restore_target_ref: Option<String>,
    pub candidate_read_database_key: Option<String>,
    pub source_version: Option<u64>,
    pub source_projection: String,
    pub pitr_recovery_point: Option<PitrRecoveryPoint>,
    pub available_at: DateTime<Utc>,
    pub idempotency_key: String,
    pub max_attempts: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingBackupJobReceipt {
    pub id: Uuid,
    pub holding_id: String,
    pub kind: String,
    pub state: String,
    pub available_at: DateTime<Utc>,
    pub max_attempts: u16,
    pub duplicate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingBackupJobSummary {
    pub id: Uuid,
    pub holding_id: String,
    pub kind: String,
    pub state: String,
    pub available_at: DateTime<Utc>,
    pub attempts: u16,
    pub max_attempts: u16,
    pub verified: bool,
    pub last_error_present: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for HoldingBackupJobRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingBackupJobRequest")
            .field("holding_id", &"<opaque>")
            .field("data_profile_key", &"<opaque>")
            .field("kind", &self.kind)
            .field("backup_ref", &"<redacted>")
            .field(
                "restore_target_ref",
                &self.restore_target_ref.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "candidate_read_database_key",
                &self
                    .candidate_read_database_key
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field("source_version", &self.source_version)
            .field(
                "pitr_recovery_point",
                &self.pitr_recovery_point.as_ref().map(|_| "<redacted>"),
            )
            .field("available_at", &self.available_at)
            .field("idempotency_key", &"<redacted>")
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingBackupJobClaim {
    pub id: Uuid,
    pub holding_id: String,
    pub data_profile_key: String,
    pub kind: String,
    pub run: HoldingBackupRun,
    pub candidate_read_database_key: Option<String>,
    pub source_version: Option<u64>,
    pub source_projection: String,
    pub attempt: u16,
    pub claim_id: Uuid,
    pub worker_id: Uuid,
}

impl fmt::Debug for HoldingBackupJobClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingBackupJobClaim")
            .field("id", &self.id)
            .field("holding_id", &"<opaque>")
            .field("data_profile_key", &"<opaque>")
            .field("kind", &self.kind)
            .field("run", &self.run)
            .field(
                "candidate_read_database_key",
                &self
                    .candidate_read_database_key
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .field("source_version", &self.source_version)
            .field("source_projection", &self.source_projection)
            .field("attempt", &self.attempt)
            .field("claim_id", &self.claim_id)
            .field("worker_id", &self.worker_id)
            .finish()
    }
}

/// Receipt returned only after a verified restore test has activated the
/// exact generation-bound candidate through a routed holding boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingRestoreTestActivation {
    pub run: HoldingBackupRun,
    pub candidate_read_database_key: String,
    pub generation: u64,
    pub source_version: u64,
}

impl fmt::Debug for HoldingBackupRun {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingBackupRun")
            .field("id", &self.id)
            .field("holding_id", &self.holding_id)
            .field("kind", &self.kind)
            .field("status", &self.status)
            .field("backup_ref", &"<redacted>")
            .field(
                "restore_target_ref",
                &self.restore_target_ref.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "pitr_recovery_point",
                &self.pitr_recovery_point.as_ref().map(|_| "<redacted>"),
            )
            .field("started_at", &self.started_at)
            .field("completed_at", &self.completed_at)
            .field("verified", &self.verified)
            .field("error", &self.error.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// The infrastructure-side equivalent of the worker readiness state. It is
/// deliberately kept in this crate so live probes do not depend on the worker
/// executable and so callers cannot accidentally treat missing metadata as
/// healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessObservationState {
    Ready,
    NotProbed,
    Unavailable,
}

impl ReadinessObservationState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotProbed => "not_probed",
            Self::Unavailable => "unavailable",
        }
    }

    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadinessObservationPolicy {
    pub database_max_age: Duration,
    pub storage_max_age: Duration,
    pub projection_max_age: Duration,
    pub backup_max_age: Duration,
}

impl Default for ReadinessObservationPolicy {
    fn default() -> Self {
        Self {
            database_max_age: Duration::minutes(5),
            storage_max_age: Duration::minutes(5),
            projection_max_age: Duration::minutes(5),
            backup_max_age: Duration::hours(24),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadinessComponentObservation {
    pub state: ReadinessObservationState,
    pub checked_at: Option<DateTime<Utc>>,
    pub reason_code: Option<String>,
}

impl ReadinessComponentObservation {
    fn not_probed() -> Self {
        Self {
            state: ReadinessObservationState::NotProbed,
            checked_at: None,
            reason_code: None,
        }
    }

    fn unavailable(reason_code: &'static str, checked_at: Option<DateTime<Utc>>) -> Self {
        Self {
            state: ReadinessObservationState::Unavailable,
            checked_at,
            reason_code: Some(reason_code.into()),
        }
    }

    fn ready(checked_at: DateTime<Utc>) -> Self {
        Self {
            state: ReadinessObservationState::Ready,
            checked_at: Some(checked_at),
            reason_code: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseReadinessObservation {
    pub state: ReadinessObservationState,
    pub schema_version: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
    pub reason_code: Option<String>,
}

impl DatabaseReadinessObservation {
    fn not_probed() -> Self {
        Self {
            state: ReadinessObservationState::NotProbed,
            schema_version: None,
            checked_at: None,
            reason_code: None,
        }
    }

    fn unavailable(
        reason_code: &'static str,
        schema_version: Option<String>,
        checked_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            state: ReadinessObservationState::Unavailable,
            schema_version,
            checked_at,
            reason_code: Some(reason_code.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionFreshnessObservation {
    pub state: ReadinessObservationState,
    pub source_version: Option<u64>,
    pub updated_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub has_error: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPitrObservation {
    pub state: ReadinessObservationState,
    pub run_id: Option<Uuid>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub verified: bool,
    pub has_error: bool,
    pub restore_target_present: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingReadinessMetadata {
    pub holding_id: String,
    pub registry_state: Option<String>,
    pub registry_version: Option<u64>,
    pub write_database: DatabaseReadinessObservation,
    pub read_database: DatabaseReadinessObservation,
    pub storage: ReadinessComponentObservation,
    pub projection_freshness: ProjectionFreshnessObservation,
    pub backup_pitr: BackupPitrObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingRoutingObservation {
    pub holding_id: String,
    pub state: ReadinessObservationState,
    pub registry_state: Option<String>,
    pub registry_version: Option<u64>,
    pub write_database: DatabaseReadinessObservation,
    pub read_database: DatabaseReadinessObservation,
    pub storage: ReadinessComponentObservation,
    pub checked_at: DateTime<Utc>,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingReadinessObservation {
    pub holding_id: String,
    pub routing: HoldingRoutingObservation,
    pub projection_freshness: ProjectionFreshnessObservation,
    pub backup_pitr: BackupPitrObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisioningState {
    Provisioning,
    Active,
    Suspended,
    Recovery,
    Failed,
}

impl ProvisioningState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Provisioning => "provisioning",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Recovery => "recovery",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HoldingProvisioner {
    control: Arc<SqlxControlPlane>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingDatabaseProvisioningConfig {
    /// A server-side PostgreSQL administrative URL. It is never returned to
    /// callers or included in provisioning receipts.
    pub admin_database_url: String,
    /// A server-side URL template containing exactly one `{database}` token.
    pub database_url_template: String,
}

impl fmt::Debug for HoldingDatabaseProvisioningConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingDatabaseProvisioningConfig")
            .field("admin_database_url", &"<redacted>")
            .field("database_url_template", &"<redacted>")
            .finish()
    }
}

impl HoldingDatabaseProvisioningConfig {
    /// Construct and validate routing configuration from already-resolved
    /// server-side values.
    pub fn from_values(
        admin_database_url: impl Into<String>,
        database_url_template: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let config = Self {
            admin_database_url: admin_database_url.into(),
            database_url_template: database_url_template.into(),
        };
        validate_database_config(&config)?;
        Ok(config)
    }

    /// Load the server-only holding database routing configuration.
    ///
    /// The administrative URL and URL template remain in the process boundary;
    /// callers receive only the validated configuration object and never a
    /// serialized credential. Requiring both variables prevents a worker or
    /// server from falling back to a shared control-plane database.
    pub fn from_env() -> Result<Self, PersistenceError> {
        let admin_database_url = env::var("ERPX_HOLDING_ADMIN_DATABASE_URL").map_err(|_| {
            PersistenceError::Configuration(
                "ERPX_HOLDING_ADMIN_DATABASE_URL is required for holding routing",
            )
        })?;
        let database_url_template =
            env::var("ERPX_HOLDING_DATABASE_URL_TEMPLATE").map_err(|_| {
                PersistenceError::Configuration(
                    "ERPX_HOLDING_DATABASE_URL_TEMPLATE is required for holding routing",
                )
            })?;
        Self::from_values(admin_database_url, database_url_template)
    }
}

/// In-memory connection material resolved from one approved database profile.
/// Only the profile key is printable; URL, username, and password are retained
/// inside `PgConnectOptions` and never rendered back into a DSN.
#[derive(Clone)]
pub struct ApprovedDatabaseProfileConnection {
    profile_key: String,
    options: PgConnectOptions,
}

impl fmt::Debug for ApprovedDatabaseProfileConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApprovedDatabaseProfileConnection")
            .field("profile_key", &self.profile_key)
            .field("connection", &"<redacted>")
            .finish()
    }
}

impl ApprovedDatabaseProfileConnection {
    pub fn from_values(
        profile_key: impl Into<String>,
        credential_free_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, PersistenceError> {
        let profile_key = profile_key.into();
        if profile_key.is_empty()
            || profile_key.len() > 64
            || !profile_key.starts_with(|value: char| value.is_ascii_lowercase())
            || profile_key.chars().any(|value| {
                !(value.is_ascii_lowercase()
                    || value.is_ascii_digit()
                    || matches!(value, '_' | '-'))
            })
        {
            return Err(PersistenceError::Configuration(
                "database profile key is invalid",
            ));
        }
        reject_database_profile_url_credentials(credential_free_url)?;
        if username.is_empty()
            || username.len() > 63
            || username.chars().any(char::is_control)
            || password.is_empty()
            || password.len() > 4096
        {
            return Err(PersistenceError::Configuration(
                "database credential profile is invalid",
            ));
        }
        let mut options = PgConnectOptions::from_str(credential_free_url).map_err(|_| {
            PersistenceError::Configuration("database profile URL must be a PostgreSQL URL")
        })?;
        options = options.username(username).password(password);
        if !is_loopback_database_host(options.get_host()) {
            options = options.ssl_mode(PgSslMode::VerifyFull);
        }
        Ok(Self {
            profile_key,
            options,
        })
    }

    pub fn from_environment(
        profile_key: &str,
        credential_profile_key: &str,
    ) -> Result<Self, PersistenceError> {
        let token = database_profile_environment_token(credential_profile_key)?;
        let required = |suffix: &str| {
            env::var(format!("ERPX_DATABASE_PROFILE_{token}_{suffix}")).map_err(|_| {
                PersistenceError::Configuration(
                    "approved database credential profile is not configured",
                )
            })
        };
        let url = required("URL")?;
        let username = required("USERNAME")?;
        let password = required("PASSWORD")?;
        Self::from_values(profile_key, &url, &username, &password)
    }

    pub fn profile_key(&self) -> &str {
        &self.profile_key
    }

    pub fn username(&self) -> &str {
        self.options.get_username()
    }

    pub async fn connect(&self, database_key: &str) -> Result<PgPool, PersistenceError> {
        validate_database_key(database_key, "database_key")?;
        Ok(PgPoolOptions::new()
            .max_connections(4)
            .connect_with(self.database_options(database_key))
            .await?)
    }

    fn admin_options(&self) -> PgConnectOptions {
        self.options.clone()
    }

    fn database_options(&self, database_key: &str) -> PgConnectOptions {
        self.options.clone().database(database_key)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PhysicalHoldingDatabases {
    pub holding_id: String,
    pub write_database_key: String,
    pub read_database_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingOrganizationBootstrap {
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub read_projected: bool,
}

impl fmt::Debug for PhysicalHoldingDatabases {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PhysicalHoldingDatabases")
            .field("holding_id", &self.holding_id)
            .field("write_database_key", &"<redacted>")
            .field("read_database_key", &"<redacted>")
            .finish()
    }
}

impl PhysicalHoldingDatabases {
    pub fn new(
        holding_id: impl Into<String>,
        write_database_key: impl Into<String>,
        read_database_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let databases = Self {
            holding_id: holding_id.into(),
            write_database_key: write_database_key.into(),
            read_database_key: read_database_key.into(),
        };
        validate_ref(&databases.holding_id, "holding_id")?;
        validate_database_key(&databases.write_database_key, "write_database_key")?;
        validate_database_key(&databases.read_database_key, "read_database_key")?;
        if databases.write_database_key == databases.read_database_key {
            return Err(PersistenceError::Configuration(
                "holding write/read database keys must be distinct",
            ));
        }
        Ok(databases)
    }

    /// Bind a replacement read database to this physical holding inventory.
    /// The resulting target cannot be constructed from a different holding's
    /// inventory and cannot point at either currently provisioned database.
    pub fn replacement_read_database(
        &self,
        candidate_read_database_key: &str,
    ) -> Result<HoldingReadDatabaseReplacement, PersistenceError> {
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        if candidate_read_database_key == self.write_database_key
            || candidate_read_database_key == self.read_database_key
        {
            return Err(PersistenceError::Configuration(
                "replacement read database must be distinct from provisioned databases",
            ));
        }
        Ok(HoldingReadDatabaseReplacement {
            holding_id: self.holding_id.clone(),
            current_read_database_key: self.read_database_key.clone(),
            candidate_read_database_key: candidate_read_database_key.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingReadDatabaseReplacement {
    holding_id: String,
    current_read_database_key: String,
    candidate_read_database_key: String,
}

impl HoldingReadDatabaseReplacement {
    pub fn holding_id(&self) -> &str {
        &self.holding_id
    }

    pub fn current_read_database_key(&self) -> &str {
        &self.current_read_database_key
    }

    pub fn candidate_read_database_key(&self) -> &str {
        &self.candidate_read_database_key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupRunKind {
    Full,
    Pitr,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingBackupTarget {
    databases: PhysicalHoldingDatabases,
    storage_namespace: Option<String>,
    backup_chain_ref: String,
    backup_ref: String,
    pitr_recovery_point: Option<PitrRecoveryPoint>,
}

impl fmt::Debug for HoldingBackupTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingBackupTarget")
            .field("holding_id", &self.databases.holding_id)
            .field("write_database_key", &self.databases.write_database_key)
            .field("read_database_key", &self.databases.read_database_key)
            .field("backup_chain_ref", &"<redacted>")
            .field("backup_ref", &"<redacted>")
            .field(
                "pitr_recovery_point",
                &self.pitr_recovery_point.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl HoldingBackupTarget {
    pub fn for_run(
        databases: PhysicalHoldingDatabases,
        backup_chain_ref: &str,
        run: &HoldingBackupRun,
    ) -> Result<Self, PersistenceError> {
        validate_ref(backup_chain_ref, "backup_chain_ref")?;
        validate_backup_run_shape(run)?;
        if databases.holding_id != run.holding_id {
            return Err(PersistenceError::InvalidData(
                "backup target holding does not match the backup run".into(),
            ));
        }
        Ok(Self {
            databases,
            storage_namespace: None,
            backup_chain_ref: backup_chain_ref.into(),
            backup_ref: run.backup_ref.clone(),
            pitr_recovery_point: run.pitr_recovery_point.clone(),
        })
    }

    /// Bind the target to the registry-owned storage namespace. The legacy
    /// constructor remains available for isolated adapter tests and falls
    /// back to the adapter's explicitly configured namespace.
    pub fn for_run_in_namespace(
        databases: PhysicalHoldingDatabases,
        storage_namespace: &str,
        backup_chain_ref: &str,
        run: &HoldingBackupRun,
    ) -> Result<Self, PersistenceError> {
        MinioObjectStore::validate_namespace_reference(storage_namespace).map_err(|_| {
            PersistenceError::InvalidData("backup storage namespace is invalid".into())
        })?;
        let mut target = Self::for_run(databases, backup_chain_ref, run)?;
        target.storage_namespace = Some(storage_namespace.into());
        Ok(target)
    }

    pub fn holding_id(&self) -> &str {
        &self.databases.holding_id
    }

    pub fn databases(&self) -> &PhysicalHoldingDatabases {
        &self.databases
    }

    pub fn backup_chain_ref(&self) -> &str {
        &self.backup_chain_ref
    }

    pub fn backup_ref(&self) -> &str {
        &self.backup_ref
    }

    pub fn storage_namespace(&self) -> Option<&str> {
        self.storage_namespace.as_deref()
    }

    pub fn pitr_recovery_point(&self) -> Option<&PitrRecoveryPoint> {
        self.pitr_recovery_point.as_ref()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HoldingRestoreTestTarget {
    backup_target: HoldingBackupTarget,
    restore_target_ref: String,
    replacement: HoldingReadDatabaseReplacement,
    source_version: u64,
}

impl fmt::Debug for HoldingRestoreTestTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HoldingRestoreTestTarget")
            .field("holding_id", &self.holding_id())
            .field(
                "candidate_read_database_key",
                &self.replacement.candidate_read_database_key,
            )
            .field("source_version", &self.source_version)
            .field("backup_ref", &"<redacted>")
            .field("restore_target_ref", &"<redacted>")
            .finish()
    }
}

impl HoldingRestoreTestTarget {
    pub fn new(
        backup_target: HoldingBackupTarget,
        restore_target_ref: &str,
        replacement: HoldingReadDatabaseReplacement,
        source_version: u64,
    ) -> Result<Self, PersistenceError> {
        validate_ref(restore_target_ref, "restore_target_ref")?;
        if source_version == 0 {
            return Err(PersistenceError::InvalidData(
                "projection source version is required".into(),
            ));
        }
        if restore_target_ref == backup_target.backup_ref {
            return Err(PersistenceError::InvalidData(
                "restore target must be distinct from backup reference".into(),
            ));
        }
        if backup_target.holding_id() != replacement.holding_id() {
            return Err(PersistenceError::InvalidData(
                "restore target and replacement read database belong to different holdings".into(),
            ));
        }
        Ok(Self {
            backup_target,
            restore_target_ref: restore_target_ref.into(),
            replacement,
            source_version,
        })
    }

    pub fn holding_id(&self) -> &str {
        self.backup_target.holding_id()
    }

    pub fn backup_target(&self) -> &HoldingBackupTarget {
        &self.backup_target
    }

    pub fn restore_target_ref(&self) -> &str {
        &self.restore_target_ref
    }

    pub fn replacement(&self) -> &HoldingReadDatabaseReplacement {
        &self.replacement
    }

    pub fn source_version(&self) -> u64 {
        self.source_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupExecutionRequest {
    target: HoldingBackupTarget,
    kind: BackupRunKind,
}

impl BackupExecutionRequest {
    pub fn new(target: HoldingBackupTarget, kind: BackupRunKind) -> Self {
        Self { target, kind }
    }

    pub fn target(&self) -> &HoldingBackupTarget {
        &self.target
    }

    pub fn kind(&self) -> BackupRunKind {
        self.kind
    }

    pub fn validate_result(
        &self,
        result: &BackupExecutionResult,
    ) -> Result<ValidatedBackupExecutionResult, PersistenceError> {
        if result.target != self.target {
            return Err(PersistenceError::InvalidData(
                "backup executor returned a result for a different holding or target".into(),
            ));
        }
        Ok(ValidatedBackupExecutionResult {
            verified: result.verified,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupExecutionResult {
    target: HoldingBackupTarget,
    verified: bool,
}

impl BackupExecutionResult {
    pub fn for_target(target: &HoldingBackupTarget, verified: bool) -> Self {
        Self {
            target: target.clone(),
            verified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedBackupExecutionResult {
    verified: bool,
}

impl ValidatedBackupExecutionResult {
    pub fn verified(self) -> bool {
        self.verified
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTestExecutionRequest {
    target: HoldingRestoreTestTarget,
}

impl RestoreTestExecutionRequest {
    pub fn new(target: HoldingRestoreTestTarget) -> Self {
        Self { target }
    }

    pub fn target(&self) -> &HoldingRestoreTestTarget {
        &self.target
    }

    pub fn validate_result(
        &self,
        result: &RestoreTestExecutionResult,
    ) -> Result<ValidatedRestoreTestResult, PersistenceError> {
        if result.target != self.target {
            return Err(PersistenceError::InvalidData(
                "restore executor returned a result for a different holding or target".into(),
            ));
        }
        Ok(ValidatedRestoreTestResult {
            outcome: result.outcome,
            replacement_read_verified: result.replacement_read_verified,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreTestOutcome {
    Verified,
    Failed,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTestExecutionResult {
    target: HoldingRestoreTestTarget,
    outcome: RestoreTestOutcome,
    replacement_read_verified: bool,
}

impl RestoreTestExecutionResult {
    pub fn for_target(
        target: &HoldingRestoreTestTarget,
        outcome: RestoreTestOutcome,
        replacement_read_verified: bool,
    ) -> Self {
        Self {
            target: target.clone(),
            outcome,
            replacement_read_verified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedRestoreTestResult {
    outcome: RestoreTestOutcome,
    replacement_read_verified: bool,
}

impl ValidatedRestoreTestResult {
    pub fn outcome(self) -> RestoreTestOutcome {
        self.outcome
    }

    pub fn replacement_read_verified(self) -> bool {
        self.replacement_read_verified
    }

    pub fn is_switch_safe(self) -> bool {
        self.outcome == RestoreTestOutcome::Verified && self.replacement_read_verified
    }
}

/// The maximum number of restore-test requests a worker may claim in one
/// pass. Real adapters may use a smaller limit, but they cannot bypass this
/// transport-neutral bound.
pub const MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE: u32 = 100;
pub const MAX_RESTORE_TEST_ATTEMPTS: u16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreTestScheduleState {
    Pending,
    Claimed,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryHistoryClassification {
    Scheduled,
    Claimed,
    ClaimLeaseExpired,
    Verified,
    RetryableFailure,
    Expired,
    ReplacementReadUnverified,
    TerminalFailure,
}

impl RecoveryHistoryClassification {
    pub const fn from_execution(
        outcome: RestoreTestOutcome,
        replacement_read_verified: bool,
    ) -> Self {
        match outcome {
            RestoreTestOutcome::Verified if replacement_read_verified => Self::Verified,
            RestoreTestOutcome::Verified => Self::ReplacementReadUnverified,
            RestoreTestOutcome::Failed => Self::RetryableFailure,
            RestoreTestOutcome::Expired => Self::Expired,
        }
    }

    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::RetryableFailure | Self::Expired | Self::ReplacementReadUnverified
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RestoreTestScheduleError {
    #[error("restore-test schedule request is invalid: {0}")]
    InvalidRequest(&'static str),
    #[error("restore-test idempotency conflict")]
    IdempotencyConflict,
    #[error("restore-test schedule was not found")]
    NotFound,
    #[error("restore-test claim is not owned by this worker")]
    ClaimLost,
    #[error("restore-test schedule is not due")]
    NotDue,
    #[error("restore-test scheduler is unavailable")]
    Unavailable,
}

/// A validated, immutable scheduling request. It contains opaque references
/// only; credentials, URLs, SQL pools, and storage clients are deliberately
/// absent so a later PostgreSQL/MinIO adapter can consume it safely.
#[derive(Clone, PartialEq, Eq)]
pub struct RestoreTestScheduleRequest {
    run: HoldingBackupRun,
    target: HoldingRestoreTestTarget,
    due_at: DateTime<Utc>,
    idempotency_key: String,
    max_attempts: u16,
}

impl fmt::Debug for RestoreTestScheduleRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoreTestScheduleRequest")
            .field("run", &self.run)
            .field("target", &self.target)
            .field("due_at", &self.due_at)
            .field("idempotency_key", &"<redacted>")
            .field("max_attempts", &self.max_attempts)
            .finish()
    }
}

impl RestoreTestScheduleRequest {
    pub fn new(
        run: HoldingBackupRun,
        target: HoldingRestoreTestTarget,
        due_at: DateTime<Utc>,
        idempotency_key: impl Into<String>,
        max_attempts: u16,
    ) -> Result<Self, RestoreTestScheduleError> {
        validate_backup_run_shape(&run)
            .map_err(|_| RestoreTestScheduleError::InvalidRequest("backup run is invalid"))?;
        if run.kind != "restore_test" || run.status != "started" {
            return Err(RestoreTestScheduleError::InvalidRequest(
                "restore-test schedules require a started restore-test run",
            ));
        }
        if target.holding_id() != run.holding_id
            || target.backup_target().backup_ref() != run.backup_ref
            || target.restore_target_ref() != run.restore_target_ref.as_deref().unwrap_or_default()
        {
            return Err(RestoreTestScheduleError::InvalidRequest(
                "restore-test target does not match the backup run",
            ));
        }
        let idempotency_key = idempotency_key.into();
        validate_ref(&idempotency_key, "idempotency_key")
            .map_err(|_| RestoreTestScheduleError::InvalidRequest("idempotency key is invalid"))?;
        if max_attempts == 0 || max_attempts > MAX_RESTORE_TEST_ATTEMPTS {
            return Err(RestoreTestScheduleError::InvalidRequest(
                "restore-test attempts exceed the bounded policy",
            ));
        }
        Ok(Self {
            run,
            target,
            due_at,
            idempotency_key,
            max_attempts,
        })
    }

    pub fn run(&self) -> &HoldingBackupRun {
        &self.run
    }

    pub fn target(&self) -> &HoldingRestoreTestTarget {
        &self.target
    }

    pub fn due_at(&self) -> DateTime<Utc> {
        self.due_at
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub const fn max_attempts(&self) -> u16 {
        self.max_attempts
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RestoreTestSchedule {
    id: Uuid,
    run: HoldingBackupRun,
    target: HoldingRestoreTestTarget,
    due_at: DateTime<Utc>,
    idempotency_key: String,
    max_attempts: u16,
    attempts: u16,
    state: RestoreTestScheduleState,
    claimed_by: Option<Uuid>,
    claimed_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for RestoreTestSchedule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoreTestSchedule")
            .field("id", &self.id)
            .field("holding_id", &"<opaque>")
            .field("run_id", &self.run.id)
            .field("due_at", &self.due_at)
            .field("idempotency_key", &"<redacted>")
            .field("max_attempts", &self.max_attempts)
            .field("attempts", &self.attempts)
            .field("state", &self.state)
            .field("claimed_by", &self.claimed_by)
            .field("claimed_at", &self.claimed_at)
            .finish()
    }
}

impl RestoreTestSchedule {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn holding_id(&self) -> &str {
        &self.run.holding_id
    }

    pub fn run_id(&self) -> Uuid {
        self.run.id
    }

    pub fn run(&self) -> &HoldingBackupRun {
        &self.run
    }

    pub fn target(&self) -> &HoldingRestoreTestTarget {
        &self.target
    }

    pub fn due_at(&self) -> DateTime<Utc> {
        self.due_at
    }

    pub fn attempts(&self) -> u16 {
        self.attempts
    }

    pub fn max_attempts(&self) -> u16 {
        self.max_attempts
    }

    pub const fn state(&self) -> RestoreTestScheduleState {
        self.state
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTestScheduleReceipt {
    schedule: RestoreTestSchedule,
    duplicate: bool,
}

impl RestoreTestScheduleReceipt {
    pub fn schedule(&self) -> &RestoreTestSchedule {
        &self.schedule
    }

    pub const fn duplicate(&self) -> bool {
        self.duplicate
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RestoreTestClaim {
    schedule_id: Uuid,
    holding_id: String,
    run_id: Uuid,
    target: HoldingRestoreTestTarget,
    attempt: u16,
    worker_id: Uuid,
    claimed_at: DateTime<Utc>,
}

impl fmt::Debug for RestoreTestClaim {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoreTestClaim")
            .field("schedule_id", &self.schedule_id)
            .field("holding_id", &"<opaque>")
            .field("run_id", &self.run_id)
            .field("target", &self.target)
            .field("attempt", &self.attempt)
            .field("worker_id", &self.worker_id)
            .field("claimed_at", &self.claimed_at)
            .finish()
    }
}

impl RestoreTestClaim {
    pub fn schedule_id(&self) -> Uuid {
        self.schedule_id
    }

    pub fn holding_id(&self) -> &str {
        &self.holding_id
    }

    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    pub fn target(&self) -> &HoldingRestoreTestTarget {
        &self.target
    }

    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTestClaimResult {
    pub schedule_id: Uuid,
    pub holding_id: String,
    pub classification: RecoveryHistoryClassification,
    pub state: RestoreTestScheduleState,
    pub attempts: u16,
    pub next_due_at: Option<DateTime<Utc>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RestoreTestHistoryEntry {
    pub schedule_id: Uuid,
    pub holding_id: String,
    pub attempt: u16,
    pub classification: RecoveryHistoryClassification,
    pub observed_at: DateTime<Utc>,
}

impl fmt::Debug for RestoreTestHistoryEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoreTestHistoryEntry")
            .field("schedule_id", &self.schedule_id)
            .field("holding_id", &"<opaque>")
            .field("attempt", &self.attempt)
            .field("classification", &self.classification)
            .field("observed_at", &self.observed_at)
            .finish()
    }
}

#[derive(Clone, Default)]
pub struct RestoreTestScheduler {
    state: Arc<Mutex<RestoreTestSchedulerState>>,
}

impl fmt::Debug for RestoreTestScheduler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RestoreTestScheduler")
            .field("state", &"<opaque scheduler state>")
            .finish()
    }
}

#[derive(Default)]
struct RestoreTestSchedulerState {
    schedules: BTreeMap<Uuid, RestoreTestSchedule>,
    idempotency: BTreeMap<String, Uuid>,
    history: BTreeMap<Uuid, Vec<RestoreTestHistoryEntry>>,
}

impl RestoreTestScheduler {
    pub fn schedule(
        &self,
        request: RestoreTestScheduleRequest,
    ) -> Result<RestoreTestScheduleReceipt, RestoreTestScheduleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RestoreTestScheduleError::Unavailable)?;
        if let Some(schedule_id) = state.idempotency.get(request.idempotency_key()) {
            let schedule = state
                .schedules
                .get(schedule_id)
                .ok_or(RestoreTestScheduleError::Unavailable)?;
            if !same_restore_test_request(schedule, &request) {
                return Err(RestoreTestScheduleError::IdempotencyConflict);
            }
            return Ok(RestoreTestScheduleReceipt {
                schedule: schedule.clone(),
                duplicate: true,
            });
        }
        let id = Uuid::new_v4();
        let schedule = RestoreTestSchedule {
            id,
            run: request.run,
            target: request.target,
            due_at: request.due_at,
            idempotency_key: request.idempotency_key,
            max_attempts: request.max_attempts,
            attempts: 0,
            state: RestoreTestScheduleState::Pending,
            claimed_by: None,
            claimed_at: None,
        };
        state
            .idempotency
            .insert(schedule.idempotency_key.clone(), id);
        state.history.insert(
            id,
            vec![RestoreTestHistoryEntry {
                schedule_id: id,
                holding_id: schedule.holding_id().into(),
                attempt: 0,
                classification: RecoveryHistoryClassification::Scheduled,
                observed_at: Utc::now(),
            }],
        );
        state.schedules.insert(id, schedule.clone());
        Ok(RestoreTestScheduleReceipt {
            schedule,
            duplicate: false,
        })
    }

    /// Claim only one holding's due work. A caller must supply the holding
    /// scope explicitly; this prevents a later adapter from accidentally
    /// turning a global claim query into a cross-holding restore operation.
    pub fn claim_due_for_holding(
        &self,
        holding_id: &str,
        worker_id: Uuid,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<RestoreTestClaim>, RestoreTestScheduleError> {
        validate_ref(holding_id, "holding_id")
            .map_err(|_| RestoreTestScheduleError::InvalidRequest("holding scope is invalid"))?;
        if worker_id.is_nil() {
            return Err(RestoreTestScheduleError::InvalidRequest(
                "worker identity is required",
            ));
        }
        if batch_size == 0 {
            return Err(RestoreTestScheduleError::InvalidRequest(
                "restore-test claim batch must be positive",
            ));
        }
        let limit = batch_size.min(MAX_RESTORE_TEST_SCHEDULE_BATCH_SIZE) as usize;
        let mut state = self
            .state
            .lock()
            .map_err(|_| RestoreTestScheduleError::Unavailable)?;
        let lease = Duration::minutes(15);
        let mut expired_claims = Vec::new();
        for schedule in state.schedules.values_mut() {
            if schedule.holding_id() != holding_id
                || schedule.state != RestoreTestScheduleState::Claimed
            {
                continue;
            }
            if schedule
                .claimed_at
                .is_some_and(|claimed_at| claimed_at + lease <= now)
            {
                schedule.state = RestoreTestScheduleState::Pending;
                schedule.claimed_by = None;
                schedule.claimed_at = None;
                expired_claims.push((
                    schedule.id,
                    schedule.holding_id().to_owned(),
                    schedule.attempts,
                ));
            }
        }
        for (schedule_id, holding_id, attempt) in expired_claims {
            state
                .history
                .entry(schedule_id)
                .or_default()
                .push(RestoreTestHistoryEntry {
                    schedule_id,
                    holding_id,
                    attempt,
                    classification: RecoveryHistoryClassification::ClaimLeaseExpired,
                    observed_at: now,
                });
        }
        let mut due_ids: Vec<Uuid> = state
            .schedules
            .values()
            .filter(|schedule| {
                schedule.holding_id() == holding_id
                    && schedule.state == RestoreTestScheduleState::Pending
                    && schedule.due_at <= now
                    && schedule.attempts < schedule.max_attempts
            })
            .map(|schedule| schedule.id)
            .collect();
        due_ids.sort_by_key(|id| {
            state
                .schedules
                .get(id)
                .map(|schedule| (schedule.due_at, schedule.id))
        });
        let mut claims = Vec::with_capacity(limit.min(due_ids.len()));
        for schedule_id in due_ids.into_iter().take(limit) {
            let (holding_id, run_id, target, attempt) = {
                let schedule = state
                    .schedules
                    .get_mut(&schedule_id)
                    .ok_or(RestoreTestScheduleError::Unavailable)?;
                schedule.attempts = schedule
                    .attempts
                    .checked_add(1)
                    .ok_or(RestoreTestScheduleError::Unavailable)?;
                schedule.state = RestoreTestScheduleState::Claimed;
                schedule.claimed_by = Some(worker_id);
                schedule.claimed_at = Some(now);
                (
                    schedule.holding_id().to_owned(),
                    schedule.run_id(),
                    schedule.target.clone(),
                    schedule.attempts,
                )
            };
            state
                .history
                .entry(schedule_id)
                .or_default()
                .push(RestoreTestHistoryEntry {
                    schedule_id,
                    holding_id: holding_id.clone(),
                    attempt,
                    classification: RecoveryHistoryClassification::Claimed,
                    observed_at: now,
                });
            claims.push(RestoreTestClaim {
                schedule_id,
                holding_id,
                run_id,
                target,
                attempt,
                worker_id,
                claimed_at: now,
            });
        }
        Ok(claims)
    }

    /// Record only the typed executor outcome. A verified result is not
    /// considered complete unless the replacement read target was verified.
    /// Retryable failures are rescheduled with bounded attempts; terminal
    /// history remains visible without exposing error text or references.
    pub fn record_execution(
        &self,
        claim: &RestoreTestClaim,
        outcome: RestoreTestOutcome,
        replacement_read_verified: bool,
        now: DateTime<Utc>,
    ) -> Result<RestoreTestClaimResult, RestoreTestScheduleError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| RestoreTestScheduleError::Unavailable)?;
        let (schedule_id, holding_id, attempt, max_attempts) = {
            let schedule = state
                .schedules
                .get(&claim.schedule_id)
                .ok_or(RestoreTestScheduleError::NotFound)?;
            if schedule.holding_id() != claim.holding_id
                || schedule.run_id() != claim.run_id
                || schedule.target != claim.target
                || schedule.state != RestoreTestScheduleState::Claimed
                || schedule.claimed_by != Some(claim.worker_id)
                || schedule.claimed_at != Some(claim.claimed_at)
                || schedule.attempts != claim.attempt
            {
                return Err(RestoreTestScheduleError::ClaimLost);
            }
            (
                schedule.id,
                schedule.holding_id().to_owned(),
                schedule.attempts,
                schedule.max_attempts,
            )
        };
        let mut classification =
            RecoveryHistoryClassification::from_execution(outcome, replacement_read_verified);
        state
            .history
            .entry(schedule_id)
            .or_default()
            .push(RestoreTestHistoryEntry {
                schedule_id,
                holding_id: holding_id.clone(),
                attempt,
                classification,
                observed_at: now,
            });
        let mut next_due_at = None;
        let mut state_after = RestoreTestScheduleState::Failed;
        if classification == RecoveryHistoryClassification::Verified {
            state_after = RestoreTestScheduleState::Completed;
        } else if classification.is_retryable() && attempt < max_attempts {
            let retry_delay_minutes = i64::from(attempt).min(5);
            let retry_at = now + Duration::minutes(retry_delay_minutes.max(1));
            state_after = RestoreTestScheduleState::Pending;
            next_due_at = Some(retry_at);
        } else {
            classification = RecoveryHistoryClassification::TerminalFailure;
            state
                .history
                .entry(schedule_id)
                .or_default()
                .push(RestoreTestHistoryEntry {
                    schedule_id,
                    holding_id: holding_id.clone(),
                    attempt,
                    classification,
                    observed_at: now,
                });
        }
        let schedule = state
            .schedules
            .get_mut(&schedule_id)
            .ok_or(RestoreTestScheduleError::Unavailable)?;
        schedule.state = state_after;
        if let Some(next_due_at) = next_due_at {
            schedule.due_at = next_due_at;
        }
        schedule.claimed_by = None;
        schedule.claimed_at = None;
        Ok(RestoreTestClaimResult {
            schedule_id,
            holding_id,
            classification,
            state: state_after,
            attempts: attempt,
            next_due_at,
        })
    }

    pub fn schedule_for_holding(
        &self,
        holding_id: &str,
        schedule_id: Uuid,
    ) -> Result<RestoreTestSchedule, RestoreTestScheduleError> {
        validate_ref(holding_id, "holding_id")
            .map_err(|_| RestoreTestScheduleError::InvalidRequest("holding scope is invalid"))?;
        let state = self
            .state
            .lock()
            .map_err(|_| RestoreTestScheduleError::Unavailable)?;
        let schedule = state
            .schedules
            .get(&schedule_id)
            .ok_or(RestoreTestScheduleError::NotFound)?;
        if schedule.holding_id() != holding_id {
            return Err(RestoreTestScheduleError::NotFound);
        }
        Ok(schedule.clone())
    }

    pub fn history_for_holding(
        &self,
        holding_id: &str,
        schedule_id: Uuid,
    ) -> Result<Vec<RestoreTestHistoryEntry>, RestoreTestScheduleError> {
        let _ = self.schedule_for_holding(holding_id, schedule_id)?;
        let state = self
            .state
            .lock()
            .map_err(|_| RestoreTestScheduleError::Unavailable)?;
        state
            .history
            .get(&schedule_id)
            .cloned()
            .ok_or(RestoreTestScheduleError::NotFound)
    }
}

fn same_restore_test_request(
    schedule: &RestoreTestSchedule,
    request: &RestoreTestScheduleRequest,
) -> bool {
    schedule.run == request.run
        && schedule.target == request.target
        && schedule.due_at == request.due_at
        && schedule.max_attempts == request.max_attempts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupExecutorError {
    Failed,
    Expired,
    Unavailable,
}

impl fmt::Display for BackupExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Failed => "backup executor failed",
            Self::Expired => "backup executor request expired",
            Self::Unavailable => "backup executor unavailable",
        })
    }
}

impl std::error::Error for BackupExecutorError {}

pub type RestoreExecutorError = BackupExecutorError;

#[async_trait]
pub trait BackupExecutor: Send + Sync {
    async fn execute(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupExecutorError>;
}

#[async_trait]
pub trait RestoreExecutor: Send + Sync {
    async fn execute(
        &self,
        request: RestoreTestExecutionRequest,
    ) -> Result<RestoreTestExecutionResult, RestoreExecutorError>;
}

/// Safe classification used by the concrete PostgreSQL/MinIO adapter before
/// it is translated to the deliberately small [`BackupExecutorError`] seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupFailureClass {
    Transient,
    Permanent,
}

/// Errors exposed by the production adapter are intentionally detail-free.
/// Connection strings, object URLs, command stderr, and storage credentials
/// never cross the executor boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BackupAdapterError {
    #[error("backup adapter configuration is unavailable")]
    Configuration,
    #[error("backup adapter is unavailable")]
    Unavailable,
    #[error("backup adapter operation expired")]
    Expired,
    #[error("backup adapter operation is transiently unavailable")]
    Transient,
    #[error("backup adapter operation failed")]
    Permanent,
    #[error("backup artifact is missing")]
    ObjectMissing,
    #[error("backup artifact exceeds the configured size bound")]
    ArtifactTooLarge,
    #[error("backup artifact checksum or size verification failed")]
    ChecksumMismatch,
    #[error("backup artifact idempotency conflict")]
    IdempotencyConflict,
    #[error("backup object inventory is unavailable")]
    InventoryUnavailable,
}

impl BackupAdapterError {
    pub const fn failure_class(self) -> BackupFailureClass {
        match self {
            Self::Unavailable | Self::Expired | Self::Transient => BackupFailureClass::Transient,
            Self::Configuration
            | Self::Permanent
            | Self::ObjectMissing
            | Self::ArtifactTooLarge
            | Self::ChecksumMismatch
            | Self::IdempotencyConflict
            | Self::InventoryUnavailable => BackupFailureClass::Permanent,
        }
    }
}

/// An argv-only process request. It has no shell mode and its `Debug` output
/// deliberately hides every argument because database URLs may be present in
/// the private command boundary.
pub struct BackupProcessCommand {
    program: String,
    args: Vec<String>,
    capture_stdout: bool,
    max_stdout_bytes: u64,
    timeout: StdDuration,
    output_directory: Option<PathBuf>,
}

impl fmt::Debug for BackupProcessCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackupProcessCommand")
            .field("program", &self.program)
            .field("args", &"<redacted>")
            .field("capture_stdout", &self.capture_stdout)
            .field("max_stdout_bytes", &self.max_stdout_bytes)
            .field("timeout", &self.timeout)
            .field("output_directory", &self.output_directory)
            .finish()
    }
}

impl BackupProcessCommand {
    pub fn new(
        program: String,
        args: Vec<String>,
        capture_stdout: bool,
        max_stdout_bytes: u64,
        timeout: StdDuration,
    ) -> Self {
        Self {
            program,
            args,
            capture_stdout,
            max_stdout_bytes,
            timeout,
            output_directory: None,
        }
    }

    pub fn with_output_directory(mut self, output_directory: PathBuf) -> Self {
        self.output_directory = Some(output_directory);
        self
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub const fn capture_stdout(&self) -> bool {
        self.capture_stdout
    }

    pub const fn max_stdout_bytes(&self) -> u64 {
        self.max_stdout_bytes
    }

    pub const fn timeout(&self) -> StdDuration {
        self.timeout
    }

    pub fn output_directory(&self) -> Option<&std::path::Path> {
        self.output_directory.as_deref()
    }
}

#[async_trait]
pub trait BackupProcessRunner: Send + Sync {
    async fn run(&self, command: BackupProcessCommand) -> Result<Vec<u8>, BackupAdapterError>;
}

#[async_trait]
pub trait BackupObjectTransport: Send + Sync {
    async fn put(
        &self,
        namespace: &str,
        object_key: &str,
        body: &[u8],
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError>;

    async fn get(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<Vec<u8>, BackupAdapterError>;

    async fn delete(
        &self,
        namespace: &str,
        object_key: &str,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError>;

    async fn list(
        &self,
        _namespace: &str,
        _prefix: &str,
        _max_entries: u32,
        _timeout: StdDuration,
    ) -> Result<Vec<ObjectInventoryEntry>, BackupAdapterError> {
        Err(BackupAdapterError::InventoryUnavailable)
    }
}

#[async_trait]
pub trait RestoredDatabaseVerifier: Send + Sync {
    async fn verify(
        &self,
        database_config: &HoldingDatabaseProvisioningConfig,
        database_key: &str,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError>;
}

#[derive(Clone)]
pub struct PostgresMinioBackupConfig {
    pub database_config: HoldingDatabaseProvisioningConfig,
    pub object_store_config: MinioObjectStoreConfig,
    pub object_namespace: String,
    pub pg_dump_program: String,
    pub pg_restore_program: String,
    pub source_side: DatabaseSide,
    pub max_artifact_bytes: u64,
    pub command_timeout: StdDuration,
    pub object_timeout: StdDuration,
}

impl PostgresMinioBackupConfig {
    pub const DEFAULT_MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
    pub const DEFAULT_COMMAND_TIMEOUT: StdDuration = StdDuration::from_secs(15 * 60);
    pub const DEFAULT_OBJECT_TIMEOUT: StdDuration = StdDuration::from_secs(60);

    pub fn from_env() -> Result<Self, BackupAdapterError> {
        let database_config = HoldingDatabaseProvisioningConfig::from_env()
            .map_err(|_| BackupAdapterError::Configuration)?;
        let object_store_config = MinioObjectStoreConfig {
            endpoint: required_backup_env("ERPX_BACKUP_MINIO_ENDPOINT")?,
            access_key: required_backup_env("ERPX_BACKUP_MINIO_ACCESS_KEY")?,
            secret_key: required_backup_env("ERPX_BACKUP_MINIO_SECRET_KEY")?,
            region: required_backup_env("ERPX_BACKUP_MINIO_REGION")?,
        };
        let object_namespace = required_backup_env("ERPX_BACKUP_MINIO_NAMESPACE")?;
        let source_side = match env::var("ERPX_BACKUP_SOURCE_SIDE")
            .unwrap_or_else(|_| "write".into())
            .as_str()
        {
            "write" => DatabaseSide::Write,
            "read" => DatabaseSide::Read,
            _ => return Err(BackupAdapterError::Configuration),
        };
        let max_artifact_bytes = parse_bounded_backup_u64(
            "ERPX_BACKUP_MAX_ARTIFACT_BYTES",
            Self::DEFAULT_MAX_ARTIFACT_BYTES,
            1,
            Self::DEFAULT_MAX_ARTIFACT_BYTES,
        )?;
        let command_timeout = parse_bounded_backup_duration(
            "ERPX_BACKUP_COMMAND_TIMEOUT_SECONDS",
            Self::DEFAULT_COMMAND_TIMEOUT,
            StdDuration::from_secs(1),
            StdDuration::from_secs(60 * 60),
        )?;
        let object_timeout = parse_bounded_backup_duration(
            "ERPX_BACKUP_OBJECT_TIMEOUT_SECONDS",
            Self::DEFAULT_OBJECT_TIMEOUT,
            StdDuration::from_secs(1),
            StdDuration::from_secs(5 * 60),
        )?;
        Self::from_values(
            database_config,
            object_store_config,
            object_namespace,
            env::var("ERPX_BACKUP_PG_DUMP_PROGRAM").unwrap_or_else(|_| "pg_dump".into()),
            env::var("ERPX_BACKUP_PG_RESTORE_PROGRAM").unwrap_or_else(|_| "pg_restore".into()),
            source_side,
            max_artifact_bytes,
            command_timeout,
            object_timeout,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_values(
        database_config: HoldingDatabaseProvisioningConfig,
        object_store_config: MinioObjectStoreConfig,
        object_namespace: impl Into<String>,
        pg_dump_program: impl Into<String>,
        pg_restore_program: impl Into<String>,
        source_side: DatabaseSide,
        max_artifact_bytes: u64,
        command_timeout: StdDuration,
        object_timeout: StdDuration,
    ) -> Result<Self, BackupAdapterError> {
        let object_namespace = object_namespace.into();
        let pg_dump_program = pg_dump_program.into();
        let pg_restore_program = pg_restore_program.into();
        let config = Self {
            database_config,
            object_store_config,
            object_namespace,
            pg_dump_program,
            pg_restore_program,
            source_side,
            max_artifact_bytes,
            command_timeout,
            object_timeout,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), BackupAdapterError> {
        validate_database_config(&self.database_config)
            .map_err(|_| BackupAdapterError::Configuration)?;
        if self.object_namespace.trim().is_empty()
            || !valid_program_name(&self.pg_dump_program)
            || !valid_program_name(&self.pg_restore_program)
            || !(1..=Self::DEFAULT_MAX_ARTIFACT_BYTES).contains(&self.max_artifact_bytes)
            || !(StdDuration::from_secs(1)..=StdDuration::from_secs(60 * 60))
                .contains(&self.command_timeout)
            || !(StdDuration::from_secs(1)..=StdDuration::from_secs(5 * 60))
                .contains(&self.object_timeout)
        {
            return Err(BackupAdapterError::Configuration);
        }
        MinioObjectStore::validate_namespace_reference(&self.object_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        MinioObjectStore::new(self.object_store_config.clone())
            .map_err(|_| BackupAdapterError::Configuration)?;
        Ok(())
    }
}

impl fmt::Debug for PostgresMinioBackupConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresMinioBackupConfig")
            .field("database_config", &"<redacted>")
            .field("object_store_config", &self.object_store_config)
            .field("object_namespace", &self.object_namespace)
            .field("pg_dump_program", &self.pg_dump_program)
            .field("pg_restore_program", &self.pg_restore_program)
            .field("source_side", &self.source_side)
            .field("max_artifact_bytes", &self.max_artifact_bytes)
            .field("command_timeout", &self.command_timeout)
            .field("object_timeout", &self.object_timeout)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct MinioBackupObjectTransport {
    store: MinioObjectStore,
}

impl MinioBackupObjectTransport {
    pub fn new(store: MinioObjectStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl BackupObjectTransport for MinioBackupObjectTransport {
    async fn put(
        &self,
        namespace: &str,
        object_key: &str,
        body: &[u8],
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError> {
        self.store
            .put_object_bounded(namespace, object_key, body, max_bytes, timeout)
            .await
            .map_err(map_object_storage_error)
    }

    async fn get(
        &self,
        namespace: &str,
        object_key: &str,
        max_bytes: u64,
        timeout: StdDuration,
    ) -> Result<Vec<u8>, BackupAdapterError> {
        self.store
            .get_object_bounded(namespace, object_key, max_bytes, timeout)
            .await
            .map_err(map_object_storage_error)
    }

    async fn delete(
        &self,
        namespace: &str,
        object_key: &str,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError> {
        self.store
            .delete_object_bounded(namespace, object_key, timeout)
            .await
            .map_err(map_object_storage_error)
    }

    async fn list(
        &self,
        namespace: &str,
        prefix: &str,
        max_entries: u32,
        timeout: StdDuration,
    ) -> Result<Vec<ObjectInventoryEntry>, BackupAdapterError> {
        self.store
            .list_objects_bounded(namespace, prefix, max_entries, timeout)
            .await
            .map_err(map_object_storage_error)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SystemBackupProcessRunner;

#[async_trait]
impl BackupProcessRunner for SystemBackupProcessRunner {
    async fn run(&self, command: BackupProcessCommand) -> Result<Vec<u8>, BackupAdapterError> {
        tokio::task::spawn_blocking(move || run_bounded_backup_command(command))
            .await
            .map_err(|_| BackupAdapterError::Unavailable)?
    }
}

#[derive(Debug, Clone, Default)]
pub struct PostgresRestoredDatabaseVerifier;

#[async_trait]
impl RestoredDatabaseVerifier for PostgresRestoredDatabaseVerifier {
    async fn verify(
        &self,
        database_config: &HoldingDatabaseProvisioningConfig,
        database_key: &str,
        timeout: StdDuration,
    ) -> Result<(), BackupAdapterError> {
        let database_url =
            render_database_url(&database_config.database_url_template, database_key)
                .map_err(|_| BackupAdapterError::Configuration)?;
        let pool = tokio::time::timeout(
            timeout,
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(timeout)
                .connect(&database_url),
        )
        .await
        .map_err(|_| BackupAdapterError::Expired)?
        .map_err(|_| BackupAdapterError::Transient)?;
        let row = tokio::time::timeout(
            timeout,
            sqlx::query(
                "SELECT current_database() AS database_name,
                        to_regclass('erpx_holding.customer_summary')::text AS relation_name,
                        EXISTS (
                            SELECT 1
                            FROM pg_class AS c
                            JOIN pg_namespace AS n ON n.oid = c.relnamespace
                            WHERE n.nspname = 'erpx_holding'
                              AND c.relname = 'customer_summary'
                              AND c.relrowsecurity
                              AND c.relforcerowsecurity
                        ) AS rls_ready",
            )
            .fetch_one(&pool),
        )
        .await
        .map_err(|_| BackupAdapterError::Expired)?
        .map_err(|_| BackupAdapterError::Transient)?;
        let current_database: String = row
            .try_get("database_name")
            .map_err(|_| BackupAdapterError::Permanent)?;
        let relation_name: Option<String> = row
            .try_get("relation_name")
            .map_err(|_| BackupAdapterError::Permanent)?;
        let rls_ready: bool = row
            .try_get("rls_ready")
            .map_err(|_| BackupAdapterError::Permanent)?;
        pool.close().await;
        if current_database != database_key
            || relation_name.as_deref() != Some("erpx_holding.customer_summary")
            || !rls_ready
        {
            return Err(BackupAdapterError::Permanent);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct PostgresMinioBackupAdapter {
    config: PostgresMinioBackupConfig,
    process_runner: Arc<dyn BackupProcessRunner>,
    object_transport: Arc<dyn BackupObjectTransport>,
    verifier: Arc<dyn RestoredDatabaseVerifier>,
}

impl fmt::Debug for PostgresMinioBackupAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresMinioBackupAdapter")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl PostgresMinioBackupAdapter {
    pub fn from_env() -> Result<Self, BackupAdapterError> {
        Self::new(PostgresMinioBackupConfig::from_env()?)
    }

    pub fn new(config: PostgresMinioBackupConfig) -> Result<Self, BackupAdapterError> {
        let object_store = MinioObjectStore::new(config.object_store_config.clone())
            .map_err(|_| BackupAdapterError::Configuration)?;
        Self::with_components(
            config,
            Arc::new(SystemBackupProcessRunner),
            Arc::new(MinioBackupObjectTransport::new(object_store)),
            Arc::new(PostgresRestoredDatabaseVerifier),
        )
    }

    pub fn with_components(
        config: PostgresMinioBackupConfig,
        process_runner: Arc<dyn BackupProcessRunner>,
        object_transport: Arc<dyn BackupObjectTransport>,
        verifier: Arc<dyn RestoredDatabaseVerifier>,
    ) -> Result<Self, BackupAdapterError> {
        config.validate()?;
        Ok(Self {
            config,
            process_runner,
            object_transport,
            verifier,
        })
    }

    pub fn config(&self) -> &PostgresMinioBackupConfig {
        &self.config
    }

    /// Delete one retention-selected backup artifact. The worker supplies the
    /// authoritative holding storage namespace and opaque chain/reference
    /// values from the control-plane registry; this adapter performs only the
    /// bounded, idempotent MinIO operation.
    pub async fn delete_backup_artifact(
        &self,
        holding_id: &str,
        storage_namespace: &str,
        backup_chain_ref: &str,
        backup_ref: &str,
    ) -> Result<(), BackupAdapterError> {
        validate_ref(holding_id, "holding_id").map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_chain_ref, "backup_chain_ref")
            .map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_ref, "backup_ref").map_err(|_| BackupAdapterError::Configuration)?;
        MinioObjectStore::validate_namespace_reference(storage_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        let object_key = backup_object_key_parts(holding_id, backup_chain_ref, backup_ref)?;
        self.object_transport
            .delete(storage_namespace, &object_key, self.config.object_timeout)
            .await
    }

    /// Delete every object belonging to one physical PITR artifact set. PITR
    /// uses a directory-shaped prefix rather than the single `.dump` object
    /// used by logical backups, so retention must enumerate that exact
    /// holding/chain/backup prefix before deleting it.
    pub async fn delete_pitr_backup_artifacts(
        &self,
        holding_id: &str,
        storage_namespace: &str,
        backup_chain_ref: &str,
        backup_ref: &str,
    ) -> Result<(), BackupAdapterError> {
        validate_ref(holding_id, "holding_id").map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_chain_ref, "backup_chain_ref")
            .map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_ref, "backup_ref").map_err(|_| BackupAdapterError::Configuration)?;
        MinioObjectStore::validate_namespace_reference(storage_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        let prefix = pitr_backup_object_prefix(holding_id, backup_chain_ref, backup_ref)?;
        let entries = self
            .object_transport
            .list(
                storage_namespace,
                &prefix,
                MAX_HOLDING_PITR_INVENTORY_ENTRIES,
                self.config.object_timeout,
            )
            .await?;
        for entry in entries {
            validate_pitr_object_key(&prefix, &entry.key)?;
            match self
                .object_transport
                .delete(storage_namespace, &entry.key, self.config.object_timeout)
                .await
            {
                Ok(()) | Err(BackupAdapterError::ObjectMissing) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// List the complete, bounded object inventory for one holding backup
    /// chain. A truncated provider page is rejected by the object store, so
    /// callers can persist a drift report without treating partial data as a
    /// match.
    pub async fn list_backup_artifacts(
        &self,
        holding_id: &str,
        storage_namespace: &str,
        backup_chain_ref: &str,
        max_entries: u32,
    ) -> Result<Vec<String>, BackupAdapterError> {
        validate_ref(holding_id, "holding_id").map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_chain_ref, "backup_chain_ref")
            .map_err(|_| BackupAdapterError::Configuration)?;
        MinioObjectStore::validate_namespace_reference(storage_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        let prefix = format!("holdings/{holding_id}/backup-chains/{backup_chain_ref}/artifacts/");
        let entries = self
            .object_transport
            .list(
                storage_namespace,
                &prefix,
                max_entries,
                self.config.object_timeout,
            )
            .await?;
        entries
            .into_iter()
            .map(|entry| {
                let backup_ref = entry
                    .key
                    .strip_prefix(&prefix)
                    .and_then(|value| value.strip_suffix(".dump"))
                    .ok_or(BackupAdapterError::Permanent)?;
                validate_ref(backup_ref, "backup_ref")
                    .map_err(|_| BackupAdapterError::Permanent)?;
                Ok(backup_ref.to_owned())
            })
            .collect()
    }

    /// List distinct PITR backup references under one bounded chain prefix.
    /// The object-store page is bounded by the physical archive limit and is
    /// rejected if the provider cannot prove it is complete.
    pub async fn list_pitr_backup_artifacts(
        &self,
        holding_id: &str,
        storage_namespace: &str,
        backup_chain_ref: &str,
        max_entries: u32,
    ) -> Result<Vec<String>, BackupAdapterError> {
        validate_ref(holding_id, "holding_id").map_err(|_| BackupAdapterError::Configuration)?;
        validate_ref(backup_chain_ref, "backup_chain_ref")
            .map_err(|_| BackupAdapterError::Configuration)?;
        if max_entries == 0 || max_entries > MAX_HOLDING_PITR_INVENTORY_ENTRIES {
            return Err(BackupAdapterError::Configuration);
        }
        MinioObjectStore::validate_namespace_reference(storage_namespace)
            .map_err(|_| BackupAdapterError::Configuration)?;
        let prefix = format!("holdings/{holding_id}/backup-chains/{backup_chain_ref}/pitr/");
        let entries = self
            .object_transport
            .list(
                storage_namespace,
                &prefix,
                max_entries,
                self.config.object_timeout,
            )
            .await?;
        let mut refs = BTreeSet::new();
        for entry in entries {
            let relative = entry
                .key
                .strip_prefix(&prefix)
                .ok_or(BackupAdapterError::Permanent)?;
            let (backup_ref, artifact_name) = relative
                .split_once('/')
                .ok_or(BackupAdapterError::Permanent)?;
            validate_ref(backup_ref, "backup_ref").map_err(|_| BackupAdapterError::Permanent)?;
            if artifact_name.is_empty()
                || artifact_name.split('/').any(|segment| {
                    segment.is_empty()
                        || segment == "."
                        || segment == ".."
                        || segment.chars().any(|character| character.is_control())
                })
            {
                return Err(BackupAdapterError::Permanent);
            }
            refs.insert(backup_ref.to_owned());
        }
        Ok(refs.into_iter().collect())
    }

    async fn execute_backup_inner(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupAdapterError> {
        if request.kind() == BackupRunKind::Pitr {
            return Err(BackupAdapterError::Permanent);
        }
        let target = request.target();
        let database_key = match self.config.source_side {
            DatabaseSide::Write => &target.databases().write_database_key,
            DatabaseSide::Read => &target.databases().read_database_key,
        };
        let database_url = render_database_url(
            &self.config.database_config.database_url_template,
            database_key,
        )
        .map_err(|_| BackupAdapterError::Configuration)?;
        let command = BackupProcessCommand::new(
            self.config.pg_dump_program.clone(),
            vec![
                "--format=custom".into(),
                "--no-owner".into(),
                "--no-acl".into(),
                format!("--dbname={database_url}"),
            ],
            true,
            self.config.max_artifact_bytes,
            self.config.command_timeout,
        );
        let artifact = self.process_runner.run(command).await?;
        if artifact.is_empty() {
            return Err(BackupAdapterError::Permanent);
        }
        let object_key = backup_object_key(target)?;
        let storage_namespace = target
            .storage_namespace()
            .unwrap_or(&self.config.object_namespace);
        match self
            .object_transport
            .get(
                storage_namespace,
                &object_key,
                self.config.max_artifact_bytes,
                self.config.object_timeout,
            )
            .await
        {
            Ok(existing) => verify_artifact_match(&existing, &artifact)?,
            Err(BackupAdapterError::ObjectMissing) => {
                self.object_transport
                    .put(
                        storage_namespace,
                        &object_key,
                        &artifact,
                        self.config.max_artifact_bytes,
                        self.config.object_timeout,
                    )
                    .await?;
                let uploaded = self
                    .object_transport
                    .get(
                        storage_namespace,
                        &object_key,
                        self.config.max_artifact_bytes,
                        self.config.object_timeout,
                    )
                    .await?;
                verify_artifact_match(&uploaded, &artifact)?;
            }
            Err(error) => return Err(error),
        }
        Ok(BackupExecutionResult::for_target(target, true))
    }

    async fn execute_restore_inner(
        &self,
        request: RestoreTestExecutionRequest,
    ) -> Result<RestoreTestExecutionResult, BackupAdapterError> {
        let target = request.target();
        let object_key = backup_object_key(target.backup_target())?;
        let storage_namespace = target
            .backup_target()
            .storage_namespace()
            .unwrap_or(&self.config.object_namespace);
        let artifact = self
            .object_transport
            .get(
                storage_namespace,
                &object_key,
                self.config.max_artifact_bytes,
                self.config.object_timeout,
            )
            .await?;
        if artifact.is_empty() {
            return Err(BackupAdapterError::Permanent);
        }
        let candidate_key = target.replacement().candidate_read_database_key();
        let database_url = render_database_url(
            &self.config.database_config.database_url_template,
            candidate_key,
        )
        .map_err(|_| BackupAdapterError::Configuration)?;
        let artifact_path = write_private_backup_artifact(&artifact)?;
        let command = BackupProcessCommand::new(
            self.config.pg_restore_program.clone(),
            vec![
                "--format=custom".into(),
                "--no-owner".into(),
                "--no-acl".into(),
                "--clean".into(),
                "--if-exists".into(),
                "--exit-on-error".into(),
                "--single-transaction".into(),
                format!("--dbname={database_url}"),
                artifact_path.to_string_lossy().into_owned(),
            ],
            false,
            0,
            self.config.command_timeout,
        );
        let process_result = self.process_runner.run(command).await;
        let _ = std::fs::remove_file(&artifact_path);
        process_result?;
        self.verifier
            .verify(
                &self.config.database_config,
                candidate_key,
                self.config.command_timeout,
            )
            .await?;
        Ok(RestoreTestExecutionResult::for_target(
            target,
            RestoreTestOutcome::Verified,
            true,
        ))
    }
}

#[async_trait]
impl BackupExecutor for PostgresMinioBackupAdapter {
    async fn execute(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupExecutorError> {
        self.execute_backup_inner(request)
            .await
            .map_err(map_backup_adapter_error)
    }
}

#[async_trait]
impl RestoreExecutor for PostgresMinioBackupAdapter {
    async fn execute(
        &self,
        request: RestoreTestExecutionRequest,
    ) -> Result<RestoreTestExecutionResult, RestoreExecutorError> {
        self.execute_restore_inner(request)
            .await
            .map_err(map_backup_adapter_error)
    }
}

fn required_backup_env(name: &str) -> Result<String, BackupAdapterError> {
    let value = env::var(name).map_err(|_| BackupAdapterError::Configuration)?;
    if value.trim().is_empty() {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(value)
}

fn parse_bounded_backup_u64(
    name: &str,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, BackupAdapterError> {
    let value = env::var(name)
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| BackupAdapterError::Configuration)?
        .unwrap_or(default);
    if !(minimum..=maximum).contains(&value) {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(value)
}

fn parse_bounded_backup_duration(
    name: &str,
    default: StdDuration,
    minimum: StdDuration,
    maximum: StdDuration,
) -> Result<StdDuration, BackupAdapterError> {
    let seconds = env::var(name)
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| BackupAdapterError::Configuration)?
        .unwrap_or(default.as_secs());
    let value = StdDuration::from_secs(seconds);
    if !(minimum..=maximum).contains(&value) {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(value)
}

fn valid_program_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && !value
            .chars()
            .any(|character| character.is_control() || character == '\0')
}

fn map_object_storage_error(error: ObjectStorageError) -> BackupAdapterError {
    match error {
        ObjectStorageError::HttpStatus { status: 404 } => BackupAdapterError::ObjectMissing,
        ObjectStorageError::HttpStatus { status }
            if status == 408 || status == 429 || status >= 500 =>
        {
            BackupAdapterError::Transient
        }
        ObjectStorageError::HttpStatus { .. } | ObjectStorageError::NamespaceAlreadyExists => {
            BackupAdapterError::Permanent
        }
        ObjectStorageError::Url(_) | ObjectStorageError::ProbeTimeout => {
            BackupAdapterError::Transient
        }
        ObjectStorageError::InvalidConfiguration(_) | ObjectStorageError::InvalidReference(_) => {
            BackupAdapterError::Configuration
        }
        ObjectStorageError::ObjectTooLarge => BackupAdapterError::ArtifactTooLarge,
        ObjectStorageError::InventoryTooLarge | ObjectStorageError::InvalidResponse => {
            BackupAdapterError::Permanent
        }
    }
}

fn map_backup_adapter_error(error: BackupAdapterError) -> BackupExecutorError {
    match error {
        BackupAdapterError::Expired => BackupExecutorError::Expired,
        BackupAdapterError::Unavailable
        | BackupAdapterError::Transient
        | BackupAdapterError::ObjectMissing => BackupExecutorError::Unavailable,
        BackupAdapterError::Configuration
        | BackupAdapterError::Permanent
        | BackupAdapterError::ArtifactTooLarge
        | BackupAdapterError::ChecksumMismatch
        | BackupAdapterError::IdempotencyConflict
        | BackupAdapterError::InventoryUnavailable => BackupExecutorError::Failed,
    }
}

fn backup_object_key(target: &HoldingBackupTarget) -> Result<String, BackupAdapterError> {
    backup_object_key_parts(
        target.holding_id(),
        target.backup_chain_ref(),
        target.backup_ref(),
    )
}

fn backup_object_key_parts(
    holding_id: &str,
    backup_chain_ref: &str,
    backup_ref: &str,
) -> Result<String, BackupAdapterError> {
    let key = format!(
        "holdings/{}/backup-chains/{}/artifacts/{}.dump",
        holding_id, backup_chain_ref, backup_ref
    );
    if key.len() > 1024 || key.split('/').any(|segment| segment == "..") {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(key)
}

fn pitr_backup_object_prefix(
    holding_id: &str,
    backup_chain_ref: &str,
    backup_ref: &str,
) -> Result<String, BackupAdapterError> {
    let key = format!("holdings/{holding_id}/backup-chains/{backup_chain_ref}/pitr/{backup_ref}/");
    if key.len() > 1024 || key.split('/').any(|segment| segment == "..") {
        return Err(BackupAdapterError::Configuration);
    }
    Ok(key)
}

fn validate_pitr_object_key(prefix: &str, object_key: &str) -> Result<(), BackupAdapterError> {
    let relative = object_key
        .strip_prefix(prefix)
        .ok_or(BackupAdapterError::Permanent)?;
    if relative.is_empty()
        || relative.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.chars().any(|character| character.is_control())
        })
    {
        return Err(BackupAdapterError::Permanent);
    }
    Ok(())
}

fn verify_artifact_match(existing: &[u8], expected: &[u8]) -> Result<(), BackupAdapterError> {
    if existing.len() != expected.len() {
        return Err(BackupAdapterError::IdempotencyConflict);
    }
    let existing_hash = Sha256::digest(existing);
    let expected_hash = Sha256::digest(expected);
    if existing_hash != expected_hash {
        return Err(BackupAdapterError::IdempotencyConflict);
    }
    Ok(())
}

fn write_private_backup_artifact(artifact: &[u8]) -> Result<PathBuf, BackupAdapterError> {
    let path = env::temp_dir().join(format!("erpx-restore-{}.dump", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(&path)
        .map_err(|_| BackupAdapterError::Unavailable)?;
    if u64::try_from(artifact.len()).map_err(|_| BackupAdapterError::ArtifactTooLarge)?
        > PostgresMinioBackupConfig::DEFAULT_MAX_ARTIFACT_BYTES
    {
        let _ = std::fs::remove_file(&path);
        return Err(BackupAdapterError::ArtifactTooLarge);
    }
    if file.write_all(artifact).is_err() || file.flush().is_err() {
        let _ = std::fs::remove_file(&path);
        return Err(BackupAdapterError::Unavailable);
    }
    Ok(path)
}

fn run_bounded_backup_command(
    command: BackupProcessCommand,
) -> Result<Vec<u8>, BackupAdapterError> {
    if !valid_program_name(&command.program)
        || command
            .args
            .iter()
            .any(|argument| argument.chars().any(|character| character == '\0'))
        || (command.capture_stdout && command.max_stdout_bytes == 0)
        || command
            .output_directory
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err(BackupAdapterError::Configuration);
    }
    let output_path = command
        .capture_stdout
        .then(|| env::temp_dir().join(format!("erpx-command-{}.out", Uuid::new_v4())));
    let output_file = if let Some(path) = &output_path {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        Some(
            options
                .open(path)
                .map_err(|_| BackupAdapterError::Unavailable)?,
        )
    } else {
        None
    };
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    process.stdin(Stdio::null()).stderr(Stdio::null());
    process.stdout(match output_file {
        Some(file) => Stdio::from(file),
        None => Stdio::null(),
    });
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(path) = &output_path {
                let _ = std::fs::remove_file(path);
            }
            return Err(BackupAdapterError::Unavailable);
        }
        Err(_) => {
            if let Some(path) = &output_path {
                let _ = std::fs::remove_file(path);
            }
            return Err(BackupAdapterError::Transient);
        }
    };
    let started = Instant::now();
    let status = loop {
        if started.elapsed() > command.timeout {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(path) = &output_path {
                let _ = std::fs::remove_file(path);
            }
            return Err(BackupAdapterError::Expired);
        }
        if let Some(path) = &output_path {
            let size = std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if size > command.max_stdout_bytes {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(path);
                return Err(BackupAdapterError::ArtifactTooLarge);
            }
        }
        if let Some(path) = &command.output_directory
            && directory_size(path) > command.max_stdout_bytes
        {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(output_path) = &output_path {
                let _ = std::fs::remove_file(output_path);
            }
            return Err(BackupAdapterError::ArtifactTooLarge);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(StdDuration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(path) = &output_path {
                    let _ = std::fs::remove_file(path);
                }
                return Err(BackupAdapterError::Transient);
            }
        }
    };
    if !status.success() {
        if let Some(path) = &output_path {
            let _ = std::fs::remove_file(path);
        }
        return Err(BackupAdapterError::Permanent);
    }
    let Some(path) = output_path else {
        return Ok(Vec::new());
    };
    let output = std::fs::read(&path).map_err(|_| BackupAdapterError::Unavailable);
    let _ = std::fs::remove_file(path);
    let output = output?;
    if u64::try_from(output.len()).map_err(|_| BackupAdapterError::ArtifactTooLarge)?
        > command.max_stdout_bytes
    {
        return Err(BackupAdapterError::ArtifactTooLarge);
    }
    Ok(output)
}

fn directory_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            entry
                .metadata()
                .ok()
                .map(|metadata| {
                    if metadata.is_dir() {
                        directory_size(&path)
                    } else {
                        metadata.len()
                    }
                })
                .unwrap_or(0)
        })
        .fold(0_u64, u64::saturating_add)
}

#[derive(Debug, Clone)]
pub struct HoldingDatabaseProvisioner {
    control: Arc<SqlxControlPlane>,
    config: HoldingDatabaseProvisioningConfig,
}

#[derive(Debug, Clone)]
pub struct HoldingDatabaseRouter {
    control: Arc<SqlxControlPlane>,
    config: HoldingDatabaseProvisioningConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutedDatabaseSide {
    Write,
    Read,
}

/// Organization scope used to select a physical holding route. Holding and
/// company operations remain on the holding pair; a branch route is selected
/// only by an exact, active control-plane binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldingDataRouteScope {
    Holding,
    Company { company_id: Uuid },
    Branch { company_id: Uuid, branch_id: Uuid },
}

pub(crate) const HOLDING_SOURCE_REBUILD_LOCK_PREFIX: &str = "erpx.source-rebuild:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldingSourceProjection {
    Operations,
    FinanceBanking,
    FinanceSubledger,
    FinanceClose,
    ItemDirectory,
}

/// A server-side transaction advisory lock used to fence source commits while
/// a replacement read database is checked and activated. Source write
/// transactions acquire the matching transaction advisory lock, so they wait
/// until the control-plane activation has completed.
pub struct HoldingSourceRebuildLock<'a> {
    transaction: Transaction<'a, Postgres>,
}

impl<'a> HoldingSourceRebuildLock<'a> {
    async fn acquire(
        pool: &'a sqlx::PgPool,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<Self, PersistenceError> {
        let lock_name = format!("{HOLDING_SOURCE_REBUILD_LOCK_PREFIX}{data_profile_key}");
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
            .bind(data_profile_key)
            .execute(&mut *transaction)
            .await?;
        let bound_holding: String =
            sqlx::query_scalar("SELECT set_config('app.holding_id', $1, true)")
                .bind(holding_id)
                .fetch_one(&mut *transaction)
                .await?;
        if bound_holding != holding_id {
            return Err(PersistenceError::InvalidData(
                "holding source lock binding was not retained".into(),
            ));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&lock_name)
            .execute(&mut *transaction)
            .await?;
        Ok(Self { transaction })
    }

    pub async fn source_event_high_watermark(
        &mut self,
        projection: HoldingSourceProjection,
        data_profile_key: &str,
    ) -> Result<u64, PersistenceError> {
        let count: i64 = match projection {
            HoldingSourceProjection::Operations => {
                sqlx::query_scalar(
                    "SELECT COALESCE(MAX(global_sequence), 0)
                     FROM erpx_cqrs_write.outbox_events
                     WHERE context_name = 'operations'
                       AND current_setting('erpx.data_profile_key') = $1",
                )
                .bind(data_profile_key)
                .fetch_one(&mut *self.transaction)
                .await?
            }
            HoldingSourceProjection::FinanceBanking => {
                sqlx::query_scalar(
                    "SELECT COUNT(*)
                     FROM erpx_holding.outbox_events
                     WHERE data_profile_key = $1
                       AND aggregate_type = ANY($2)",
                )
                .bind(data_profile_key)
                .bind([
                    "finance_bank_account",
                    "finance_bank_statement_import",
                    "finance_bank_reconciliation",
                ])
                .fetch_one(&mut *self.transaction)
                .await?
            }
            HoldingSourceProjection::FinanceSubledger => {
                sqlx::query_scalar(
                    "SELECT COUNT(*)
                     FROM erpx_holding.outbox_events
                     WHERE data_profile_key = $1
                       AND aggregate_type = ANY($2)",
                )
                .bind(data_profile_key)
                .bind([
                    "payable_invoice",
                    "receivable_invoice",
                    "vendor_payment",
                    "customer_payment",
                    "allocation",
                    "cash_receipt",
                    "cash_disbursement",
                    "cash_reconciliation",
                    "vendor_subledger_entry",
                    "customer_subledger_entry",
                ])
                .fetch_one(&mut *self.transaction)
                .await?
            }
            HoldingSourceProjection::FinanceClose => {
                sqlx::query_scalar(
                    "SELECT COUNT(*)
                     FROM erpx_holding.outbox_events
                     WHERE data_profile_key = $1
                       AND aggregate_type = $2",
                )
                .bind(data_profile_key)
                .bind("finance_close_review")
                .fetch_one(&mut *self.transaction)
                .await?
            }
            HoldingSourceProjection::ItemDirectory => {
                // Directory owner facts are appended by many independent
                // context UoWs. A short SHARE table lock fences every INSERT
                // while the final watermark and control-plane route switch
                // are committed, without requiring those owners to import an
                // infrastructure advisory-lock contract.
                sqlx::query("LOCK TABLE erpx_cqrs_write.outbox_events IN SHARE MODE")
                    .execute(&mut *self.transaction)
                    .await?;
                sqlx::query_scalar(
                    "SELECT COALESCE(MAX(global_sequence), 0)
                     FROM erpx_cqrs_write.outbox_events
                     WHERE holding_id = current_setting('app.holding_id')::uuid
                       AND event_type = ANY($1)",
                )
                .bind([
                    erpx_integration_contracts::ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    erpx_integration_contracts::AUTHORITY_EVIDENCE_EVENT_TYPE,
                    erpx_integration_contracts::organization::ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE,
                    erpx_organization_sqlx::SNAPSHOT_EVENT_TYPE,
                ])
                .fetch_one(&mut *self.transaction)
                .await?
            }
        };
        u64::try_from(count).map_err(|_| {
            PersistenceError::InvalidData("source event high-watermark is invalid".into())
        })
    }

    pub async fn commit(self) -> Result<(), PersistenceError> {
        self.transaction.commit().await?;
        Ok(())
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        self.transaction.rollback().await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct RoutedHoldingDatabase {
    pool: sqlx::PgPool,
    pub holding_id: String,
    /// Canonical domain holding identity authorized by the control-plane
    /// customer/holding binding. This may differ from the opaque physical
    /// route identity used by the registry.
    pub canonical_holding_id: Uuid,
    pub data_profile_key: String,
    pub database_key: String,
    pub side: RoutedDatabaseSide,
}

impl HoldingDatabaseRouter {
    async fn canonical_holding_id_for_route(
        &self,
        holding_id: &str,
    ) -> Result<Uuid, PersistenceError> {
        let canonical = sqlx::query_scalar::<_, Uuid>(
            "SELECT canonical_holding_id
             FROM erpx_control.customer_holding_bindings
             WHERE holding_id=$1 AND binding_state='configured'",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        canonical
            .or_else(|| {
                Uuid::parse_str(holding_id)
                    .ok()
                    .filter(|value| !value.is_nil())
            })
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "holding route has no canonical holding identity".into(),
                )
            })
    }

    pub fn new(
        control: Arc<SqlxControlPlane>,
        config: HoldingDatabaseProvisioningConfig,
    ) -> Result<Self, PersistenceError> {
        validate_database_config(&config)?;
        Ok(Self { control, config })
    }

    /// Prepare a replacement read database through the server-only
    /// provisioning boundary. The candidate is never promoted by this call.
    pub async fn prepare_replacement_read_database(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        candidate_read_database_key: &str,
    ) -> Result<(), PersistenceError> {
        HoldingDatabaseProvisioner::new(self.control.clone(), self.config.clone())?
            .provision_replacement_read_database(
                holding_id,
                data_profile_key,
                candidate_read_database_key,
            )
            .await
    }

    /// Return whether the exact candidate generation is already active. This
    /// makes a worker retry after an activation/acknowledgement ambiguity
    /// idempotent without reopening a candidate route.
    pub async fn replacement_read_is_activated(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<bool, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM erpx_control.holding_registry AS r
                 JOIN erpx_control.holding_projection_switches AS s
                   ON s.holding_id = r.holding_id
                 WHERE r.holding_id = $1
                   AND r.read_database_key = $2
                   AND s.current_read_database_key = $2
                   AND s.candidate_read_database_key <> $2
                   AND s.state = 'activated'
                   AND s.version = $3 + 1
             )",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(generation)
        .fetch_one(&self.control.pools().write)
        .await?;
        Ok(active)
    }

    /// Return whether a candidate completed its exact fenced rebuild but has
    /// not yet been activated. This closes the crash window between the ready
    /// transition and the registry/switch activation transaction.
    pub async fn replacement_read_is_ready(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
        source_version: u64,
    ) -> Result<bool, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let source_version = i64::try_from(source_version).map_err(|_| {
            PersistenceError::InvalidData("projection source version is invalid".into())
        })?;
        let ready: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM erpx_control.holding_registry AS r
                 JOIN erpx_control.holding_projection_switches AS s
                   ON s.holding_id = r.holding_id
                 WHERE r.holding_id = $1
                   AND r.state = 'active'
                   AND r.read_database_key = s.current_read_database_key
                   AND s.candidate_read_database_key = $2
                   AND s.state = 'ready'
                   AND s.version = $3
                   AND s.source_version = $4
             )",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(generation)
        .bind(source_version)
        .fetch_one(&self.control.pools().write)
        .await?;
        Ok(ready)
    }

    /// Return the source watermark bound to an exact ready candidate. Manual
    /// activation callers must compare this value while holding the holding
    /// source fence before promoting the candidate.
    pub async fn replacement_read_source_version(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<Option<u64>, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let source_version: Option<i64> = sqlx::query_scalar(
            "SELECT source_version
             FROM erpx_control.holding_projection_switches
             WHERE holding_id = $1
               AND candidate_read_database_key = $2
               AND state = 'ready'
               AND version = $3",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(generation)
        .fetch_optional(&self.control.pools().write)
        .await?;
        source_version
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    PersistenceError::InvalidData("projection source version is invalid".into())
                })
            })
            .transpose()
    }

    pub async fn complete_replacement_read_database(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<(), PersistenceError> {
        HoldingProvisioner::new(self.control.clone())
            .complete_read_rebuild_if_generation(
                holding_id,
                candidate_read_database_key,
                generation,
            )
            .await
    }

    pub async fn fail_replacement_read_database(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
        error: &'static str,
    ) -> Result<(), PersistenceError> {
        HoldingProvisioner::new(self.control.clone())
            .fail_read_rebuild_if_generation(
                holding_id,
                candidate_read_database_key,
                generation,
                error,
            )
            .await
    }

    pub async fn activate_replacement_read_database(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<(), PersistenceError> {
        HoldingProvisioner::new(self.control.clone())
            .activate_read_database_if_generation(
                holding_id,
                candidate_read_database_key,
                generation,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn activate_replacement_read_database_with_human_decision(
        &self,
        holding_id: &str,
        projection_name: &str,
        candidate_read_database_key: &str,
        generation: u64,
        command_id: Uuid,
        correlation_id: Uuid,
        decision: &HumanDecisionProof,
    ) -> Result<(), PersistenceError> {
        HoldingProvisioner::new(self.control.clone())
            .activate_read_database_if_generation_with_human_decision(
                holding_id,
                projection_name,
                candidate_read_database_key,
                generation,
                command_id,
                correlation_id,
                decision,
            )
            .await
    }

    /// Resolve a holding only through the control-plane registry. The returned
    /// pool is server-side; clients receive neither this pool nor a URL.
    pub async fn route(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        side: RoutedDatabaseSide,
    ) -> Result<RoutedHoldingDatabase, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_ref(data_profile_key, "data_profile_key")?;
        let deployment_mode: Option<String> = sqlx::query_scalar(
            "SELECT deployment_mode
             FROM erpx_control.holding_infrastructure_profiles
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        match deployment_mode.as_deref() {
            Some("erpx_managed") => {}
            Some("customer_data_plane") => {
                return Err(PersistenceError::InvalidData(
                    "customer data plane requests must route through the enrolled agent".into(),
                ));
            }
            Some(_) => {
                return Err(PersistenceError::InvalidData(
                    "holding execution route is not configured".into(),
                ));
            }
            None => {
                // Holding registry rows created before the layered customer
                // administration model have no customer binding and are
                // explicitly ERPX-managed by the rollout contract. A bound
                // holding missing its profile is inconsistent and must never
                // fall back from a customer data plane into managed storage.
                let has_customer_binding: bool = sqlx::query_scalar(
                    "SELECT EXISTS (
                         SELECT 1
                         FROM erpx_control.customer_holding_bindings
                         WHERE holding_id = $1
                     )",
                )
                .bind(holding_id)
                .fetch_one(&self.control.pools().write)
                .await?;
                if has_customer_binding {
                    return Err(PersistenceError::InvalidData(
                        "holding execution route is not configured".into(),
                    ));
                }
            }
        }
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if registration.state != "active" {
            return Err(PersistenceError::InvalidData(
                "holding is not active for request routing".into(),
            ));
        }
        if registration.data_profile_key != data_profile_key {
            return Err(PersistenceError::InvalidData(
                "holding scope does not match the holding registry".into(),
            ));
        }
        let database_key = match side {
            RoutedDatabaseSide::Write => registration.write_database_key,
            RoutedDatabaseSide::Read => registration.read_database_key,
        };
        validate_database_key(&database_key, "database_key")?;
        let database_url = render_database_url(&self.config.database_url_template, &database_key)?;
        let canonical_holding_id = self.canonical_holding_id_for_route(holding_id).await?;
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await?;
        let routed = RoutedHoldingDatabase {
            pool,
            holding_id: holding_id.into(),
            canonical_holding_id,
            data_profile_key: data_profile_key.into(),
            database_key,
            side,
        };
        if let Err(error) = routed.probe().await {
            routed.close().await;
            return Err(error);
        }
        Ok(routed)
    }

    /// Resolve a physical database from an authenticated organization scope.
    /// Caller-provided database identifiers are never accepted. Branch routes
    /// fail closed unless the holding opted into branch isolation and the exact
    /// holding/holding/company/branch binding is active.
    pub async fn route_scoped(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        scope: HoldingDataRouteScope,
        side: RoutedDatabaseSide,
    ) -> Result<RoutedHoldingDatabase, PersistenceError> {
        match scope {
            HoldingDataRouteScope::Holding => self.route(holding_id, data_profile_key, side).await,
            HoldingDataRouteScope::Company { company_id } => {
                if company_id.is_nil() {
                    return Err(PersistenceError::InvalidData(
                        "company route scope cannot be nil".into(),
                    ));
                }
                self.route(holding_id, data_profile_key, side).await
            }
            HoldingDataRouteScope::Branch {
                company_id,
                branch_id,
            } => {
                validate_ref(holding_id, "holding_id")?;
                validate_ref(data_profile_key, "data_profile_key")?;
                if company_id.is_nil() || branch_id.is_nil() {
                    return Err(PersistenceError::InvalidData(
                        "branch route scope cannot contain nil identifiers".into(),
                    ));
                }
                let row = sqlx::query(
                    "SELECT r.write_database_key, r.read_database_key,
                            p.profile_key, p.credential_profile_key, p.deployment_mode
                     FROM erpx_control.branch_database_routes r
                     JOIN erpx_control.holding_registry t
                       ON t.holding_id=r.holding_id AND t.data_profile_key=r.data_profile_key
                     JOIN erpx_control.database_profiles p
                       ON p.profile_key=r.profile_key
                     JOIN erpx_control.holding_database_profile_bindings b
                       ON b.holding_id=r.holding_id AND b.profile_key=r.profile_key
                     WHERE r.holding_id=$1 AND r.data_profile_key=$2
                       AND r.company_id=$3 AND r.branch_id=$4
                       AND r.state='active' AND t.state='active'
                       AND t.isolation_mode='holding_and_branch_pairs'
                       AND p.state='active' AND b.active",
                )
                .bind(holding_id)
                .bind(data_profile_key)
                .bind(company_id)
                .bind(branch_id)
                .fetch_optional(&self.control.pools().write)
                .await?
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "active branch database route is unavailable".into(),
                    )
                })?;
                let deployment_mode: String = row.try_get("deployment_mode")?;
                if deployment_mode != "erpx_managed" {
                    return Err(PersistenceError::InvalidData(
                        "customer branch data plane requests must route through the enrolled agent"
                            .into(),
                    ));
                }
                let database_key: String = match side {
                    RoutedDatabaseSide::Write => row.try_get("write_database_key")?,
                    RoutedDatabaseSide::Read => row.try_get("read_database_key")?,
                };
                validate_database_key(&database_key, "database_key")?;
                let profile_key: String = row.try_get("profile_key")?;
                let credential_profile_key: String = row.try_get("credential_profile_key")?;
                let profile = ApprovedDatabaseProfileConnection::from_environment(
                    &profile_key,
                    &credential_profile_key,
                )?;
                let canonical_holding_id = self.canonical_holding_id_for_route(holding_id).await?;
                let pool = PgPoolOptions::new()
                    .max_connections(10)
                    .connect_with(profile.database_options(&database_key))
                    .await?;
                let routed = RoutedHoldingDatabase {
                    pool,
                    holding_id: holding_id.into(),
                    canonical_holding_id,
                    data_profile_key: data_profile_key.into(),
                    database_key,
                    side,
                };
                if let Err(error) = routed.probe().await {
                    routed.close().await;
                    return Err(error);
                }
                Ok(routed)
            }
        }
    }

    /// Resolve a replacement read database only after the control-plane
    /// switch state has bound the candidate to this holding. The candidate is
    /// never selected from caller-supplied credentials or an arbitrary URL;
    /// the server-only routing template remains the sole connection source.
    pub async fn route_replacement_read(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        candidate_read_database_key: &str,
    ) -> Result<RoutedHoldingDatabase, PersistenceError> {
        self.route_replacement_read_fenced(
            holding_id,
            data_profile_key,
            candidate_read_database_key,
            None,
            None,
        )
        .await
    }

    /// Resolve a replacement read database only when the exact switch
    /// generation and source high-watermark requested by the worker are still
    /// current. This is the only candidate route used by durable rebuild
    /// jobs; the unfenced method above remains for the older explicit seam.
    pub async fn route_replacement_read_for_generation(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        candidate_read_database_key: &str,
        generation: u64,
        source_version: u64,
    ) -> Result<RoutedHoldingDatabase, PersistenceError> {
        self.route_replacement_read_fenced(
            holding_id,
            data_profile_key,
            candidate_read_database_key,
            Some(generation),
            Some(source_version),
        )
        .await
    }

    async fn route_replacement_read_fenced(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        candidate_read_database_key: &str,
        generation: Option<u64>,
        source_version: Option<u64>,
    ) -> Result<RoutedHoldingDatabase, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_ref(data_profile_key, "data_profile_key")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = generation
            .map(|value| {
                i64::try_from(value).map_err(|_| {
                    PersistenceError::InvalidData("projection switch generation is invalid".into())
                })
            })
            .transpose()?;
        let source_version = source_version
            .map(|value| {
                i64::try_from(value).map_err(|_| {
                    PersistenceError::InvalidData("projection source version is invalid".into())
                })
            })
            .transpose()?;
        if generation.is_some() != source_version.is_some() {
            return Err(PersistenceError::InvalidData(
                "candidate route fence is incomplete".into(),
            ));
        }
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if registration.state != "active" {
            return Err(PersistenceError::InvalidData(
                "holding is not active for replacement routing".into(),
            ));
        }
        if registration.data_profile_key != data_profile_key {
            return Err(PersistenceError::InvalidData(
                "holding scope does not match the holding registry".into(),
            ));
        }
        let mut control_transaction = self.control.pools().write.begin().await?;
        let switch = sqlx::query(
            "SELECT r.state AS registry_state, r.data_profile_key, r.read_database_key,
                    s.current_read_database_key,
                    s.candidate_read_database_key, s.state AS switch_state,
                    s.version, s.source_version
             FROM erpx_control.holding_registry AS r
             JOIN erpx_control.holding_projection_switches AS s
               ON s.holding_id = r.holding_id
             WHERE r.holding_id = $1
               AND ($2::BIGINT IS NULL OR s.version = $2)
               AND ($3::BIGINT IS NULL OR s.source_version = $3)
             FOR SHARE OF r, s",
        )
        .bind(holding_id)
        .bind(generation)
        .bind(source_version)
        .fetch_optional(&mut *control_transaction)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "replacement read state is not registered".into(),
        ))?;
        let registered_state: String = switch.try_get("registry_state")?;
        let registered_holding: String = switch.try_get("data_profile_key")?;
        let registered_current: String = switch.try_get("read_database_key")?;
        let current: String = switch.try_get("current_read_database_key")?;
        let candidate: String = switch.try_get("candidate_read_database_key")?;
        let state: String = switch.try_get("switch_state")?;
        if registered_state != "active"
            || registered_holding != data_profile_key
            || registered_current != current
        {
            let _ = control_transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read state is stale".into(),
            ));
        }
        if candidate != candidate_read_database_key
            || current == candidate_read_database_key
            || !matches!(state.as_str(), "rebuilding" | "ready")
        {
            let _ = control_transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read target is not active for this holding".into(),
            ));
        }
        control_transaction.commit().await?;
        let database_url = render_database_url(
            &self.config.database_url_template,
            candidate_read_database_key,
        )?;
        let canonical_holding_id = self.canonical_holding_id_for_route(holding_id).await?;
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await?;
        let routed = RoutedHoldingDatabase {
            pool,
            holding_id: holding_id.into(),
            canonical_holding_id,
            data_profile_key: data_profile_key.into(),
            database_key: candidate_read_database_key.into(),
            side: RoutedDatabaseSide::Read,
        };
        if routed.probe_replacement_read().await.is_err() {
            routed.close().await;
            return Err(PersistenceError::InvalidData(
                "replacement read database readiness probe failed".into(),
            ));
        }
        Ok(routed)
    }

    /// Compose fresh control-plane metadata with live write/read route
    /// probes. A stale or missing durable observation can never be promoted
    /// by a successful socket probe, and a failed live probe is represented as
    /// unavailable without exposing the underlying connection details.
    pub async fn observe_holding_readiness(
        &self,
        holding_id: &str,
        now: DateTime<Utc>,
        policy: ReadinessObservationPolicy,
    ) -> Result<HoldingReadinessObservation, PersistenceError> {
        let metadata = HoldingProvisioner::new(self.control.clone())
            .observe_holding_readiness_metadata(holding_id, now, policy)
            .await?;
        let registry = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?;

        let (live_write, live_read) = match registry.as_ref() {
            Some(registration) if registration.state == "active" => {
                let write = self
                    .probe_route(
                        holding_id,
                        &registration.data_profile_key,
                        RoutedDatabaseSide::Write,
                    )
                    .await;
                let read = self
                    .probe_route(
                        holding_id,
                        &registration.data_profile_key,
                        RoutedDatabaseSide::Read,
                    )
                    .await;
                (write, read)
            }
            Some(_) => (
                ReadinessComponentObservation::unavailable("holding_not_active", None),
                ReadinessComponentObservation::unavailable("holding_not_active", None),
            ),
            None => (
                ReadinessComponentObservation::unavailable("holding_not_registered", None),
                ReadinessComponentObservation::unavailable("holding_not_registered", None),
            ),
        };
        let write_database = merge_database_observation(metadata.write_database, live_write);
        let read_database = merge_database_observation(metadata.read_database, live_read);
        let registry_state = metadata.registry_state.clone();
        let registry_version = metadata.registry_version;
        let routing_state = aggregate_readiness_states([
            if registry_state.as_deref() == Some("active") {
                ReadinessObservationState::Ready
            } else {
                ReadinessObservationState::Unavailable
            },
            write_database.state,
            read_database.state,
            metadata.storage.state,
        ]);
        let reason_code = first_reason([
            if registry_state.as_deref() == Some("active") {
                None
            } else if registry_state.is_none() {
                Some("holding_not_registered")
            } else {
                Some("holding_not_active")
            },
            write_database.reason_code.as_deref(),
            read_database.reason_code.as_deref(),
            metadata.storage.reason_code.as_deref(),
        ]);
        Ok(HoldingReadinessObservation {
            holding_id: holding_id.into(),
            routing: HoldingRoutingObservation {
                holding_id: holding_id.into(),
                state: routing_state,
                registry_state,
                registry_version,
                write_database,
                read_database,
                storage: metadata.storage,
                checked_at: now,
                reason_code,
            },
            projection_freshness: metadata.projection_freshness,
            backup_pitr: metadata.backup_pitr,
        })
    }

    async fn probe_route(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        side: RoutedDatabaseSide,
    ) -> ReadinessComponentObservation {
        let routed = match self.route(holding_id, data_profile_key, side).await {
            Ok(routed) => routed,
            Err(_) => {
                return ReadinessComponentObservation::unavailable(
                    "holding_database_route_failed",
                    None,
                );
            }
        };
        let result = routed.probe().await;
        routed.close().await;
        match result {
            Ok(observation) => observation,
            Err(_) => {
                ReadinessComponentObservation::unavailable("holding_database_probe_failed", None)
            }
        }
    }
}

impl RoutedHoldingDatabase {
    /// Borrow the already-routed server-side pool for infrastructure adapters.
    /// The pool carries no URL or credential serialization API and must remain
    /// on the server side of the worker boundary.
    pub fn database_pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    pub async fn acquire_source_rebuild_lock(
        &self,
    ) -> Result<HoldingSourceRebuildLock<'_>, PersistenceError> {
        HoldingSourceRebuildLock::acquire(&self.pool, &self.holding_id, &self.data_profile_key)
            .await
    }

    /// Begin a scoped transaction. The RLS setting is transaction-local and
    /// therefore cannot leak to another request using the same pool.
    pub async fn begin_scoped(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        let holding_id = Uuid::parse_str(&self.holding_id).map_err(|_| {
            PersistenceError::InvalidData(
                "current routed database requires a UUID holding_id".into(),
            )
        })?;
        if holding_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "current routed database requires a non-nil holding_id".into(),
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let bound: String = sqlx::query_scalar("SELECT set_config('app.holding_id', $1, true)")
            .bind(holding_id.to_string())
            .fetch_one(&mut *transaction)
            .await?;
        if bound != holding_id.to_string() {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "routed database holding binding could not be verified".into(),
            ));
        }
        sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
            .bind(&self.data_profile_key)
            .execute(&mut *transaction)
            .await?;
        Ok(transaction)
    }

    /// Probe the routed database without returning its URL, credentials, or
    /// database name. The relation check proves that the route reached the
    /// expected holding-side schema rather than merely accepting a TCP
    /// connection.
    pub async fn probe(&self) -> Result<ReadinessComponentObservation, PersistenceError> {
        let relation = match self.side {
            RoutedDatabaseSide::Write => "erpx_cqrs_write.command_receipts",
            RoutedDatabaseSide::Read => "erpx_cqrs_read.projection_generations",
        };
        verify_holding_database_contract(&self.pool, &self.database_key, relation).await?;
        Ok(ReadinessComponentObservation::ready(Utc::now()))
    }

    /// Verify the migration and RLS contract required before a replacement
    /// read pool can receive a projector. A socket and one relation are not
    /// sufficient evidence that a disposable candidate is safe to write.
    pub async fn probe_replacement_read(&self) -> Result<(), PersistenceError> {
        if self.side != RoutedDatabaseSide::Read {
            return Err(PersistenceError::InvalidData(
                "replacement probe requires a read route".into(),
            ));
        }
        let mut transaction = self.begin_scoped().await?;
        let identity: (String, Option<String>) = sqlx::query_as(
            "SELECT current_database(), to_regclass('erpx_cqrs_read.projection_generations')::text",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if identity.0 != self.database_key
            || identity.1.as_deref() != Some("erpx_cqrs_read.projection_generations")
        {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read database identity or base schema is invalid".into(),
            ));
        }

        let relations = sqlx::query(
            "SELECT n.nspname || '.' || c.relname AS relation_name,
                    c.relrowsecurity, c.relforcerowsecurity,
                    EXISTS (
                        SELECT 1 FROM pg_policy AS p WHERE p.polrelid = c.oid
                    ) AS has_policy
             FROM pg_class AS c
             JOIN pg_namespace AS n ON n.oid = c.relnamespace
             WHERE (n.nspname, c.relname) IN (
                   ('erpx_cqrs_read', 'projection_generations'),
                   ('erpx_cqrs_read', 'projection_checkpoints'),
                   ('erpx_organization_read', 'node_projection'),
                   ('erpx_operations_read', 'exception_projection'),
                   ('erpx_operations_read', 'source_signal_projection'),
                   ('erpx_operations_read', 'work_queue_policy_projection')
             )",
        )
        .fetch_all(&mut *transaction)
        .await?;
        if relations.len() != 6
            || relations.iter().any(|row| {
                !row.try_get::<bool, _>("relrowsecurity").unwrap_or(false)
                    || !row
                        .try_get::<bool, _>("relforcerowsecurity")
                        .unwrap_or(false)
                    || !row.try_get::<bool, _>("has_policy").unwrap_or(false)
            })
        {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read migration or RLS contract is incomplete".into(),
            ));
        }

        let visible_foreign_scope: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_operations_read.exception_projection
                 WHERE holding_id <> erpx_cqrs_read.current_holding_id()
             )",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if visible_foreign_scope {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read RLS scope is not isolated".into(),
            ));
        }
        transaction.rollback().await?;
        Ok(())
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}

impl HoldingDatabaseProvisioner {
    pub fn new(
        control: Arc<SqlxControlPlane>,
        config: HoldingDatabaseProvisioningConfig,
    ) -> Result<Self, PersistenceError> {
        validate_database_config(&config)?;
        Ok(Self { control, config })
    }

    /// Construct the branch-only profile runtime. Its connection methods
    /// require an `ApprovedDatabaseProfileConnection`; the inert local
    /// fallback is private state and is never used to route a database.
    pub fn for_approved_profiles(control: Arc<SqlxControlPlane>) -> Self {
        Self {
            control,
            config: HoldingDatabaseProvisioningConfig {
                admin_database_url: "postgresql://localhost/postgres?sslmode=disable".into(),
                database_url_template: "postgresql://localhost/{database}?sslmode=disable".into(),
            },
        }
    }

    /// Create and migrate the two physical per-holding databases. A partial
    /// failure is left in the control plane's provisioning state so a retry
    /// can complete it; no database is dropped implicitly.
    pub async fn provision_databases(
        &self,
        holding_id: &str,
    ) -> Result<PhysicalHoldingDatabases, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if !matches!(registration.state.as_str(), "provisioning" | "recovery") {
            return Err(PersistenceError::InvalidData(
                "holding databases can only be provisioned in provisioning or recovery state"
                    .into(),
            ));
        }
        validate_database_key(&registration.write_database_key, "write_database_key")?;
        validate_database_key(&registration.read_database_key, "read_database_key")?;
        // Database keys are global physical resources, not merely holding
        // metadata. Serialize the check on both keys and reject any existing
        // owner before creating or migrating a database. This closes the
        // race where two holding registrations could otherwise point at the
        // same physical database while each was still in provisioning.
        let mut control_transaction = self.control.pools().write.begin().await?;
        let mut lock_keys = [
            registration.write_database_key.as_str(),
            registration.read_database_key.as_str(),
        ];
        lock_keys.sort_unstable();
        for database_key in lock_keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(database_key)
                .execute(&mut *control_transaction)
                .await?;
        }
        let key_conflict: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                 FROM erpx_control.holding_registry
                 WHERE holding_id <> $1
                   AND (write_database_key IN ($2, $3)
                        OR read_database_key IN ($2, $3))
             )",
        )
        .bind(holding_id)
        .bind(&registration.write_database_key)
        .bind(&registration.read_database_key)
        .fetch_one(&mut *control_transaction)
        .await?;
        if key_conflict {
            return Err(PersistenceError::Configuration(
                "holding database key is already owned by another holding",
            ));
        }
        control_transaction.commit().await?;
        self.create_database_if_missing(&registration.write_database_key)
            .await?;
        self.create_database_if_missing(&registration.read_database_key)
            .await?;
        self.migrate_database(&registration.write_database_key, DatabaseSide::Write)
            .await?;
        self.migrate_database(&registration.read_database_key, DatabaseSide::Read)
            .await?;

        let checked_at = Utc::now();
        let metadata = HoldingProvisioner::new(self.control.clone());
        metadata
            .record_database_health(&HoldingDatabaseHealth {
                holding_id: holding_id.into(),
                side: DatabaseSide::Write,
                database_key: registration.write_database_key.clone(),
                schema_version: "holding-write-0001".into(),
                reachable: true,
                checked_at,
                details: serde_json::json!({"migration": "applied", "rls": "forced"}),
            })
            .await?;
        metadata
            .record_database_health(&HoldingDatabaseHealth {
                holding_id: holding_id.into(),
                side: DatabaseSide::Read,
                database_key: registration.read_database_key.clone(),
                schema_version: "holding-read-0001".into(),
                reachable: true,
                checked_at,
                details: serde_json::json!({"migration": "applied", "rls": "forced"}),
            })
            .await?;
        PhysicalHoldingDatabases::new(
            holding_id,
            registration.write_database_key,
            registration.read_database_key,
        )
    }

    /// Create, migrate, and verify an already-reserved branch database pair.
    /// The route and keys must come from the control-plane provisioning job;
    /// this method never accepts a URL or creates/activates route metadata.
    pub async fn provision_branch_databases(
        &self,
        route: &BranchDatabaseRoute,
    ) -> Result<(), PersistenceError> {
        if route.state != BranchProvisioningState::Provisioning {
            return Err(PersistenceError::InvalidData(
                "branch databases require a claimed provisioning route".into(),
            ));
        }
        validate_ref(&route.holding_key, "holding_key")?;
        validate_ref(&route.data_profile_key, "data_profile_key")?;
        validate_database_key(&route.write_database_key, "write_database_key")?;
        validate_database_key(&route.read_database_key, "read_database_key")?;
        if route.holding_id != holding_holding_scope_id(&route.data_profile_key)
            || route.company_id.is_nil()
            || route.branch_id.is_nil()
            || route.write_database_key == route.read_database_key
        {
            return Err(PersistenceError::InvalidData(
                "branch database route binding is invalid".into(),
            ));
        }
        let mut tx = self.control.pools().write.begin().await?;
        let mut keys = [
            route.write_database_key.as_str(),
            route.read_database_key.as_str(),
        ];
        keys.sort_unstable();
        for key in keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        let collision: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.holding_registry
                 WHERE write_database_key IN ($1,$2) OR read_database_key IN ($1,$2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.branch_database_routes
                 WHERE NOT (data_profile_key=$3 AND branch_id=$4)
                   AND (write_database_key IN ($1,$2) OR read_database_key IN ($1,$2))
             )",
        )
        .bind(&route.write_database_key)
        .bind(&route.read_database_key)
        .bind(&route.data_profile_key)
        .bind(route.branch_id)
        .fetch_one(&mut *tx)
        .await?;
        if collision {
            return Err(PersistenceError::Configuration(
                "branch database key is already owned by another route",
            ));
        }
        tx.commit().await?;
        self.create_database_if_missing(&route.write_database_key)
            .await?;
        self.create_database_if_missing(&route.read_database_key)
            .await?;
        self.migrate_database(&route.write_database_key, DatabaseSide::Write)
            .await?;
        self.migrate_database(&route.read_database_key, DatabaseSide::Read)
            .await?;
        let checked_at = Utc::now();
        for (side, key, schema_version) in [
            ("write", &route.write_database_key, "holding-write-current"),
            ("read", &route.read_database_key, "holding-read-current"),
        ] {
            sqlx::query(
                "INSERT INTO erpx_control.branch_database_health
                    (holding_id, branch_id, side, database_key, schema_version,
                     reachable, checked_at, failure_category)
                 VALUES ($1,$2,$3,$4,$5,TRUE,$6,NULL)
                 ON CONFLICT (holding_id, branch_id, side) DO UPDATE
                 SET database_key=EXCLUDED.database_key,
                     schema_version=EXCLUDED.schema_version,
                     reachable=TRUE, checked_at=EXCLUDED.checked_at,
                     failure_category=NULL",
            )
            .bind(&route.holding_key)
            .bind(route.branch_id)
            .bind(side)
            .bind(key)
            .bind(schema_version)
            .bind(checked_at)
            .execute(&self.control.pools().write)
            .await?;
        }
        Ok(())
    }

    /// Provision a branch through the exact holding-approved profile whose
    /// credentials were resolved outside ERPX storage.
    pub async fn provision_branch_databases_with_profile(
        &self,
        route: &BranchDatabaseRoute,
        profile: &ApprovedDatabaseProfileConnection,
    ) -> Result<(), PersistenceError> {
        if route.profile_key != profile.profile_key() {
            return Err(PersistenceError::InvalidData(
                "resolved database profile does not match the branch route".into(),
            ));
        }
        if route.state != BranchProvisioningState::Provisioning {
            return Err(PersistenceError::InvalidData(
                "branch databases require a claimed provisioning route".into(),
            ));
        }
        validate_ref(&route.holding_key, "holding_key")?;
        validate_ref(&route.data_profile_key, "data_profile_key")?;
        validate_database_key(&route.write_database_key, "write_database_key")?;
        validate_database_key(&route.read_database_key, "read_database_key")?;
        if route.holding_id != holding_holding_scope_id(&route.data_profile_key)
            || route.company_id.is_nil()
            || route.branch_id.is_nil()
            || route.write_database_key == route.read_database_key
        {
            return Err(PersistenceError::InvalidData(
                "branch database route binding is invalid".into(),
            ));
        }
        let mut tx = self.control.pools().write.begin().await?;
        let mut keys = [
            route.write_database_key.as_str(),
            route.read_database_key.as_str(),
        ];
        keys.sort_unstable();
        for key in keys {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        let collision: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.holding_registry
                 WHERE write_database_key IN ($1,$2) OR read_database_key IN ($1,$2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.branch_database_routes
                 WHERE NOT (data_profile_key=$3 AND branch_id=$4)
                   AND (write_database_key IN ($1,$2) OR read_database_key IN ($1,$2))
             )",
        )
        .bind(&route.write_database_key)
        .bind(&route.read_database_key)
        .bind(&route.data_profile_key)
        .bind(route.branch_id)
        .fetch_one(&mut *tx)
        .await?;
        if collision {
            return Err(PersistenceError::Configuration(
                "branch database key is already owned by another route",
            ));
        }
        tx.commit().await?;

        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(profile.admin_options())
            .await?;
        for database_key in [&route.write_database_key, &route.read_database_key] {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
                    .bind(database_key)
                    .fetch_one(&admin)
                    .await?;
            if !exists {
                let statement = format!(
                    "CREATE DATABASE {} OWNER {}",
                    quote_identifier(database_key),
                    quote_identifier(profile.options.get_username()),
                );
                if let Err(error) = sqlx::query(&statement).execute(&admin).await
                    && !is_duplicate_database(&error)
                {
                    admin.close().await;
                    return Err(PersistenceError::Database(error));
                }
            }
        }
        admin.close().await;

        for (side, database_key, schema_version) in [
            (
                DatabaseSide::Write,
                &route.write_database_key,
                "holding-write-current",
            ),
            (
                DatabaseSide::Read,
                &route.read_database_key,
                "holding-read-current",
            ),
        ] {
            let pool = PgPoolOptions::new()
                .max_connections(3)
                .connect_with(profile.database_options(database_key))
                .await?;
            let migration = match side {
                DatabaseSide::Write => migrate_holding_write(&pool).await,
                DatabaseSide::Read => migrate_holding_read(&pool).await,
            }
            .map_err(PersistenceError::Migration);
            if migration.is_ok() {
                let relation = match side {
                    DatabaseSide::Write => "erpx_cqrs_write.command_receipts",
                    DatabaseSide::Read => "erpx_cqrs_read.projection_generations",
                };
                verify_holding_database_contract(&pool, database_key, relation).await?;
            }
            pool.close().await;
            migration?;
            sqlx::query(
                "INSERT INTO erpx_control.branch_database_health
                    (holding_id, branch_id, side, database_key, schema_version,
                     reachable, checked_at, failure_category)
                 VALUES ($1,$2,$3,$4,$5,TRUE,$6,NULL)
                 ON CONFLICT (holding_id, branch_id, side) DO UPDATE
                 SET database_key=EXCLUDED.database_key,
                     schema_version=EXCLUDED.schema_version,
                     reachable=TRUE, checked_at=EXCLUDED.checked_at,
                     failure_category=NULL",
            )
            .bind(&route.holding_key)
            .bind(route.branch_id)
            .bind(side.as_str())
            .bind(database_key)
            .bind(schema_version)
            .bind(Utc::now())
            .execute(&self.control.pools().write)
            .await?;
        }
        Ok(())
    }

    /// Seed the authoritative branch aggregate and its local read projection
    /// before the route is made active. The deterministic event and
    /// idempotency identifiers make interrupted worker retries safe.
    pub async fn initialize_branch_organization(
        &self,
        route: &BranchDatabaseRoute,
        job: &BranchProvisioningJob,
        worker_id: Uuid,
    ) -> Result<(), PersistenceError> {
        if route.state != BranchProvisioningState::Provisioning
            || route.holding_key != job.holding_id
            || route.branch_id != job.branch_id
            || worker_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "branch organization provisioning binding is invalid".into(),
            ));
        }
        // Organization hierarchy remains authoritative in the holding data
        // plane. A branch database pair receives scoped projections only and
        // must never become a second write authority for the same branch.
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(&route.holding_key)
            .await?
            .ok_or_else(|| PersistenceError::InvalidData("holding is not registered".into()))?;
        if registration.data_profile_key != route.data_profile_key {
            return Err(PersistenceError::InvalidData(
                "branch organization route does not match holding holding authority".into(),
            ));
        }
        let write_url = render_database_url(
            &self.config.database_url_template,
            &registration.write_database_key,
        )?;
        let write_pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&write_url)
            .await?;
        let result = async {
            initialize_branch_organization_rows(&write_pool, route, job, worker_id, Utc::now())
                .await?;
            for unit in &job.enabled_business_units {
                sqlx::query(
                    "INSERT INTO erpx_control.branch_business_unit_configurations
                        (holding_id, branch_id, business_unit, active,
                         configuration_version, updated_by)
                     VALUES ($1,$2,$3,TRUE,1,$4)
                     ON CONFLICT (holding_id, branch_id, business_unit) DO UPDATE
                     SET active=TRUE, updated_at=now()",
                )
                .bind(&route.holding_key)
                .bind(route.branch_id)
                .bind(unit.as_str())
                .bind(format!("erpx-worker:{worker_id}"))
                .execute(&self.control.pools().write)
                .await?;
            }
            Ok::<_, PersistenceError>(())
        }
        .await;
        write_pool.close().await;
        result
    }

    pub async fn initialize_branch_organization_with_profile(
        &self,
        route: &BranchDatabaseRoute,
        job: &BranchProvisioningJob,
        worker_id: Uuid,
        profile: &ApprovedDatabaseProfileConnection,
    ) -> Result<(), PersistenceError> {
        if route.profile_key != profile.profile_key()
            || route.state != BranchProvisioningState::Provisioning
            || route.holding_key != job.holding_id
            || route.branch_id != job.branch_id
            || worker_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "branch organization provisioning profile is invalid".into(),
            ));
        }
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(&route.holding_key)
            .await?
            .ok_or_else(|| PersistenceError::InvalidData("holding is not registered".into()))?;
        if registration.data_profile_key != route.data_profile_key {
            return Err(PersistenceError::InvalidData(
                "branch organization route does not match holding holding authority".into(),
            ));
        }
        let write_pool = PgPoolOptions::new()
            .max_connections(3)
            .connect_with(profile.database_options(&registration.write_database_key))
            .await?;
        let result = async {
            initialize_branch_organization_rows(&write_pool, route, job, worker_id, Utc::now())
                .await?;
            for unit in &job.enabled_business_units {
                sqlx::query(
                    "INSERT INTO erpx_control.branch_business_unit_configurations
                        (holding_id, branch_id, business_unit, active,
                         configuration_version, updated_by)
                     VALUES ($1,$2,$3,TRUE,1,$4)
                     ON CONFLICT (holding_id, branch_id, business_unit) DO UPDATE
                     SET active=TRUE, updated_at=now()",
                )
                .bind(&route.holding_key)
                .bind(route.branch_id)
                .bind(unit.as_str())
                .bind(format!("erpx-worker:{worker_id}"))
                .execute(&self.control.pools().write)
                .await?;
            }
            Ok::<_, PersistenceError>(())
        }
        .await;
        write_pool.close().await;
        result
    }

    /// Initialize the holding-local organization root after both physical
    /// databases have migrated. The typed Organization command transaction
    /// writes normalized authority, receipt, audit, and outbox evidence. The
    /// disposable read model is populated asynchronously by its projector.
    pub async fn initialize_holding_organization(
        &self,
        holding_key: &str,
        holding_id: Uuid,
        owner_principal_ref: &str,
    ) -> Result<HoldingOrganizationBootstrap, PersistenceError> {
        validate_ref(holding_key, "holding_key")?;
        validate_principal_ref(owner_principal_ref, "owner_principal_ref")?;
        if holding_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "organization holding ID cannot be nil".into(),
            ));
        }
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_key)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        let expected_holding_id = Uuid::parse_str(&registration.holding_id).map_err(|_| {
            PersistenceError::InvalidData("registered holding ID must be a UUID".into())
        })?;
        if expected_holding_id != holding_id {
            return Err(PersistenceError::InvalidData(
                "organization holding ID does not match the holding registry".into(),
            ));
        }
        if !matches!(registration.state.as_str(), "provisioning" | "recovery") {
            return Err(PersistenceError::InvalidData(
                "organization bootstrap requires provisioning or recovery state".into(),
            ));
        }
        let write_url = render_database_url(
            &self.config.database_url_template,
            &registration.write_database_key,
        )?;
        let read_url = render_database_url(
            &self.config.database_url_template,
            &registration.read_database_key,
        )?;
        let write_pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&write_url)
            .await?;
        let read_pool = match PgPoolOptions::new()
            .max_connections(3)
            .connect(&read_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                write_pool.close().await;
                return Err(PersistenceError::Database(error));
            }
        };
        let result = initialize_holding_organization_rows(
            &write_pool,
            &read_pool,
            holding_key,
            &registration.data_profile_key,
            holding_id,
            owner_principal_ref,
            Utc::now(),
        )
        .await;
        write_pool.close().await;
        read_pool.close().await;
        result
    }

    /// Create and migrate a disposable replacement read database that is
    /// already bound to an active rebuilding switch. The candidate is never
    /// written into the active holding registry and cannot be routed through
    /// the normal read path until the transactional switch completes.
    pub async fn provision_replacement_read_database(
        &self,
        holding_id: &str,
        data_profile_key: &str,
        candidate_read_database_key: &str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_ref(data_profile_key, "data_profile_key")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if registration.state != "active" {
            return Err(PersistenceError::InvalidData(
                "replacement read database requires an active holding".into(),
            ));
        }
        if registration.write_database_key == candidate_read_database_key
            || registration.read_database_key == candidate_read_database_key
        {
            return Err(PersistenceError::Configuration(
                "replacement read database must be distinct from provisioned databases",
            ));
        }
        let mut control_transaction = self.control.pools().write.begin().await?;
        let switch = sqlx::query(
            "SELECT r.state AS registry_state, r.data_profile_key, r.read_database_key,
                    s.current_read_database_key,
                    s.candidate_read_database_key, s.state AS switch_state
             FROM erpx_control.holding_registry AS r
             JOIN erpx_control.holding_projection_switches AS s
               ON s.holding_id = r.holding_id
             WHERE r.holding_id = $1
             FOR SHARE OF r, s",
        )
        .bind(holding_id)
        .fetch_optional(&mut *control_transaction)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "replacement read state is not registered".into(),
        ))?;
        let registered_state: String = switch.try_get("registry_state")?;
        let registered_holding: String = switch.try_get("data_profile_key")?;
        let registered_current: String = switch.try_get("read_database_key")?;
        let current: String = switch.try_get("current_read_database_key")?;
        let candidate: String = switch.try_get("candidate_read_database_key")?;
        let state: String = switch.try_get("switch_state")?;
        if registered_state != "active"
            || registered_holding != data_profile_key
            || registered_current != current
            || current != registration.read_database_key
            || candidate != candidate_read_database_key
            || state != "rebuilding"
        {
            let _ = control_transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "replacement read candidate is not in rebuilding state".into(),
            ));
        }
        control_transaction.commit().await?;

        self.create_database_if_missing(candidate_read_database_key)
            .await?;
        self.migrate_database(candidate_read_database_key, DatabaseSide::Read)
            .await
    }

    /// Prepare a restore-test candidate after a durable backup job has been
    /// claimed. The candidate remains outside the active registry and is not
    /// routable until the provisioner's fenced restore activation completes.
    pub async fn provision_restore_test_candidate_database(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if registration.state != "active" {
            return Err(PersistenceError::InvalidData(
                "restore candidate requires an active holding".into(),
            ));
        }
        if registration.write_database_key == candidate_read_database_key
            || registration.read_database_key == candidate_read_database_key
        {
            return Err(PersistenceError::Configuration(
                "restore candidate must be distinct from provisioned databases",
            ));
        }
        let candidate_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.holding_registry
                 WHERE holding_id <> $1
                   AND (write_database_key = $2 OR read_database_key = $2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.holding_projection_switches
                 WHERE holding_id <> $1 AND candidate_read_database_key = $2
             )",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .fetch_one(&self.control.pools().write)
        .await?;
        if candidate_owned {
            return Err(PersistenceError::Configuration(
                "restore candidate is already owned by another holding",
            ));
        }
        self.create_database_if_missing(candidate_read_database_key)
            .await?;
        self.migrate_database(candidate_read_database_key, DatabaseSide::Read)
            .await
    }

    /// Ensure the registered holding namespace exists through an injected
    /// object-store operation, then persist the verification result. A
    /// pending/unreachable health row is written before the external call so a
    /// stale successful check cannot activate a holding while verification is
    /// in progress or after it fails.
    pub async fn provision_storage_namespace<F, Fut>(
        &self,
        holding_id: &str,
        ensure_namespace: F,
    ) -> Result<HoldingStorageHealth, PersistenceError>
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = Result<(), ObjectStorageError>>,
    {
        validate_ref(holding_id, "holding_id")?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if !matches!(registration.state.as_str(), "provisioning" | "recovery") {
            return Err(PersistenceError::InvalidData(
                "holding storage can only be provisioned in provisioning or recovery state".into(),
            ));
        }

        let metadata = HoldingProvisioner::new(self.control.clone());
        let pending = HoldingStorageHealth {
            holding_id: holding_id.into(),
            storage_namespace: registration.storage_namespace.clone(),
            reachable: false,
            checked_at: Utc::now(),
            details: serde_json::json!({
                "adapter": "object_store",
                "operation": "ensure_namespace",
                "verified": false,
                "state": "pending"
            }),
        };
        metadata.record_storage_health(&pending).await?;

        if ensure_namespace(registration.storage_namespace.clone())
            .await
            .is_err()
        {
            let failed = HoldingStorageHealth {
                checked_at: Utc::now(),
                details: serde_json::json!({
                    "adapter": "object_store",
                    "operation": "ensure_namespace",
                    "verified": false,
                    "state": "failed"
                }),
                ..pending
            };
            // Keep the already-false pending record if the failure update
            // itself cannot be persisted; either way activation must fail.
            metadata.record_storage_health(&failed).await?;
            return Err(PersistenceError::InvalidData(
                "holding object storage namespace verification failed".into(),
            ));
        }

        let healthy = HoldingStorageHealth {
            holding_id: holding_id.into(),
            storage_namespace: registration.storage_namespace,
            reachable: true,
            checked_at: Utc::now(),
            details: serde_json::json!({
                "adapter": "object_store",
                "operation": "ensure_namespace",
                "verified": true,
                "state": "healthy"
            }),
        };
        metadata.record_storage_health(&healthy).await?;
        Ok(healthy)
    }

    async fn create_database_if_missing(&self, database_key: &str) -> Result<(), PersistenceError> {
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.config.admin_database_url)
            .await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(database_key)
                .fetch_one(&admin)
                .await?;
        if !exists {
            let owner = database_owner_from_template(&self.config.database_url_template)?;
            let statement = format!(
                "CREATE DATABASE {} OWNER {}",
                quote_identifier(database_key),
                quote_identifier(&owner),
            );
            if let Err(error) = sqlx::query(&statement).execute(&admin).await
                && !is_duplicate_database(&error)
            {
                admin.close().await;
                return Err(PersistenceError::Database(error));
            }
        }
        admin.close().await;
        Ok(())
    }

    async fn migrate_database(
        &self,
        database_key: &str,
        side: DatabaseSide,
    ) -> Result<(), PersistenceError> {
        let database_url = render_database_url(&self.config.database_url_template, database_key)?;
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .connect(&database_url)
            .await?;
        let result = match side {
            DatabaseSide::Write => migrate_holding_write(&pool).await,
            DatabaseSide::Read => migrate_holding_read(&pool).await,
        };
        let result = result.map_err(PersistenceError::Migration);
        let result = if result.is_ok() {
            let relation = match side {
                DatabaseSide::Write => "erpx_cqrs_write.command_receipts",
                DatabaseSide::Read => "erpx_cqrs_read.projection_generations",
            };
            verify_holding_database_contract(&pool, database_key, relation).await
        } else {
            result
        };
        pool.close().await;
        result
    }
}

/// Check the physical database and the holding-side security contract after a
/// route or migration has selected a database. A successful connection alone
/// is not evidence that the server-side template reached the registry-owned
/// database or that every holding relation is protected by forced RLS.
async fn verify_holding_database_contract(
    pool: &sqlx::PgPool,
    database_key: &str,
    expected_relation: &str,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT current_database() AS database_name,
                to_regclass($1)::text AS relation_name,
                NOT EXISTS (
                    SELECT 1
                    FROM pg_class AS c
                    JOIN pg_namespace AS n ON n.oid = c.relnamespace
                    WHERE n.nspname = 'erpx_holding'
                      AND c.relkind IN ('r', 'p')
                      AND (NOT c.relrowsecurity OR NOT c.relforcerowsecurity)
                ) AS forced_rls_ready",
    )
    .bind(expected_relation)
    .fetch_one(pool)
    .await?;
    let database_name: String = row.try_get("database_name")?;
    let relation_name: Option<String> = row.try_get("relation_name")?;
    let forced_rls_ready: bool = row.try_get("forced_rls_ready")?;
    if database_name != database_key {
        return Err(PersistenceError::InvalidData(
            "holding database identity did not match its route".into(),
        ));
    }
    if relation_name.as_deref() != Some(expected_relation) {
        return Err(PersistenceError::InvalidData(
            "holding database schema relation is missing".into(),
        ));
    }
    if !forced_rls_ready {
        return Err(PersistenceError::InvalidData(
            "holding database relations are not protected by forced RLS".into(),
        ));
    }
    Ok(())
}

impl HoldingProvisioner {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    /// Register the holding-bound owner bootstrap intent before physical
    /// resources are created. Replays are accepted only when every immutable
    /// reference matches the original request.
    pub async fn register_owner_seed(
        &self,
        holding_id: &str,
        principal_ref: &str,
        approver_ref: &str,
    ) -> Result<HoldingOwnerSeed, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;
        validate_principal_ref(principal_ref, "owner_principal_ref")?;
        validate_principal_ref(approver_ref, "owner_approver_ref")?;
        if principal_ref == approver_ref {
            return Err(PersistenceError::InvalidData(
                "owner principal and approver must differ".into(),
            ));
        }
        let mut transaction = self.control.pools().write.begin().await?;
        let registry = sqlx::query(
            "SELECT data_profile_key
             FROM erpx_control.holding_registry
             WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(holding_id.to_string())
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("holding is not registered".into()))?;
        let data_profile_key: String = registry.try_get("data_profile_key")?;
        let role_assignment_id =
            holding_owner_assignment_id(&holding_id.to_string(), principal_ref);
        let row = sqlx::query(
            "INSERT INTO erpx_control.holding_owner_seeds
                (holding_id, data_profile_key, principal_ref, approver_ref,
                 role_assignment_id, state)
             VALUES ($1, $2, $3, $4, $5, 'pending')
             ON CONFLICT (holding_id) DO UPDATE
             SET updated_at = now()
             WHERE erpx_control.holding_owner_seeds.data_profile_key = EXCLUDED.data_profile_key
               AND erpx_control.holding_owner_seeds.principal_ref = EXCLUDED.principal_ref
               AND erpx_control.holding_owner_seeds.approver_ref = EXCLUDED.approver_ref
             RETURNING holding_id, data_profile_key, principal_ref, approver_ref,
                       role_assignment_id, state, recovery_issued_at",
        )
        .bind(holding_id)
        .bind(&data_profile_key)
        .bind(principal_ref)
        .bind(approver_ref)
        .bind(role_assignment_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "holding owner seed already exists with different immutable references".into(),
            )
        })?;
        transaction.commit().await?;
        holding_owner_seed_from_row(&row)
    }

    pub async fn get_owner_seed(
        &self,
        holding_id: &str,
    ) -> Result<Option<HoldingOwnerSeed>, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let holding_id = Uuid::parse_str(holding_id)
            .map_err(|_| PersistenceError::InvalidData("holding_id must be a UUID".into()))?;
        let row = sqlx::query(
            "SELECT holding_id, data_profile_key, principal_ref, approver_ref,
                    role_assignment_id, state, recovery_issued_at
             FROM erpx_control.holding_owner_seeds
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        row.as_ref().map(holding_owner_seed_from_row).transpose()
    }

    /// Read only the durable control-plane observations needed for readiness.
    /// Secrets, database keys, storage namespaces, backup references, and
    /// error text are intentionally not part of the returned model.
    pub async fn observe_holding_readiness_metadata(
        &self,
        holding_id: &str,
        now: DateTime<Utc>,
        policy: ReadinessObservationPolicy,
    ) -> Result<HoldingReadinessMetadata, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let registry = sqlx::query(
            "SELECT state, registry_version, write_database_key,
                    read_database_key, storage_namespace
             FROM erpx_control.holding_registry
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        let Some(registry) = registry else {
            return Ok(HoldingReadinessMetadata {
                holding_id: holding_id.into(),
                registry_state: None,
                registry_version: None,
                write_database: DatabaseReadinessObservation::not_probed(),
                read_database: DatabaseReadinessObservation::not_probed(),
                storage: ReadinessComponentObservation::not_probed(),
                projection_freshness: empty_projection_observation(),
                backup_pitr: empty_backup_observation(),
            });
        };

        let registry_state: String = registry.try_get("state")?;
        let registry_version = positive_observation_u64(
            registry.try_get("registry_version")?,
            "holding registry version",
        )?;
        let expected_write_database: String = registry.try_get("write_database_key")?;
        let expected_read_database: String = registry.try_get("read_database_key")?;
        let expected_storage_namespace: String = registry.try_get("storage_namespace")?;

        let health_rows = sqlx::query(
            "SELECT side, database_key, schema_version, reachable, checked_at
             FROM erpx_control.holding_database_health
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_all(&self.control.pools().write)
        .await?;
        let mut write_database = DatabaseReadinessObservation::not_probed();
        let mut read_database = DatabaseReadinessObservation::not_probed();
        for row in health_rows {
            let side: String = row.try_get("side")?;
            let database_key: String = row.try_get("database_key")?;
            let schema_version: String = row.try_get("schema_version")?;
            let reachable: bool = row.try_get("reachable")?;
            let checked_at: DateTime<Utc> = row.try_get("checked_at")?;
            let component =
                classify_persisted_observation(reachable, checked_at, now, policy.database_max_age);
            let expected = match side.as_str() {
                "write" => Some(&expected_write_database),
                "read" => Some(&expected_read_database),
                _ => None,
            };
            let observation = if expected != Some(&database_key) {
                DatabaseReadinessObservation::unavailable(
                    "database_registry_mismatch",
                    Some(schema_version),
                    Some(checked_at),
                )
            } else {
                database_observation_from_component(component, schema_version, checked_at)
            };
            match side.as_str() {
                "write" => write_database = observation,
                "read" => read_database = observation,
                _ => {
                    write_database = DatabaseReadinessObservation::unavailable(
                        "database_health_side_invalid",
                        None,
                        Some(checked_at),
                    );
                    read_database = DatabaseReadinessObservation::unavailable(
                        "database_health_side_invalid",
                        None,
                        Some(checked_at),
                    );
                }
            }
        }

        let storage = sqlx::query(
            "SELECT storage_namespace, reachable, checked_at
             FROM erpx_control.holding_storage_health
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?
        .map(|row| {
            let namespace: Result<String, sqlx::Error> = row.try_get("storage_namespace");
            let reachable: Result<bool, sqlx::Error> = row.try_get("reachable");
            let checked_at: Result<DateTime<Utc>, sqlx::Error> = row.try_get("checked_at");
            match (namespace, reachable, checked_at) {
                (Ok(namespace), Ok(reachable), Ok(checked_at))
                    if namespace == expected_storage_namespace =>
                {
                    classify_persisted_observation(
                        reachable,
                        checked_at,
                        now,
                        policy.storage_max_age,
                    )
                }
                (Ok(_), _, Ok(checked_at)) => ReadinessComponentObservation::unavailable(
                    "storage_registry_mismatch",
                    Some(checked_at),
                ),
                (_, _, Ok(checked_at)) => ReadinessComponentObservation::unavailable(
                    "storage_health_invalid",
                    Some(checked_at),
                ),
                _ => ReadinessComponentObservation::unavailable("storage_health_invalid", None),
            }
        })
        .unwrap_or_else(ReadinessComponentObservation::not_probed);

        let projection_freshness = sqlx::query(
            "SELECT current_read_database_key, state, source_version,
                    last_error, activated_at, updated_at
             FROM erpx_control.holding_projection_switches
             WHERE holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?
        .map(|row| {
            let current_key: Result<String, sqlx::Error> = row.try_get("current_read_database_key");
            let state: Result<String, sqlx::Error> = row.try_get("state");
            let source_version: Result<i64, sqlx::Error> = row.try_get("source_version");
            let last_error: Result<Option<String>, sqlx::Error> = row.try_get("last_error");
            let activated_at: Result<Option<DateTime<Utc>>, sqlx::Error> =
                row.try_get("activated_at");
            let updated_at: Result<DateTime<Utc>, sqlx::Error> = row.try_get("updated_at");
            match (
                current_key,
                state,
                source_version,
                last_error,
                activated_at,
                updated_at,
            ) {
                (
                    Ok(current_key),
                    Ok(state),
                    Ok(source_version),
                    Ok(last_error),
                    Ok(activated_at),
                    Ok(updated_at),
                ) => projection_observation(
                    &expected_read_database,
                    current_key,
                    state,
                    source_version,
                    last_error.is_some(),
                    activated_at,
                    updated_at,
                    now,
                    policy.projection_max_age,
                ),
                _ => ProjectionFreshnessObservation {
                    state: ReadinessObservationState::Unavailable,
                    source_version: None,
                    updated_at: None,
                    activated_at: None,
                    has_error: true,
                    reason_code: Some("projection_metadata_invalid".into()),
                },
            }
        })
        .unwrap_or_else(empty_projection_observation);

        let backup_pitr = sqlx::query(
            "SELECT id, kind, status, started_at, completed_at, verified,
                    error, restore_target_ref
             FROM erpx_control.holding_backup_runs
             WHERE holding_id = $1
             ORDER BY started_at DESC, id DESC
             LIMIT 1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?
        .map(|row| {
            let id: Result<Uuid, sqlx::Error> = row.try_get("id");
            let kind: Result<String, sqlx::Error> = row.try_get("kind");
            let status: Result<String, sqlx::Error> = row.try_get("status");
            let started_at: Result<DateTime<Utc>, sqlx::Error> = row.try_get("started_at");
            let completed_at: Result<Option<DateTime<Utc>>, sqlx::Error> =
                row.try_get("completed_at");
            let verified: Result<bool, sqlx::Error> = row.try_get("verified");
            let error: Result<Option<String>, sqlx::Error> = row.try_get("error");
            let restore_target_ref: Result<Option<String>, sqlx::Error> =
                row.try_get("restore_target_ref");
            match (
                id,
                kind,
                status,
                started_at,
                completed_at,
                verified,
                error,
                restore_target_ref,
            ) {
                (
                    Ok(id),
                    Ok(kind),
                    Ok(status),
                    Ok(started_at),
                    Ok(completed_at),
                    Ok(verified),
                    Ok(error),
                    Ok(restore_target_ref),
                ) => backup_observation(
                    id,
                    kind,
                    status,
                    started_at,
                    completed_at,
                    verified,
                    error.is_some(),
                    restore_target_ref.is_some(),
                    now,
                    policy.backup_max_age,
                ),
                _ => BackupPitrObservation {
                    state: ReadinessObservationState::Unavailable,
                    run_id: None,
                    kind: None,
                    status: None,
                    started_at: None,
                    completed_at: None,
                    verified: false,
                    has_error: true,
                    restore_target_present: false,
                    reason_code: Some("backup_metadata_invalid".into()),
                },
            }
        })
        .unwrap_or_else(empty_backup_observation);

        Ok(HoldingReadinessMetadata {
            holding_id: holding_id.into(),
            registry_state: Some(registry_state),
            registry_version: Some(registry_version),
            write_database,
            read_database,
            storage,
            projection_freshness,
            backup_pitr,
        })
    }

    pub async fn begin(
        &self,
        registration: HoldingStoreRegistration,
    ) -> Result<(), PersistenceError> {
        if registration.state != "provisioning" {
            return Err(PersistenceError::InvalidData(
                "new holding provisioning must start in provisioning state".into(),
            ));
        }
        SqlxHoldingStoreRegistry::new(self.control.clone())
            .register(registration.clone())
            .await?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.holding_provisioning
                (holding_id, state, write_database_key, read_database_key, storage_namespace, version)
             VALUES ($1, $2, $3, $4, $5, 1)
             ON CONFLICT (holding_id) DO NOTHING",
        )
        .bind(&registration.holding_id)
        .bind(ProvisioningState::Provisioning.as_str())
        .bind(&registration.write_database_key)
        .bind(&registration.read_database_key)
        .bind(&registration.storage_namespace)
        .execute(&self.control.pools().write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "holding provisioning already exists".into(),
            ));
        }
        Ok(())
    }

    pub async fn schedule_backup_job(
        &self,
        request: HoldingBackupJobRequest,
    ) -> Result<HoldingBackupJobReceipt, PersistenceError> {
        validate_backup_job_request(&request)?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(&request.holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if registration.data_profile_key != request.data_profile_key
            || !matches!(registration.state.as_str(), "active" | "recovery")
        {
            return Err(PersistenceError::InvalidData(
                "holding backup scope or state is invalid".into(),
            ));
        }
        let request_hash = backup_job_request_hash(&request);
        let mut transaction = self.control.pools().write.begin().await?;
        // Re-check and lock the registry row in the same transaction as the
        // job insert. The earlier read is useful for a fast rejection, but it
        // cannot by itself prevent a concurrent suspend/holding reassignment
        // from admitting a job after the holding left a backup-capable state.
        let registry_row = sqlx::query(
            "SELECT data_profile_key, state
             FROM erpx_control.holding_registry
             WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(&request.holding_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(registry_row) = registry_row else {
            return Err(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ));
        };
        let registered_holding: String = registry_row.try_get("data_profile_key")?;
        let registered_state: String = registry_row.try_get("state")?;
        if registered_holding != request.data_profile_key
            || !matches!(registered_state.as_str(), "active" | "recovery")
        {
            return Err(PersistenceError::InvalidData(
                "holding backup scope or state is invalid".into(),
            ));
        }
        if let Some(row) = sqlx::query(
            "SELECT id, request_hash, kind, state, available_at, max_attempts
             FROM erpx_control.holding_backup_jobs
             WHERE holding_id = $1 AND idempotency_key = $2
             FOR UPDATE",
        )
        .bind(&request.holding_id)
        .bind(&request.idempotency_key)
        .fetch_optional(&mut *transaction)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                let _ = transaction.rollback().await;
                return Err(PersistenceError::InvalidData(
                    "holding backup idempotency conflict".into(),
                ));
            }
            let receipt = HoldingBackupJobReceipt {
                id: row.try_get("id")?,
                holding_id: request.holding_id,
                kind: row.try_get("kind")?,
                state: row.try_get("state")?,
                available_at: row.try_get("available_at")?,
                max_attempts: u16::try_from(row.try_get::<i32, _>("max_attempts")?).map_err(
                    |_| PersistenceError::InvalidData("backup attempts are invalid".into()),
                )?,
                duplicate: true,
            };
            transaction.commit().await?;
            return Ok(receipt);
        }
        let id = Uuid::new_v4();
        let pitr_recovery_point = pitr_recovery_point_json(&request.pitr_recovery_point)?;
        sqlx::query(
            "INSERT INTO erpx_control.holding_backup_jobs
                (id, holding_id, data_profile_key, kind, backup_ref, restore_target_ref,
                 candidate_read_database_key, source_version, source_projection,
                 pitr_recovery_point, state, available_at, max_attempts, request_hash, idempotency_key)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', $11, $12, $13, $14)",
        )
        .bind(id)
        .bind(&request.holding_id)
        .bind(&request.data_profile_key)
        .bind(&request.kind)
        .bind(&request.backup_ref)
        .bind(&request.restore_target_ref)
        .bind(&request.candidate_read_database_key)
        .bind(
            request
                .source_version
                .map(|value| {
                    i64::try_from(value).map_err(|_| {
                        PersistenceError::InvalidData("backup source version is too large".into())
                    })
                })
                .transpose()?,
        )
        .bind(&request.source_projection)
        .bind(&pitr_recovery_point)
        .bind(request.available_at)
        .bind(i32::from(request.max_attempts))
        .bind(&request_hash)
        .bind(&request.idempotency_key)
        .execute(&mut *transaction)
        .await?;
        append_holding_backup_job_event(
            &mut transaction,
            id,
            &request.holding_id,
            &request.data_profile_key,
            &request.kind,
            "pending",
            0,
            request.max_attempts,
            false,
            false,
            request.available_at,
            HOLDING_BACKUP_JOB_SCHEDULED_EVENT,
            Some(&format!(
                "holding-backup-job-schedule-{}",
                request.idempotency_key
            )),
            Utc::now(),
            None,
        )
        .await?;
        transaction.commit().await?;
        Ok(HoldingBackupJobReceipt {
            id,
            holding_id: request.holding_id,
            kind: request.kind,
            state: "pending".into(),
            available_at: request.available_at,
            max_attempts: request.max_attempts,
            duplicate: false,
        })
    }

    pub async fn list_backup_jobs(
        &self,
        holding_id: &str,
        page_size: u32,
    ) -> Result<Vec<HoldingBackupJobSummary>, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        if page_size == 0 || page_size > MAX_HOLDING_BACKUP_JOB_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup job page size is outside its bound".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT id, holding_id, kind, state, available_at, attempts, max_attempts,
                    verified, error_present, created_at, updated_at
             FROM erpx_control.holding_backup_jobs
             WHERE holding_id = $1
             ORDER BY created_at DESC, id DESC
             LIMIT $2",
        )
        .bind(holding_id)
        .bind(i64::from(page_size))
        .fetch_all(&self.control.pools().read)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(HoldingBackupJobSummary {
                    id: row.try_get("id")?,
                    holding_id: row.try_get("holding_id")?,
                    kind: row.try_get("kind")?,
                    state: row.try_get("state")?,
                    available_at: row.try_get("available_at")?,
                    attempts: u16::try_from(row.try_get::<i32, _>("attempts")?).map_err(|_| {
                        PersistenceError::InvalidData("backup attempts are invalid".into())
                    })?,
                    max_attempts: u16::try_from(row.try_get::<i32, _>("max_attempts")?).map_err(
                        |_| PersistenceError::InvalidData("backup max attempts are invalid".into()),
                    )?,
                    verified: row.try_get("verified")?,
                    last_error_present: row.try_get("error_present")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    /// Re-check the holding/holding binding at the trusted worker boundary.
    /// Claim SQL applies the same predicate, but execution must not rely on a
    /// caller-provided holding reference if the registry changes between the
    /// claim and the external backup operation.
    pub async fn validate_active_backup_job_binding(
        &self,
        holding_id: &str,
        data_profile_key: &str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_ref(data_profile_key, "data_profile_key")?;
        let bound: Option<bool> = sqlx::query_scalar(
            "SELECT TRUE
             FROM erpx_control.holding_registry
             WHERE holding_id = $1 AND data_profile_key = $2 AND state = 'active'",
        )
        .bind(holding_id)
        .bind(data_profile_key)
        .fetch_optional(&self.control.pools().write)
        .await?;
        if bound != Some(true) {
            return Err(PersistenceError::InvalidData(
                "backup job holding and holding binding is no longer active".into(),
            ));
        }
        Ok(())
    }

    /// Refresh the secret-free control read projection from the authoritative
    /// control write table. This is worker-owned projection work rather than
    /// a second command write; a replacement read database can rebuild it by
    /// replaying the same bounded source rows.
    pub async fn project_backup_jobs_to_read(
        &self,
        page_size: u32,
    ) -> Result<u32, PersistenceError> {
        if page_size == 0 || page_size > MAX_HOLDING_BACKUP_JOB_PROJECTION_BATCH_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup projection page size is outside its bound".into(),
            ));
        }
        let mut after = None;
        let mut projected = 0_u32;
        loop {
            let (count, next) = self
                .project_backup_jobs_to_read_page(page_size, after)
                .await?;
            projected = projected.saturating_add(count);
            if count < page_size {
                return Ok(projected);
            }
            let Some(next) = next else {
                return Ok(projected);
            };
            after = Some(next);
        }
    }

    async fn project_backup_jobs_to_read_page(
        &self,
        page_size: u32,
        after: Option<(DateTime<Utc>, Uuid)>,
    ) -> Result<(u32, Option<(DateTime<Utc>, Uuid)>), PersistenceError> {
        let rows = if let Some((updated_at, id)) = after {
            sqlx::query(
                "SELECT id, holding_id, kind, state, available_at, attempts, max_attempts,
                        verified, (last_error IS NOT NULL) AS error_present,
                        created_at, updated_at
                 FROM erpx_control.holding_backup_jobs
                 WHERE (updated_at, id) > ($1, $2)
                 ORDER BY updated_at, id
                 LIMIT $3",
            )
            .bind(updated_at)
            .bind(id)
            .bind(i64::from(page_size))
            .fetch_all(&self.control.pools().write)
            .await?
        } else {
            sqlx::query(
                "SELECT id, holding_id, kind, state, available_at, attempts, max_attempts,
                        verified, (last_error IS NOT NULL) AS error_present,
                        created_at, updated_at
                 FROM erpx_control.holding_backup_jobs
                 ORDER BY updated_at, id
                 LIMIT $1",
            )
            .bind(i64::from(page_size))
            .fetch_all(&self.control.pools().write)
            .await?
        };
        let mut transaction = self.control.pools().read.begin().await?;
        for row in &rows {
            sqlx::query(
                "INSERT INTO erpx_control.holding_backup_jobs
                    (id, holding_id, kind, state, available_at, attempts, max_attempts,
                     verified, error_present, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT (id) DO UPDATE SET
                    holding_id = EXCLUDED.holding_id, kind = EXCLUDED.kind,
                    state = EXCLUDED.state, available_at = EXCLUDED.available_at,
                    attempts = EXCLUDED.attempts, max_attempts = EXCLUDED.max_attempts,
                    verified = EXCLUDED.verified, error_present = EXCLUDED.error_present,
                    created_at = EXCLUDED.created_at, updated_at = EXCLUDED.updated_at",
            )
            .bind(row.try_get::<Uuid, _>("id")?)
            .bind(row.try_get::<String, _>("holding_id")?)
            .bind(row.try_get::<String, _>("kind")?)
            .bind(row.try_get::<String, _>("state")?)
            .bind(row.try_get::<DateTime<Utc>, _>("available_at")?)
            .bind(row.try_get::<i32, _>("attempts")?)
            .bind(row.try_get::<i32, _>("max_attempts")?)
            .bind(row.try_get::<bool, _>("verified")?)
            .bind(row.try_get::<bool, _>("error_present")?)
            .bind(row.try_get::<DateTime<Utc>, _>("created_at")?)
            .bind(row.try_get::<DateTime<Utc>, _>("updated_at")?)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        let next = rows
            .last()
            .map(|row| -> Result<(DateTime<Utc>, Uuid), PersistenceError> {
                Ok((row.try_get("updated_at")?, row.try_get("id")?))
            })
            .transpose()?;
        Ok((u32::try_from(rows.len()).unwrap_or(u32::MAX), next))
    }

    /// Build a replacement target from the current, server-side holding
    /// registry. The registry keys are never exposed through transport; this
    /// helper is for the trusted worker boundary only.
    pub async fn replacement_read_database_for_holding(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
    ) -> Result<HoldingReadDatabaseReplacement, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        PhysicalHoldingDatabases::new(
            &registration.holding_id,
            registration.write_database_key,
            registration.read_database_key,
        )?
        .replacement_read_database(candidate_read_database_key)
    }

    /// Claim due backup work from the control write database. The claim and
    /// attempt increment are one transaction, and expired leases are
    /// reclaimable by another worker. Only bounded opaque references leave
    /// the persistence boundary; no credentials or database URLs are
    /// returned.
    pub async fn claim_backup_jobs(
        &self,
        worker_id: Uuid,
        _now: DateTime<Utc>,
        lease: Duration,
        page_size: u32,
    ) -> Result<Vec<HoldingBackupJobClaim>, PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "backup worker identity is required".into(),
            ));
        }
        if !(MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS..=MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(PersistenceError::InvalidData(
                "backup worker lease is outside its bound".into(),
            ));
        }
        if page_size == 0 || page_size > MAX_HOLDING_BACKUP_JOB_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "backup job page size is outside its bound".into(),
            ));
        }

        let mut transaction = self.control.pools().write.begin().await?;
        // Close any started run whose owning claim lease expired before the
        // next worker can reclaim the job. The job/attempt/run link created in
        // the claim transaction makes this cleanup deterministic and leaves
        // no orphaned started history after a worker crash.
        sqlx::query(
            "UPDATE erpx_control.holding_backup_runs r
             SET status = 'failed', completed_at = clock_timestamp(),
                 verified = FALSE, error = 'backup_lease_expired'
             WHERE r.status = 'started'
               AND EXISTS (
                   SELECT 1
                   FROM erpx_control.holding_backup_job_runs l
                   JOIN erpx_control.holding_backup_jobs j ON j.id = l.job_id
                   WHERE l.run_id = r.id
                     AND j.state = 'claimed'
                     AND j.lease_until IS NOT NULL
                     AND j.lease_until <= clock_timestamp()
               )",
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.holding_backup_jobs
             SET state = 'dead_letter', worker_id = NULL, claim_id = NULL,
                 lease_until = NULL, verified = FALSE,
                 last_error = 'backup_lease_exhausted', updated_at = clock_timestamp()
             WHERE state = 'claimed' AND lease_until IS NOT NULL
               AND lease_until <= clock_timestamp() AND attempts >= max_attempts",
        )
        .execute(&mut *transaction)
        .await?;
        let rows = sqlx::query(
            "SELECT j.id, j.holding_id, j.data_profile_key, j.kind, j.backup_ref,
                    j.restore_target_ref, j.candidate_read_database_key,
                    j.source_version, j.source_projection, j.pitr_recovery_point,
                    j.available_at, j.attempts, j.max_attempts
             FROM erpx_control.holding_backup_jobs j
             JOIN erpx_control.holding_registry r ON r.holding_id = j.holding_id
             WHERE r.state = 'active'
               AND r.data_profile_key = j.data_profile_key
               AND ((j.state IN ('pending', 'retryable') AND j.available_at <= clock_timestamp())
                    OR (j.state = 'claimed' AND j.lease_until IS NOT NULL
                        AND j.lease_until <= clock_timestamp()))
               AND j.attempts < j.max_attempts
             ORDER BY j.available_at, j.id
             LIMIT $1
             FOR UPDATE OF j, r SKIP LOCKED",
        )
        .bind(i64::from(page_size))
        .fetch_all(&mut *transaction)
        .await?;

        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row.try_get("id")?;
            let holding_id: String = row.try_get("holding_id")?;
            let data_profile_key: String = row.try_get("data_profile_key")?;
            let kind: String = row.try_get("kind")?;
            let backup_ref: String = row.try_get("backup_ref")?;
            let restore_target_ref: Option<String> = row.try_get("restore_target_ref")?;
            let candidate_read_database_key: Option<String> =
                row.try_get("candidate_read_database_key")?;
            let source_version = row
                .try_get::<Option<i64>, _>("source_version")?
                .map(|value| {
                    u64::try_from(value).map_err(|_| {
                        PersistenceError::InvalidData("backup source version is invalid".into())
                    })
                })
                .transpose()?;
            let source_projection: String = row.try_get("source_projection")?;
            let pitr_recovery_point: Option<PitrRecoveryPoint> = row
                .try_get::<Option<Value>, _>("pitr_recovery_point")?
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| {
                    PersistenceError::InvalidData("PITR recovery point is invalid".into())
                })?;
            let available_at: DateTime<Utc> = row.try_get("available_at")?;
            if !matches!(
                source_projection.as_str(),
                "operations" | "finance_banking" | "finance_subledger" | "finance_close"
            ) {
                return Err(PersistenceError::InvalidData(
                    "backup source projection is invalid".into(),
                ));
            }
            let attempts: u16 = u16::try_from(row.try_get::<i32, _>("attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?;
            let max_attempts: u16 = u16::try_from(row.try_get::<i32, _>("max_attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?;
            let next_attempt = attempts.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("backup attempts exceeded their bound".into())
            })?;
            let claim_id = Uuid::new_v4();
            let affected = sqlx::query(
                "UPDATE erpx_control.holding_backup_jobs
                 SET state = 'claimed', attempts = $3, worker_id = $4,
                     claim_id = $5,
                     lease_until = clock_timestamp() + ($6::double precision * interval '1 second'),
                     updated_at = clock_timestamp()
                 WHERE id = $1
                   AND state IN ('pending', 'retryable', 'claimed')
                   AND (state <> 'claimed' OR lease_until <= clock_timestamp())
                   AND attempts = $2
                 RETURNING attempts",
            )
            .bind(id)
            .bind(i32::from(attempts))
            .bind(i32::from(next_attempt))
            .bind(worker_id)
            .bind(claim_id)
            .bind(lease.num_seconds() as f64)
            .fetch_optional(&mut *transaction)
            .await?;
            if affected.is_none() {
                continue;
            }
            validate_ref(&holding_id, "holding_id")?;
            validate_ref(&data_profile_key, "data_profile_key")?;
            validate_ref(&backup_ref, "backup_ref")?;
            if kind == "restore_test" {
                validate_ref(
                    restore_target_ref.as_deref().ok_or_else(|| {
                        PersistenceError::InvalidData("restore target is missing".into())
                    })?,
                    "restore_target_ref",
                )?;
                validate_database_key(
                    candidate_read_database_key.as_deref().ok_or_else(|| {
                        PersistenceError::InvalidData("restore candidate is missing".into())
                    })?,
                    "candidate_read_database_key",
                )?;
                if source_version == Some(0) || source_version.is_none() {
                    return Err(PersistenceError::InvalidData(
                        "restore source version is invalid".into(),
                    ));
                }
            } else if kind == "pitr" {
                pitr_recovery_point
                    .as_ref()
                    .ok_or(PersistenceError::InvalidData(
                        "PITR recovery point is missing".into(),
                    ))?
                    .validate(Utc::now())
                    .map_err(|_| {
                        PersistenceError::InvalidData("PITR recovery point is invalid".into())
                    })?;
            } else if kind == "full" {
                if pitr_recovery_point.is_some() {
                    return Err(PersistenceError::InvalidData(
                        "full backup contains PITR metadata".into(),
                    ));
                }
            } else {
                return Err(PersistenceError::InvalidData(
                    "backup job kind is invalid".into(),
                ));
            }
            let run_id = Uuid::new_v4();
            let started_at = Utc::now();
            sqlx::query(
                "INSERT INTO erpx_control.holding_backup_runs
                    (id, holding_id, kind, status, backup_ref, restore_target_ref,
                     pitr_recovery_point, started_at, completed_at, verified, error)
                 VALUES ($1, $2, $3, 'started', $4, $5, $6, $7, NULL, FALSE, NULL)",
            )
            .bind(run_id)
            .bind(&holding_id)
            .bind(&kind)
            .bind(&backup_ref)
            .bind(&restore_target_ref)
            .bind(&pitr_recovery_point_json(&pitr_recovery_point)?)
            .bind(started_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO erpx_control.holding_backup_job_runs (job_id, attempt, run_id)
                 VALUES ($1, $2, $3)",
            )
            .bind(id)
            .bind(i32::from(next_attempt))
            .bind(run_id)
            .execute(&mut *transaction)
            .await?;
            append_holding_backup_job_event(
                &mut transaction,
                id,
                &holding_id,
                &data_profile_key,
                &kind,
                "claimed",
                next_attempt,
                max_attempts,
                false,
                false,
                available_at,
                HOLDING_BACKUP_JOB_CLAIMED_EVENT,
                None,
                started_at,
                None,
            )
            .await?;
            claims.push(HoldingBackupJobClaim {
                id,
                holding_id: holding_id.clone(),
                data_profile_key,
                kind: kind.clone(),
                run: HoldingBackupRun {
                    id: run_id,
                    holding_id,
                    kind,
                    status: "started".into(),
                    backup_ref,
                    restore_target_ref,
                    pitr_recovery_point,
                    started_at,
                    completed_at: None,
                    verified: false,
                    error: None,
                },
                candidate_read_database_key,
                source_version,
                source_projection,
                attempt: next_attempt,
                claim_id,
                worker_id,
            });
        }
        transaction.commit().await?;
        Ok(claims)
    }

    /// Mark a claimed job complete only while its lease is still owned by the
    /// same worker. A successful backup/restore test must have already
    /// persisted a verified `holding_backup_runs` row through the execution
    /// seam before this transition is accepted.
    pub async fn complete_backup_job(
        &self,
        claim: &HoldingBackupJobClaim,
        now: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_backup_job_claim(claim)?;
        let mut transaction = self.control.pools().write.begin().await?;
        let pitr_recovery_point = pitr_recovery_point_json(&claim.run.pitr_recovery_point)?;
        let run_row = sqlx::query(
            "SELECT r.status, r.verified
             FROM erpx_control.holding_backup_job_runs link
             JOIN erpx_control.holding_backup_runs r ON r.id = link.run_id
             WHERE link.job_id = $1 AND link.attempt = $2 AND link.run_id = $3
               AND r.holding_id = $4 AND r.kind = $5 AND r.backup_ref = $6
               AND r.restore_target_ref IS NOT DISTINCT FROM $7
               AND r.pitr_recovery_point IS NOT DISTINCT FROM $8
             FOR UPDATE OF r",
        )
        .bind(claim.id)
        .bind(i32::from(claim.attempt))
        .bind(claim.run.id)
        .bind(&claim.holding_id)
        .bind(&claim.kind)
        .bind(&claim.run.backup_ref)
        .bind(&claim.run.restore_target_ref)
        .bind(&pitr_recovery_point)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(run_row) = run_row else {
            return Err(PersistenceError::InvalidData(
                "backup job completion requires its claimed run evidence".into(),
            ));
        };
        let run_status: String = run_row.try_get("status")?;
        let run_verified: bool = run_row.try_get("verified")?;
        if run_status != "completed" || !run_verified {
            return Err(PersistenceError::InvalidData(
                "backup job completion requires a verified completed run".into(),
            ));
        }
        let row = sqlx::query(
            "UPDATE erpx_control.holding_backup_jobs
             SET state = 'completed', worker_id = NULL, claim_id = NULL,
                 lease_until = NULL, verified = TRUE, last_error = NULL,
                 updated_at = clock_timestamp()
             WHERE id = $1 AND holding_id = $2 AND data_profile_key = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()
             RETURNING available_at, attempts, max_attempts",
        )
        .bind(claim.id)
        .bind(&claim.holding_id)
        .bind(&claim.data_profile_key)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        };
        append_holding_backup_job_event(
            &mut transaction,
            claim.id,
            &claim.holding_id,
            &claim.data_profile_key,
            &claim.kind,
            "completed",
            u16::try_from(row.try_get::<i32, _>("attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?,
            u16::try_from(row.try_get::<i32, _>("max_attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?,
            true,
            false,
            row.try_get("available_at")?,
            HOLDING_BACKUP_JOB_COMPLETED_EVENT,
            None,
            now,
            None,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Release a claimed job with a bounded, code-only error. Retryable work
    /// receives an exponential backoff capped at one hour; non-retryable
    /// work and exhausted attempts become durable dead letters.
    pub async fn fail_backup_job(
        &self,
        claim: &HoldingBackupJobClaim,
        _now: DateTime<Utc>,
        error_code: &str,
        retryable: bool,
    ) -> Result<bool, PersistenceError> {
        validate_backup_job_claim(claim)?;
        validate_backup_error_code(error_code)?;
        let mut transaction = self.control.pools().write.begin().await?;
        let row = sqlx::query(
            "SELECT attempts, max_attempts
             FROM erpx_control.holding_backup_jobs
             WHERE id = $1 AND holding_id = $2 AND data_profile_key = $3
               AND state = 'claimed' AND claim_id = $4 AND worker_id = $5
               AND lease_until > clock_timestamp()
             FOR UPDATE",
        )
        .bind(claim.id)
        .bind(&claim.holding_id)
        .bind(&claim.data_profile_key)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        };
        let attempts: i32 = row.try_get("attempts")?;
        let max_attempts: i32 = row.try_get("max_attempts")?;
        let should_retry = retryable && attempts < max_attempts;
        let state = if should_retry {
            "retryable"
        } else {
            "dead_letter"
        };
        let database_now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *transaction)
            .await?;
        let next_available_at = if should_retry {
            database_now + backup_retry_delay(attempts)
        } else {
            database_now
        };
        let row = sqlx::query(
            "UPDATE erpx_control.holding_backup_jobs
             SET state = $6, available_at = $5, worker_id = NULL,
                 claim_id = NULL, lease_until = NULL, verified = FALSE,
                 last_error = $4, updated_at = clock_timestamp()
             WHERE id = $1 AND holding_id = $2 AND data_profile_key = $3
               AND state = 'claimed' AND claim_id = $7 AND worker_id = $8
               AND lease_until > clock_timestamp()
             RETURNING available_at, attempts, max_attempts, state",
        )
        .bind(claim.id)
        .bind(&claim.holding_id)
        .bind(&claim.data_profile_key)
        .bind(error_code)
        .bind(next_available_at)
        .bind(state)
        .bind(claim.claim_id)
        .bind(claim.worker_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.worker_id,
                event_id: claim.id,
            });
        };
        append_holding_backup_job_event(
            &mut transaction,
            claim.id,
            &claim.holding_id,
            &claim.data_profile_key,
            &claim.kind,
            row.try_get("state")?,
            u16::try_from(row.try_get::<i32, _>("attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?,
            u16::try_from(row.try_get::<i32, _>("max_attempts")?)
                .map_err(|_| PersistenceError::InvalidData("backup attempts are invalid".into()))?,
            false,
            true,
            row.try_get("available_at")?,
            HOLDING_BACKUP_JOB_FAILED_EVENT,
            None,
            Utc::now(),
            Some(error_code),
        )
        .await?;
        transaction.commit().await?;
        Ok(!should_retry)
    }

    pub async fn record_database_health(
        &self,
        health: &HoldingDatabaseHealth,
    ) -> Result<(), PersistenceError> {
        validate_ref(&health.holding_id, "holding_id")?;
        validate_ref(&health.database_key, "database_key")?;
        validate_ref(&health.schema_version, "schema_version")?;
        validate_health_details(&health.details)?;
        if health.checked_at > Utc::now() {
            return Err(PersistenceError::InvalidData(
                "database health observation cannot be from the future".into(),
            ));
        }
        let mut transaction = self.control.pools().write.begin().await?;
        let expected: Option<String> = sqlx::query_scalar(
            "SELECT CASE WHEN $2 = 'write' THEN write_database_key ELSE read_database_key END
             FROM erpx_control.holding_registry WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(&health.holding_id)
        .bind(health.side.as_str())
        .fetch_optional(&mut *transaction)
        .await?;
        if expected.as_deref() != Some(&health.database_key) {
            return Err(PersistenceError::InvalidData(
                "database health key does not match the holding registry".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.holding_database_health
                (holding_id, side, database_key, schema_version, reachable, checked_at, details)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (holding_id, side) DO UPDATE SET
                database_key = EXCLUDED.database_key,
                schema_version = EXCLUDED.schema_version,
                reachable = EXCLUDED.reachable,
                checked_at = EXCLUDED.checked_at,
                details = EXCLUDED.details",
        )
        .bind(&health.holding_id)
        .bind(health.side.as_str())
        .bind(&health.database_key)
        .bind(&health.schema_version)
        .bind(health.reachable)
        .bind(health.checked_at)
        .bind(&health.details)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn record_storage_health(
        &self,
        health: &HoldingStorageHealth,
    ) -> Result<(), PersistenceError> {
        validate_ref(&health.holding_id, "holding_id")?;
        validate_ref(&health.storage_namespace, "storage_namespace")?;
        validate_health_details(&health.details)?;
        if health.checked_at > Utc::now() {
            return Err(PersistenceError::InvalidData(
                "storage health observation cannot be from the future".into(),
            ));
        }
        let mut transaction = self.control.pools().write.begin().await?;
        let expected: Option<String> = sqlx::query_scalar(
            "SELECT storage_namespace FROM erpx_control.holding_registry
             WHERE holding_id = $1
             FOR UPDATE",
        )
        .bind(&health.holding_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if expected.as_deref() != Some(&health.storage_namespace) {
            return Err(PersistenceError::InvalidData(
                "storage health namespace does not match the holding registry".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.holding_storage_health
                (holding_id, storage_namespace, reachable, checked_at, details)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (holding_id) DO UPDATE SET
                storage_namespace = EXCLUDED.storage_namespace,
                reachable = EXCLUDED.reachable,
                checked_at = EXCLUDED.checked_at,
                details = EXCLUDED.details",
        )
        .bind(&health.holding_id)
        .bind(&health.storage_namespace)
        .bind(health.reachable)
        .bind(health.checked_at)
        .bind(&health.details)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn record_backup_run(&self, run: &HoldingBackupRun) -> Result<(), PersistenceError> {
        validate_backup_run_shape(run)?;
        let pitr_recovery_point = pitr_recovery_point_json(&run.pitr_recovery_point)?;
        // PostgreSQL timestamptz has microsecond precision. Normalize before
        // the idempotent comparison so a replay of a Rust timestamp carrying
        // sub-microsecond nanos is not mistaken for a different run.
        let started_at = postgres_timestamp(run.started_at);
        let completed_at = run.completed_at.map(postgres_timestamp);
        let mut transaction = self.control.pools().write.begin().await?;
        sqlx::query(
            "INSERT INTO erpx_control.holding_backup_runs
                (id, holding_id, kind, status, backup_ref, restore_target_ref,
                 pitr_recovery_point, started_at, completed_at, verified, error)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(run.id)
        .bind(&run.holding_id)
        .bind(&run.kind)
        .bind(&run.status)
        .bind(&run.backup_ref)
        .bind(&run.restore_target_ref)
        .bind(&pitr_recovery_point)
        .bind(started_at)
        .bind(completed_at)
        .bind(run.verified)
        .bind(&run.error)
        .execute(&mut *transaction)
        .await?;
        let stored = sqlx::query(
            "SELECT holding_id, kind, status, backup_ref, restore_target_ref,
                    pitr_recovery_point, started_at, completed_at, verified, error
             FROM erpx_control.holding_backup_runs
             WHERE id = $1",
        )
        .bind(run.id)
        .fetch_one(&mut *transaction)
        .await?;
        let matches = stored.try_get::<String, _>("holding_id")? == run.holding_id
            && stored.try_get::<String, _>("kind")? == run.kind
            && stored.try_get::<String, _>("status")? == run.status
            && stored.try_get::<String, _>("backup_ref")? == run.backup_ref
            && stored.try_get::<Option<String>, _>("restore_target_ref")? == run.restore_target_ref
            && stored.try_get::<Option<Value>, _>("pitr_recovery_point")? == pitr_recovery_point
            && stored.try_get::<DateTime<Utc>, _>("started_at")? == started_at
            && stored.try_get::<Option<DateTime<Utc>>, _>("completed_at")? == completed_at
            && stored.try_get::<bool, _>("verified")? == run.verified
            && stored.try_get::<Option<String>, _>("error")? == run.error;
        if !matches {
            return Err(PersistenceError::IdempotencyConflict {
                key: format!("backup-run:{}", run.id),
            });
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Persist a failed attempt that could not reach the typed executor (for
    /// example, candidate database preparation failed). This keeps the run
    /// history complete without exposing arbitrary worker error text.
    pub async fn record_failed_backup_run(
        &self,
        run: HoldingBackupRun,
        error: &'static str,
    ) -> Result<(), PersistenceError> {
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "failed backup history must begin with started status".into(),
            ));
        }
        self.record_backup_run(&run).await?;
        self.fail_backup_run(run.id, error).await
    }

    /// Run one backup/PITR/restore-test operation through an injected
    /// production executor. This method persists `started` before invoking
    /// the callback, then persists either `completed`/`verified` or `failed`.
    /// It deliberately does not invoke `pg_dump`, `pg_restore`, shell commands,
    /// or database deletion itself.
    pub async fn execute_backup<E, Fut>(
        &self,
        run: HoldingBackupRun,
        executor: E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
        Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
    {
        validate_backup_run_shape(&run)?;
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "backup execution must begin with started status".into(),
            ));
        }
        let registration = self.validate_backup_registration(&run).await?;
        self.execute_backup_internal(run, registration, executor, None, None, false)
            .await
            .map(|(completed, _)| completed)
    }

    /// Execute a full or PITR backup through a transport-neutral, bounded
    /// adapter. The adapter receives no credentials, URLs, SQL pools, or
    /// mutable registry object; its request is bound to one validated holding
    /// target. Only a matching, verified result can be recorded as completed.
    pub async fn execute_backup_with_executor<E>(
        &self,
        run: HoldingBackupRun,
        executor: &E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: BackupExecutor + ?Sized,
    {
        validate_backup_run_shape(&run)?;
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "backup execution must begin with started status".into(),
            ));
        }
        let kind = match run.kind.as_str() {
            "full" => BackupRunKind::Full,
            "pitr" => BackupRunKind::Pitr,
            "restore_test" => {
                return Err(PersistenceError::InvalidData(
                    "restore tests require the restore executor seam".into(),
                ));
            }
            _ => unreachable!("backup shape validation checked kind"),
        };
        let registration = self.validate_backup_registration(&run).await?;
        let databases = PhysicalHoldingDatabases::new(
            registration.holding_id.clone(),
            registration.write_database_key,
            registration.read_database_key,
        )?;
        let target = HoldingBackupTarget::for_run_in_namespace(
            databases,
            &registration.storage_namespace,
            &registration.backup_chain_ref,
            &run,
        )?;
        let request = BackupExecutionRequest::new(target, kind);
        self.execute_typed_backup(run, request, executor).await
    }

    /// Execute a restore test through an injected adapter. The replacement
    /// target must have been created from the same holding's physical database
    /// inventory, and a failed or expired result is persisted as failed with
    /// no projection switch attempt.
    pub async fn execute_restore_test_with_executor<E>(
        &self,
        run: HoldingBackupRun,
        replacement: HoldingReadDatabaseReplacement,
        source_version: u64,
        executor: &E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: RestoreExecutor + ?Sized,
    {
        validate_backup_run_shape(&run)?;
        if run.kind != "restore_test" {
            return Err(PersistenceError::InvalidData(
                "restore executor requires a restore test".into(),
            ));
        }
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "backup execution must begin with started status".into(),
            ));
        }
        let restore_target_ref =
            run.restore_target_ref
                .as_deref()
                .ok_or(PersistenceError::InvalidData(
                    "restore tests require a restore target reference".into(),
                ))?;
        let registration = self.validate_backup_registration(&run).await?;
        if replacement.holding_id != registration.holding_id
            || replacement.current_read_database_key != registration.read_database_key
        {
            return Err(PersistenceError::InvalidData(
                "replacement read database is not bound to the holding registry".into(),
            ));
        }
        i64::try_from(source_version).map_err(|_| {
            PersistenceError::InvalidData("projection source version exceeds database range".into())
        })?;
        let databases = PhysicalHoldingDatabases::new(
            registration.holding_id.clone(),
            registration.write_database_key,
            registration.read_database_key,
        )?;
        let backup_target = HoldingBackupTarget::for_run_in_namespace(
            databases,
            &registration.storage_namespace,
            &registration.backup_chain_ref,
            &run,
        )?;
        let target = HoldingRestoreTestTarget::new(
            backup_target,
            restore_target_ref,
            replacement,
            source_version,
        )?;
        let request = RestoreTestExecutionRequest::new(target);
        self.execute_typed_restore(run, request, executor).await
    }

    async fn execute_typed_backup<E>(
        &self,
        run: HoldingBackupRun,
        request: BackupExecutionRequest,
        executor: &E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: BackupExecutor + ?Sized,
    {
        self.record_backup_run(&run).await?;
        let result = match executor.execute(request.clone()).await {
            Ok(result) => result,
            Err(BackupExecutorError::Expired) => {
                self.fail_backup_run(run.id, "backup executor request expired")
                    .await?;
                return Err(PersistenceError::InvalidData(
                    "backup executor request expired".into(),
                ));
            }
            Err(_) => {
                self.fail_backup_run(run.id, "backup executor failed")
                    .await?;
                return Err(PersistenceError::InvalidData(
                    "backup executor failed".into(),
                ));
            }
        };
        let validated = match request.validate_result(&result) {
            Ok(validated) => validated,
            Err(error) => {
                self.fail_backup_run(run.id, "backup executor returned an invalid result")
                    .await?;
                return Err(error);
            }
        };
        if !validated.verified() {
            self.fail_backup_run(run.id, "backup was not verified")
                .await?;
            return Err(PersistenceError::InvalidData(
                "backup was not verified".into(),
            ));
        }
        let completed_at = Utc::now();
        self.complete_backup_run(run.id, completed_at, true).await?;
        Ok(HoldingBackupRun {
            status: "completed".into(),
            completed_at: Some(completed_at),
            verified: true,
            ..run
        })
    }

    async fn execute_typed_restore<E>(
        &self,
        run: HoldingBackupRun,
        request: RestoreTestExecutionRequest,
        executor: &E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: RestoreExecutor + ?Sized,
    {
        self.record_backup_run(&run).await?;
        let result = match executor.execute(request.clone()).await {
            Ok(result) => result,
            Err(BackupExecutorError::Expired) => {
                self.fail_backup_run(run.id, "restore test expired").await?;
                return Err(PersistenceError::InvalidData("restore test expired".into()));
            }
            Err(_) => {
                self.fail_backup_run(run.id, "restore executor failed")
                    .await?;
                return Err(PersistenceError::InvalidData(
                    "restore executor failed".into(),
                ));
            }
        };
        let validated = match request.validate_result(&result) {
            Ok(validated) => validated,
            Err(error) => {
                self.fail_backup_run(run.id, "restore executor returned an invalid result")
                    .await?;
                return Err(error);
            }
        };
        let failure = match validated.outcome() {
            RestoreTestOutcome::Failed => Some("restore test failed"),
            RestoreTestOutcome::Expired => Some("restore test expired"),
            RestoreTestOutcome::Verified if !validated.replacement_read_verified() => {
                Some("replacement read target was not verified")
            }
            RestoreTestOutcome::Verified => None,
        };
        if let Some(error) = failure {
            self.fail_backup_run(run.id, error).await?;
            return Err(PersistenceError::InvalidData(error.into()));
        }
        let completed_at = Utc::now();
        self.complete_backup_run(run.id, completed_at, true).await?;
        let completed = HoldingBackupRun {
            status: "completed".into(),
            completed_at: Some(completed_at),
            verified: true,
            ..run
        };
        let replacement = request.target().replacement();
        if self
            .begin_read_rebuild(
                &completed.holding_id,
                replacement.candidate_read_database_key(),
                request.target().source_version(),
            )
            .await
            .is_err()
        {
            self.fail_backup_run(completed.id, "replacement read rebuild could not start")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read rebuild could not start".into(),
            ));
        }
        if self
            .complete_read_rebuild(
                &completed.holding_id,
                replacement.candidate_read_database_key(),
            )
            .await
            .is_err()
        {
            let _ = self
                .fail_read_rebuild(
                    &completed.holding_id,
                    replacement.candidate_read_database_key(),
                    "replacement read rebuild could not complete",
                )
                .await;
            self.fail_backup_run(completed.id, "replacement read rebuild could not complete")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read rebuild could not complete".into(),
            ));
        }
        if self
            .activate_read_database(&completed.holding_id)
            .await
            .is_err()
        {
            let _ = self
                .fail_read_rebuild(
                    &completed.holding_id,
                    replacement.candidate_read_database_key(),
                    "replacement read activation failed",
                )
                .await;
            self.fail_backup_run(completed.id, "replacement read activation failed")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read activation failed".into(),
            ));
        }
        Ok(completed)
    }

    /// Execute a verified restore test and activate a replacement read
    /// database only after the injected executor verifies both the restore
    /// target and the replacement read target. Any switch failure is recorded
    /// as failed and leaves the previous read database selected.
    pub async fn execute_restore_test_with_read_replacement<E, Fut>(
        &self,
        run: HoldingBackupRun,
        candidate_read_database_key: &str,
        source_version: u64,
        executor: E,
    ) -> Result<HoldingBackupRun, PersistenceError>
    where
        E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
        Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
    {
        validate_backup_run_shape(&run)?;
        if run.kind != "restore_test" {
            return Err(PersistenceError::InvalidData(
                "replacement read activation requires a restore test".into(),
            ));
        }
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "backup execution must begin with started status".into(),
            ));
        }
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        if source_version == 0 {
            return Err(PersistenceError::InvalidData(
                "projection source version is required".into(),
            ));
        }
        i64::try_from(source_version).map_err(|_| {
            PersistenceError::InvalidData("projection source version exceeds database range".into())
        })?;
        let registration = self.validate_backup_registration(&run).await?;
        if registration.write_database_key == candidate_read_database_key
            || registration.read_database_key == candidate_read_database_key
        {
            return Err(PersistenceError::Configuration(
                "replacement read database must be distinct from provisioned databases",
            ));
        }
        let (completed, _) = self
            .execute_backup_internal(
                run,
                registration,
                executor,
                Some(candidate_read_database_key.into()),
                Some(source_version),
                true,
            )
            .await?;

        if self
            .begin_read_rebuild(
                &completed.holding_id,
                candidate_read_database_key,
                source_version,
            )
            .await
            .is_err()
        {
            self.fail_backup_run(completed.id, "replacement read rebuild could not start")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read rebuild could not start".into(),
            ));
        }
        if self
            .complete_read_rebuild(&completed.holding_id, candidate_read_database_key)
            .await
            .is_err()
        {
            self.fail_read_rebuild(
                &completed.holding_id,
                candidate_read_database_key,
                "replacement read rebuild could not complete",
            )
            .await?;
            self.fail_backup_run(completed.id, "replacement read rebuild could not complete")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read rebuild could not complete".into(),
            ));
        }
        if self
            .activate_read_database(&completed.holding_id)
            .await
            .is_err()
        {
            self.fail_read_rebuild(
                &completed.holding_id,
                candidate_read_database_key,
                "replacement read activation failed",
            )
            .await?;
            self.fail_backup_run(completed.id, "replacement read activation failed")
                .await?;
            return Err(PersistenceError::InvalidData(
                "replacement read activation failed".into(),
            ));
        }
        Ok(completed)
    }

    /// Execute a verified restore test and promote a replacement read model
    /// through a server-side routed boundary. The source lock is held for the
    /// final watermark check, generation completion, and exact activation;
    /// no metadata-only activation path is reachable from this API.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_restore_test_with_routed_read_replacement<E, Fut>(
        &self,
        router: &HoldingDatabaseRouter,
        run: HoldingBackupRun,
        data_profile_key: &str,
        candidate_read_database_key: &str,
        source_projection: HoldingSourceProjection,
        source_version: u64,
        executor: E,
    ) -> Result<HoldingRestoreTestActivation, PersistenceError>
    where
        E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
        Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
    {
        self.execute_restore_test_with_routed_read_replacement_for_projection(
            router,
            run,
            data_profile_key,
            candidate_read_database_key,
            source_projection,
            source_version,
            executor,
        )
        .await
    }

    /// Fenced restore activation for a selected source projection.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_restore_test_with_routed_read_replacement_for_projection<E, Fut>(
        &self,
        router: &HoldingDatabaseRouter,
        run: HoldingBackupRun,
        data_profile_key: &str,
        candidate_read_database_key: &str,
        source_projection: HoldingSourceProjection,
        source_version: u64,
        executor: E,
    ) -> Result<HoldingRestoreTestActivation, PersistenceError>
    where
        E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
        Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
    {
        validate_backup_run_shape(&run)?;
        if run.kind != "restore_test" {
            return Err(PersistenceError::InvalidData(
                "route-aware activation requires a restore test".into(),
            ));
        }
        if run.status != "started" {
            return Err(PersistenceError::InvalidData(
                "backup execution must begin with started status".into(),
            ));
        }
        validate_ref(data_profile_key, "data_profile_key")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        if source_version == 0 {
            return Err(PersistenceError::InvalidData(
                "projection source version is required".into(),
            ));
        }
        i64::try_from(source_version).map_err(|_| {
            PersistenceError::InvalidData("projection source version exceeds database range".into())
        })?;
        if !Arc::ptr_eq(&self.control, &router.control) {
            return Err(PersistenceError::InvalidData(
                "restore route and provisioner must share a control plane".into(),
            ));
        }

        let registration = self.validate_backup_registration(&run).await?;
        if registration.data_profile_key != data_profile_key {
            return Err(PersistenceError::InvalidData(
                "restore route holding scope does not match the holding registry".into(),
            ));
        }
        if registration.write_database_key == candidate_read_database_key
            || registration.read_database_key == candidate_read_database_key
        {
            return Err(PersistenceError::Configuration(
                "replacement read database must be distinct from provisioned databases",
            ));
        }

        let (completed, _) = self
            .execute_backup_internal(
                run,
                registration,
                executor,
                Some(candidate_read_database_key.into()),
                Some(source_version),
                true,
            )
            .await?;

        let write_route = match router
            .route(
                &completed.holding_id,
                data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
        {
            Ok(route) => route,
            Err(error) => {
                self.fail_backup_run(completed.id, "restore source route failed")
                    .await?;
                return Err(error);
            }
        };
        let mut source_lock = match write_route.acquire_source_rebuild_lock().await {
            Ok(lock) => lock,
            Err(error) => {
                self.fail_backup_run(completed.id, "restore source lock failed")
                    .await?;
                return Err(error);
            }
        };
        let observed_source_version = match source_lock
            .source_event_high_watermark(source_projection, data_profile_key)
            .await
        {
            Ok(version) => version,
            Err(error) => {
                if let Err(rollback_error) = source_lock.rollback().await {
                    tracing::error!(error = %rollback_error, "restore source rollback failed");
                }
                self.fail_backup_run(completed.id, "restore source watermark failed")
                    .await?;
                return Err(error);
            }
        };
        if observed_source_version != source_version {
            if let Err(rollback_error) = source_lock.rollback().await {
                tracing::error!(error = %rollback_error, "restore source rollback failed");
            }
            self.fail_backup_run(completed.id, "restore source watermark is stale")
                .await?;
            return Err(PersistenceError::InvalidData(
                "restore source watermark is stale".into(),
            ));
        }

        let generation = match self
            .begin_read_rebuild_with_generation(
                &completed.holding_id,
                candidate_read_database_key,
                source_version,
            )
            .await
        {
            Ok(generation) => generation,
            Err(error) => {
                if let Err(rollback_error) = source_lock.rollback().await {
                    tracing::error!(error = %rollback_error, "restore source rollback failed");
                }
                self.fail_backup_run(completed.id, "replacement read rebuild could not start")
                    .await?;
                return Err(error);
            }
        };

        let candidate_route = match router
            .route_replacement_read_for_generation(
                &completed.holding_id,
                data_profile_key,
                candidate_read_database_key,
                generation,
                source_version,
            )
            .await
        {
            Ok(route) => route,
            Err(error) => {
                if let Err(failure_error) = self
                    .fail_read_rebuild_if_generation(
                        &completed.holding_id,
                        candidate_read_database_key,
                        generation,
                        "replacement read route verification failed",
                    )
                    .await
                {
                    tracing::error!(
                        error = %failure_error,
                        "restore replacement read failure transition failed"
                    );
                }
                if let Err(rollback_error) = source_lock.rollback().await {
                    tracing::error!(error = %rollback_error, "restore source rollback failed");
                }
                self.fail_backup_run(completed.id, "replacement read route verification failed")
                    .await?;
                return Err(error);
            }
        };
        candidate_route.close().await;

        let final_source_version = match source_lock
            .source_event_high_watermark(source_projection, data_profile_key)
            .await
        {
            Ok(version) => version,
            Err(error) => {
                if let Err(failure_error) = self
                    .fail_read_rebuild_if_generation(
                        &completed.holding_id,
                        candidate_read_database_key,
                        generation,
                        "restore source watermark failed",
                    )
                    .await
                {
                    tracing::error!(
                        error = %failure_error,
                        "restore replacement read failure transition failed"
                    );
                }
                if let Err(rollback_error) = source_lock.rollback().await {
                    tracing::error!(error = %rollback_error, "restore source rollback failed");
                }
                self.fail_backup_run(completed.id, "restore source watermark failed")
                    .await?;
                return Err(error);
            }
        };
        if final_source_version != source_version {
            if let Err(failure_error) = self
                .fail_read_rebuild_if_generation(
                    &completed.holding_id,
                    candidate_read_database_key,
                    generation,
                    "restore source watermark is stale",
                )
                .await
            {
                tracing::error!(
                    error = %failure_error,
                    "restore replacement read failure transition failed"
                );
            }
            if let Err(rollback_error) = source_lock.rollback().await {
                tracing::error!(error = %rollback_error, "restore source rollback failed");
            }
            self.fail_backup_run(completed.id, "restore source watermark is stale")
                .await?;
            return Err(PersistenceError::InvalidData(
                "restore source watermark is stale".into(),
            ));
        }

        if let Err(error) = self
            .complete_read_rebuild_if_generation(
                &completed.holding_id,
                candidate_read_database_key,
                generation,
            )
            .await
        {
            if let Err(failure_error) = self
                .fail_read_rebuild_if_generation(
                    &completed.holding_id,
                    candidate_read_database_key,
                    generation,
                    "replacement read rebuild could not complete",
                )
                .await
            {
                tracing::error!(
                    error = %failure_error,
                    "restore replacement read failure transition failed"
                );
            }
            if let Err(rollback_error) = source_lock.rollback().await {
                tracing::error!(error = %rollback_error, "restore source rollback failed");
            }
            self.fail_backup_run(completed.id, "replacement read rebuild could not complete")
                .await?;
            return Err(error);
        }
        if let Err(error) = self
            .activate_read_database_if_generation(
                &completed.holding_id,
                candidate_read_database_key,
                generation,
            )
            .await
        {
            if let Err(failure_error) = self
                .fail_read_rebuild_if_generation(
                    &completed.holding_id,
                    candidate_read_database_key,
                    generation,
                    "replacement read activation failed",
                )
                .await
            {
                tracing::error!(
                    error = %failure_error,
                    "restore replacement read failure transition failed"
                );
            }
            if let Err(rollback_error) = source_lock.rollback().await {
                tracing::error!(error = %rollback_error, "restore source rollback failed");
            }
            self.fail_backup_run(completed.id, "replacement read activation failed")
                .await?;
            return Err(error);
        }
        source_lock.commit().await?;

        Ok(HoldingRestoreTestActivation {
            run: completed,
            candidate_read_database_key: candidate_read_database_key.into(),
            generation,
            source_version,
        })
    }

    /// Typed worker-facing variant of the fenced restore path. It binds the
    /// executor request to the same registration, candidate, and source
    /// projection before delegating to the routed activation state machine.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_restore_test_with_routed_read_replacement_executor<E>(
        &self,
        router: &HoldingDatabaseRouter,
        run: HoldingBackupRun,
        data_profile_key: &str,
        candidate_read_database_key: &str,
        source_projection: HoldingSourceProjection,
        source_version: u64,
        executor: &E,
    ) -> Result<HoldingRestoreTestActivation, PersistenceError>
    where
        E: RestoreExecutor + ?Sized,
    {
        let restore_target_ref = run.restore_target_ref.clone();
        self.execute_restore_test_with_routed_read_replacement_for_projection(
            router,
            run,
            data_profile_key,
            candidate_read_database_key,
            source_projection,
            source_version,
            move |registration, run, candidate, version| async move {
                let restore_target_ref =
                    restore_target_ref
                        .as_deref()
                        .ok_or(PersistenceError::InvalidData(
                            "restore target reference is required".into(),
                        ))?;
                let candidate = candidate.ok_or(PersistenceError::InvalidData(
                    "restore candidate is required".into(),
                ))?;
                let source_version = version.ok_or(PersistenceError::InvalidData(
                    "restore source version is required".into(),
                ))?;
                let databases = PhysicalHoldingDatabases::new(
                    registration.holding_id.clone(),
                    registration.write_database_key,
                    registration.read_database_key,
                )?;
                let backup_target = HoldingBackupTarget::for_run_in_namespace(
                    databases.clone(),
                    &registration.storage_namespace,
                    &registration.backup_chain_ref,
                    &run,
                )?;
                let replacement = databases.replacement_read_database(&candidate)?;
                let target = HoldingRestoreTestTarget::new(
                    backup_target,
                    restore_target_ref,
                    replacement,
                    source_version,
                )?;
                let request = RestoreTestExecutionRequest::new(target);
                let result = executor
                    .execute(request.clone())
                    .await
                    .map_err(|_| PersistenceError::InvalidData("restore executor failed".into()))?;
                let validated = request.validate_result(&result)?;
                Ok((
                    matches!(validated.outcome(), RestoreTestOutcome::Verified),
                    validated.replacement_read_verified(),
                ))
            },
        )
        .await
    }

    async fn execute_backup_internal<E, Fut>(
        &self,
        run: HoldingBackupRun,
        registration: HoldingStoreRegistration,
        executor: E,
        candidate_read_database_key: Option<String>,
        source_version: Option<u64>,
        require_replacement_verification: bool,
    ) -> Result<(HoldingBackupRun, (bool, bool)), PersistenceError>
    where
        E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
        Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
    {
        self.record_backup_run(&run).await?;

        match invoke_backup_executor(
            registration,
            run.clone(),
            candidate_read_database_key,
            source_version,
            executor,
        )
        .await
        {
            Ok((verified, replacement_read_verified))
                if run.kind == "restore_test"
                    && (!verified
                        || (require_replacement_verification && !replacement_read_verified)) =>
            {
                self.fail_backup_run(
                    run.id,
                    if verified {
                        "replacement read target was not verified"
                    } else {
                        "restore test was not verified"
                    },
                )
                .await?;
                Err(PersistenceError::InvalidData(
                    if verified {
                        "replacement read target was not verified"
                    } else {
                        "restore test executor did not verify the target"
                    }
                    .into(),
                ))
            }
            Ok((false, _)) => {
                self.fail_backup_run(run.id, "backup was not verified")
                    .await?;
                Err(PersistenceError::InvalidData(
                    "backup was not verified".into(),
                ))
            }
            Ok((verified, replacement_read_verified)) => {
                let completed_at = Utc::now();
                self.complete_backup_run(run.id, completed_at, verified)
                    .await?;
                Ok((
                    HoldingBackupRun {
                        status: "completed".into(),
                        completed_at: Some(completed_at),
                        verified,
                        ..run
                    },
                    (verified, replacement_read_verified),
                ))
            }
            Err(_) => {
                self.fail_backup_run(run.id, "backup executor failed")
                    .await?;
                Err(PersistenceError::InvalidData(
                    "backup executor failed".into(),
                ))
            }
        }
    }

    async fn validate_backup_registration(
        &self,
        run: &HoldingBackupRun,
    ) -> Result<HoldingStoreRegistration, PersistenceError> {
        let registration = SqlxHoldingStoreRegistry::new(self.control.clone())
            .get(&run.holding_id)
            .await?
            .ok_or(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ))?;
        if !matches!(registration.state.as_str(), "active" | "recovery") {
            return Err(PersistenceError::InvalidData(
                "holding is not in a backup-capable registry state".into(),
            ));
        }
        let provisioning_state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM erpx_control.holding_provisioning WHERE holding_id = $1",
        )
        .bind(&run.holding_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        if !matches!(
            provisioning_state.as_deref(),
            Some("active") | Some("recovery")
        ) {
            return Err(PersistenceError::InvalidData(
                "holding is not in a backup-capable provisioning state".into(),
            ));
        }
        validate_database_key(&registration.write_database_key, "write_database_key")?;
        validate_database_key(&registration.read_database_key, "read_database_key")?;
        validate_ref(&registration.backup_chain_ref, "backup_chain_ref")?;
        Ok(registration)
    }

    async fn complete_backup_run(
        &self,
        run_id: Uuid,
        completed_at: DateTime<Utc>,
        verified: bool,
    ) -> Result<(), PersistenceError> {
        let result = sqlx::query(
            "UPDATE erpx_control.holding_backup_runs
             SET status = 'completed', completed_at = $2, verified = $3, error = NULL
             WHERE id = $1 AND status = 'started'",
        )
        .bind(run_id)
        .bind(completed_at)
        .bind(verified)
        .execute(&self.control.pools().write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "backup run is not in started state".into(),
            ));
        }
        Ok(())
    }

    async fn fail_backup_run(
        &self,
        run_id: Uuid,
        error: &'static str,
    ) -> Result<(), PersistenceError> {
        let completed_at = Utc::now();
        let result = sqlx::query(
            "UPDATE erpx_control.holding_backup_runs
             SET status = 'failed', completed_at = $2, verified = FALSE, error = $3
             WHERE id = $1 AND status IN ('started', 'completed')",
        )
        .bind(run_id)
        .bind(completed_at)
        .bind(error)
        .execute(&self.control.pools().write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "backup run is not in started state".into(),
            ));
        }
        Ok(())
    }

    pub async fn transition_holding_state(
        &self,
        holding_id: &str,
        target: ProvisioningState,
    ) -> Result<HoldingStoreRegistration, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        let mut tx = self.control.pools().write.begin().await?;
        let row = sqlx::query(
            "SELECT r.data_profile_key, r.write_database_key, r.read_database_key,
                    r.storage_namespace, r.credential_key_ref, r.backup_chain_ref,
                    r.state AS registry_state, r.registry_version,
                    p.state AS provisioning_state
             FROM erpx_control.holding_registry r
             JOIN erpx_control.holding_provisioning p ON p.holding_id = r.holding_id
             WHERE r.holding_id = $1
             FOR UPDATE OF r, p",
        )
        .bind(holding_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            let _ = tx.rollback().await;
            return Err(PersistenceError::InvalidData(
                "holding is not registered".into(),
            ));
        };
        let registry_state: String = row.try_get("registry_state")?;
        let provisioning_state: String = row.try_get("provisioning_state")?;
        if registry_state != provisioning_state {
            let _ = tx.rollback().await;
            return Err(PersistenceError::InvalidData(
                "holding lifecycle state is inconsistent".into(),
            ));
        }
        let target_state = target.as_str();
        let allowed = lifecycle_transition_allowed(&registry_state, target);
        if !allowed {
            let _ = tx.rollback().await;
            return Err(PersistenceError::InvalidData(
                "holding lifecycle transition is not allowed".into(),
            ));
        }
        if target == ProvisioningState::Active {
            // Activation is a lifecycle write, so it must use the same
            // freshness boundary as the durable readiness read model. A
            // historical reachable row (or a probe timestamp from the
            // future) is not evidence that the dependency is healthy now.
            let ready = sqlx::query_scalar::<_, bool>(
                "SELECT (
                    EXISTS (SELECT 1 FROM erpx_control.holding_database_health
                            WHERE holding_id = $1 AND side = 'write'
                              AND database_key = $2 AND reachable
                              AND checked_at >= now() - interval '5 minutes'
                              AND checked_at <= now())
                    AND EXISTS (SELECT 1 FROM erpx_control.holding_database_health
                            WHERE holding_id = $1 AND side = 'read'
                              AND database_key = $3 AND reachable
                              AND checked_at >= now() - interval '5 minutes'
                              AND checked_at <= now())
                    AND EXISTS (SELECT 1 FROM erpx_control.holding_storage_health
                            WHERE holding_id = $1 AND storage_namespace = $4 AND reachable
                              AND checked_at >= now() - interval '5 minutes'
                              AND checked_at <= now())
                 )",
            )
            .bind(holding_id)
            .bind(row.try_get::<String, _>("write_database_key")?)
            .bind(row.try_get::<String, _>("read_database_key")?)
            .bind(row.try_get::<String, _>("storage_namespace")?)
            .fetch_one(&mut *tx)
            .await?;
            if !ready {
                let _ = tx.rollback().await;
                return Err(PersistenceError::InvalidData(
                    "holding write/read databases and storage must be healthy before activation"
                        .into(),
                ));
            }
        }
        sqlx::query(
            "UPDATE erpx_control.holding_provisioning
             SET state = $2, version = version + 1,
                 last_error = CASE WHEN $2 = 'active' THEN NULL ELSE last_error END,
                 updated_at = now()
             WHERE holding_id = $1 AND state = $3",
        )
        .bind(holding_id)
        .bind(target_state)
        .bind(&registry_state)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_registry
             SET state = $2, registry_version = registry_version + 1, updated_at = now()
             WHERE holding_id = $1 AND state = $3",
        )
        .bind(holding_id)
        .bind(target_state)
        .bind(&registry_state)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            let _ = tx.rollback().await;
            return Err(PersistenceError::InvalidData(
                "holding lifecycle transition lost its registry claim".into(),
            ));
        }
        let registration = HoldingStoreRegistration {
            holding_id: holding_id.into(),
            data_profile_key: row.try_get("data_profile_key")?,
            write_database_key: row.try_get("write_database_key")?,
            read_database_key: row.try_get("read_database_key")?,
            storage_namespace: row.try_get("storage_namespace")?,
            credential_key_ref: row.try_get("credential_key_ref")?,
            backup_chain_ref: row.try_get("backup_chain_ref")?,
            state: target_state.into(),
            registry_version: u64::try_from(row.try_get::<i64, _>("registry_version")?)
                .map_err(|_| {
                    PersistenceError::InvalidData("holding registry version is invalid".into())
                })?
                .checked_add(1)
                .ok_or(PersistenceError::InvalidData(
                    "holding registry version exceeded its bound".into(),
                ))?,
        };
        tx.commit().await?;
        Ok(registration)
    }

    pub async fn activate(&self, holding_id: &str) -> Result<(), PersistenceError> {
        self.transition_holding_state(holding_id, ProvisioningState::Active)
            .await
            .map(|_| ())
    }

    pub async fn suspend(
        &self,
        holding_id: &str,
    ) -> Result<HoldingStoreRegistration, PersistenceError> {
        self.transition_holding_state(holding_id, ProvisioningState::Suspended)
            .await
    }

    pub async fn enter_recovery(
        &self,
        holding_id: &str,
    ) -> Result<HoldingStoreRegistration, PersistenceError> {
        self.transition_holding_state(holding_id, ProvisioningState::Recovery)
            .await
    }

    pub async fn begin_read_rebuild(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        source_version: u64,
    ) -> Result<(), PersistenceError> {
        self.begin_read_rebuild_with_generation(
            holding_id,
            candidate_read_database_key,
            source_version,
        )
        .await
        .map(|_| ())
    }

    /// Start a replacement-read rebuild and return its monotonic switch
    /// generation. The generation is the fencing token used by workers when
    /// completing, failing, or activating the candidate.
    pub async fn begin_read_rebuild_with_generation(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        source_version: u64,
    ) -> Result<u64, PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        if source_version == 0 {
            return Err(PersistenceError::InvalidData(
                "projection source version is required".into(),
            ));
        }
        let mut tx = self.control.pools().write.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(candidate_read_database_key)
            .execute(&mut *tx)
            .await?;
        let (state, write_database, current): (String, String, String) = sqlx::query_as(
            "SELECT state, write_database_key, read_database_key
             FROM erpx_control.holding_registry WHERE holding_id = $1 FOR UPDATE",
        )
        .bind(holding_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "holding is not registered".into(),
        ))?;
        if state != "active" {
            return Err(PersistenceError::InvalidData(
                "replacement read rebuild requires an active holding".into(),
            ));
        }
        if write_database == candidate_read_database_key || current == candidate_read_database_key {
            return Err(PersistenceError::Configuration(
                "replacement read database must be distinct from provisioned databases",
            ));
        }
        let candidate_owned: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_control.holding_registry
                 WHERE holding_id <> $1
                   AND (write_database_key = $2 OR read_database_key = $2)
             ) OR EXISTS (
                 SELECT 1 FROM erpx_control.holding_projection_switches
                 WHERE holding_id <> $1 AND candidate_read_database_key = $2
             )",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .fetch_one(&mut *tx)
        .await?;
        if candidate_owned {
            return Err(PersistenceError::Configuration(
                "replacement read database is already owned by another holding",
            ));
        }
        let version: i64 = sqlx::query_scalar(
            "INSERT INTO erpx_control.holding_projection_switches
                (holding_id, current_read_database_key, candidate_read_database_key,
                 state, source_version, version)
             VALUES ($1, $2, $3, 'rebuilding', $4, 1)
             ON CONFLICT (holding_id) DO UPDATE SET
                candidate_read_database_key = EXCLUDED.candidate_read_database_key,
                state = 'rebuilding', source_version = EXCLUDED.source_version,
                version = erpx_control.holding_projection_switches.version + 1,
                last_error = NULL, updated_at = now()
             WHERE erpx_control.holding_projection_switches.state IN ('ready', 'activated', 'failed')
             RETURNING version",
        )
        .bind(holding_id)
        .bind(current)
        .bind(candidate_read_database_key)
        .bind(i64::try_from(source_version).map_err(|_| {
            PersistenceError::InvalidData("projection source version exceeds database range".into())
        })?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "projection switch is already active or not resettable".into(),
        ))?;
        tx.commit().await?;
        u64::try_from(version).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })
    }

    pub async fn complete_read_rebuild(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version
             FROM erpx_control.holding_projection_switches
             WHERE holding_id = $1 AND candidate_read_database_key = $2
               AND state = 'rebuilding'",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .fetch_optional(&self.control.pools().write)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "read rebuild is not active for this candidate".into(),
        ))?;
        self.complete_read_rebuild_if_generation(
            holding_id,
            candidate_read_database_key,
            u64::try_from(version).map_err(|_| {
                PersistenceError::InvalidData("projection switch generation is invalid".into())
            })?,
        )
        .await
    }

    pub async fn complete_read_rebuild_if_generation(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_projection_switches
             SET state = 'ready', updated_at = now()
             WHERE holding_id = $1 AND candidate_read_database_key = $2
               AND state = 'rebuilding' AND version = $3",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(generation)
        .execute(&self.control.pools().write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "read rebuild is not active for this candidate".into(),
            ));
        }
        Ok(())
    }

    pub async fn fail_read_rebuild(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        error: &'static str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version
             FROM erpx_control.holding_projection_switches
             WHERE holding_id = $1 AND candidate_read_database_key = $2
               AND state IN ('rebuilding', 'ready')",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .fetch_optional(&self.control.pools().write)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "read rebuild is not active for failure transition".into(),
        ))?;
        self.fail_read_rebuild_if_generation(
            holding_id,
            candidate_read_database_key,
            u64::try_from(version).map_err(|_| {
                PersistenceError::InvalidData("projection switch generation is invalid".into())
            })?,
            error,
        )
        .await
    }

    pub async fn fail_read_rebuild_if_generation(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
        error: &'static str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_projection_switches
             SET state = 'failed', last_error = $3, updated_at = now()
             WHERE holding_id = $1 AND candidate_read_database_key = $2
               AND state IN ('rebuilding', 'ready') AND version = $4",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(error)
        .bind(generation)
        .execute(&self.control.pools().write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "read rebuild is not active for failure transition".into(),
            ));
        }
        Ok(())
    }

    pub async fn activate_read_database(&self, holding_id: &str) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        // This legacy metadata-only entry point has no holding write route and
        // therefore cannot hold the source fence. Refuse activation instead
        // of allowing backup/recovery callers to bypass the routed scheduler.
        Err(PersistenceError::InvalidData(
            "direct read activation requires a routed holding source fence".into(),
        ))
    }

    /// Activate exactly the candidate and generation that a worker rebuilt.
    /// The candidate, source generation, and current registry target are all
    /// checked under one control-plane transaction so a stale worker cannot
    /// switch a newer rebuild.
    pub async fn activate_read_database_if_generation(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
    ) -> Result<(), PersistenceError> {
        self.activate_read_database_if_generation_internal(
            holding_id,
            candidate_read_database_key,
            generation,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn activate_read_database_if_generation_with_human_decision(
        &self,
        holding_id: &str,
        projection_name: &str,
        candidate_read_database_key: &str,
        generation: u64,
        command_id: Uuid,
        correlation_id: Uuid,
        decision: &HumanDecisionProof,
    ) -> Result<(), PersistenceError> {
        validate_projection_rebuild_activation_decision(
            holding_id,
            projection_name,
            candidate_read_database_key,
            generation,
            command_id,
            decision,
        )?;
        self.activate_read_database_if_generation_internal(
            holding_id,
            candidate_read_database_key,
            generation,
            Some((projection_name, correlation_id, decision)),
        )
        .await
    }

    async fn activate_read_database_if_generation_internal(
        &self,
        holding_id: &str,
        candidate_read_database_key: &str,
        generation: u64,
        human_decision: Option<(&str, Uuid, &HumanDecisionProof)>,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding_id")?;
        validate_database_key(candidate_read_database_key, "candidate_read_database_key")?;
        let generation = i64::try_from(generation).map_err(|_| {
            PersistenceError::InvalidData("projection switch generation is invalid".into())
        })?;
        let mut tx = self.control.pools().write.begin().await?;
        let row = sqlx::query(
            "SELECT current_read_database_key, candidate_read_database_key
             FROM erpx_control.holding_projection_switches
             WHERE holding_id = $1 AND candidate_read_database_key = $2
               AND state = 'ready' AND version = $3 FOR UPDATE",
        )
        .bind(holding_id)
        .bind(candidate_read_database_key)
        .bind(generation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::InvalidData(
            "replacement read database generation is not ready for activation".into(),
        ))?;
        let current: String = row.try_get("current_read_database_key")?;
        let candidate: String = row.try_get("candidate_read_database_key")?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_registry
             SET read_database_key = $2, registry_version = registry_version + 1, updated_at = now()
             WHERE holding_id = $1 AND state = 'active' AND read_database_key = $3",
        )
        .bind(holding_id)
        .bind(&candidate)
        .bind(&current)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            let _ = tx.rollback().await;
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "holding_projection_switch".into(),
                aggregate_id: Uuid::nil(),
                expected: None,
                actual: None,
            });
        }
        let result = sqlx::query(
            "UPDATE erpx_control.holding_projection_switches
             SET current_read_database_key = candidate_read_database_key,
                 candidate_read_database_key = current_read_database_key,
                 state = 'activated', version = version + 1, activated_at = now(), updated_at = now()
             WHERE holding_id = $1 AND state = 'ready' AND version = $2
               AND candidate_read_database_key = $3",
        )
        .bind(holding_id)
        .bind(generation)
        .bind(candidate_read_database_key)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            let _ = tx.rollback().await;
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "holding_projection_switch".into(),
                aggregate_id: Uuid::nil(),
                expected: Some(generation as u64),
                actual: None,
            });
        }
        if let Some((projection_name, correlation_id, decision)) = human_decision {
            sqlx::query(
                "INSERT INTO erpx_control.audit_events
                    (id, actor_ref, customer_account_id, action, purpose,
                     correlation_id, payload, recorded_at)
                 VALUES ($1, $2, NULL, 'platform.projection_rebuild.activate',
                         'activate a verified projection rebuild generation', $3, $4, $5)",
            )
            .bind(Uuid::new_v4())
            .bind(decision.principal_ref())
            .bind(correlation_id)
            .bind(json!({
                "holding_id": holding_id,
                "projection_name": projection_name,
                "candidate_read_database_key": candidate_read_database_key,
                "generation": generation,
                "human_decision": {
                    "principal_ref": decision.principal_ref(),
                    "actor_kind": decision.actor().kind,
                    "session_id": decision.actor().session_id.map(|session| session.as_uuid()),
                    "authority_scope": decision.authority_scope(),
                    "decision_ref": decision.decision_ref(),
                    "decided_at": decision.decided_at(),
                }
            }))
            .bind(decision.decided_at())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

pub fn projection_rebuild_activation_decision_ref(
    command_id: Uuid,
    holding_id: &str,
    projection_name: &str,
    candidate_read_database_key: &str,
    generation: u64,
) -> String {
    format!(
        "platform_administration:activate_projection_rebuild:{command_id}:{holding_id}:{projection_name}:{candidate_read_database_key}:{generation}:activate"
    )
}

fn validate_projection_rebuild_activation_decision(
    holding_id: &str,
    projection_name: &str,
    candidate_read_database_key: &str,
    generation: u64,
    command_id: Uuid,
    decision: &HumanDecisionProof,
) -> Result<(), PersistenceError> {
    let expected_ref = projection_rebuild_activation_decision_ref(
        command_id,
        holding_id,
        projection_name,
        candidate_read_database_key,
        generation,
    );
    if !matches!(decision.authority_scope(), AuthorityScope::Platform)
        || decision.decision_ref() != expected_ref
    {
        return Err(PersistenceError::InvalidData(
            "projection rebuild activation human decision is invalid".into(),
        ));
    }
    Ok(())
}

async fn invoke_backup_executor<E, Fut>(
    registration: HoldingStoreRegistration,
    run: HoldingBackupRun,
    candidate_read_database_key: Option<String>,
    source_version: Option<u64>,
    executor: E,
) -> Result<(bool, bool), PersistenceError>
where
    E: FnOnce(HoldingStoreRegistration, HoldingBackupRun, Option<String>, Option<u64>) -> Fut,
    Fut: Future<Output = Result<(bool, bool), PersistenceError>>,
{
    executor(
        registration,
        run,
        candidate_read_database_key,
        source_version,
    )
    .await
}

fn validate_backup_job_request(request: &HoldingBackupJobRequest) -> Result<(), PersistenceError> {
    validate_ref(&request.holding_id, "holding_id")?;
    validate_ref(&request.data_profile_key, "data_profile_key")?;
    validate_ref(&request.backup_ref, "backup_ref")?;
    validate_ref(&request.idempotency_key, "idempotency_key")?;
    if !matches!(request.kind.as_str(), "full" | "pitr" | "restore_test") {
        return Err(PersistenceError::InvalidData(
            "backup job kind is invalid".into(),
        ));
    }
    if !matches!(
        request.source_projection.as_str(),
        "operations" | "finance_banking" | "finance_subledger" | "finance_close"
    ) {
        return Err(PersistenceError::InvalidData(
            "backup source projection is invalid".into(),
        ));
    }
    if request.max_attempts == 0 || request.max_attempts > MAX_HOLDING_BACKUP_JOB_ATTEMPTS {
        return Err(PersistenceError::InvalidData(
            "backup job attempts are outside their bound".into(),
        ));
    }
    let now = Utc::now();
    if request.available_at < now - Duration::minutes(5)
        || request.available_at > now + Duration::days(365)
    {
        return Err(PersistenceError::InvalidData(
            "backup job availability is outside its bound".into(),
        ));
    }
    match request.kind.as_str() {
        "full"
            if request.restore_target_ref.is_none()
                && request.candidate_read_database_key.is_none()
                && request.source_version.is_none()
                && request.pitr_recovery_point.is_none() => {}
        "pitr" => {
            if request.restore_target_ref.is_some()
                || request.candidate_read_database_key.is_some()
                || request.source_version.is_some()
            {
                return Err(PersistenceError::InvalidData(
                    "PITR jobs cannot contain restore-test fields".into(),
                ));
            }
            request
                .pitr_recovery_point
                .as_ref()
                .ok_or(PersistenceError::InvalidData(
                    "PITR jobs require a recovery point".into(),
                ))?
                .validate(request.available_at)
                .map_err(|_| {
                    PersistenceError::InvalidData("PITR recovery point is invalid".into())
                })?;
        }
        "restore_test" => {
            validate_ref(
                request
                    .restore_target_ref
                    .as_deref()
                    .ok_or(PersistenceError::InvalidData(
                        "restore target reference is required".into(),
                    ))?,
                "restore_target_ref",
            )?;
            validate_database_key(
                request.candidate_read_database_key.as_deref().ok_or(
                    PersistenceError::InvalidData("candidate read database key is required".into()),
                )?,
                "candidate_read_database_key",
            )?;
            let source_version = request.source_version.ok_or(PersistenceError::InvalidData(
                "restore source version is required".into(),
            ))?;
            i64::try_from(source_version).map_err(|_| {
                PersistenceError::InvalidData("restore source version is too large".into())
            })?;
        }
        _ => {
            return Err(PersistenceError::InvalidData(
                "full backup job fields are invalid".into(),
            ));
        }
    }
    Ok(())
}

fn pitr_recovery_point_json(
    recovery_point: &Option<PitrRecoveryPoint>,
) -> Result<Option<Value>, PersistenceError> {
    recovery_point
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(PersistenceError::from)
}

/// Persist the safe lifecycle projection and its outbox notification in the
/// same control-plane transaction as the backup job mutation. Backup and
/// restore references intentionally never cross this event boundary.
#[allow(clippy::too_many_arguments)]
async fn append_holding_backup_job_event(
    transaction: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    holding_id: &str,
    data_profile_key: &str,
    kind: &str,
    state: &str,
    attempts: u16,
    max_attempts: u16,
    verified: bool,
    error_present: bool,
    available_at: DateTime<Utc>,
    event_type: &str,
    idempotency_key: Option<&str>,
    occurred_at: DateTime<Utc>,
    error_code: Option<&str>,
) -> Result<Uuid, PersistenceError> {
    validate_ref(holding_id, "holding_id")?;
    validate_ref(data_profile_key, "data_profile_key")?;
    validate_ref(kind, "kind")?;
    validate_ref(state, "state")?;
    if event_type.is_empty()
        || event_type.len() > 200
        || event_type.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
        })
    {
        return Err(PersistenceError::InvalidData(
            "backup job event type is outside its bound".into(),
        ));
    }
    if job_id.is_nil() || attempts > MAX_HOLDING_BACKUP_JOB_ATTEMPTS || max_attempts == 0 {
        return Err(PersistenceError::InvalidData(
            "backup job event bounds are invalid".into(),
        ));
    }
    if let Some(error_code) = error_code {
        validate_backup_error_code(error_code)?;
    }
    if let Some(idempotency_key) = idempotency_key {
        validate_ref(idempotency_key, "idempotency_key")?;
    }
    let next_version: i64 = sqlx::query_scalar(
        "UPDATE erpx_control.holding_backup_jobs
         SET aggregate_version=aggregate_version+1
         WHERE id=$1 AND holding_id=$2 AND data_profile_key=$3 AND kind=$4
           AND state=$5 AND attempts=$6 AND max_attempts=$7
           AND verified=$8
         RETURNING aggregate_version",
    )
    .bind(job_id)
    .bind(holding_id)
    .bind(data_profile_key)
    .bind(kind)
    .bind(state)
    .bind(i32::from(attempts))
    .bind(i32::from(max_attempts))
    .bind(verified)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| {
        PersistenceError::InvalidData(
            "backup job event does not match typed lifecycle authority".into(),
        )
    })?;
    let payload = json!({
        "job_id": job_id,
        "holding_id": holding_id,
        "data_profile_key": data_profile_key,
        "kind": kind,
        "state": state,
        "available_at": available_at,
        "attempts": attempts,
        "max_attempts": max_attempts,
        "verified": verified,
        "error_present": error_present,
        "error_code": error_code,
        "updated_at": occurred_at,
    });
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             causation_id, idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, $6, NULL, $1, NULL, $7, $8, $9, $9)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(HOLDING_BACKUP_JOB_AGGREGATE)
    .bind(job_id)
    .bind(next_version)
    .bind(json!({"holding_id": holding_id, "data_profile_key": data_profile_key}))
    .bind(idempotency_key)
    .bind(&payload)
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(event_id)
}

fn backup_job_request_hash(request: &HoldingBackupJobRequest) -> String {
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        request.holding_id,
        request.data_profile_key,
        request.kind,
        request.backup_ref,
        request.restore_target_ref.as_deref().unwrap_or_default(),
        request
            .candidate_read_database_key
            .as_deref()
            .unwrap_or_default(),
        request.source_version.unwrap_or_default(),
        request.available_at.to_rfc3339(),
        request.idempotency_key,
        request.max_attempts,
        request.source_projection,
        request
            .pitr_recovery_point
            .as_ref()
            .map(|point| {
                format!(
                    "{}|{}|{}|{}",
                    point
                        .target_time
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    point.target_lsn.as_deref().unwrap_or_default(),
                    point.wal_archive_ref,
                    point.physical_restore_target_ref,
                )
            })
            .unwrap_or_default(),
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn validate_backup_job_claim(claim: &HoldingBackupJobClaim) -> Result<(), PersistenceError> {
    validate_ref(&claim.holding_id, "holding_id")?;
    validate_ref(&claim.data_profile_key, "data_profile_key")?;
    validate_ref(&claim.kind, "kind")?;
    if claim.id.is_nil() || claim.claim_id.is_nil() || claim.worker_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "backup job claim identifiers are required".into(),
        ));
    }
    if claim.kind != claim.run.kind || claim.run.holding_id != claim.holding_id {
        return Err(PersistenceError::InvalidData(
            "backup job claim run binding is invalid".into(),
        ));
    }
    if claim.attempt == 0 || claim.attempt > MAX_HOLDING_BACKUP_JOB_ATTEMPTS {
        return Err(PersistenceError::InvalidData(
            "backup job claim attempt is outside its bound".into(),
        ));
    }
    Ok(())
}

fn validate_backup_error_code(error_code: &str) -> Result<(), PersistenceError> {
    if error_code.is_empty()
        || error_code.len() > 100
        || error_code
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    {
        return Err(PersistenceError::InvalidData(
            "backup failure code is outside its bound".into(),
        ));
    }
    Ok(())
}

fn backup_retry_delay(attempt: i32) -> Duration {
    let exponent = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(0)
        .min(10);
    Duration::seconds((1_i64 << exponent).min(3600))
}

fn postgres_timestamp(value: DateTime<Utc>) -> DateTime<Utc> {
    value - Duration::nanoseconds(i64::from(value.timestamp_subsec_nanos() % 1_000))
}

fn lifecycle_transition_allowed(current: &str, target: ProvisioningState) -> bool {
    match target {
        ProvisioningState::Active => {
            matches!(current, "provisioning" | "recovery" | "suspended")
        }
        ProvisioningState::Suspended => current == "active",
        ProvisioningState::Recovery => matches!(current, "active" | "suspended"),
        ProvisioningState::Provisioning | ProvisioningState::Failed => false,
    }
}

fn validate_backup_run_shape(run: &HoldingBackupRun) -> Result<(), PersistenceError> {
    if run.id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "backup run ID is required".into(),
        ));
    }
    validate_ref(&run.holding_id, "holding_id")?;
    validate_ref(&run.backup_ref, "backup_ref")?;
    if run
        .completed_at
        .is_some_and(|completed_at| completed_at < run.started_at)
    {
        return Err(PersistenceError::InvalidData(
            "backup completion cannot precede its start".into(),
        ));
    }
    if !matches!(run.kind.as_str(), "full" | "pitr" | "restore_test") {
        return Err(PersistenceError::InvalidData(
            "backup kind must be full, pitr, or restore_test".into(),
        ));
    }
    if !matches!(run.status.as_str(), "started" | "completed" | "failed") {
        return Err(PersistenceError::InvalidData(
            "backup status must be started, completed, or failed".into(),
        ));
    }
    match run.kind.as_str() {
        "restore_test" => {
            let target = run
                .restore_target_ref
                .as_deref()
                .ok_or(PersistenceError::InvalidData(
                    "restore tests require a restore target reference".into(),
                ))?;
            validate_ref(target, "restore_target_ref")?;
            if target == run.backup_ref {
                return Err(PersistenceError::InvalidData(
                    "restore target must be distinct from backup reference".into(),
                ));
            }
        }
        "full" | "pitr" if run.restore_target_ref.is_some() => {
            return Err(PersistenceError::InvalidData(
                "full and pitr runs cannot have a restore target reference".into(),
            ));
        }
        _ => {}
    }
    if run.kind == "full" && run.pitr_recovery_point.is_some() {
        return Err(PersistenceError::InvalidData(
            "full backup runs cannot contain PITR metadata".into(),
        ));
    }
    if run.kind == "pitr" {
        run.pitr_recovery_point
            .as_ref()
            .ok_or(PersistenceError::InvalidData(
                "PITR runs require recovery-point metadata".into(),
            ))?
            .validate(run.started_at)
            .map_err(|_| PersistenceError::InvalidData("PITR recovery point is invalid".into()))?;
    }
    match run.status.as_str() {
        "started" if run.completed_at.is_some() || run.verified || run.error.is_some() => {
            Err(PersistenceError::InvalidData(
                "started backup runs cannot contain completion fields".into(),
            ))
        }
        "completed" if run.completed_at.is_none() || run.error.is_some() => {
            Err(PersistenceError::InvalidData(
                "completed backup runs require completion time and no error".into(),
            ))
        }
        "completed" if run.kind == "restore_test" && !run.verified => Err(
            PersistenceError::InvalidData("completed restore tests must be verified".into()),
        ),
        "failed" if run.completed_at.is_none() || run.verified => {
            Err(PersistenceError::InvalidData(
                "failed backup runs require completion time and cannot be verified".into(),
            ))
        }
        "failed"
            if run
                .error
                .as_deref()
                .is_none_or(|error| error.trim().is_empty()) =>
        {
            Err(PersistenceError::InvalidData(
                "failed backup runs require an error".into(),
            ))
        }
        _ => Ok(()),
    }
}

fn empty_projection_observation() -> ProjectionFreshnessObservation {
    ProjectionFreshnessObservation {
        state: ReadinessObservationState::NotProbed,
        source_version: None,
        updated_at: None,
        activated_at: None,
        has_error: false,
        reason_code: None,
    }
}

fn empty_backup_observation() -> BackupPitrObservation {
    BackupPitrObservation {
        state: ReadinessObservationState::NotProbed,
        run_id: None,
        kind: None,
        status: None,
        started_at: None,
        completed_at: None,
        verified: false,
        has_error: false,
        restore_target_present: false,
        reason_code: None,
    }
}

fn positive_observation_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn classify_persisted_observation(
    reachable: bool,
    checked_at: DateTime<Utc>,
    now: DateTime<Utc>,
    max_age: Duration,
) -> ReadinessComponentObservation {
    if max_age < Duration::zero() {
        return ReadinessComponentObservation::unavailable(
            "invalid_observation_policy",
            Some(checked_at),
        );
    }
    if checked_at > now {
        return ReadinessComponentObservation::unavailable(
            "observation_from_future",
            Some(checked_at),
        );
    }
    if !reachable {
        return ReadinessComponentObservation::unavailable(
            "dependency_unreachable",
            Some(checked_at),
        );
    }
    if now.signed_duration_since(checked_at) > max_age {
        return ReadinessComponentObservation::unavailable("observation_stale", Some(checked_at));
    }
    ReadinessComponentObservation::ready(checked_at)
}

fn database_observation_from_component(
    component: ReadinessComponentObservation,
    schema_version: String,
    checked_at: DateTime<Utc>,
) -> DatabaseReadinessObservation {
    DatabaseReadinessObservation {
        state: component.state,
        schema_version: Some(schema_version),
        checked_at: component.checked_at.or(Some(checked_at)),
        reason_code: component.reason_code,
    }
}

fn merge_database_observation(
    durable: DatabaseReadinessObservation,
    live: ReadinessComponentObservation,
) -> DatabaseReadinessObservation {
    if durable.state == ReadinessObservationState::Unavailable {
        return durable;
    }
    if live.state == ReadinessObservationState::Unavailable {
        return DatabaseReadinessObservation {
            state: ReadinessObservationState::Unavailable,
            schema_version: durable.schema_version,
            checked_at: live.checked_at.or(durable.checked_at),
            reason_code: live
                .reason_code
                .or_else(|| Some("holding_database_probe_failed".into())),
        };
    }
    if durable.state != ReadinessObservationState::Ready
        || live.state != ReadinessObservationState::Ready
    {
        return DatabaseReadinessObservation {
            state: ReadinessObservationState::NotProbed,
            schema_version: durable.schema_version,
            checked_at: live.checked_at.or(durable.checked_at),
            reason_code: durable
                .reason_code
                .or(live.reason_code)
                .or_else(|| Some("durable_database_health_not_probed".into())),
        };
    }
    DatabaseReadinessObservation {
        state: ReadinessObservationState::Ready,
        schema_version: durable.schema_version,
        checked_at: live.checked_at.or(durable.checked_at),
        reason_code: None,
    }
}

fn aggregate_readiness_states(states: [ReadinessObservationState; 4]) -> ReadinessObservationState {
    if states.contains(&ReadinessObservationState::Unavailable) {
        ReadinessObservationState::Unavailable
    } else if states
        .iter()
        .all(|state| *state == ReadinessObservationState::Ready)
    {
        ReadinessObservationState::Ready
    } else {
        ReadinessObservationState::NotProbed
    }
}

fn first_reason(reasons: [Option<&str>; 4]) -> Option<String> {
    reasons.into_iter().flatten().next().map(str::to_owned)
}

#[allow(clippy::too_many_arguments)]
fn projection_observation(
    expected_read_database: &str,
    current_read_database: String,
    state: String,
    source_version: i64,
    has_error: bool,
    activated_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    now: DateTime<Utc>,
    max_age: Duration,
) -> ProjectionFreshnessObservation {
    let source_version = u64::try_from(source_version).ok();
    let base = classify_persisted_observation(true, updated_at, now, max_age);
    let reason_code = if current_read_database != expected_read_database {
        Some("projection_read_database_mismatch")
    } else if source_version.is_none() {
        Some("projection_source_version_invalid")
    } else if has_error {
        Some("projection_has_error")
    } else if state != "activated" {
        Some("projection_not_activated")
    } else if activated_at.is_none_or(|value| value > now) {
        Some("projection_activation_timestamp_invalid")
    } else if base.state != ReadinessObservationState::Ready {
        base.reason_code.as_deref()
    } else {
        None
    };
    let final_state = if reason_code.is_none() {
        ReadinessObservationState::Ready
    } else {
        ReadinessObservationState::Unavailable
    };
    ProjectionFreshnessObservation {
        state: final_state,
        source_version,
        updated_at: Some(updated_at),
        activated_at,
        has_error,
        reason_code: reason_code.map(str::to_owned),
    }
}

#[allow(clippy::too_many_arguments)]
fn backup_observation(
    run_id: Uuid,
    kind: String,
    status: String,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    verified: bool,
    has_error: bool,
    restore_target_present: bool,
    now: DateTime<Utc>,
    max_age: Duration,
) -> BackupPitrObservation {
    let age_state = completed_at
        .map(|completed_at| classify_persisted_observation(true, completed_at, now, max_age));
    let reason_code = if !matches!(kind.as_str(), "full" | "pitr" | "restore_test") {
        Some("backup_kind_invalid")
    } else if status != "completed" {
        Some(if status == "started" {
            "backup_in_progress"
        } else {
            "backup_failed"
        })
    } else if completed_at
        .is_none_or(|completed_at| completed_at < started_at || completed_at > now)
    {
        Some("backup_completion_timestamp_invalid")
    } else if !verified {
        Some("backup_not_verified")
    } else if has_error {
        Some("backup_has_error")
    } else if age_state
        .as_ref()
        .is_none_or(|state| state.state != ReadinessObservationState::Ready)
    {
        age_state
            .as_ref()
            .and_then(|state| state.reason_code.as_deref())
            .or(Some("backup_not_probed"))
    } else {
        None
    };
    BackupPitrObservation {
        state: if reason_code.is_none() {
            ReadinessObservationState::Ready
        } else {
            ReadinessObservationState::Unavailable
        },
        run_id: Some(run_id),
        kind: Some(kind),
        status: Some(status),
        started_at: Some(started_at),
        completed_at,
        verified,
        has_error,
        restore_target_present,
        reason_code: reason_code.map(str::to_owned),
    }
}

fn validate_ref(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be opaque, path-safe, and non-empty"
        )));
    }
    Ok(())
}

fn validate_principal_ref(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| {
            character.is_control() || matches!(character, '/' | '\\') || character.is_whitespace()
        })
    {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be opaque, path-safe, and non-empty"
        )));
    }
    Ok(())
}

fn validate_health_details(details: &Value) -> Result<(), PersistenceError> {
    if !details.is_object() {
        return Err(PersistenceError::InvalidData(
            "health observation details must be a JSON object".into(),
        ));
    }
    let serialized = serde_json::to_vec(details).map_err(|_| {
        PersistenceError::InvalidData("health observation details are not serializable".into())
    })?;
    if serialized.len() > MAX_HEALTH_DETAILS_BYTES {
        return Err(PersistenceError::InvalidData(
            "health observation details exceed their size bound".into(),
        ));
    }
    if contains_sensitive_health_key(details) {
        return Err(PersistenceError::InvalidData(
            "health observation details cannot contain credentials or secrets".into(),
        ));
    }
    Ok(())
}

fn contains_sensitive_health_key(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            [
                "password",
                "secret",
                "token",
                "credential",
                "private_key",
                "database_url",
            ]
            .iter()
            .any(|needle| key.contains(needle))
                || contains_sensitive_health_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_sensitive_health_key),
        _ => false,
    }
}

async fn initialize_holding_organization_rows(
    write_pool: &sqlx::PgPool,
    _read_pool: &sqlx::PgPool,
    holding_key: &str,
    data_profile_key: &str,
    holding_id: Uuid,
    owner_principal_ref: &str,
    at: DateTime<Utc>,
) -> Result<HoldingOrganizationBootstrap, PersistenceError> {
    let holding_uuid = Uuid::parse_str(holding_key).map_err(|_| {
        PersistenceError::InvalidData(
            "current organization bootstrap requires a UUID holding_id".into(),
        )
    })?;
    if holding_uuid.is_nil() {
        return Err(PersistenceError::InvalidData(
            "current organization bootstrap requires a non-nil holding_id".into(),
        ));
    }
    let command_uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:organization-bootstrap:command:{holding_id}").as_bytes(),
    );
    let correlation_uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:organization-bootstrap:correlation:{holding_id}").as_bytes(),
    );
    let node_id = OrganizationNodeId::try_from_uuid(holding_id)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let scope = AuthorityScope::Holding { holding_id };
    let access = OrganizationBootstrapAccess {
        worker_principal_ref: format!("erpx-bootstrap:{holding_id}"),
        represented_owner_ref: owner_principal_ref.to_owned(),
        scope: scope.clone(),
    };
    let pool = WritePool::for_holding(write_pool.clone(), holding_uuid)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let receipt = OrganizationCommandHandler::new(
        OrganizationWriteRepository::new(pool.clone()),
        OrganizationCommandAdapter::new(pool),
        access.clone(),
        access,
        OrganizationHierarchyRepository,
        FixedClock::new(at),
    )
    .handle(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(command_uuid)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        idempotency_key: IdempotencyKey::try_new(format!(
            "erpx:organization-bootstrap:{holding_id}"
        ))
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        correlation_id: CorrelationId::try_from_uuid(correlation_uuid)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        causation_id: None,
        actor: KernelActorAttribution {
            principal_ref: format!("erpx-bootstrap:{holding_id}"),
            kind: KernelActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: Some(owner_principal_ref.to_owned()),
            support_capability_id: None,
        },
        active_scope: scope,
        audit: AuditContext {
            purpose: "organization.bootstrap".into(),
            customer_visible: true,
            module: Some("organization".into()),
        },
        expected_aggregate_version: None,
        command: CreateOrganizationNode {
            node_id,
            placement: OrganizationPlacement::Holding,
            code: data_profile_key.to_owned(),
            name: data_profile_key.to_owned(),
        },
    })
    .await
    .map_err(|error| {
        PersistenceError::InvalidData(format!("organization bootstrap CQRS failure: {error:?}"))
    })?;
    let event_id = receipt
        .committed_events
        .last()
        .map(|position| position.event_id.as_uuid())
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "organization bootstrap command did not emit durable outbox evidence".into(),
            )
        })?;

    Ok(HoldingOrganizationBootstrap {
        holding_key: holding_key.into(),
        data_profile_key: data_profile_key.into(),
        holding_id,
        aggregate_version: receipt.aggregate.version,
        event_id,
        // Projection is intentionally asynchronous. The Organization worker
        // consumes the committed outbox event and activates the generation;
        // bootstrap never dual-writes the disposable read model.
        read_projected: false,
    })
}

async fn initialize_branch_organization_rows(
    write_pool: &sqlx::PgPool,
    route: &BranchDatabaseRoute,
    job: &BranchProvisioningJob,
    worker_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let holding_uuid = route.holding_id;
    if holding_uuid.is_nil() {
        return Err(PersistenceError::InvalidData(
            "current branch bootstrap requires a non-nil holding_id".into(),
        ));
    }
    let correlation_uuid = if job.correlation_id.is_nil() {
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "erpx:branch-organization:correlation:{}:{}",
                route.holding_id, route.branch_id
            )
            .as_bytes(),
        )
    } else {
        job.correlation_id
    };
    // A branch's company and holding roots are authoritative nodes in the
    // same CQRS write plane.  Local read fixtures may already describe them,
    // but the atomic hierarchy guard still requires the write-side parents.
    create_bootstrap_organization_node(
        write_pool,
        route.holding_id,
        route.holding_id,
        OrganizationPlacement::Holding,
        route.data_profile_key.clone(),
        route.data_profile_key.clone(),
        format!("holding:{}", route.holding_id),
        correlation_uuid,
        worker_id,
        &route.requested_by,
        at,
    )
    .await?;
    create_bootstrap_organization_node(
        write_pool,
        route.holding_id,
        route.company_id,
        OrganizationPlacement::Company {
            holding_id: route.holding_id,
        },
        format!("COMPANY-{}", route.company_id.simple()),
        format!("Company {}", route.company_id.simple()),
        format!("company:{}:{}", route.holding_id, route.company_id),
        correlation_uuid,
        worker_id,
        &route.requested_by,
        at,
    )
    .await?;
    create_bootstrap_organization_node(
        write_pool,
        route.holding_id,
        route.branch_id,
        OrganizationPlacement::Branch {
            holding_id: route.holding_id,
            company_id: route.company_id,
        },
        job.branch_code.clone(),
        job.branch_name.clone(),
        format!("branch:{}:{}", route.holding_id, route.branch_id),
        correlation_uuid,
        worker_id,
        &route.requested_by,
        at,
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create_bootstrap_organization_node(
    write_pool: &sqlx::PgPool,
    holding_id: Uuid,
    node_uuid: Uuid,
    placement: OrganizationPlacement,
    code: String,
    name: String,
    command_key: String,
    correlation_uuid: Uuid,
    worker_id: Uuid,
    represented_owner_ref: &str,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let node_id = OrganizationNodeId::try_from_uuid(node_uuid)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let active_scope = AuthorityScope::Holding { holding_id };
    let worker_principal_ref = format!("erpx-worker:{worker_id}");
    let access = OrganizationBootstrapAccess {
        worker_principal_ref: worker_principal_ref.clone(),
        represented_owner_ref: represented_owner_ref.to_owned(),
        scope: active_scope.clone(),
    };
    let command_uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:organization-bootstrap:command:{command_key}").as_bytes(),
    );
    let target_scope = placement
        .active_scope(node_id)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let pool = WritePool::for_holding(write_pool.clone(), holding_id)
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    OrganizationCommandHandler::new(
        OrganizationWriteRepository::new(pool.clone()),
        OrganizationCommandAdapter::new(pool),
        access.clone(),
        access,
        OrganizationHierarchyRepository,
        FixedClock::new(at),
    )
    .handle(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(command_uuid)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        idempotency_key: IdempotencyKey::try_new(format!(
            "erpx:organization-bootstrap:{command_key}"
        ))
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        correlation_id: CorrelationId::try_from_uuid(correlation_uuid)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?,
        causation_id: None,
        actor: KernelActorAttribution {
            principal_ref: worker_principal_ref,
            kind: KernelActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: Some(represented_owner_ref.to_owned()),
            support_capability_id: None,
        },
        active_scope,
        audit: AuditContext {
            purpose: "organization.branch.bootstrap".into(),
            customer_visible: true,
            module: Some("organization".into()),
        },
        expected_aggregate_version: None,
        command: CreateOrganizationNode {
            node_id,
            placement,
            code,
            name,
        },
    })
    .await
    .map_err(|error| {
        PersistenceError::InvalidData(format!(
            "organization bootstrap CQRS failure for {target_scope:?}: {error:?}"
        ))
    })?;
    Ok(())
}

#[derive(Debug, Clone)]
struct OrganizationBootstrapAccess {
    worker_principal_ref: String,
    represented_owner_ref: String,
    scope: AuthorityScope,
}

#[async_trait]
impl OrganizationAuthorizationPort for OrganizationBootstrapAccess {
    async fn authorize(
        &self,
        request: AuthorizationRequest<'_>,
    ) -> Result<(), OrganizationPortError> {
        if request.actor.kind != KernelActorKind::SystemWorker
            || request.actor.principal_ref != self.worker_principal_ref
            || request.actor.represented_principal_ref.as_deref()
                != Some(self.represented_owner_ref.as_str())
            || request.authority_scope != &self.scope
            || !self.scope.contains(request.target_scope)
        {
            return Err(OrganizationPortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl OrganizationEntitlementPort for OrganizationBootstrapAccess {
    async fn ensure_organization_enabled(
        &self,
        scope: &AuthorityScope,
        _write: bool,
    ) -> Result<(), OrganizationPortError> {
        if self.scope.contains(scope) {
            Ok(())
        } else {
            Err(OrganizationPortError::EntitlementDenied)
        }
    }
}

/// Initialize the first holding organization using already-authenticated,
/// in-memory pools. This entry point exists for the installation CLI so its
/// credential-free database URL never has to be rendered into a DSN containing
/// a username or password.
pub async fn initialize_holding_organization_in_pools(
    write_pool: &sqlx::PgPool,
    read_pool: &sqlx::PgPool,
    holding_key: &str,
    data_profile_key: &str,
    holding_id: Uuid,
    owner_principal_ref: &str,
    at: DateTime<Utc>,
) -> Result<HoldingOrganizationBootstrap, PersistenceError> {
    validate_ref(holding_key, "holding_key")?;
    validate_ref(data_profile_key, "data_profile_key")?;
    validate_principal_ref(owner_principal_ref, "owner_principal_ref")?;
    if holding_id.is_nil() || holding_holding_scope_id(data_profile_key) != holding_id {
        return Err(PersistenceError::InvalidData(
            "organization holding ID does not match the holding reference".into(),
        ));
    }
    initialize_holding_organization_rows(
        write_pool,
        read_pool,
        holding_key,
        data_profile_key,
        holding_id,
        owner_principal_ref,
        at,
    )
    .await
}

fn holding_owner_seed_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<HoldingOwnerSeed, PersistenceError> {
    let data_profile_key: String = row.try_get("data_profile_key")?;
    let holding_id: Uuid = row.try_get("holding_id")?;
    Ok(HoldingOwnerSeed {
        holding_key: holding_id.to_string(),
        holding_id,
        data_profile_key,
        principal_ref: row.try_get("principal_ref")?,
        approver_ref: row.try_get("approver_ref")?,
        role_assignment_id: row.try_get("role_assignment_id")?,
        state: row.try_get("state")?,
        recovery_issued_at: row.try_get("recovery_issued_at")?,
    })
}

fn validate_database_key(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    validate_ref(value, field)?;
    if value.len() > 63
        || value
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-')))
    {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be a safe PostgreSQL database identifier"
        )));
    }
    Ok(())
}

fn validate_database_config(
    config: &HoldingDatabaseProvisioningConfig,
) -> Result<(), PersistenceError> {
    if config.admin_database_url.trim().is_empty()
        || config.database_url_template.matches("{database}").count() != 1
    {
        return Err(PersistenceError::Configuration(
            "holding database provisioning requires an admin URL and one {database} template token",
        ));
    }
    validate_tls_database_url(&config.admin_database_url, "holding admin database URL")?;
    let rendered_template = config
        .database_url_template
        .replace("{database}", "erpx_tls_validation_database");
    validate_tls_database_url(&rendered_template, "holding database URL template")?;
    database_owner_from_template(&config.database_url_template)?;
    Ok(())
}

fn database_profile_environment_token(value: &str) -> Result<String, PersistenceError> {
    if value.is_empty()
        || value.len() > 64
        || !value.starts_with(|character: char| character.is_ascii_lowercase())
        || value.chars().any(|character| {
            !(character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-'))
        })
    {
        return Err(PersistenceError::Configuration(
            "database credential profile key is invalid",
        ));
    }
    let mut token = String::with_capacity(value.len() * 2);
    for character in value.chars() {
        match character {
            '-' => token.push_str("__"),
            '_' => token.push('_'),
            value => token.push(value.to_ascii_uppercase()),
        }
    }
    Ok(token)
}

fn reject_database_profile_url_credentials(value: &str) -> Result<(), PersistenceError> {
    let rest = value
        .strip_prefix("postgresql://")
        .or_else(|| value.strip_prefix("postgres://"))
        .ok_or(PersistenceError::Configuration(
            "database profile URL must use postgresql://",
        ))?;
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    let lower = rest.to_ascii_lowercase();
    if authority.contains('@')
        || lower.contains("password=")
        || lower.contains("user=")
        || lower.contains("username=")
    {
        return Err(PersistenceError::Configuration(
            "database profile URL must not contain credentials",
        ));
    }
    Ok(())
}

fn database_owner_from_template(template: &str) -> Result<String, PersistenceError> {
    let rendered = template.replace("{database}", "erpx_owner_validation_database");
    let options = PgConnectOptions::from_str(&rendered).map_err(|_| {
        PersistenceError::Configuration("holding database URLs must be valid PostgreSQL URLs")
    })?;
    let owner = options.get_username();
    if owner.is_empty() || owner.len() > 63 || owner.chars().any(|character| character.is_control())
    {
        return Err(PersistenceError::Configuration(
            "holding database URL must identify a bounded database owner",
        ));
    }
    Ok(owner.to_owned())
}

fn validate_tls_database_url(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    let options = PgConnectOptions::from_str(value).map_err(|_| {
        PersistenceError::Configuration("holding database URLs must be valid PostgreSQL URLs")
    })?;
    let loopback = database_url_host(value).is_some_and(is_loopback_database_host);
    let transport_allowed = matches!(options.get_ssl_mode(), PgSslMode::VerifyFull)
        || (loopback && matches!(options.get_ssl_mode(), PgSslMode::Disable));
    if !transport_allowed {
        return Err(PersistenceError::Configuration(match field {
            "holding admin database URL" => {
                "holding admin database communication requires sslmode=verify-full outside loopback"
            }
            _ => "holding database communication requires sslmode=verify-full outside loopback",
        }));
    }
    Ok(())
}

fn database_url_host(value: &str) -> Option<&str> {
    let authority = value.split_once("://")?.1.split('/').next()?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host_port.starts_with('[') {
        return host_port
            .strip_prefix('[')
            .and_then(|value| value.split_once(']'))
            .map(|(host, _)| host);
    }
    Some(
        host_port
            .rsplit_once(':')
            .map_or(host_port, |(host, _)| host),
    )
}

fn is_loopback_database_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(crate) fn render_database_url(
    template: &str,
    database_key: &str,
) -> Result<String, PersistenceError> {
    validate_database_key(database_key, "database_key")?;
    Ok(template.replace("{database}", database_key))
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn is_duplicate_database(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(database) if database.code().as_deref() == Some("42P04"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_rebuild_activation_rejects_mismatched_proof_before_persistence() {
        let command_id = Uuid::from_u128(51);
        let decision = HumanDecisionProof::try_new(
            KernelActorAttribution {
                principal_ref: "platform-admin".into(),
                kind: KernelActorKind::PlatformUser,
                session_id: Some(
                    erpx_kernel::SessionId::try_from_uuid(Uuid::from_u128(52)).unwrap(),
                ),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Platform,
            projection_rebuild_activation_decision_ref(
                command_id,
                "holding-a",
                "operations",
                "read-candidate",
                9,
            ),
            DateTime::from_timestamp(1_785_000_000, 0).unwrap(),
        )
        .unwrap();
        assert!(
            validate_projection_rebuild_activation_decision(
                "holding-a",
                "operations",
                "read-candidate",
                10,
                command_id,
                &decision,
            )
            .is_err()
        );
    }

    #[test]
    fn item_directory_source_fence_is_exactly_holding_bound_and_typed() {
        let source = include_str!("provisioning.rs");
        let lock = source
            .split("impl<'a> HoldingSourceRebuildLock<'a>")
            .nth(1)
            .expect("source lock")
            .split("pub struct RoutedHoldingDatabase")
            .next()
            .expect("source lock boundary");
        assert!(lock.contains("set_config('app.holding_id', $1, true)"));
        assert!(lock.contains("bound_holding != holding_id"));
        assert!(lock.contains("holding_id = current_setting('app.holding_id')::uuid"));
        assert!(lock.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(!lock.contains("ITEM_DIRECTORY_FACT_EVENT_TYPE"));
        assert!(lock.contains("AUTHORITY_EVIDENCE_EVENT_TYPE"));
        assert!(lock.contains("ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE"));
        assert!(lock.contains("SNAPSHOT_EVENT_TYPE"));
        assert!(!lock.contains("available_at <= now()"));
        let table_lock = lock
            .find("LOCK TABLE erpx_cqrs_write.outbox_events IN SHARE MODE")
            .expect("Item Directory source table fence");
        let watermark = lock[table_lock..]
            .find("SELECT COALESCE(MAX(global_sequence), 0)")
            .expect("Item Directory source watermark");
        assert!(watermark > 0);
        assert!(!lock.contains("current_setting('app.holding_id', true)"));
    }

    #[test]
    fn operations_restore_fence_uses_the_normalized_outbox_position() {
        let source = include_str!("provisioning.rs");
        let lock = source
            .split("impl<'a> HoldingSourceRebuildLock<'a>")
            .nth(1)
            .expect("source lock")
            .split("pub struct RoutedHoldingDatabase")
            .next()
            .expect("source lock boundary");
        assert!(lock.contains("COALESCE(MAX(global_sequence), 0)"));
        assert!(lock.contains("context_name = 'operations'"));
        assert!(!lock.contains("operations_events"));
    }

    #[test]
    fn holding_routing_config_requires_one_database_template_token() {
        let config = HoldingDatabaseProvisioningConfig::from_values(
            "postgresql://admin@localhost/postgres?sslmode=verify-full",
            "postgresql://holding@localhost/{database}?sslmode=verify-full",
        )
        .expect("valid holding routing configuration");
        assert_eq!(
            config.database_url_template.matches("{database}").count(),
            1
        );
        assert_eq!(
            database_owner_from_template(&config.database_url_template).expect("database owner"),
            "holding"
        );

        assert!(
            HoldingDatabaseProvisioningConfig::from_values(
                "postgresql://admin@localhost/postgres?sslmode=verify-full",
                "postgresql://holding@localhost/holding?sslmode=verify-full",
            )
            .is_err()
        );
        assert!(
            HoldingDatabaseProvisioningConfig::from_values(
                "postgresql://admin@localhost/postgres?sslmode=verify-full",
                "postgresql://holding@localhost/{database}/{database}?sslmode=verify-full",
            )
            .is_err()
        );
    }

    #[test]
    fn approved_database_profile_keeps_credentials_out_of_urls_and_debug() {
        let profile = ApprovedDatabaseProfileConnection::from_values(
            "default",
            "postgresql://localhost/postgres?sslmode=disable",
            "erpx_owner",
            "not-rendered-password",
        )
        .expect("credential-free loopback profile");
        let debug = format!("{profile:?}");
        assert!(debug.contains("default"));
        assert!(!debug.contains("not-rendered-password"));
        assert!(
            ApprovedDatabaseProfileConnection::from_values(
                "default",
                "postgresql://embedded:password@localhost/postgres?sslmode=disable",
                "erpx_owner",
                "external-password",
            )
            .is_err()
        );
        assert_eq!(
            database_profile_environment_token("customer-prod").expect("environment token"),
            "CUSTOMER__PROD"
        );
    }

    #[test]
    fn holding_routing_config_allows_plaintext_only_on_loopback() {
        assert!(
            HoldingDatabaseProvisioningConfig::from_values(
                "postgres://admin@127.0.0.1/postgres?sslmode=disable",
                "postgres://holding@localhost/{database}?sslmode=disable",
            )
            .is_ok()
        );
        for (admin, template) in [
            (
                "postgres://admin@database.internal/postgres?sslmode=disable",
                "postgres://holding@database.internal/{database}?sslmode=disable",
            ),
            (
                "postgresql://admin@localhost/postgres?sslmode=require",
                "postgresql://holding@localhost/{database}?sslmode=require",
            ),
            (
                "postgresql://admin@localhost/postgres?sslmode=verify-full",
                "postgresql://holding@localhost/{database}?sslmode=prefer",
            ),
        ] {
            assert!(
                HoldingDatabaseProvisioningConfig::from_values(admin, template).is_err(),
                "insecure holding database URL must fail closed"
            );
        }
    }

    #[test]
    fn backup_job_request_is_bounded_redacted_idempotent_and_pitr_safe() {
        let request = HoldingBackupJobRequest {
            holding_id: "holding-test".into(),
            data_profile_key: "holding-test".into(),
            kind: "full".into(),
            backup_ref: "backup-secret-ref".into(),
            restore_target_ref: None,
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            pitr_recovery_point: None,
            available_at: Utc::now(),
            idempotency_key: "backup-idempotency-secret".into(),
            max_attempts: 3,
        };
        assert!(validate_backup_job_request(&request).is_ok());
        let debug = format!("{request:?}");
        assert!(!debug.contains("backup-secret-ref"));
        assert!(!debug.contains("backup-idempotency-secret"));
        let mut changed = request.clone();
        changed.backup_ref = "other-ref".into();
        assert_ne!(
            backup_job_request_hash(&request),
            backup_job_request_hash(&changed)
        );

        let mut pitr = request.clone();
        pitr.kind = "pitr".into();
        assert!(validate_backup_job_request(&pitr).is_err());
        pitr.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: Some(pitr.available_at),
            target_lsn: None,
            wal_archive_ref: "wal-archive".into(),
            physical_restore_target_ref: "physical-target".into(),
        });
        assert!(validate_backup_job_request(&pitr).is_ok());
        let mut restore = request;
        restore.kind = "restore_test".into();
        assert!(validate_backup_job_request(&restore).is_err());
    }

    #[test]
    fn owner_scope_and_assignment_ids_are_stable_and_namespaced() {
        let holding_id = holding_holding_scope_id("holding-opaque");
        assert_eq!(holding_id, holding_holding_scope_id("holding-opaque"));
        assert_ne!(holding_id, holding_holding_scope_id("holding-other"));
        assert_eq!(holding_id.get_version(), Some(uuid::Version::Random));
        let assignment_id = holding_owner_assignment_id("holding-a", "owner-a");
        assert_eq!(
            assignment_id,
            holding_owner_assignment_id("holding-a", "owner-a")
        );
        assert_ne!(
            assignment_id,
            holding_owner_assignment_id("holding-b", "owner-a")
        );
        let seed = HoldingOwnerSeed {
            holding_key: "holding-a".into(),
            data_profile_key: "holding-opaque".into(),
            holding_id,
            principal_ref: "owner-a".into(),
            approver_ref: "approver-a".into(),
            role_assignment_id: assignment_id,
            state: "pending".into(),
            recovery_issued_at: None,
        };
        let debug = format!("{seed:?}");
        assert!(!debug.contains("recovery_code"));
    }

    #[test]
    fn owner_principal_validation_accepts_namespaced_identity_refs() {
        assert!(validate_principal_ref("local:holding-owner-v2", "principal_ref").is_ok());
        assert!(validate_principal_ref("directory:holding:owner", "principal_ref").is_ok());
        assert!(validate_principal_ref("owner/ref", "principal_ref").is_err());
        assert!(validate_principal_ref("owner ref", "principal_ref").is_err());
    }

    #[test]
    fn holding_lifecycle_transition_matrix_is_closed() {
        assert!(lifecycle_transition_allowed(
            "provisioning",
            ProvisioningState::Active
        ));
        assert!(lifecycle_transition_allowed(
            "recovery",
            ProvisioningState::Active
        ));
        assert!(lifecycle_transition_allowed(
            "suspended",
            ProvisioningState::Active
        ));
        assert!(lifecycle_transition_allowed(
            "active",
            ProvisioningState::Suspended
        ));
        assert!(lifecycle_transition_allowed(
            "active",
            ProvisioningState::Recovery
        ));
        assert!(lifecycle_transition_allowed(
            "suspended",
            ProvisioningState::Recovery
        ));
        assert!(!lifecycle_transition_allowed(
            "active",
            ProvisioningState::Provisioning
        ));
        assert!(!lifecycle_transition_allowed(
            "provisioning",
            ProvisioningState::Suspended
        ));
        assert!(!lifecycle_transition_allowed(
            "failed",
            ProvisioningState::Active
        ));
    }

    fn started_run(kind: &str, restore_target_ref: Option<&str>) -> HoldingBackupRun {
        HoldingBackupRun {
            id: Uuid::from_u128(1),
            holding_id: "holding-test".into(),
            kind: kind.into(),
            status: "started".into(),
            backup_ref: "backup-test".into(),
            restore_target_ref: restore_target_ref.map(str::to_owned),
            pitr_recovery_point: (kind == "pitr").then(|| PitrRecoveryPoint {
                target_time: Some(Utc::now()),
                target_lsn: None,
                wal_archive_ref: "wal-archive-test".into(),
                physical_restore_target_ref: "physical-target-test".into(),
            }),
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        }
    }

    fn restore_schedule_request(
        holding_id: &str,
        schedule_id: u128,
        due_at: DateTime<Utc>,
        idempotency_key: &str,
        max_attempts: u16,
    ) -> RestoreTestScheduleRequest {
        let mut run = started_run("restore_test", Some("restore-target-secret"));
        run.id = Uuid::from_u128(schedule_id);
        run.holding_id = holding_id.into();
        run.backup_ref = format!("{holding_id}-backup-ref-secret");
        let databases = PhysicalHoldingDatabases::new(
            holding_id,
            format!("{holding_id}-write"),
            format!("{holding_id}-read"),
        )
        .expect("physical databases");
        let backup_target = HoldingBackupTarget::for_run(databases, "backup-chain-secret", &run)
            .expect("backup target");
        let replacement = backup_target
            .databases()
            .replacement_read_database(&format!("{holding_id}-replacement"))
            .expect("replacement read database");
        let target =
            HoldingRestoreTestTarget::new(backup_target, "restore-target-secret", replacement, 42)
                .expect("restore target");
        RestoreTestScheduleRequest::new(run, target, due_at, idempotency_key, max_attempts)
            .expect("restore schedule request")
    }

    #[test]
    fn restore_test_scheduler_is_holding_scoped_and_due_claims_are_idempotent() {
        let scheduler = RestoreTestScheduler::default();
        let now = DateTime::parse_from_rfc3339("2026-07-24T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&Utc);
        let future = restore_schedule_request(
            "holding-a",
            100,
            now + Duration::hours(1),
            "restore-idempotency-a",
            3,
        );
        let receipt = scheduler.schedule(future.clone()).expect("schedule");
        assert!(!receipt.duplicate());
        let duplicate = scheduler.schedule(future).expect("duplicate schedule");
        assert!(duplicate.duplicate());
        assert_eq!(receipt.schedule().id(), duplicate.schedule().id());
        assert!(
            scheduler
                .claim_due_for_holding("holding-b", Uuid::from_u128(9), now, 100)
                .expect("other holding claim")
                .is_empty()
        );
        assert!(
            scheduler
                .claim_due_for_holding("holding-a", Uuid::from_u128(9), now, 100)
                .expect("future claim")
                .is_empty()
        );

        let due = restore_schedule_request("holding-a", 101, now, "restore-idempotency-b", 3);
        let due_receipt = scheduler.schedule(due).expect("due schedule");
        let claims = scheduler
            .claim_due_for_holding("holding-a", Uuid::from_u128(9), now, 100)
            .expect("due claim");
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].schedule_id(), due_receipt.schedule().id());
        assert!(
            scheduler
                .claim_due_for_holding("holding-a", Uuid::from_u128(10), now, 100)
                .expect("duplicate claim")
                .is_empty()
        );
    }

    #[test]
    fn restore_test_scheduler_rejects_cross_holding_and_idempotency_conflicts() {
        let scheduler = RestoreTestScheduler::default();
        let now = Utc::now();
        let request = restore_schedule_request("holding-a", 102, now, "restore-conflict", 3);
        scheduler.schedule(request.clone()).expect("schedule");
        let conflicting_due = restore_schedule_request(
            "holding-a",
            103,
            now + Duration::minutes(1),
            "restore-conflict",
            3,
        );
        assert_eq!(
            scheduler.schedule(conflicting_due),
            Err(RestoreTestScheduleError::IdempotencyConflict)
        );

        let other_holding_request = restore_schedule_request("holding-b", 104, now, "other", 3);
        let other_target = other_holding_request.target().clone();
        let mut other_run = other_holding_request.run().clone();
        other_run.holding_id = "holding-c".into();
        assert!(matches!(
            RestoreTestScheduleRequest::new(other_run, other_target, now, "cross-holding", 3,),
            Err(RestoreTestScheduleError::InvalidRequest(_))
        ));
    }

    #[test]
    fn restore_test_scheduler_classifies_retry_failure_and_terminal_history() {
        let scheduler = RestoreTestScheduler::default();
        let now = DateTime::parse_from_rfc3339("2026-07-24T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&Utc);
        let receipt = scheduler
            .schedule(restore_schedule_request(
                "holding-a",
                105,
                now,
                "restore-retry",
                2,
            ))
            .expect("schedule");
        let worker = Uuid::from_u128(11);
        let first_claim = scheduler
            .claim_due_for_holding("holding-a", worker, now, 1)
            .expect("first claim")
            .pop()
            .expect("claim exists");
        let first = scheduler
            .record_execution(&first_claim, RestoreTestOutcome::Failed, false, now)
            .expect("retry result");
        assert_eq!(
            first.classification,
            RecoveryHistoryClassification::RetryableFailure
        );
        assert_eq!(first.state, RestoreTestScheduleState::Pending);
        let retry_at = first.next_due_at.expect("retry due time");
        assert!(
            scheduler
                .claim_due_for_holding("holding-a", worker, now, 1)
                .expect("not due claim")
                .is_empty()
        );
        let second_claim = scheduler
            .claim_due_for_holding("holding-a", worker, retry_at, 1)
            .expect("second claim")
            .pop()
            .expect("second claim exists");
        let terminal = scheduler
            .record_execution(&second_claim, RestoreTestOutcome::Expired, false, retry_at)
            .expect("terminal result");
        assert_eq!(
            terminal.classification,
            RecoveryHistoryClassification::TerminalFailure
        );
        assert_eq!(terminal.state, RestoreTestScheduleState::Failed);
        let history = scheduler
            .history_for_holding("holding-a", receipt.schedule().id())
            .expect("history");
        assert_eq!(
            history
                .iter()
                .map(|entry| entry.classification)
                .collect::<Vec<_>>(),
            vec![
                RecoveryHistoryClassification::Scheduled,
                RecoveryHistoryClassification::Claimed,
                RecoveryHistoryClassification::RetryableFailure,
                RecoveryHistoryClassification::Claimed,
                RecoveryHistoryClassification::Expired,
                RecoveryHistoryClassification::TerminalFailure,
            ]
        );
    }

    #[test]
    fn restore_test_scheduler_requires_replacement_read_verification_and_redacts_refs() {
        let scheduler = RestoreTestScheduler::default();
        let now = Utc::now();
        let receipt = scheduler
            .schedule(restore_schedule_request(
                "holding-a",
                106,
                now,
                "restore-verification-secret",
                2,
            ))
            .expect("schedule");
        let claim = scheduler
            .claim_due_for_holding("holding-a", Uuid::from_u128(12), now, 1)
            .expect("claim")
            .pop()
            .expect("claim exists");
        let unverified = scheduler
            .record_execution(&claim, RestoreTestOutcome::Verified, false, now)
            .expect("unverified replacement result");
        assert_eq!(
            unverified.classification,
            RecoveryHistoryClassification::ReplacementReadUnverified
        );
        assert_eq!(unverified.state, RestoreTestScheduleState::Pending);
        let retry_at = unverified.next_due_at.expect("retry due time");
        let retry_claim = scheduler
            .claim_due_for_holding("holding-a", Uuid::from_u128(12), retry_at, 1)
            .expect("replacement retry claim")
            .pop()
            .expect("replacement retry exists");
        let verified = scheduler
            .record_execution(&retry_claim, RestoreTestOutcome::Verified, true, retry_at)
            .expect("verified replacement result");
        assert_eq!(
            verified.classification,
            RecoveryHistoryClassification::Verified
        );
        assert_eq!(verified.state, RestoreTestScheduleState::Completed);

        let debug = format!("{:?}", receipt.schedule());
        assert!(!debug.contains("backup-ref-secret"));
        assert!(!debug.contains("restore-target-secret"));
        assert!(!debug.contains("restore-verification-secret"));
        assert!(!format!("{:?}", claim).contains("backup-ref-secret"));
    }

    #[test]
    fn backup_shape_validation_is_strict_about_status_and_targets() {
        assert!(validate_backup_run_shape(&started_run("full", None)).is_ok());
        assert!(validate_backup_run_shape(&started_run("pitr", None)).is_ok());
        assert!(
            validate_backup_run_shape(&started_run("restore_test", Some("restore-test"))).is_ok()
        );

        assert!(
            validate_backup_run_shape(&started_run("full", Some("unexpected-target"))).is_err()
        );
        assert!(validate_backup_run_shape(&started_run("restore_test", None)).is_err());
        assert!(
            validate_backup_run_shape(&started_run("restore_test", Some("backup-test"))).is_err()
        );

        let mut completed = started_run("restore_test", Some("restore-test"));
        completed.status = "completed".into();
        completed.completed_at = Some(Utc::now());
        assert!(validate_backup_run_shape(&completed).is_err());
        completed.verified = true;
        assert!(validate_backup_run_shape(&completed).is_ok());

        let mut reversed = started_run("full", None);
        reversed.status = "completed".into();
        reversed.completed_at = Some(reversed.started_at - Duration::seconds(1));
        reversed.verified = true;
        assert!(validate_backup_run_shape(&reversed).is_err());
    }

    #[test]
    fn health_details_are_bounded_structured_and_secret_free() {
        assert!(validate_health_details(&json!({"source": "probe"})).is_ok());
        assert!(validate_health_details(&json!(["probe"])).is_err());
        assert!(
            validate_health_details(&json!({
                "credential_ref": "vault://database"
            }))
            .is_err()
        );
        assert!(
            validate_health_details(&json!({
                "diagnostic": "x".repeat(MAX_HEALTH_DETAILS_BYTES)
            }))
            .is_err()
        );
    }

    #[tokio::test]
    async fn injected_executor_receives_physical_keys_without_external_services() {
        let registration = HoldingStoreRegistration {
            holding_id: "holding-test".into(),
            data_profile_key: "holding-test".into(),
            write_database_key: "holding-write-test".into(),
            read_database_key: "holding-read-test".into(),
            storage_namespace: "holding-storage-test".into(),
            credential_key_ref: "holding-key-test".into(),
            backup_chain_ref: "holding-backup-test".into(),
            state: "active".into(),
            registry_version: 1,
        };
        let run = started_run("restore_test", Some("restore-test"));
        let result = invoke_backup_executor(
            registration,
            run,
            Some("holding-read-replacement".into()),
            Some(42),
            |registration, run, candidate, source_version| async move {
                assert_eq!(registration.write_database_key, "holding-write-test");
                assert_eq!(registration.read_database_key, "holding-read-test");
                assert_eq!(registration.holding_id, run.holding_id);
                assert_eq!(candidate.as_deref(), Some("holding-read-replacement"));
                assert_eq!(source_version, Some(42));
                Ok((true, true))
            },
        )
        .await
        .expect("deterministic executor");
        assert_eq!(result, (true, true));
    }

    #[test]
    fn persisted_readiness_observations_fail_closed_for_missing_stale_and_future_data() {
        let now = DateTime::parse_from_rfc3339("2026-07-24T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&Utc);
        let policy = ReadinessObservationPolicy {
            database_max_age: Duration::minutes(5),
            storage_max_age: Duration::minutes(5),
            projection_max_age: Duration::minutes(5),
            backup_max_age: Duration::hours(24),
        };
        assert_eq!(
            ReadinessComponentObservation::not_probed().state,
            ReadinessObservationState::NotProbed
        );
        assert_eq!(
            classify_persisted_observation(
                true,
                now - Duration::minutes(6),
                now,
                policy.database_max_age,
            )
            .state,
            ReadinessObservationState::Unavailable
        );
        assert_eq!(
            classify_persisted_observation(
                true,
                now + Duration::seconds(1),
                now,
                policy.database_max_age,
            )
            .state,
            ReadinessObservationState::Unavailable
        );
        assert_eq!(
            classify_persisted_observation(false, now, now, policy.storage_max_age,).state,
            ReadinessObservationState::Unavailable
        );
    }

    #[test]
    fn projection_and_backup_observations_require_activated_recent_verified_state() {
        let now = DateTime::parse_from_rfc3339("2026-07-24T00:00:00Z")
            .expect("fixed timestamp")
            .with_timezone(&Utc);
        let policy = ReadinessObservationPolicy::default();
        let projection = projection_observation(
            "read-database",
            "read-database".into(),
            "activated".into(),
            42,
            false,
            Some(now),
            now,
            now,
            policy.projection_max_age,
        );
        assert_eq!(projection.state, ReadinessObservationState::Ready);
        let stale_projection = projection_observation(
            "read-database",
            "read-database".into(),
            "activated".into(),
            42,
            false,
            Some(now - Duration::minutes(6)),
            now - Duration::minutes(6),
            now,
            policy.projection_max_age,
        );
        assert_eq!(
            stale_projection.state,
            ReadinessObservationState::Unavailable
        );
        let backup = backup_observation(
            Uuid::from_u128(1),
            "pitr".into(),
            "completed".into(),
            now - Duration::minutes(1),
            Some(now),
            true,
            false,
            false,
            now,
            policy.backup_max_age,
        );
        assert_eq!(backup.state, ReadinessObservationState::Ready);
        let unverified = backup_observation(
            Uuid::from_u128(2),
            "pitr".into(),
            "completed".into(),
            now - Duration::minutes(1),
            Some(now),
            false,
            false,
            false,
            now,
            policy.backup_max_age,
        );
        assert_eq!(unverified.state, ReadinessObservationState::Unavailable);
    }

    #[test]
    fn readiness_models_exclude_credentials_and_external_references() {
        let metadata = HoldingReadinessMetadata {
            holding_id: "holding-safe".into(),
            registry_state: Some("active".into()),
            registry_version: Some(2),
            write_database: DatabaseReadinessObservation::not_probed(),
            read_database: DatabaseReadinessObservation::not_probed(),
            storage: ReadinessComponentObservation::not_probed(),
            projection_freshness: empty_projection_observation(),
            backup_pitr: empty_backup_observation(),
        };
        let debug = format!("{metadata:?}");
        assert!(!debug.contains("credential_key_ref"));
        assert!(!debug.contains("backup_ref"));
        assert!(!debug.contains("storage_namespace"));
        assert!(!debug.contains("database_key"));
    }
}
