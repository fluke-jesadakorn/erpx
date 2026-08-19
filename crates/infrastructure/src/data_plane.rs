//! Persistence for customer-hosted data-plane configuration and Root access.
//!
//! This adapter deliberately accepts only hashes, fingerprints, and validated
//! secret-manager references. Raw database, directory, storage, enrollment, or
//! capability credentials have no persistence API in this module.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    contracts::AuthorityScope as Scope,
    data_plane::{
        ApprovalPolicy, BusinessUnitConfiguration, BusinessUnitDirectoryRoleMapping,
        BusinessUnitId, ConfigurationLayer, ConfigurationLifecycle, DataPlaneAgentBinding,
        DataPlaneAgentState, DataPlaneContractError, DesiredConfiguration, HoldingDeploymentMode,
        HoldingExecutionRoute, HoldingInfrastructureProfile, ObservedDataPlaneState,
        RootAccessApproval, RootAccessCapability, RootAccessMode, RootAccessRequest,
    },
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{PersistenceError, SqlxControlPlane};

const MAX_CONFIGURATION_BYTES: usize = 64 * 1024;
const MAX_CONFIGURATION_DEPTH: usize = 16;
const MAX_ENROLLMENT_LIFETIME: Duration = Duration::minutes(30);

#[derive(Clone)]
pub struct DataPlaneEnrollmentIssue {
    pub enrollment_id: Uuid,
    pub holding_id: String,
    pub canonical_holding_id: Uuid,
    /// SHA-256 of the one-time bootstrap token. Never the bearer token.
    pub token_hash: [u8; 32],
    /// SHA-256 of the signed installation manifest.
    pub manifest_hash: [u8; 32],
    pub issued_by: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub correlation_id: Uuid,
}

#[derive(Clone)]
pub struct DataPlaneEnrollmentConsumption {
    pub enrollment_id: Uuid,
    pub presented_token_hash: [u8; 32],
    pub presented_manifest_hash: [u8; 32],
    pub binding: DataPlaneAgentBinding,
    pub agent_version: String,
    pub certificate_expires_at: DateTime<Utc>,
    pub enrolled_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct RootAccessGrant {
    pub approval: RootAccessApproval,
    pub capability_id: Uuid,
    /// SHA-256 of the one-time capability bearer value.
    pub capability_token_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootAccessQueueRecord {
    pub request: RootAccessRequest,
    pub state: String,
    pub customer_approver_ref: Option<String>,
    pub customer_approved_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingInfrastructureSummary {
    pub holding_id: String,
    pub canonical_holding_id: Uuid,
    pub deployment_mode: HoldingDeploymentMode,
    pub profile_version: u64,
    pub agent_id: Option<Uuid>,
    pub agent_state: Option<DataPlaneAgentState>,
    pub agent_version: Option<String>,
    pub observed_version: Option<u64>,
    pub desired_version: Option<u64>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub observed_at: Option<DateTime<Utc>>,
    pub configuration_drifted: bool,
    pub write_healthy: Option<bool>,
    pub read_healthy: Option<bool>,
    pub storage_healthy: Option<bool>,
    pub directory_ready: Option<bool>,
    pub projections_healthy: Option<bool>,
    pub backup_verified: Option<bool>,
    pub root_access_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovedDesiredConfiguration {
    pub desired: DesiredConfiguration,
    pub desired_state: Value,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct SqlxDataPlaneControlStore {
    control: Arc<SqlxControlPlane>,
}

impl SqlxDataPlaneControlStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self { control }
    }

    /// Save logical unit configuration inside the holding boundary. Physical
    /// database, storage, and connector credentials are intentionally absent
    /// from this contract.
    pub async fn save_business_unit_configuration(
        &self,
        configuration: &BusinessUnitConfiguration,
        expected_version: u64,
        correlation_id: Uuid,
    ) -> Result<(), PersistenceError> {
        configuration.validate().map_err(contract_error)?;
        validate_correlation_id(correlation_id)?;
        if configuration.configuration_version != expected_version.saturating_add(1) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "business-unit configuration".into(),
                aggregate_id: configuration_id(
                    &configuration.holding_id,
                    configuration.business_unit,
                ),
                expected: Some(expected_version.saturating_add(1)),
                actual: Some(configuration.configuration_version),
            });
        }
        validate_desired_state(&configuration.retention_policy)?;
        validate_desired_state(&configuration.projection_policy)?;
        validate_desired_state(&configuration.workflow_policy)?;
        let organization_scope = serde_json::to_value(&configuration.organization_scope)?;
        let directory_role_mappings = serde_json::to_value(&configuration.directory_role_mappings)?;
        let mut tx = self.control.pools.write.begin().await?;
        let saved = sqlx::query(
            "INSERT INTO erpx_control.business_unit_configurations
                (holding_id, canonical_holding_id, business_unit, configuration_version,
                 active, organization_scope, directory_role_mappings,
                 integration_references, retention_policy, projection_policy,
                 workflow_policy, updated_by, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (holding_id, business_unit) DO UPDATE SET
                 canonical_holding_id = EXCLUDED.canonical_holding_id,
                 configuration_version = EXCLUDED.configuration_version,
                 active = EXCLUDED.active,
                 organization_scope = EXCLUDED.organization_scope,
                 directory_role_mappings = EXCLUDED.directory_role_mappings,
                 integration_references = EXCLUDED.integration_references,
                 retention_policy = EXCLUDED.retention_policy,
                 projection_policy = EXCLUDED.projection_policy,
                 workflow_policy = EXCLUDED.workflow_policy,
                 updated_by = EXCLUDED.updated_by,
                 updated_at = EXCLUDED.updated_at
             WHERE erpx_control.business_unit_configurations.configuration_version = $14
             RETURNING configuration_version",
        )
        .bind(configuration.holding_id)
        .bind(configuration.holding_id)
        .bind(configuration.business_unit.as_str())
        .bind(checked_i64(
            configuration.configuration_version,
            "business-unit configuration version",
        )?)
        .bind(configuration.active)
        .bind(organization_scope)
        .bind(directory_role_mappings)
        .bind(&configuration.integration_references)
        .bind(&configuration.retention_policy)
        .bind(&configuration.projection_policy)
        .bind(&configuration.workflow_policy)
        .bind(&configuration.updated_by)
        .bind(configuration.updated_at)
        .bind(checked_i64(
            expected_version,
            "expected configuration version",
        )?)
        .fetch_optional(&mut *tx)
        .await?;
        if saved.is_none() {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "business-unit configuration".into(),
                aggregate_id: configuration_id(
                    &configuration.holding_id,
                    configuration.business_unit,
                ),
                expected: Some(expected_version),
                actual: None,
            });
        }
        append_control_event(
            &mut tx,
            "business_unit.configuration.saved",
            "business_unit_configuration",
            configuration_id(&configuration.holding_id, configuration.business_unit),
            configuration.configuration_version,
            &configuration.holding_id,
            configuration.holding_id,
            &configuration.updated_by,
            "versioned business-unit configuration update",
            correlation_id,
            json!({
                "holding_id": configuration.holding_id,
                "business_unit": configuration.business_unit.as_str(),
                "configuration_version": configuration.configuration_version,
                "active": configuration.active,
            }),
            configuration.updated_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn business_unit_configuration(
        &self,
        holding_id: &str,
        business_unit: BusinessUnitId,
    ) -> Result<Option<BusinessUnitConfiguration>, PersistenceError> {
        validate_reference(holding_id, "holding_id", 128)?;
        let row = sqlx::query(
            "SELECT canonical_holding_id, configuration_version, active,
                    organization_scope, directory_role_mappings,
                    integration_references, retention_policy, projection_policy,
                    workflow_policy, updated_by, updated_at
             FROM erpx_control.business_unit_configurations
             WHERE holding_id = $1 AND business_unit = $2",
        )
        .bind(holding_id)
        .bind(business_unit.as_str())
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.map(|row| {
            let configuration = BusinessUnitConfiguration {
                holding_id: row.try_get("canonical_holding_id")?,
                business_unit,
                configuration_version: required_u64(&row, "configuration_version")?,
                active: row.try_get("active")?,
                organization_scope: serde_json::from_value(row.try_get("organization_scope")?)?,
                directory_role_mappings: serde_json::from_value::<
                    Vec<BusinessUnitDirectoryRoleMapping>,
                >(row.try_get("directory_role_mappings")?)?,
                integration_references: row.try_get("integration_references")?,
                retention_policy: row.try_get("retention_policy")?,
                projection_policy: row.try_get("projection_policy")?,
                workflow_policy: row.try_get("workflow_policy")?,
                updated_by: row.try_get("updated_by")?,
                updated_at: row.try_get("updated_at")?,
            };
            configuration.validate().map_err(contract_error)?;
            Ok(configuration)
        })
        .transpose()
    }

    /// Replace a holding infrastructure profile using exact-version optimistic
    /// concurrency. The referenced secret names are stored; secret values are
    /// not accepted by the domain type.
    pub async fn save_infrastructure_profile(
        &self,
        profile: &HoldingInfrastructureProfile,
        expected_version: u64,
        actor_ref: &str,
        reason: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        profile.validate().map_err(contract_error)?;
        validate_reference(actor_ref, "actor_ref", 255)?;
        validate_reference(reason, "reason", 512)?;
        validate_correlation_id(correlation_id)?;
        if expected_version == 0 || profile.version != expected_version.saturating_add(1) {
            return Err(PersistenceError::InvalidData(
                "infrastructure profile version must advance exactly once".into(),
            ));
        }

        let mut tx = self.control.pools.write.begin().await?;
        let result = sqlx::query(
            "UPDATE erpx_control.holding_infrastructure_profiles
             SET deployment_mode = $4,
                 write_database_ref = $5,
                 read_database_ref = $6,
                 storage_ref = $7,
                 directory_credential_ref = $8,
                 backup_profile_ref = $9,
                 profile_version = $10,
                 updated_by = $11,
                 updated_at = $12
             WHERE holding_id = $1
               AND canonical_holding_id = $2
               AND profile_version = $3",
        )
        .bind(profile.holding_id)
        .bind(profile.holding_id)
        .bind(checked_i64(expected_version, "expected profile version")?)
        .bind(profile.deployment_mode.as_str())
        .bind(profile.write_database_ref.as_str())
        .bind(profile.read_database_ref.as_str())
        .bind(profile.storage_ref.as_str())
        .bind(
            profile
                .directory_credential_ref
                .as_ref()
                .map(|reference| reference.as_str()),
        )
        .bind(profile.backup_profile_ref.as_str())
        .bind(checked_i64(profile.version, "profile version")?)
        .bind(actor_ref)
        .bind(at)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() != 1 {
            let actual = sqlx::query_scalar::<_, i64>(
                "SELECT profile_version
                 FROM erpx_control.holding_infrastructure_profiles
                 WHERE holding_id = $1 AND canonical_holding_id = $2",
            )
            .bind(profile.holding_id)
            .bind(profile.holding_id)
            .fetch_optional(&mut *tx)
            .await?
            .and_then(|version| u64::try_from(version).ok());
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "holding_infrastructure_profile".into(),
                aggregate_id: profile.holding_id,
                expected: Some(expected_version),
                actual,
            });
        }

