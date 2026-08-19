//! Durable, secret-free branch database provisioning coordination.
//!
//! The control plane owns route identity, idempotency, lifecycle and health.
//! Connection URLs and credentials are resolved only by the provisioning
//! runtime and never appear in these public records.

use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use erpx_domain::data_plane::BusinessUnitId;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{PersistenceError, SqlxControlPlane, holding_holding_scope_id};

pub const BRANCH_INFRASTRUCTURE_PROVISION: &str = "branch.infrastructure.provision";
pub const MAX_BRANCH_PROVISIONING_ATTEMPTS: u32 = 10;
pub const BRANCH_PROVISIONING_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchProvisioningState {
    Queued,
    Provisioning,
    Verifying,
    Active,
    Failed,
    Suspended,
}

impl BranchProvisioningState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Provisioning => "provisioning",
            Self::Verifying => "verifying",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Suspended => "suspended",
        }
    }

    fn parse(value: &str) -> Result<Self, PersistenceError> {
        match value {
            "queued" => Ok(Self::Queued),
            "provisioning" => Ok(Self::Provisioning),
            "verifying" => Ok(Self::Verifying),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            "suspended" => Ok(Self::Suspended),
            _ => Err(PersistenceError::InvalidData(
                "stored branch provisioning state is invalid".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseProfileRegistration {
    pub profile_key: String,
    pub database_prefix: String,
    pub deployment_mode: String,
    pub credential_profile_key: String,
    pub state: String,
    pub profile_version: u64,
    pub created_by: String,
}

impl DatabaseProfileRegistration {
    pub fn validate(&self) -> Result<(), PersistenceError> {
        validate_profile_key(&self.profile_key)?;
        validate_database_prefix(&self.database_prefix)?;
        validate_ref(&self.credential_profile_key, "credential profile key", 255)?;
        validate_ref(&self.created_by, "database profile creator", 255)?;
        if !matches!(
            self.deployment_mode.as_str(),
            "erpx_managed" | "customer_data_plane"
        ) || !matches!(self.state.as_str(), "active" | "suspended")
            || self.profile_version == 0
        {
            return Err(PersistenceError::InvalidData(
                "database profile lifecycle is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionBranchRequest {
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub branch_code: String,
    pub branch_name: String,
    pub registration_number: String,
    pub profile_key: String,
    pub enabled_business_units: Vec<BusinessUnitId>,
    pub idempotency_key: String,
    pub requested_by: String,
    pub correlation_id: Uuid,
}

impl ProvisionBranchRequest {
    pub fn validate(&self) -> Result<(), PersistenceError> {
        validate_ref(&self.holding_key, "holding key", 128)?;
        validate_ref(&self.data_profile_key, "holding reference", 255)?;
        validate_ref(&self.branch_code, "branch code", 128)?;
        validate_ref(&self.branch_name, "branch name", 255)?;
        validate_ref(&self.idempotency_key, "idempotency key", 255)?;
        validate_ref(&self.requested_by, "requesting principal", 255)?;
        validate_profile_key(&self.profile_key)?;
        if self.holding_id.is_nil()
            || self.company_id.is_nil()
            || self.branch_id.is_nil()
            || self.correlation_id.is_nil()
            || self.registration_number.len() > 255
            || self.enabled_business_units.is_empty()
            || self.enabled_business_units.len() > BusinessUnitId::ALL.len()
        {
            return Err(PersistenceError::InvalidData(
                "branch provisioning request is invalid".into(),
            ));
        }
        let unique = self
            .enabled_business_units
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if unique.len() != self.enabled_business_units.len() {
            return Err(PersistenceError::InvalidData(
                "branch business units must be unique".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchDatabaseRoute {
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub profile_key: String,
    pub write_database_key: String,
    pub read_database_key: String,
    pub state: BranchProvisioningState,
    pub route_version: u64,
    pub failure_category: Option<String>,
    pub requested_by: String,
    pub requested_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchProvisioningJob {
    pub job_id: Uuid,
    pub holding_id: String,
    pub branch_id: Uuid,
    pub branch_code: String,
    pub branch_name: String,
    pub registration_number: String,
    pub enabled_business_units: Vec<BusinessUnitId>,
    pub idempotency_key: String,
    pub state: BranchProvisioningState,
    pub attempt_count: u32,
    pub correlation_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct BranchProvisioningClaim {
    pub job: BranchProvisioningJob,
    pub route: BranchDatabaseRoute,
    pub worker_id: Uuid,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxBranchProvisioningStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxBranchProvisioningStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    pub async fn register_profile(
        &self,
        profile: &DatabaseProfileRegistration,
    ) -> Result<(), PersistenceError> {
        profile.validate()?;
        let version = i64::try_from(profile.profile_version)
            .map_err(|_| PersistenceError::InvalidData("profile version overflow".into()))?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.database_profiles
                (profile_key, database_prefix, deployment_mode,
                 credential_profile_key, state, profile_version, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (profile_key) DO UPDATE SET updated_at=now()
             WHERE erpx_control.database_profiles.database_prefix=EXCLUDED.database_prefix
               AND erpx_control.database_profiles.deployment_mode=EXCLUDED.deployment_mode
               AND erpx_control.database_profiles.credential_profile_key=EXCLUDED.credential_profile_key
               AND erpx_control.database_profiles.state=EXCLUDED.state
               AND erpx_control.database_profiles.profile_version=EXCLUDED.profile_version
             RETURNING profile_key",
        )
        .bind(&profile.profile_key)
        .bind(&profile.database_prefix)
        .bind(&profile.deployment_mode)
        .bind(&profile.credential_profile_key)
        .bind(&profile.state)
        .bind(version)
        .bind(&profile.created_by)
        .fetch_optional(&self.control.pools().write)
        .await?;
        if result.is_none() {
            return Err(PersistenceError::InvalidData(
                "database profile already exists with different immutable configuration".into(),
            ));
        }
        Ok(())
    }

    pub async fn bind_profile(
        &self,
        holding_id: &str,
        profile_key: &str,
        bound_by: &str,
    ) -> Result<(), PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        validate_profile_key(profile_key)?;
        validate_ref(bound_by, "profile binding actor", 255)?;
        sqlx::query(
            "INSERT INTO erpx_control.holding_database_profile_bindings
                (holding_id, profile_key, active, bound_by)
             VALUES ($1,$2,TRUE,$3)
             ON CONFLICT (holding_id, profile_key) DO UPDATE
             SET active=TRUE, bound_by=EXCLUDED.bound_by, bound_at=now()",
        )
        .bind(holding_id)
        .bind(profile_key)
        .bind(bound_by)
        .execute(&self.control.pools().write)
        .await?;
        Ok(())
    }

    pub async fn approved_profile(
        &self,
        holding_id: &str,
        profile_key: &str,
    ) -> Result<DatabaseProfileRegistration, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        validate_profile_key(profile_key)?;
        let row = sqlx::query(
            "SELECT p.profile_key, p.database_prefix, p.deployment_mode,
                    p.credential_profile_key, p.state, p.profile_version, p.created_by
             FROM erpx_control.database_profiles p
             JOIN erpx_control.holding_database_profile_bindings b
               ON b.profile_key=p.profile_key AND b.holding_id=$1
             WHERE p.profile_key=$2 AND p.state='active' AND b.active",
        )
        .bind(holding_id)
        .bind(profile_key)
        .fetch_optional(&self.control.pools().write)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("database profile is not approved".into()))?;
        let profile_version = u64::try_from(row.try_get::<i64, _>("profile_version")?)
            .map_err(|_| PersistenceError::InvalidData("profile version overflow".into()))?;
        let profile = DatabaseProfileRegistration {
            profile_key: row.try_get("profile_key")?,
            database_prefix: row.try_get("database_prefix")?,
            deployment_mode: row.try_get("deployment_mode")?,
            credential_profile_key: row.try_get("credential_profile_key")?,
            state: row.try_get("state")?,
            profile_version,
            created_by: row.try_get("created_by")?,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub async fn enqueue(
        &self,
        request: &ProvisionBranchRequest,
    ) -> Result<BranchProvisioningJob, PersistenceError> {
        request.validate()?;
        let mut tx = self.control.pools().write.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!(
                "branch-route:{}:{}",
                request.holding_id, request.branch_id
            ))
            .execute(&mut *tx)
            .await?;
        let holding = sqlx::query(
            "SELECT holding_id, data_profile_key, state, isolation_mode, write_database_key, read_database_key
             FROM erpx_control.holding_registry
             WHERE holding_id=$1 OR data_profile_key=$2
             FOR UPDATE",
        )
        .bind(request.holding_id.to_string())
        .bind(&request.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("holding is not registered".into()))?;
        // The request boundary is canonical UUID based, while the registry
        // remains the durable owner of the route key.  Keep using the exact
        // registry key for all FK-backed writes so retained local fixtures
        // (and any future opaque registry labels) cannot drift from the
        // holding/data-profile pair selected above.
        let registry_holding_id: String = holding.try_get("holding_id")?;
        let data_profile_key: String = holding.try_get("data_profile_key")?;
        let holding_state: String = holding.try_get("state")?;
        let isolation_mode: String = holding.try_get("isolation_mode")?;
        if data_profile_key != request.data_profile_key
            || holding_holding_scope_id(&data_profile_key) != request.holding_id
            || holding_state != "active"
            || isolation_mode != "holding_and_branch_pairs"
        {
            return Err(PersistenceError::InvalidData(
                "holding is not eligible for branch database provisioning".into(),
            ));
        }
        let profile = sqlx::query(
            "SELECT p.database_prefix
             FROM erpx_control.database_profiles p
             JOIN erpx_control.holding_database_profile_bindings b
               ON b.profile_key=p.profile_key AND b.holding_id=$1
             WHERE p.profile_key=$2 AND p.state='active' AND b.active",
        )
        .bind(&registry_holding_id)
        .bind(&request.profile_key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("database profile is not approved".into()))?;
        let database_prefix: String = profile.try_get("database_prefix")?;
        let (write_database_key, read_database_key) =
            branch_database_keys(&database_prefix, &request.holding_id, request.branch_id)?;
        let holding_write: String = holding.try_get("write_database_key")?;
        let holding_read: String = holding.try_get("read_database_key")?;
        if [write_database_key.as_str(), read_database_key.as_str()]
            .iter()
            .any(|key| *key == holding_write || *key == holding_read)
        {
            return Err(PersistenceError::InvalidData(
                "branch database key collides with the holding route".into(),
            ));
        }
        let route = sqlx::query(
            "INSERT INTO erpx_control.branch_database_routes
                (holding_id, data_profile_key, company_id, branch_id,
                 profile_key, write_database_key, read_database_key, state,
                 route_version, requested_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,'queued',1,$8)
             ON CONFLICT (holding_id, branch_id) DO UPDATE SET updated_at=now()
             WHERE erpx_control.branch_database_routes.data_profile_key=EXCLUDED.data_profile_key
               AND erpx_control.branch_database_routes.company_id=EXCLUDED.company_id
               AND erpx_control.branch_database_routes.profile_key=EXCLUDED.profile_key
               AND erpx_control.branch_database_routes.write_database_key=EXCLUDED.write_database_key
               AND erpx_control.branch_database_routes.read_database_key=EXCLUDED.read_database_key
             RETURNING holding_id",
        )
        .bind(&registry_holding_id)
        .bind(&request.data_profile_key)
        .bind(request.company_id)
        .bind(request.branch_id)
        .bind(&request.profile_key)
        .bind(&write_database_key)
        .bind(&read_database_key)
        .bind(&request.requested_by)
        .fetch_optional(&mut *tx)
        .await?;
        if route.is_none() {
            return Err(PersistenceError::InvalidData(
                "branch route already exists with different immutable configuration".into(),
            ));
        }
        let job_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "erpx:branch-provisioning:{}:{}",
                request.holding_id, request.idempotency_key
            )
            .as_bytes(),
        );
        let business_units = request
            .enabled_business_units
            .iter()
            .map(|unit| unit.as_str().to_owned())
            .collect::<Vec<_>>();
        let row = sqlx::query(
            "INSERT INTO erpx_control.branch_provisioning_jobs
                (job_id, holding_id, branch_id, idempotency_key, branch_code,
                 branch_name, registration_number, enabled_business_units,
                 state, correlation_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'queued',$9)
             ON CONFLICT (holding_id, idempotency_key) DO UPDATE SET updated_at=now()
             WHERE erpx_control.branch_provisioning_jobs.job_id=EXCLUDED.job_id
               AND erpx_control.branch_provisioning_jobs.branch_id=EXCLUDED.branch_id
               AND erpx_control.branch_provisioning_jobs.branch_code=EXCLUDED.branch_code
               AND erpx_control.branch_provisioning_jobs.branch_name=EXCLUDED.branch_name
               AND erpx_control.branch_provisioning_jobs.registration_number=EXCLUDED.registration_number
               AND erpx_control.branch_provisioning_jobs.enabled_business_units=EXCLUDED.enabled_business_units
             RETURNING job_id, holding_id, branch_id, idempotency_key, branch_code,
                       branch_name, registration_number, enabled_business_units,
                       state, attempt_count, correlation_id, created_at, updated_at, completed_at",
        )
        .bind(job_id)
        .bind(&registry_holding_id)
        .bind(request.branch_id)
        .bind(&request.idempotency_key)
        .bind(&request.branch_code)
        .bind(&request.branch_name)
        .bind(&request.registration_number)
        .bind(&business_units)
        .bind(request.correlation_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "branch idempotency key was reused with a different request".into(),
            )
        })?;
        tx.commit().await?;
        branch_job_from_row(&row)
    }

    pub async fn get_job(
        &self,
        holding_id: &str,
        job_id: Uuid,
    ) -> Result<Option<BranchProvisioningJob>, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        let row = sqlx::query(
            "SELECT job_id, holding_id, branch_id, idempotency_key, branch_code,
                    branch_name, registration_number, enabled_business_units,
                    state, attempt_count, correlation_id, created_at, updated_at, completed_at
             FROM erpx_control.branch_provisioning_jobs
             WHERE holding_id=$1 AND job_id=$2",
        )
        .bind(holding_id)
        .bind(job_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        row.as_ref().map(branch_job_from_row).transpose()
    }

    pub async fn list_jobs(
        &self,
        holding_id: &str,
        page_size: u32,
    ) -> Result<Vec<BranchProvisioningJob>, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        if !(1..=100).contains(&page_size) {
            return Err(PersistenceError::InvalidData(
                "branch provisioning page size must be 1..=100".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT job_id, holding_id, branch_id, idempotency_key, branch_code,
                    branch_name, registration_number, enabled_business_units,
                    state, attempt_count, correlation_id, created_at, updated_at, completed_at
             FROM erpx_control.branch_provisioning_jobs
             WHERE holding_id=$1
             ORDER BY created_at DESC, job_id DESC
             LIMIT $2",
        )
        .bind(holding_id)
        .bind(i64::from(page_size))
        .fetch_all(&self.control.pools().write)
        .await?;
        rows.iter().map(branch_job_from_row).collect()
    }

    pub async fn retry(
        &self,
        holding_id: &str,
        job_id: Uuid,
        correlation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<BranchProvisioningJob, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        if job_id.is_nil() || correlation_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "branch retry identifiers are invalid".into(),
            ));
        }
        let mut tx = self.control.pools().write.begin().await?;
        let row = sqlx::query(
            "UPDATE erpx_control.branch_provisioning_jobs
             SET state='queued', claimed_by=NULL, claimed_at=NULL, lease_expires_at=NULL,
                 next_attempt_at=$3, failure_category=NULL, correlation_id=$4,
                 completed_at=NULL, updated_at=$3
             WHERE holding_id=$1 AND job_id=$2 AND state='failed'
               AND attempt_count < $5
             RETURNING job_id, holding_id, branch_id, idempotency_key, branch_code,
                       branch_name, registration_number, enabled_business_units,
                       state, attempt_count, correlation_id, created_at, updated_at, completed_at",
        )
        .bind(holding_id)
        .bind(job_id)
        .bind(now)
        .bind(correlation_id)
        .bind(i32::try_from(MAX_BRANCH_PROVISIONING_ATTEMPTS).expect("bounded attempts"))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData("failed branch provisioning job is not retryable".into())
        })?;
        let job = branch_job_from_row(&row)?;
        let route = sqlx::query(
            "UPDATE erpx_control.branch_database_routes
             SET state='queued', failure_category=NULL, activated_at=NULL,
                 suspended_at=NULL, route_version=route_version+1, updated_at=$3
             WHERE holding_id=$1 AND branch_id=$2 AND state='failed'",
        )
        .bind(holding_id)
        .bind(job.branch_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        if route.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "failed branch route is not retryable".into(),
            ));
        }
        tx.commit().await?;
        Ok(job)
    }

    pub async fn suspend(
        &self,
        holding_id: &str,
        branch_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<BranchDatabaseRoute, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        if branch_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "branch ID cannot be nil".into(),
            ));
        }
        let mut tx = self.control.pools().write.begin().await?;
        let route = sqlx::query(
            "UPDATE erpx_control.branch_database_routes
             SET state='suspended', suspended_at=$3, activated_at=NULL,
                 failure_category=NULL, route_version=route_version+1, updated_at=$3
             WHERE holding_id=$1 AND branch_id=$2 AND state='active'
             RETURNING holding_id, data_profile_key, company_id, branch_id,
                       profile_key, write_database_key, read_database_key, state,
                       route_version, failure_category, requested_by, requested_at, activated_at",
        )
        .bind(holding_id)
        .bind(branch_id)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("active branch route was not found".into()))?;
        sqlx::query(
            "UPDATE erpx_control.branch_provisioning_jobs
             SET state='suspended', completed_at=NULL, updated_at=$3
             WHERE holding_id=$1 AND branch_id=$2 AND state='active'",
        )
        .bind(holding_id)
        .bind(branch_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let route = branch_route_from_row(&route)?;
        tx.commit().await?;
        Ok(route)
    }

    pub async fn route(
        &self,
        holding_id: &str,
        branch_id: Uuid,
    ) -> Result<Option<BranchDatabaseRoute>, PersistenceError> {
        validate_ref(holding_id, "holding ID", 128)?;
        let row = sqlx::query(
            "SELECT holding_id, data_profile_key, company_id, branch_id,
                    profile_key, write_database_key, read_database_key, state,
                    route_version, failure_category, requested_by, requested_at, activated_at
             FROM erpx_control.branch_database_routes
             WHERE holding_id=$1 AND branch_id=$2",
        )
        .bind(holding_id)
        .bind(branch_id)
        .fetch_optional(&self.control.pools().write)
        .await?;
        row.as_ref().map(branch_route_from_row).transpose()
    }

    pub async fn claim_next(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<BranchProvisioningClaim>, PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "branch provisioning worker ID cannot be nil".into(),
            ));
        }
        let lease_expires_at = now + Duration::seconds(BRANCH_PROVISIONING_LEASE_SECONDS);
        let mut tx = self.control.pools().write.begin().await?;
        let row = sqlx::query(
            "SELECT job_id
             FROM erpx_control.branch_provisioning_jobs
             WHERE state='queued'
               AND completed_at IS NULL
               AND next_attempt_at <= $1
               AND attempt_count < $2
               AND (lease_expires_at IS NULL OR lease_expires_at <= $1)
             ORDER BY next_attempt_at, created_at, job_id
             LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .bind(i32::try_from(MAX_BRANCH_PROVISIONING_ATTEMPTS).expect("bounded attempts"))
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let job_id: Uuid = row.try_get("job_id")?;
        let row = sqlx::query(
            "UPDATE erpx_control.branch_provisioning_jobs
             SET state='provisioning', attempt_count=attempt_count+1,
                 claimed_by=$2, claimed_at=$3, lease_expires_at=$4,
                 failure_category=NULL, updated_at=$3
             WHERE job_id=$1
             RETURNING job_id, holding_id, branch_id, idempotency_key, branch_code,
                       branch_name, registration_number, enabled_business_units,
                       state, attempt_count, correlation_id, created_at, updated_at, completed_at",
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(now)
        .bind(lease_expires_at)
        .fetch_one(&mut *tx)
        .await?;
        let job = branch_job_from_row(&row)?;
        sqlx::query(
            "UPDATE erpx_control.branch_database_routes
             SET state='provisioning', failure_category=NULL,
                 route_version=route_version+1, activated_at=NULL,
                 suspended_at=NULL, updated_at=$3
             WHERE holding_id=$1 AND branch_id=$2 AND state='queued'",
        )
        .bind(&job.holding_id)
        .bind(job.branch_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let route_row = sqlx::query(
            "SELECT holding_id, data_profile_key, company_id, branch_id,
                    profile_key, write_database_key, read_database_key, state,
                    route_version, failure_category, requested_by, requested_at, activated_at
             FROM erpx_control.branch_database_routes
             WHERE holding_id=$1 AND branch_id=$2",
        )
        .bind(&job.holding_id)
        .bind(job.branch_id)
        .fetch_one(&mut *tx)
        .await?;
        let route = branch_route_from_row(&route_row)?;
        tx.commit().await?;
        Ok(Some(BranchProvisioningClaim {
            job,
            route,
            worker_id,
            lease_expires_at,
        }))
    }

    pub async fn mark_verifying(
        &self,
        claim: &BranchProvisioningClaim,
        now: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.transition_claim(claim, "verifying", None, now, false)
            .await
    }

    pub async fn complete(
        &self,
        claim: &BranchProvisioningClaim,
        now: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.transition_claim(claim, "active", None, now, true)
            .await
    }

    pub async fn fail(
        &self,
        claim: &BranchProvisioningClaim,
        failure_category: &str,
        now: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_failure_category(failure_category)?;
        self.transition_claim(claim, "failed", Some(failure_category), now, false)
            .await
    }

    async fn transition_claim(
        &self,
        claim: &BranchProvisioningClaim,
        target: &str,
        failure_category: Option<&str>,
        now: DateTime<Utc>,
        complete: bool,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.control.pools().write.begin().await?;
        let result = sqlx::query(
            "UPDATE erpx_control.branch_provisioning_jobs
             SET state=$4, failure_category=$5,
                 claimed_by=CASE WHEN $4='verifying' THEN claimed_by ELSE NULL END,
                 claimed_at=CASE WHEN $4='verifying' THEN claimed_at ELSE NULL END,
                 lease_expires_at=CASE WHEN $4='verifying' THEN lease_expires_at ELSE NULL END,
                 next_attempt_at=CASE WHEN $4='failed' THEN $6 ELSE next_attempt_at END,
                 completed_at=CASE WHEN $7 THEN $3 ELSE NULL END,
                 updated_at=$3
             WHERE job_id=$1 AND claimed_by=$2 AND state IN ('provisioning','verifying')",
        )
        .bind(claim.job.job_id)
        .bind(claim.worker_id)
        .bind(now)
        .bind(target)
        .bind(failure_category)
        .bind(now + Duration::seconds(30))
        .bind(complete)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "branch provisioning claim is stale".into(),
            ));
        }
        sqlx::query(
            "UPDATE erpx_control.branch_database_routes
             SET state=$3, failure_category=$4,
                 route_version=route_version+1,
                 activated_at=CASE WHEN $3='active' THEN $5 ELSE NULL END,
                 suspended_at=NULL, updated_at=$5
             WHERE holding_id=$1 AND branch_id=$2 AND state IN ('provisioning','verifying')",
        )
        .bind(&claim.job.holding_id)
        .bind(claim.job.branch_id)
        .bind(target)
        .bind(failure_category)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

pub fn branch_database_keys(
    prefix: &str,
    holding_id: &Uuid,
    branch_id: Uuid,
) -> Result<(String, String), PersistenceError> {
    validate_database_prefix(prefix)?;
    if holding_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "holding ID is invalid".into(),
        ));
    }
    if branch_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "branch ID cannot be nil".into(),
        ));
    }
    let holding_token = digest_token(holding_id.as_bytes(), 10);
    let branch_token = branch_id.simple().to_string()[..12].to_owned();
    let base = format!("{prefix}_{holding_token}_{branch_token}");
    let write = format!("{base}_write");
    let read = format!("{base}_read");
    if write.len() > 63 || read.len() > 63 {
        return Err(PersistenceError::InvalidData(
            "derived branch database key exceeds PostgreSQL limits".into(),
        ));
    }
    Ok((write, read))
}

fn branch_job_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<BranchProvisioningJob, PersistenceError> {
    let units: Vec<String> = row.try_get("enabled_business_units")?;
    let enabled_business_units = units
        .iter()
        .map(|unit| {
            BusinessUnitId::parse(unit).map_err(|_| {
                PersistenceError::InvalidData("stored branch business unit is not canonical".into())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BranchProvisioningJob {
        job_id: row.try_get("job_id")?,
        holding_id: row.try_get("holding_id")?,
        branch_id: row.try_get("branch_id")?,
        branch_code: row.try_get("branch_code")?,
        branch_name: row.try_get("branch_name")?,
        registration_number: row.try_get("registration_number")?,
        enabled_business_units,
        idempotency_key: row.try_get("idempotency_key")?,
        state: BranchProvisioningState::parse(&row.try_get::<String, _>("state")?)?,
        attempt_count: u32::try_from(row.try_get::<i32, _>("attempt_count")?)
            .map_err(|_| PersistenceError::InvalidData("stored attempt count is invalid".into()))?,
        correlation_id: row.try_get("correlation_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

fn branch_route_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<BranchDatabaseRoute, PersistenceError> {
    let data_profile_key: String = row.try_get("data_profile_key")?;
    Ok(BranchDatabaseRoute {
        holding_key: row.try_get("holding_id")?,
        holding_id: holding_holding_scope_id(&data_profile_key),
        data_profile_key,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        profile_key: row.try_get("profile_key")?,
        write_database_key: row.try_get("write_database_key")?,
        read_database_key: row.try_get("read_database_key")?,
        state: BranchProvisioningState::parse(&row.try_get::<String, _>("state")?)?,
        route_version: u64::try_from(row.try_get::<i64, _>("route_version")?)
            .map_err(|_| PersistenceError::InvalidData("stored route version is invalid".into()))?,
        failure_category: row.try_get("failure_category")?,
        requested_by: row.try_get("requested_by")?,
        requested_at: row.try_get("requested_at")?,
        activated_at: row.try_get("activated_at")?,
    })
}

fn validate_profile_key(value: &str) -> Result<(), PersistenceError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' => true,
            b'0'..=b'9' | b'_' | b'-' => index > 0,
            _ => false,
        });
    if !valid {
        return Err(PersistenceError::InvalidData(
            "database profile key is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_database_prefix(value: &str) -> Result<(), PersistenceError> {
    let valid = (2..=16).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' => true,
            b'0'..=b'9' => index > 0,
            b'_' => index > 0,
            _ => false,
        });
    if !valid {
        return Err(PersistenceError::InvalidData(
            "database prefix must be 2-16 lowercase ASCII characters".into(),
        ));
    }
    Ok(())
}

fn validate_ref(value: &str, field: &'static str, maximum: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_failure_category(value: &str) -> Result<(), PersistenceError> {
    validate_ref(value, "branch provisioning failure category", 128)?;
    let normalized = value.to_ascii_lowercase();
    if [
        "password",
        "secret",
        "token",
        "credential",
        "postgres://",
        "postgresql://",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
    {
        return Err(PersistenceError::InvalidData(
            "branch failure category contains forbidden material".into(),
        ));
    }
    Ok(())
}

fn digest_token(value: &[u8], length: usize) -> String {
    let digest = Sha256::digest(value);
    hex::encode(digest)[..length].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_keys_are_bounded_deterministic_and_separate() {
        let branch_id = Uuid::from_u128(42);
        let holding_id = Uuid::from_u128(41);
        let first = branch_database_keys("erpx", &holding_id, branch_id).expect("keys");
        let second = branch_database_keys("erpx", &holding_id, branch_id).expect("keys");
        assert_eq!(first, second);
        assert_ne!(first.0, first.1);
        assert!(first.0.len() <= 63);
        assert!(first.1.len() <= 63);
        assert!(!format!("{first:?}").contains("postgres://"));
    }

    #[test]
    fn request_rejects_duplicate_business_units_and_nil_scope() {
        let unit = BusinessUnitId::ALL[0];
        let request = ProvisionBranchRequest {
            holding_key: "holding".into(),
            data_profile_key: "holding".into(),
            holding_id: Uuid::nil(),
            company_id: Uuid::new_v4(),
            branch_id: Uuid::new_v4(),
            branch_code: "B1".into(),
            branch_name: "Branch".into(),
            registration_number: String::new(),
            profile_key: "default".into(),
            enabled_business_units: vec![unit, unit],
            idempotency_key: "idem".into(),
            requested_by: "owner".into(),
            correlation_id: Uuid::new_v4(),
        };
        assert!(request.validate().is_err());
    }
}