        append_control_event(
            &mut tx,
            "holding.infrastructure.profile.updated",
            "holding_infrastructure_profile",
            profile.holding_id,
            profile.version,
            &profile.holding_id,
            profile.holding_id,
            actor_ref,
            reason,
            correlation_id,
            json!({
                "holding_id": profile.holding_id,
                "deployment_mode": profile.deployment_mode.as_str(),
                "profile_version": profile.version,
                "secret_values_persisted": false,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Persist a hash-only, short-lived enrollment challenge.
    pub async fn issue_agent_enrollment(
        &self,
        issue: &DataPlaneEnrollmentIssue,
    ) -> Result<(), PersistenceError> {
        validate_enrollment_issue(issue)?;
        let mut tx = self.control.pools.write.begin().await?;
        let customer_hosted: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM erpx_control.holding_infrastructure_profiles
                WHERE holding_id = $1
                  AND canonical_holding_id = $2
                  AND deployment_mode = 'customer_data_plane'
             )",
        )
        .bind(&issue.holding_id)
        .bind(issue.canonical_holding_id)
        .fetch_one(&mut *tx)
        .await?;
        if !customer_hosted {
            return Err(PersistenceError::InvalidData(
                "agent enrollment requires an approved customer data-plane profile".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.data_plane_agent_enrollments
                (enrollment_id, holding_id, canonical_holding_id, token_hash,
                 manifest_hash, issued_by, issued_at, expires_at, correlation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(issue.enrollment_id)
        .bind(&issue.holding_id)
        .bind(issue.canonical_holding_id)
        .bind(issue.token_hash.as_slice())
        .bind(issue.manifest_hash.as_slice())
        .bind(&issue.issued_by)
        .bind(issue.issued_at)
        .bind(issue.expires_at)
        .bind(issue.correlation_id)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.agent.enrollment.issued",
            "data_plane_agent_enrollment",
            issue.enrollment_id,
            1,
            &issue.holding_id,
            issue.canonical_holding_id,
            &issue.issued_by,
            "customer data-plane enrollment",
            issue.correlation_id,
            json!({
                "holding_id": issue.holding_id,
                "expires_at": issue.expires_at,
                "hash_only": true,
            }),
            issue.issued_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Consume an enrollment exactly once and bind the certificate fingerprint
    /// to one holding/holding/agent tuple.
    pub async fn consume_agent_enrollment(
        &self,
        consumption: &DataPlaneEnrollmentConsumption,
    ) -> Result<(), PersistenceError> {
        consumption.binding.validate().map_err(contract_error)?;
        validate_reference(&consumption.agent_version, "agent_version", 128)?;
        if consumption.enrollment_id.is_nil()
            || consumption.presented_token_hash == [0; 32]
            || consumption.presented_manifest_hash == [0; 32]
            || consumption.certificate_expires_at <= consumption.enrolled_at
            || matches!(
                consumption.binding.state,
                DataPlaneAgentState::Unenrolled
                    | DataPlaneAgentState::Enrolling
                    | DataPlaneAgentState::Suspended
            )
        {
            return Err(PersistenceError::InvalidData(
                "invalid agent enrollment consumption".into(),
            ));
        }

        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT holding_id, canonical_holding_id, token_hash, manifest_hash,
                    expires_at, correlation_id, issued_by
             FROM erpx_control.data_plane_agent_enrollments
             WHERE enrollment_id = $1
               AND consumed_at IS NULL
               AND revoked_at IS NULL
             FOR UPDATE",
        )
        .bind(consumption.enrollment_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "active data-plane enrollment",
            id: consumption.enrollment_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let canonical_holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let expected_token_hash: Vec<u8> = row.try_get("token_hash")?;
        let expected_manifest_hash: Vec<u8> = row.try_get("manifest_hash")?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let correlation_id: Uuid = row.try_get("correlation_id")?;
        let issued_by: String = row.try_get("issued_by")?;
        if _holding_key != consumption.binding.holding_id.to_string()
            || canonical_holding_id != consumption.binding.holding_id
            || consumption.enrolled_at >= expires_at
            || !constant_time_hash_eq(&expected_token_hash, &consumption.presented_token_hash)
            || !constant_time_hash_eq(
                &expected_manifest_hash,
                &consumption.presented_manifest_hash,
            )
        {
            return Err(PersistenceError::InvalidData(
                "enrollment proof or holding binding is invalid".into(),
            ));
        }

        sqlx::query(
            "UPDATE erpx_control.data_plane_agent_enrollments
             SET consumed_at = $2, consumed_by_agent_id = $3
             WHERE enrollment_id = $1",
        )
        .bind(consumption.enrollment_id)
        .bind(consumption.enrolled_at)
        .bind(consumption.binding.agent_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.data_plane_agents
                (agent_id, holding_id, canonical_holding_id, certificate_fingerprint,
                 capabilities, state, observed_version, agent_version,
                 certificate_expires_at, enrolled_at, last_heartbeat_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
        )
        .bind(consumption.binding.agent_id)
        .bind(consumption.binding.holding_id)
        .bind(consumption.binding.holding_id)
        .bind(&consumption.binding.certificate_fingerprint)
        .bind(agent_capability_keys(&consumption.binding.capabilities))
        .bind(consumption.binding.state.as_str())
        .bind(checked_i64(
            consumption.binding.observed_version,
            "observed version",
        )?)
        .bind(&consumption.agent_version)
        .bind(consumption.certificate_expires_at)
        .bind(consumption.enrolled_at)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.agent.enrolled",
            "data_plane_agent",
            consumption.binding.agent_id,
            1,
            &consumption.binding.holding_id,
            consumption.binding.holding_id,
            &issued_by,
            "customer data-plane enrollment consumed",
            correlation_id,
            json!({
                "holding_id": consumption.binding.holding_id,
                "agent_id": consumption.binding.agent_id,
                "certificate_fingerprint": consumption.binding.certificate_fingerprint,
                "bootstrap_secret_persisted": false,
            }),
            consumption.enrolled_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Record redacted observed state. Older heartbeats and observed versions
    /// are rejected so a reconnect cannot roll state backwards.
    pub async fn record_observation(
        &self,
        observation: &ObservedDataPlaneState,
        agent_version: &str,
    ) -> Result<DataPlaneAgentState, PersistenceError> {
        observation.validate().map_err(contract_error)?;
        validate_reference(agent_version, "agent_version", 128)?;
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT holding_id, canonical_holding_id, observed_version,
                    last_heartbeat_at, state
             FROM erpx_control.data_plane_agents
             WHERE agent_id = $1
             FOR UPDATE",
        )
        .bind(observation.agent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "data-plane agent",
            id: observation.agent_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let stored_version: i64 = row.try_get("observed_version")?;
        let last_heartbeat: Option<DateTime<Utc>> = row.try_get("last_heartbeat_at")?;
        let stored_state: String = row.try_get("state")?;
        if stored_state == "suspended" {
            return Err(PersistenceError::InvalidData(
                "suspended agent cannot publish observations".into(),
            ));
        }
        if holding_id.to_string() != observation.holding_id
            || i64::try_from(observation.observed_version).ok() < Some(stored_version)
            || last_heartbeat.is_some_and(|last| observation.observed_at <= last)
        {
            return Err(PersistenceError::InvalidData(
                "out-of-order or wrong-holding observation rejected".into(),
            ));
        }

        let state = if observation.is_healthy() {
            DataPlaneAgentState::Healthy
        } else if observation.is_drifted() {
            DataPlaneAgentState::Drifted
        } else {
            DataPlaneAgentState::Degraded
        };
        sqlx::query(
            "UPDATE erpx_control.data_plane_agents
             SET state = $2,
                 observed_version = $3,
                 agent_version = $4,
                 last_heartbeat_at = $5
             WHERE agent_id = $1",
        )
        .bind(observation.agent_id)
        .bind(state.as_str())
        .bind(checked_i64(
            observation.observed_version,
            "observed version",
        )?)
        .bind(agent_version)
        .bind(observation.observed_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.data_plane_observations
                (agent_id, holding_id, observed_version, desired_version,
                 write_schema_version, read_schema_version, write_healthy,
                 read_healthy, storage_healthy, directory_ready,
                 projections_healthy, backup_verified, failure_category,
                 correlation_id, observed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (agent_id) DO UPDATE
             SET observed_version = EXCLUDED.observed_version,
                 desired_version = EXCLUDED.desired_version,
                 write_schema_version = EXCLUDED.write_schema_version,
                 read_schema_version = EXCLUDED.read_schema_version,
                 write_healthy = EXCLUDED.write_healthy,
                 read_healthy = EXCLUDED.read_healthy,
                 storage_healthy = EXCLUDED.storage_healthy,
                 directory_ready = EXCLUDED.directory_ready,
                 projections_healthy = EXCLUDED.projections_healthy,
                 backup_verified = EXCLUDED.backup_verified,
                 failure_category = EXCLUDED.failure_category,
                 correlation_id = EXCLUDED.correlation_id,
                 observed_at = EXCLUDED.observed_at",
        )
        .bind(observation.agent_id)
        .bind(&observation.holding_id)
        .bind(checked_i64(
            observation.observed_version,
            "observed version",
        )?)
        .bind(checked_i64(observation.desired_version, "desired version")?)
        .bind(&observation.write_schema_version)
        .bind(&observation.read_schema_version)
        .bind(observation.write_healthy)
        .bind(observation.read_healthy)
        .bind(observation.storage_healthy)
        .bind(observation.directory_ready)
        .bind(observation.projections_healthy)
        .bind(observation.backup_verified)
        .bind(&observation.failure_category)
        .bind(observation.correlation_id)
        .bind(observation.observed_at)
        .execute(&mut *tx)
        .await?;

        if observation.is_healthy() && observation.observed_version == observation.desired_version {
            let activated = sqlx::query(
                "UPDATE erpx_control.holding_desired_configurations
                 SET lifecycle = 'active', applied_at = $4
                 WHERE holding_id = $1
                   AND target_agent_id = $2
                   AND desired_version = $3
                   AND lifecycle IN ('approved', 'applying', 'verified')
                 RETURNING configuration_id",
            )
            .bind(&observation.holding_id)
            .bind(observation.agent_id)
            .bind(checked_i64(
                observation.observed_version,
                "observed version",
            )?)
            .bind(observation.observed_at)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(activated) = activated {
                let configuration_id: Uuid = activated.try_get("configuration_id")?;
                append_control_event(
                    &mut tx,
                    "holding.configuration.active",
                    "holding_desired_configuration",
                    configuration_id,
                    event_sequence(observation.observed_at)?,
                    &observation.holding_id,
                    holding_id,
                    "data-plane-agent",
                    "customer agent verified desired configuration",
                    observation.correlation_id,
                    json!({
                        "holding_id": observation.holding_id,
                        "configuration_id": configuration_id,
                        "desired_version": observation.desired_version,
                    }),
                    observation.observed_at,
                )
                .await?;
            }
        }

        // An agent outage must terminate every interactive Root session. A
        // heartbeat can only report connected health states, so active sessions
        // remain bound to the newly observed state and expiry checks.
        append_control_event(
            &mut tx,
            "holding.agent.observation.recorded",
            "data_plane_agent",
            observation.agent_id,
            event_sequence(observation.observed_at)?,
            &observation.holding_id,
            holding_id,
            "data-plane-agent",
            "redacted data-plane health observation",
            observation.correlation_id,
            json!({
                "holding_id": observation.holding_id,
                "agent_id": observation.agent_id,
                "state": state.as_str(),
                "observed_version": observation.observed_version,
                "desired_version": observation.desired_version,
            }),
            observation.observed_at,
        )
        .await?;
        tx.commit().await?;
        Ok(state)
    }

    /// Store a versioned desired-state document after rejecting credential-like
    /// fields and verifying its supplied SHA-256 digest.
    pub async fn propose_configuration(
        &self,
        desired: &DesiredConfiguration,
        desired_state: &Value,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        desired.validate(at).map_err(contract_error)?;
        validate_correlation_id(correlation_id)?;
        validate_desired_state(desired_state)?;
        if !matches!(
            desired.lifecycle,
            ConfigurationLifecycle::Draft | ConfigurationLifecycle::AwaitingApproval
        ) {
            return Err(PersistenceError::InvalidData(
                "new desired configuration must be draft or awaiting approval".into(),
            ));
        }
        let canonical = serde_json::to_vec(desired_state)?;
        let calculated_hash: [u8; 32] = Sha256::digest(&canonical).into();
        if calculated_hash
            .ct_eq(&desired.configuration_hash)
            .unwrap_u8()
            != 1
        {
            return Err(PersistenceError::InvalidData(
                "desired configuration digest mismatch".into(),
            ));
        }
        let (layer, business_unit) = configuration_layer_parts(desired.layer);
        let mut tx = self.control.pools.write.begin().await?;
        let actual_observed: i64 = match desired.target_agent_id {
            Some(agent_id) => sqlx::query_scalar(
                "SELECT observed_version
                     FROM erpx_control.data_plane_agents
                     WHERE agent_id = $1
                       AND holding_id = $2
                       AND canonical_holding_id = $3",
            )
            .bind(agent_id)
            .bind(desired.holding_id)
            .bind(desired.holding_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "data-plane agent",
                id: agent_id,
            })?,
            None => {
                sqlx::query_scalar(
                    "SELECT COALESCE(MAX(desired_version), 0)
                 FROM erpx_control.holding_desired_configurations
                 WHERE holding_id = $1",
                )
                .bind(desired.holding_id)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        if actual_observed
            != checked_i64(
                desired.expected_observed_version,
                "expected observed version",
            )?
        {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "holding_desired_configuration".into(),
                aggregate_id: desired.holding_id,
                expected: Some(desired.expected_observed_version),
                actual: u64::try_from(actual_observed).ok(),
            });
        }
        sqlx::query(
            "INSERT INTO erpx_control.holding_desired_configurations
                (configuration_id, holding_id, canonical_holding_id, target_agent_id,
                 deployment_mode, configuration_layer, business_unit,
                 expected_observed_version, desired_version, lifecycle,
                 configuration_hash, desired_state, secret_references,
                 required_capabilities, requester, reason, requested_at,
                 expires_at, correlation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, $15, $16, $17, $18, $19)",
        )
        .bind(desired.id)
        .bind(desired.holding_id)
        .bind(desired.holding_id)
        .bind(desired.target_agent_id)
        .bind(desired.deployment_mode.as_str())
        .bind(layer)
        .bind(business_unit)
        .bind(checked_i64(
            desired.expected_observed_version,
            "expected observed version",
        )?)
        .bind(checked_i64(desired.desired_version, "desired version")?)
        .bind(desired.lifecycle.as_str())
        .bind(desired.configuration_hash.as_slice())
        .bind(desired_state)
        .bind(
            desired
                .secret_references
                .iter()
                .map(|reference| reference.as_str().to_owned())
                .collect::<Vec<_>>(),
        )
        .bind(agent_capability_keys(&desired.required_capabilities))
        .bind(&desired.requester)
        .bind(&desired.reason)
        .bind(at)
        .bind(desired.expires_at)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.configuration.proposed",
            "holding_desired_configuration",
            desired.id,
            desired.desired_version,
            &desired.holding_id,
            desired.holding_id,
            &desired.requester,
            &desired.reason,
            correlation_id,
            json!({
                "holding_id": desired.holding_id,
                "configuration_id": desired.id,
                "layer": desired.layer.canonical_key(),
                "desired_version": desired.desired_version,
                "lifecycle": desired.lifecycle.as_str(),
                "secret_values_persisted": false,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn approve_configuration(
        &self,
        configuration_id: Uuid,
        approver: &str,
        fresh_mfa_at: DateTime<Utc>,
        at: DateTime<Utc>,
        correlation_id: Uuid,
    ) -> Result<(), PersistenceError> {
        self.approve_configuration_with_policy(
            configuration_id,
            approver,
            fresh_mfa_at,
            at,
            correlation_id,
            ApprovalPolicy::Strict,
        )
        .await
    }

    pub async fn approve_configuration_with_policy(
        &self,
        configuration_id: Uuid,
        approver: &str,
        fresh_mfa_at: DateTime<Utc>,
        at: DateTime<Utc>,
        correlation_id: Uuid,
        approval_policy: ApprovalPolicy,
    ) -> Result<(), PersistenceError> {
        validate_reference(approver, "approver", 255)?;
        validate_correlation_id(correlation_id)?;
        if configuration_id.is_nil()
            || fresh_mfa_at > at
            || at - fresh_mfa_at > Duration::minutes(5)
        {
            return Err(PersistenceError::InvalidData(
                "configuration approval requires fresh MFA".into(),
            ));
        }
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT holding_id, canonical_holding_id, requester, desired_version,
                    expires_at, lifecycle
             FROM erpx_control.holding_desired_configurations
             WHERE configuration_id = $1
             FOR UPDATE",
        )
        .bind(configuration_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "desired configuration",
            id: configuration_id,
        })?;
        let requester: String = row.try_get("requester")?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let lifecycle: String = row.try_get("lifecycle")?;
        if (requester == approver && matches!(approval_policy, ApprovalPolicy::Strict))
            || at >= expires_at
            || lifecycle != "awaiting_approval"
        {
            return Err(PersistenceError::InvalidData(
                "configuration is not eligible for customer approval".into(),
            ));
        }
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let version = u64::try_from(row.try_get::<i64, _>("desired_version")?)
            .map_err(|_| PersistenceError::InvalidData("invalid desired version".into()))?;
        sqlx::query(
            "UPDATE erpx_control.holding_desired_configurations
             SET lifecycle = 'approved',
                 customer_approver = $2,
                 customer_approved_at = $3
             WHERE configuration_id = $1",
        )
        .bind(configuration_id)
        .bind(approver)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.configuration.approved",
            "holding_desired_configuration",
            configuration_id,
            version,
            &holding_id,
            holding_id,
            approver,
            "customer-approved configuration",
            correlation_id,
            json!({
                "holding_id": holding_id,
                "configuration_id": configuration_id,
                "desired_version": version,
                "fresh_mfa": true,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Resolve immutable authorization fields before MFA or approval writes.
    pub async fn desired_configuration_authorization_target(
        &self,
        configuration_id: Uuid,
    ) -> Result<(String, Uuid, String), PersistenceError> {
        if configuration_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "desired configuration ID is required".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT holding_id, canonical_holding_id, requester
             FROM erpx_control.holding_desired_configurations
             WHERE configuration_id=$1",
        )
        .bind(configuration_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "desired configuration",
            id: configuration_id,
        })?;
        Ok((
            row.try_get("holding_id")?,
            row.try_get("canonical_holding_id")?,
            row.try_get("requester")?,
        ))
    }

    pub async fn approved_configuration(
        &self,
        configuration_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<ApprovedDesiredConfiguration, PersistenceError> {
        let row = sqlx::query(
            "SELECT configuration_id, holding_id, canonical_holding_id,
                    target_agent_id, deployment_mode, configuration_layer,
                    business_unit, expected_observed_version, desired_version,
                    configuration_hash, desired_state, secret_references,
                    required_capabilities, requester, reason, expires_at,
                    correlation_id
             FROM erpx_control.holding_desired_configurations
             WHERE configuration_id = $1 AND lifecycle = 'approved'",
        )
        .bind(configuration_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "approved desired configuration",
            id: configuration_id,
        })?;
        decode_approved_configuration(&row, at)
    }

    pub async fn next_approved_configuration(
        &self,
        agent_id: Uuid,
        observed_version: u64,
        at: DateTime<Utc>,
    ) -> Result<Option<ApprovedDesiredConfiguration>, PersistenceError> {
        let row = sqlx::query(
            "SELECT configuration_id, holding_id, canonical_holding_id,
                    target_agent_id, deployment_mode, configuration_layer,
                    business_unit, expected_observed_version, desired_version,
                    configuration_hash, desired_state, secret_references,
                    required_capabilities, requester, reason, expires_at,
                    correlation_id
             FROM erpx_control.holding_desired_configurations
             WHERE target_agent_id = $1
               AND lifecycle = 'approved'
               AND expected_observed_version = $2
               AND expires_at > $3
             ORDER BY desired_version ASC
             LIMIT 1",
        )
        .bind(agent_id)
        .bind(checked_i64(observed_version, "observed version")?)
        .bind(at)
        .fetch_optional(&self.control.pools.write)
        .await?;
        row.as_ref()
            .map(|row| decode_approved_configuration(row, at))
            .transpose()
    }

    pub async fn reject_configuration(
        &self,
        configuration_id: Uuid,
        rejected_by: &str,
        reason: &str,
        at: DateTime<Utc>,
        correlation_id: Uuid,
    ) -> Result<(), PersistenceError> {
        validate_reference(rejected_by, "rejected_by", 255)?;
        validate_reference(reason, "rejection reason", 512)?;
        validate_correlation_id(correlation_id)?;
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "UPDATE erpx_control.holding_desired_configurations
             SET lifecycle = 'rejected', rejected_at = $2
             WHERE configuration_id = $1 AND lifecycle = 'awaiting_approval'
             RETURNING holding_id, canonical_holding_id, desired_version",
        )
        .bind(configuration_id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "configuration awaiting approval",
            id: configuration_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let version = u64::try_from(row.try_get::<i64, _>("desired_version")?)
            .map_err(|_| PersistenceError::InvalidData("invalid desired version".into()))?;
        append_control_event(
            &mut tx,
            "holding.configuration.rejected",
            "holding_desired_configuration",
            configuration_id,
            version,
            &holding_id,
            holding_id,
            rejected_by,
            reason,
            correlation_id,
            json!({
                "holding_id": holding_id,
                "configuration_id": configuration_id,
                "desired_version": version,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn request_root_access(
        &self,
        request: &RootAccessRequest,
        canonical_holding_id: Uuid,
        correlation_id: Uuid,
    ) -> Result<(), PersistenceError> {
        request.validate().map_err(contract_error)?;
        validate_correlation_id(correlation_id)?;
        if canonical_holding_id.is_nil()
            || scope_holding_id(&request.active_scope) != canonical_holding_id
        {
            return Err(PersistenceError::InvalidData(
                "Root access scope must match the exact holding holding".into(),
            ));
        }
        let mut tx = self.control.pools.write.begin().await?;
        let eligible_agent: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM erpx_control.data_plane_agents
                WHERE agent_id = $1
                  AND holding_id = $2
                  AND canonical_holding_id = $3
                  AND state <> 'suspended'
             )",
        )
        .bind(request.agent_id)
        .bind(&request.holding_id)
        .bind(canonical_holding_id)
        .fetch_one(&mut *tx)
        .await?;
        if !eligible_agent {
            return Err(PersistenceError::InvalidData(
                "Root access request has no eligible holding agent".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_control.holding_root_access_requests
                (request_id, requester_ref, device_ref, holding_id,
                 canonical_holding_id, agent_id, active_scope, mode,
                 allowed_actions, business_units, reason, ticket_reference,
                 requested_at, requested_until, correlation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, $15)",
        )
        .bind(request.id)
        .bind(&request.requester)
        .bind(&request.device_ref)
        .bind(&request.holding_id)
        .bind(canonical_holding_id)
        .bind(request.agent_id)
        .bind(serde_json::to_value(&request.active_scope)?)
        .bind(root_access_mode_key(request.mode))
        .bind(&request.allowed_actions)
        .bind(business_unit_keys(&request.business_units))
        .bind(&request.reason)
        .bind(&request.ticket_reference)
        .bind(request.requested_at)
        .bind(request.requested_until)
        .bind(correlation_id)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.root_access.requested",
            "holding_root_access_request",
            request.id,
            1,
            &request.holding_id,
            canonical_holding_id,
            &request.requester,
            &request.reason,
            correlation_id,
            json!({
                "holding_id": request.holding_id,
                "request_id": request.id,
                "agent_id": request.agent_id,
                "mode": root_access_mode_key(request.mode),
                "scope": request.active_scope.canonical_key(),
                "allowed_actions": request.allowed_actions,
                "business_units": business_unit_keys(&request.business_units),
                "requested_until": request.requested_until,
            }),
            request.requested_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_root_access_requests(
        &self,
        holding_id: &str,
        include_closed: bool,
        page_size: u32,
    ) -> Result<Vec<RootAccessQueueRecord>, PersistenceError> {
        validate_reference(holding_id, "holding_id", 128)?;
        if page_size == 0 || page_size > 100 {
            return Err(PersistenceError::InvalidData(
                "Root access request page size is outside 1..=100".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT request_id, requester_ref, device_ref, holding_id,
                    agent_id, active_scope, mode, allowed_actions,
                    business_units, reason, ticket_reference, requested_at,
                    requested_until, state, customer_approver_ref,
                    customer_approved_at, rejected_at, revoked_at,
                    revocation_reason
             FROM erpx_control.holding_root_access_requests
             WHERE holding_id = $1
               AND ($2 OR state IN ('awaiting_customer_approval', 'approved', 'active'))
             ORDER BY requested_at DESC, request_id DESC
             LIMIT $3",
        )
        .bind(holding_id)
        .bind(include_closed)
        .bind(i64::from(page_size))
        .fetch_all(&self.control.pools.write)
        .await?;
        rows.iter().map(decode_root_access_queue_record).collect()
    }

    /// Approve and activate one Root session capability. The caller returns the
    /// opaque bearer value directly to the broker; this store receives only its
    /// SHA-256 digest.
    pub async fn approve_root_access(
        &self,
        grant: &RootAccessGrant,
        correlation_id: Uuid,
    ) -> Result<RootAccessCapability, PersistenceError> {
        self.approve_root_access_with_policy(grant, correlation_id, ApprovalPolicy::Strict)
            .await
    }

    pub async fn approve_root_access_with_policy(
        &self,
        grant: &RootAccessGrant,
        correlation_id: Uuid,
        approval_policy: ApprovalPolicy,
    ) -> Result<RootAccessCapability, PersistenceError> {
        validate_correlation_id(correlation_id)?;
        if grant.capability_id.is_nil() || grant.capability_token_hash == [0; 32] {
            return Err(PersistenceError::InvalidData(
                "invalid Root access capability".into(),
            ));
        }
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "SELECT requester_ref, device_ref, holding_id, canonical_holding_id,
                    agent_id, active_scope, mode, allowed_actions,
                    business_units, reason, ticket_reference, requested_at,
                    requested_until, state
             FROM erpx_control.holding_root_access_requests
             WHERE request_id = $1
             FOR UPDATE",
        )
        .bind(grant.approval.request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Root access request",
            id: grant.approval.request_id,
        })?;
        let state: String = row.try_get("state")?;
        if state != "awaiting_customer_approval" {
            return Err(PersistenceError::InvalidData(
                "Root access request is not awaiting customer approval".into(),
            ));
        }
        let request = decode_root_access_request(grant.approval.request_id, &row)?;
        grant
            .approval
            .validate_for_with_policy(&request, approval_policy)
            .map_err(contract_error)?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let agent_state: String = sqlx::query_scalar(
            "SELECT state
             FROM erpx_control.data_plane_agents
             WHERE agent_id = $1 AND holding_id = $2 AND canonical_holding_id = $3
             FOR UPDATE",
        )
        .bind(request.agent_id)
        .bind(&request.holding_id)
        .bind(holding_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "data-plane agent",
            id: request.agent_id,
        })?;
        let parsed_agent_state = parse_agent_state(&agent_state)?;
        if !parsed_agent_state.permits_root_session() {
            return Err(PersistenceError::InvalidData(
                "Root access cannot start while the customer agent is unavailable".into(),
            ));
        }
        let capability = RootAccessCapability {
            id: grant.capability_id,
            request_id: request.id,
            token_hash: grant.capability_token_hash,
            root_principal: request.requester.clone(),
            device_ref: request.device_ref.clone(),
            holding_id: request.holding_id.clone(),
            agent_id: request.agent_id,
            scope: request.active_scope.clone(),
            allowed_actions: request.allowed_actions.clone(),
            business_units: request.business_units.clone(),
            issued_at: grant.approval.approved_at,
            expires_at: request.requested_until,
            revoked_at: None,
        };
        sqlx::query(
            "UPDATE erpx_control.holding_root_access_requests
             SET state = 'active',
                 customer_approver_ref = $2,
                 customer_approved_at = $3,
                 customer_mfa_at = $3
             WHERE request_id = $1",
        )
        .bind(request.id)
        .bind(&grant.approval.approver)
        .bind(grant.approval.approved_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.holding_root_access_capabilities
                (capability_id, request_id, token_hash, root_principal_ref,
                 device_ref, holding_id, agent_id, active_scope,
                 allowed_actions, business_units, issued_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(capability.id)
        .bind(capability.request_id)
        .bind(capability.token_hash.as_slice())
        .bind(&capability.root_principal)
        .bind(&capability.device_ref)
        .bind(&capability.holding_id)
        .bind(capability.agent_id)
        .bind(serde_json::to_value(&capability.scope)?)
        .bind(&capability.allowed_actions)
        .bind(business_unit_keys(&capability.business_units))
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.root_access.activated",
            "holding_root_access_request",
            request.id,
            2,
            &request.holding_id,
            holding_id,
            &grant.approval.approver,
            "customer-approved Root access",
            correlation_id,
            json!({
                "holding_id": request.holding_id,
                "request_id": request.id,
                "capability_id": capability.id,
                "root_principal": request.requester,
                "agent_id": request.agent_id,
                "expires_at": capability.expires_at,
                "fresh_mfa": true,
                "hash_only": true,
            }),
            grant.approval.approved_at,
        )
        .await?;
        tx.commit().await?;
        Ok(capability)
    }

    /// Resolve immutable Root request authorization fields before MFA or any
    /// state transition.
    pub async fn root_access_authorization_target(
        &self,
        request_id: Uuid,
    ) -> Result<(String, Uuid, String), PersistenceError> {
        if request_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Root access request ID is required".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT holding_id, canonical_holding_id, requester_ref
             FROM erpx_control.holding_root_access_requests
             WHERE request_id=$1",
        )
        .bind(request_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Root access request",
            id: request_id,
        })?;
        Ok((
            row.try_get("holding_id")?,
            row.try_get("canonical_holding_id")?,
            row.try_get("requester_ref")?,
        ))
    }

    pub async fn revoke_root_access(
        &self,
        request_id: Uuid,
        revoked_by: &str,
        reason: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_reference(revoked_by, "revoked_by", 255)?;
        validate_reference(reason, "revocation reason", 512)?;
        validate_correlation_id(correlation_id)?;
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "UPDATE erpx_control.holding_root_access_requests
             SET state = 'revoked', revoked_at = $2, revocation_reason = $3
             WHERE request_id = $1
               AND state IN ('approved', 'active')
             RETURNING holding_id, canonical_holding_id",
        )
        .bind(request_id)
        .bind(at)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "active Root access request",
            id: request_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        sqlx::query(
            "UPDATE erpx_control.holding_root_access_capabilities
             SET revoked_at = $2
             WHERE request_id = $1 AND revoked_at IS NULL",
        )
        .bind(request_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.root_access.revoked",
            "holding_root_access_request",
            request_id,
            3,
            &holding_id,
            holding_id,
            revoked_by,
            reason,
            correlation_id,
            json!({
                "holding_id": holding_id,
                "request_id": request_id,
                "revoked_at": at,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn root_access_capability(
        &self,
        request_id: Uuid,
    ) -> Result<RootAccessCapability, PersistenceError> {
        let row = sqlx::query(
            "SELECT capability_id, request_id, token_hash,
                    root_principal_ref, device_ref, holding_id, agent_id,
                    active_scope, allowed_actions, business_units,
                    issued_at, expires_at, revoked_at
             FROM erpx_control.holding_root_access_capabilities
             WHERE request_id = $1",
        )
        .bind(request_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Root access capability",
            id: request_id,
        })?;
        let token_hash: Vec<u8> = row.try_get("token_hash")?;
        let token_hash = token_hash.try_into().map_err(|_| {
            PersistenceError::InvalidData("invalid persisted capability hash".into())
        })?;
        let business_units: Vec<String> = row.try_get("business_units")?;
        Ok(RootAccessCapability {
            id: row.try_get("capability_id")?,
            request_id: row.try_get("request_id")?,
            token_hash,
            root_principal: row.try_get("root_principal_ref")?,
            device_ref: row.try_get("device_ref")?,
            holding_id: row.try_get("holding_id")?,
            agent_id: row.try_get("agent_id")?,
            scope: serde_json::from_value(row.try_get("active_scope")?)?,
            allowed_actions: row.try_get("allowed_actions")?,
            business_units: business_units
                .iter()
                .map(|unit| BusinessUnitId::parse(unit).map_err(contract_error))
                .collect::<Result<Vec<_>, _>>()?,
            issued_at: row.try_get("issued_at")?,
            expires_at: row.try_get("expires_at")?,
            revoked_at: row.try_get("revoked_at")?,
        })
    }

    pub async fn reject_root_access(
        &self,
        request_id: Uuid,
        rejected_by: &str,
        reason: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_reference(rejected_by, "rejected_by", 255)?;
        validate_reference(reason, "rejection reason", 512)?;
        validate_correlation_id(correlation_id)?;
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "UPDATE erpx_control.holding_root_access_requests
             SET state = 'rejected', rejected_at = $2
             WHERE request_id = $1 AND state = 'awaiting_customer_approval'
             RETURNING holding_id, canonical_holding_id",
        )
        .bind(request_id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "Root access request awaiting approval",
            id: request_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        append_control_event(
            &mut tx,
            "holding.root_access.rejected",
            "holding_root_access_request",
            request_id,
            2,
            &holding_id,
            holding_id,
            rejected_by,
            reason,
            correlation_id,
            json!({"holding_id": holding_id, "request_id": request_id}),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Fail closed on loss of the outbound agent connection and revoke every
    /// active Root session for that agent in the same transaction.
    pub async fn mark_agent_offline(
        &self,
        agent_id: Uuid,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<u64, PersistenceError> {
        validate_correlation_id(correlation_id)?;
        let mut tx = self.control.pools.write.begin().await?;
        let row = sqlx::query(
            "UPDATE erpx_control.data_plane_agents
             SET state = 'offline'
             WHERE agent_id = $1 AND state <> 'suspended'
             RETURNING holding_id, canonical_holding_id",
        )
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "active data-plane agent",
            id: agent_id,
        })?;
        let _holding_key: String = row.try_get("holding_id")?;
        let holding_id: Uuid = row.try_get("canonical_holding_id")?;
        let revoked = sqlx::query(
            "UPDATE erpx_control.holding_root_access_capabilities
             SET revoked_at = $2
             WHERE agent_id = $1 AND revoked_at IS NULL AND expires_at > $2",
        )
        .bind(agent_id)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query(
            "UPDATE erpx_control.holding_root_access_requests
             SET state = 'revoked',
                 revoked_at = $2,
                 revocation_reason = 'customer data-plane agent disconnected'
             WHERE agent_id = $1 AND state = 'active'",
        )
        .bind(agent_id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        append_control_event(
            &mut tx,
            "holding.agent.offline",
            "data_plane_agent",
            agent_id,
            event_sequence(at)?,
            &holding_id,
            holding_id,
            "data-plane-broker",
            "outbound agent connection closed",
            correlation_id,
            json!({
                "holding_id": holding_id,
                "agent_id": agent_id,
                "root_sessions_revoked": revoked,
            }),
            at,
        )
        .await?;
        tx.commit().await?;
        Ok(revoked)
    }

    pub async fn authenticate_agent_certificate(
        &self,
        agent_id: Uuid,
        holding_id: &str,
        certificate_fingerprint: &str,
    ) -> Result<DataPlaneAgentBinding, PersistenceError> {
        validate_reference(holding_id, "holding_id", 128)?;
        if agent_id.is_nil()
            || certificate_fingerprint.len() != 64
            || !certificate_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(PersistenceError::InvalidData(
                "invalid agent certificate binding".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT canonical_holding_id, certificate_fingerprint, capabilities,
                    state, observed_version
             FROM erpx_control.data_plane_agents
             WHERE agent_id = $1 AND holding_id = $2",
        )
        .bind(agent_id)
        .bind(holding_id)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "data-plane agent certificate binding",
            id: agent_id,
        })?;
        let stored_fingerprint: String = row.try_get("certificate_fingerprint")?;
        if stored_fingerprint
            .as_bytes()
            .ct_eq(certificate_fingerprint.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(PersistenceError::InvalidData(
                "agent certificate fingerprint does not match enrollment".into(),
            ));
        }
        let capability_keys: Vec<String> = row.try_get("capabilities")?;
        let binding = DataPlaneAgentBinding {
            holding_id: row.try_get("canonical_holding_id")?,
            agent_id,
            certificate_fingerprint: stored_fingerprint,
            capabilities: capability_keys
                .iter()
                .map(|value| parse_agent_capability(value))
                .collect::<Result<Vec<_>, _>>()?,
            state: parse_agent_state(&row.try_get::<String, _>("state")?)?,
            observed_version: required_u64(&row, "observed_version")?,
        };
        binding.validate().map_err(contract_error)?;
        if matches!(binding.state, DataPlaneAgentState::Suspended) {
            return Err(PersistenceError::InvalidData(
                "data-plane agent is suspended".into(),
            ));
        }
        Ok(binding)
    }

    pub async fn infrastructure_summary(
        &self,
        holding_id: &str,
        at: DateTime<Utc>,
    ) -> Result<HoldingInfrastructureSummary, PersistenceError> {
        validate_reference(holding_id, "holding_id", 128)?;
        let row = sqlx::query(
            "SELECT profile.holding_id, profile.canonical_holding_id,
                    profile.deployment_mode, profile.profile_version,
                    agent.agent_id, agent.state AS agent_state,
                    agent.agent_version, agent.last_heartbeat_at,
                    observation.observed_version,
                    observation.desired_version,
                    observation.observed_at,
                    observation.write_healthy, observation.read_healthy,
                    observation.storage_healthy, observation.directory_ready,
                    observation.projections_healthy, observation.backup_verified,
                    EXISTS (
                        SELECT 1
                        FROM erpx_control.holding_root_access_capabilities capability
                        WHERE capability.holding_id = profile.holding_id
                          AND capability.revoked_at IS NULL
                          AND capability.expires_at > $2
                    ) AS root_access_active
             FROM erpx_control.holding_infrastructure_profiles profile
             LEFT JOIN erpx_control.data_plane_agents agent
                    ON agent.holding_id = profile.holding_id
             LEFT JOIN erpx_control.data_plane_observations observation
                    ON observation.agent_id = agent.agent_id
             WHERE profile.holding_id = $1",
        )
        .bind(holding_id)
        .bind(at)
        .fetch_optional(&self.control.pools.write)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData("holding infrastructure profile was not found".into())
        })?;
        let mode: String = row.try_get("deployment_mode")?;
        let agent_state = row
            .try_get::<Option<String>, _>("agent_state")?
            .as_deref()
            .map(parse_agent_state)
            .transpose()?;
        let observed_version = optional_u64(&row, "observed_version")?;
        let desired_version = optional_u64(&row, "desired_version")?;
        Ok(HoldingInfrastructureSummary {
            holding_id: row.try_get("holding_id")?,
            canonical_holding_id: row.try_get("canonical_holding_id")?,
            deployment_mode: parse_deployment_mode(&mode)?,
            profile_version: required_u64(&row, "profile_version")?,
            agent_id: row.try_get("agent_id")?,
            agent_state,
            agent_version: row.try_get("agent_version")?,
            observed_version,
            desired_version,
            last_heartbeat_at: row.try_get("last_heartbeat_at")?,
            observed_at: row.try_get("observed_at")?,
            configuration_drifted: observed_version
                .zip(desired_version)
                .is_some_and(|(observed, desired)| observed != desired),
            write_healthy: row.try_get("write_healthy")?,
            read_healthy: row.try_get("read_healthy")?,
            storage_healthy: row.try_get("storage_healthy")?,
            directory_ready: row.try_get("directory_ready")?,
            projections_healthy: row.try_get("projections_healthy")?,
            backup_verified: row.try_get("backup_verified")?,
            root_access_active: row.try_get("root_access_active")?,
        })
    }

    /// Resolve the execution boundary without falling back between managed and
    /// customer-hosted resources. An unavailable customer agent is an explicit
    /// failure, never a reason to route into an ERPX-managed database.
    pub async fn execution_route(
        &self,
        holding_id: &str,
    ) -> Result<HoldingExecutionRoute, PersistenceError> {
        validate_reference(holding_id, "holding_id", 128)?;
        let row = sqlx::query(
            "SELECT profile.deployment_mode, profile.write_database_ref,
                    profile.read_database_ref, profile.storage_ref,
                    agent.agent_id, agent.state AS agent_state
             FROM erpx_control.holding_infrastructure_profiles profile
             LEFT JOIN erpx_control.data_plane_agents agent
                    ON agent.holding_id = profile.holding_id
             WHERE profile.holding_id = $1",
        )
        .bind(holding_id)
        .fetch_optional(&self.control.pools.write)
        .await?;
        let Some(row) = row else {
            // Only an unbound legacy registry row may use the managed
            // compatibility route. Once a holding has a customer binding, a
            // missing profile is drift and cannot silently become managed.
            let has_customer_binding: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1
                     FROM erpx_control.customer_holding_bindings
                     WHERE holding_id = $1
                 )",
            )
            .bind(holding_id)
            .fetch_one(&self.control.pools.write)
            .await?;
            if has_customer_binding {
                return Err(PersistenceError::InvalidData(
                    "holding execution route was not found".into(),
                ));
            }
            let legacy = sqlx::query(
                "SELECT write_database_key, read_database_key, storage_namespace
                 FROM erpx_control.holding_registry
                 WHERE holding_id = $1 AND state = 'active'",
            )
            .bind(holding_id)
            .fetch_optional(&self.control.pools.write)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData("holding execution route was not found".into())
            })?;
            return Ok(HoldingExecutionRoute::ErpxManaged {
                write_database_key: legacy.try_get("write_database_key")?,
                read_database_key: legacy.try_get("read_database_key")?,
                storage_namespace: legacy.try_get("storage_namespace")?,
            });
        };
        let deployment_mode: String = row.try_get("deployment_mode")?;
        match parse_deployment_mode(&deployment_mode)? {
            HoldingDeploymentMode::ErpxManaged => Ok(HoldingExecutionRoute::ErpxManaged {
                write_database_key: row.try_get("write_database_ref")?,
                read_database_key: row.try_get("read_database_ref")?,
                storage_namespace: row.try_get("storage_ref")?,
            }),
            HoldingDeploymentMode::CustomerDataPlane => {
                let agent_id: Option<Uuid> = row.try_get("agent_id")?;
                let agent_state: Option<String> = row.try_get("agent_state")?;
                let Some(agent_id) = agent_id else {
                    return Err(PersistenceError::InvalidData(
                        "customer data plane unavailable: agent is not enrolled".into(),
                    ));
                };
                let state = agent_state
                    .as_deref()
                    .map(parse_agent_state)
                    .transpose()?
                    .ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "customer data plane unavailable: agent state is missing".into(),
                        )
                    })?;
                if !state.permits_root_session() {
                    return Err(PersistenceError::InvalidData(
                        "customer data plane unavailable".into(),
                    ));
                }
                Ok(HoldingExecutionRoute::CustomerAgent { agent_id })
            }
        }
    }
}

fn event_sequence(at: DateTime<Utc>) -> Result<u64, PersistenceError> {
    u64::try_from(at.timestamp_micros()).map_err(|_| {
        PersistenceError::InvalidData("audit event timestamp is outside the supported range".into())
    })
}

fn validate_enrollment_issue(issue: &DataPlaneEnrollmentIssue) -> Result<(), PersistenceError> {
    validate_reference(&issue.holding_id, "holding_id", 128)?;
    validate_reference(&issue.issued_by, "issued_by", 255)?;
    validate_correlation_id(issue.correlation_id)?;
    let lifetime = issue.expires_at - issue.issued_at;
    if issue.enrollment_id.is_nil()
        || issue.canonical_holding_id.is_nil()
        || issue.token_hash == [0; 32]
        || issue.manifest_hash == [0; 32]
        || lifetime <= Duration::zero()
        || lifetime > MAX_ENROLLMENT_LIFETIME
    {
        return Err(PersistenceError::InvalidData(
            "invalid data-plane enrollment issue".into(),
        ));
    }
    Ok(())
}

fn validate_desired_state(value: &Value) -> Result<(), PersistenceError> {
    let encoded = serde_json::to_vec(value)?;
    if encoded.len() > MAX_CONFIGURATION_BYTES || !value.is_object() {
        return Err(PersistenceError::InvalidData(
            "desired configuration must be a bounded JSON object".into(),
        ));
    }
    inspect_desired_value(value, 0)
}

fn inspect_desired_value(value: &Value, depth: usize) -> Result<(), PersistenceError> {
    if depth > MAX_CONFIGURATION_DEPTH {
        return Err(PersistenceError::InvalidData(
            "desired configuration is nested too deeply".into(),
        ));
    }
    match value {
        Value::Object(values) => {
            for (key, child) in values {
                let key = key.to_ascii_lowercase().replace(['-', ' '], "_");
                if [
                    "password",
                    "passwd",
                    "secret",
                    "credential",
                    "private_key",
                    "access_key",
                    "bind_password",
                    "connection_string",
                    "token",
                    "bearer_token",
                    "refresh_token",
                    "bootstrap_token",
                ]
                .iter()
                .any(|sensitive| key.contains(sensitive))
                {
                    return Err(PersistenceError::InvalidData(
                        "desired configuration contains a credential-like field".into(),
                    ));
                }
                inspect_desired_value(child, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                inspect_desired_value(child, depth + 1)?;
            }
        }
        Value::String(value) => {
            let lower = value.to_ascii_lowercase();
            if lower.contains("-----begin")
                || lower.starts_with("bearer ")
                || lower.contains("://") && lower.contains('@')
            {
                return Err(PersistenceError::InvalidData(
                    "desired configuration contains credential material".into(),
                ));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn append_control_event(
    tx: &mut Transaction<'_, Postgres>,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    holding_key: impl ToString,
    holding_id: Uuid,
    actor_ref: &str,
    purpose: &str,
    correlation_id: Uuid,
    payload: Value,
    occurred_at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    let event_id = Uuid::new_v4();
    let aggregate_version = aggregate_version.max(1);
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at)
         VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(checked_i64(aggregate_version, "aggregate version")?)
    .bind(json!({"holding_key": holding_key.to_string(), "holding_id": holding_id}))
    .bind(json!({"principal_ref": actor_ref}))
    .bind(correlation_id)
    .bind(format!(
        "data-plane:{event_type}:{aggregate_id}:{aggregate_version}"
    ))
    .bind(&payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose,
             correlation_id, payload, recorded_at)
         VALUES ($1, $2, NULL, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_ref)
    .bind(event_type)
    .bind(purpose)
    .bind(correlation_id)
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn decode_root_access_request(
    request_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<RootAccessRequest, PersistenceError> {
    let mode: String = row.try_get("mode")?;
    let business_units: Vec<String> = row.try_get("business_units")?;
    Ok(RootAccessRequest {
        id: request_id,
        requester: row.try_get("requester_ref")?,
        device_ref: row.try_get("device_ref")?,
        holding_id: row.try_get("holding_id")?,
        agent_id: row.try_get("agent_id")?,
        active_scope: serde_json::from_value(row.try_get("active_scope")?)?,
        mode: parse_root_access_mode(&mode)?,
        allowed_actions: row.try_get("allowed_actions")?,
        business_units: business_units
            .iter()
            .map(|value| BusinessUnitId::parse(value).map_err(contract_error))
            .collect::<Result<Vec<_>, _>>()?,
        reason: row.try_get("reason")?,
        ticket_reference: row.try_get("ticket_reference")?,
        requested_at: row.try_get("requested_at")?,
        requested_until: row.try_get("requested_until")?,
    })
}

fn decode_root_access_queue_record(
    row: &sqlx::postgres::PgRow,
) -> Result<RootAccessQueueRecord, PersistenceError> {
    let request_id: Uuid = row.try_get("request_id")?;
    let request = decode_root_access_request(request_id, row)?;
    request.validate().map_err(contract_error)?;
    let state: String = row.try_get("state")?;
    if !matches!(
        state.as_str(),
        "awaiting_customer_approval" | "approved" | "rejected" | "active" | "revoked" | "expired"
    ) {
        return Err(PersistenceError::InvalidData(
            "unknown persisted Root access state".into(),
        ));
    }
    Ok(RootAccessQueueRecord {
        request,
        state,
        customer_approver_ref: row.try_get("customer_approver_ref")?,
        customer_approved_at: row.try_get("customer_approved_at")?,
        rejected_at: row.try_get("rejected_at")?,
        revoked_at: row.try_get("revoked_at")?,
        revocation_reason: row.try_get("revocation_reason")?,
    })
}

fn decode_approved_configuration(
    row: &sqlx::postgres::PgRow,
    at: DateTime<Utc>,
) -> Result<ApprovedDesiredConfiguration, PersistenceError> {
    let configuration_hash: Vec<u8> = row.try_get("configuration_hash")?;
    let configuration_hash: [u8; 32] = configuration_hash.try_into().map_err(|_| {
        PersistenceError::InvalidData("invalid persisted configuration hash".into())
    })?;
    let deployment_mode = parse_deployment_mode(&row.try_get::<String, _>("deployment_mode")?)?;
    let layer = match row.try_get::<String, _>("configuration_layer")?.as_str() {
        "platform" => ConfigurationLayer::Platform,
        "holding" => ConfigurationLayer::Holding,
        "business_unit" => ConfigurationLayer::BusinessUnit(
            BusinessUnitId::parse(&row.try_get::<String, _>("business_unit")?)
                .map_err(contract_error)?,
        ),
        _ => {
            return Err(PersistenceError::InvalidData(
                "unknown persisted configuration layer".into(),
            ));
        }
    };
    let secret_references: Vec<String> = row.try_get("secret_references")?;
    let required_capabilities: Vec<String> = row.try_get("required_capabilities")?;
    let desired = DesiredConfiguration {
        id: row.try_get("configuration_id")?,
        holding_id: row.try_get("canonical_holding_id")?,
        deployment_mode,
        target_agent_id: row.try_get("target_agent_id")?,
        layer,
        expected_observed_version: required_u64(row, "expected_observed_version")?,
        desired_version: required_u64(row, "desired_version")?,
        lifecycle: ConfigurationLifecycle::Approved,
        configuration_hash,
        secret_references: secret_references
            .into_iter()
            .map(|value| {
                erpx_domain::data_plane::SecretReference::new(value).map_err(contract_error)
            })
            .collect::<Result<Vec<_>, _>>()?,
        required_capabilities: required_capabilities
            .iter()
            .map(|value| parse_agent_capability(value))
            .collect::<Result<Vec<_>, _>>()?,
        requester: row.try_get("requester")?,
        reason: row.try_get("reason")?,
        expires_at: row.try_get("expires_at")?,
    };
    desired.validate(at).map_err(contract_error)?;
    let desired_state: Value = row.try_get("desired_state")?;
    validate_desired_state(&desired_state)?;
    Ok(ApprovedDesiredConfiguration {
        desired,
        desired_state,
        correlation_id: row.try_get("correlation_id")?,
    })
}

fn configuration_layer_parts(layer: ConfigurationLayer) -> (&'static str, Option<&'static str>) {
    match layer {
        ConfigurationLayer::Platform => ("platform", None),
        ConfigurationLayer::Holding => ("holding", None),
        ConfigurationLayer::BusinessUnit(unit) => ("business_unit", Some(unit.as_str())),
    }
}

fn root_access_mode_key(mode: RootAccessMode) -> &'static str {
    match mode {
        RootAccessMode::HealthDiagnostic => "health_diagnostic",
        RootAccessMode::InfrastructureMaintenance => "infrastructure_maintenance",
        RootAccessMode::BusinessDiagnostic => "business_diagnostic",
        RootAccessMode::BusinessMutation => "business_mutation",
        RootAccessMode::EmergencyRecovery => "emergency_recovery",
    }
}

fn parse_root_access_mode(value: &str) -> Result<RootAccessMode, PersistenceError> {
    match value {
        "health_diagnostic" => Ok(RootAccessMode::HealthDiagnostic),
        "infrastructure_maintenance" => Ok(RootAccessMode::InfrastructureMaintenance),
        "business_diagnostic" => Ok(RootAccessMode::BusinessDiagnostic),
        "business_mutation" => Ok(RootAccessMode::BusinessMutation),
        "emergency_recovery" => Ok(RootAccessMode::EmergencyRecovery),
        _ => Err(PersistenceError::InvalidData(
            "unknown persisted Root access mode".into(),
        )),
    }
}

fn parse_deployment_mode(value: &str) -> Result<HoldingDeploymentMode, PersistenceError> {
    match value {
        "erpx_managed" => Ok(HoldingDeploymentMode::ErpxManaged),
        "customer_data_plane" => Ok(HoldingDeploymentMode::CustomerDataPlane),
        _ => Err(PersistenceError::InvalidData(
            "unknown persisted holding deployment mode".into(),
        )),
    }
}

fn parse_agent_state(value: &str) -> Result<DataPlaneAgentState, PersistenceError> {
    match value {
        "unenrolled" => Ok(DataPlaneAgentState::Unenrolled),
        "enrolling" => Ok(DataPlaneAgentState::Enrolling),
        "connected" => Ok(DataPlaneAgentState::Connected),
        "healthy" => Ok(DataPlaneAgentState::Healthy),
        "drifted" => Ok(DataPlaneAgentState::Drifted),
        "degraded" => Ok(DataPlaneAgentState::Degraded),
        "offline" => Ok(DataPlaneAgentState::Offline),
        "suspended" => Ok(DataPlaneAgentState::Suspended),
        "upgrade_required" => Ok(DataPlaneAgentState::UpgradeRequired),
        _ => Err(PersistenceError::InvalidData(
            "unknown persisted data-plane agent state".into(),
        )),
    }
}

fn agent_capability_keys(values: &[erpx_domain::data_plane::AgentCapability]) -> Vec<String> {
    values
        .iter()
        .map(|capability| capability.as_str().to_owned())
        .collect()
}

fn parse_agent_capability(
    value: &str,
) -> Result<erpx_domain::data_plane::AgentCapability, PersistenceError> {
    use erpx_domain::data_plane::AgentCapability;
    match value {
        "typed_holding_queries" => Ok(AgentCapability::TypedHoldingQueries),
        "typed_holding_commands" => Ok(AgentCapability::TypedHoldingCommands),
        "directory_sync" => Ok(AgentCapability::DirectorySync),
        "projection_worker" => Ok(AgentCapability::ProjectionWorker),
        "backup" => Ok(AgentCapability::Backup),
        "restore" => Ok(AgentCapability::Restore),
        "migration" => Ok(AgentCapability::Migration),
        "configuration_reconciliation" => Ok(AgentCapability::ConfigurationReconciliation),
        "health_telemetry" => Ok(AgentCapability::HealthTelemetry),
        _ => Err(PersistenceError::InvalidData(
            "unknown persisted data-plane agent capability".into(),
        )),
    }
}

fn business_unit_keys(values: &[BusinessUnitId]) -> Vec<String> {
    values.iter().map(|unit| unit.as_str().to_owned()).collect()
}

fn configuration_id(holding_id: &Uuid, business_unit: BusinessUnitId) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx://holding/{holding_id}/business-unit/{}",
            business_unit.as_str()
        )
        .as_bytes(),
    )
}

fn scope_holding_id(scope: &Scope) -> Uuid {
    match scope {
        Scope::Platform => Uuid::nil(),
        Scope::Holding { holding_id }
        | Scope::Company { holding_id, .. }
        | Scope::Branch { holding_id, .. }
        | Scope::Subtree { holding_id, .. } => *holding_id,
    }
}

fn constant_time_hash_eq(stored: &[u8], presented: &[u8; 32]) -> bool {
    stored.len() == 32 && stored.ct_eq(presented.as_slice()).unwrap_u8() == 1
}

fn checked_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds SQL range")))
}

fn required_u64(row: &sqlx::postgres::PgRow, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(row.try_get::<i64, _>(field)?)
        .map_err(|_| PersistenceError::InvalidData(format!("invalid persisted {field}")))
}

fn optional_u64(row: &sqlx::postgres::PgRow, field: &str) -> Result<Option<u64>, PersistenceError> {
    row.try_get::<Option<i64>, _>(field)?
        .map(|value| {
            u64::try_from(value)
                .map_err(|_| PersistenceError::InvalidData(format!("invalid persisted {field}")))
        })
        .transpose()
}

fn validate_reference(value: &str, field: &str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!(
            "invalid {field} reference"
        )));
    }
    Ok(())
}

fn validate_correlation_id(value: Uuid) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(
            "correlation_id must be non-nil".into(),
        ));
    }
    Ok(())
}

fn contract_error(error: DataPlaneContractError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_state_rejects_embedded_secret_material() {
        assert!(validate_desired_state(&json!({"database_password": "hidden"})).is_err());
        assert!(
            validate_desired_state(&json!({"database": "postgres://user:pass@db/holding"}))
                .is_err()
        );
        assert!(validate_desired_state(&json!({"token": "opaque"})).is_err());
    }

    #[test]
    fn desired_state_accepts_redacted_configuration_metadata() {
        validate_desired_state(&json!({
            "projection": {"max_lag_seconds": 30},
            "directory": {"provider": "active-directory", "enabled": true},
            "business_units": ["finance", "inventory"]
        }))
        .expect("safe metadata should be accepted");
    }

    #[test]
    fn hash_comparison_is_exact_and_constant_time_primitive_backed() {
        let expected = [7_u8; 32];
        assert!(constant_time_hash_eq(expected.as_slice(), &expected));
        assert!(!constant_time_hash_eq(&expected[..31], &expected));
        assert!(!constant_time_hash_eq(expected.as_slice(), &[8_u8; 32]));
    }
}
