//! SQL-backed adapters owned by the Server composition layer.
//!
//! Transport and orchestration stay in `lib.rs`; SQL statements and their
//! transaction boundaries stay here so bounded-context composition cannot
//! grow new persistence logic accidentally.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    access::{HOLDING_OWNER_ROLE, RiskTier, RoleAssignment, Scope},
    corporate_identity::{
        BUSINESS_UNIT_BRAND_MANAGE, BUSINESS_UNIT_BRAND_PUBLISH, BUSINESS_UNIT_BRAND_READ,
        TENANT_BRAND_LOCK, TENANT_BRAND_MANAGE, TENANT_BRAND_PUBLISH, TENANT_BRAND_READ,
    },
    data_plane::{
        BUSINESS_UNIT_CONFIGURATION_MANAGE, BUSINESS_UNIT_CONFIGURATION_READ, TENANT_AGENT_ENROLL,
        TENANT_AGENT_ROTATE, TENANT_INFRASTRUCTURE_APPLY, TENANT_INFRASTRUCTURE_APPROVE,
        TENANT_INFRASTRUCTURE_PROPOSE, TENANT_INFRASTRUCTURE_READ, TENANT_ROOT_ACCESS_APPROVE,
        TENANT_ROOT_ACCESS_READ, TENANT_ROOT_ACCESS_REVOKE,
    },
};
use erpx_identity::{PasswordService, RecoveryCodeSet};
use erpx_infrastructure::{SqlxControlPlane, TenantOwnerSeed};
use serde_json::Value;
use sqlx::Row;
use tonic::Status;
use uuid::Uuid;

pub(super) async fn directory_audit_customer(
    store: &SqlxControlPlane,
    holding_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT customer_id FROM erpx_control.customer_tenant_bindings
         WHERE root_holding_id=$1",
    )
    .bind(holding_id)
    .fetch_optional(&store.pools().write)
    .await
}

pub(super) async fn branch_capacity_available(
    store: &SqlxControlPlane,
    tenant_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM erpx_control.customer_tenant_bindings b
             JOIN erpx_control.entitlement_allocation_books e
               ON e.customer_id=b.customer_id
             WHERE b.tenant_id=$1 AND e.active
               AND (e.active_until IS NULL OR e.active_until > now())
               AND COALESCE(
                    CASE WHEN e.payload #>> '{capacity,branches}' ~ '^[0-9]+$'
                         THEN (e.payload #>> '{capacity,branches}')::BIGINT END,
                    0
               ) > (
                    SELECT count(*) FROM erpx_control.branch_database_routes r
                    WHERE r.tenant_id=$1 AND r.state <> 'suspended'
               )
         )",
    )
    .bind(tenant_id)
    .fetch_one(&store.pools().write)
    .await
}

#[derive(Debug)]
pub(super) struct TenantOwnerSeedReceipt {
    pub(super) seeded: bool,
    pub(super) principal_ref: String,
    pub(super) recovery_code: Option<String>,
}

/// Persist the identity, recovery code, finite holding-owner assignment, and
/// owner-seed state in one control-write transaction. The plaintext recovery
/// code is retained only in the returned value and is never logged or stored.
pub(super) async fn seed_tenant_owner(
    store: &SqlxControlPlane,
    seed: &TenantOwnerSeed,
    grantor_ref: &str,
    at: DateTime<Utc>,
) -> Result<TenantOwnerSeedReceipt, Status> {
    if seed.state == "seeded" {
        return Ok(TenantOwnerSeedReceipt {
            seeded: true,
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }
    if seed.state != "pending" {
        return Err(Status::failed_precondition(
            "tenant owner seed is in an invalid state",
        ));
    }
    let recovery_batch = RecoveryCodeSet::issue(
        seed.principal_ref.clone(),
        1,
        &PasswordService::default(),
        at,
    )
    .map_err(|_| Status::internal("owner recovery material could not be generated"))?;
    let recovery_code = recovery_batch.code(0).map(str::to_owned);
    let role_assignment = RoleAssignment {
        id: seed.role_assignment_id,
        principal_ref: seed.principal_ref.clone(),
        role_key: HOLDING_OWNER_ROLE.into(),
        permissions: vec![
            BUSINESS_UNIT_BRAND_MANAGE.into(),
            BUSINESS_UNIT_BRAND_PUBLISH.into(),
            BUSINESS_UNIT_BRAND_READ.into(),
            BUSINESS_UNIT_CONFIGURATION_MANAGE.into(),
            BUSINESS_UNIT_CONFIGURATION_READ.into(),
            "branch.infrastructure.provision".into(),
            "organization.admin".into(),
            "organization.read".into(),
            "operations.read".into(),
            "operations.exception.manage".into(),
            "role.assignment.read".into(),
            "role.assignment.approve".into(),
            "role.assignment.review".into(),
            "role.assignment.renew".into(),
            super::TENANT_DIRECTORY_ACTIVATE_PERMISSION.into(),
            super::TENANT_DIRECTORY_MANAGE_PERMISSION.into(),
            super::TENANT_DIRECTORY_READ_PERMISSION.into(),
            "role.assignment.recover".into(),
            TENANT_AGENT_ENROLL.into(),
            TENANT_AGENT_ROTATE.into(),
            TENANT_BRAND_LOCK.into(),
            TENANT_BRAND_MANAGE.into(),
            TENANT_BRAND_PUBLISH.into(),
            TENANT_BRAND_READ.into(),
            TENANT_INFRASTRUCTURE_APPLY.into(),
            TENANT_INFRASTRUCTURE_APPROVE.into(),
            TENANT_INFRASTRUCTURE_PROPOSE.into(),
            TENANT_INFRASTRUCTURE_READ.into(),
            TENANT_ROOT_ACCESS_READ.into(),
            TENANT_ROOT_ACCESS_APPROVE.into(),
            TENANT_ROOT_ACCESS_REVOKE.into(),
        ],
        functional_domain: "organization".into(),
        authority_level: 100,
        approval_limit_minor: 0,
        scope: Scope::Holding {
            holding_id: seed.holding_id,
        },
        valid_from: at,
        valid_until: at + Duration::days(90),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Administrator,
        grant_reason: "initial holding owner bootstrap".into(),
        ticket_reference: None,
        grantor: grantor_ref.into(),
        approver: Some(seed.approver_ref.clone()),
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    role_assignment
        .validate_for_grant(None)
        .map_err(|_| Status::failed_precondition("initial owner assignment is invalid"))?;
    let role_payload = serde_json::to_value(&role_assignment)
        .map_err(|_| Status::internal("owner assignment could not be serialized"))?;

    let mut transaction = store
        .pools()
        .write
        .begin()
        .await
        .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
    let seed_row = sqlx::query(
        "SELECT holding_ref, holding_id, principal_ref, approver_ref,
                role_assignment_id, state, recovery_issued_at
         FROM erpx_control.tenant_owner_seeds
         WHERE tenant_id = $1
         FOR UPDATE",
    )
    .bind(&seed.tenant_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?
    .ok_or_else(|| Status::failed_precondition("tenant owner seed is missing"))?;
    let stored_state: String = seed_row
        .try_get("state")
        .map_err(|_| Status::internal("tenant owner seed record is invalid"))?;
    let stored_recovery_issued_at: Option<DateTime<Utc>> =
        seed_row
            .try_get("recovery_issued_at")
            .map_err(|_| Status::internal("tenant owner seed record is invalid"))?;
    let immutable_match = seed_row
        .try_get::<String, _>("holding_ref")
        .map(|value| value == seed.holding_ref)
        .unwrap_or(false)
        && seed_row
            .try_get::<Uuid, _>("holding_id")
            .map(|value| value == seed.holding_id)
            .unwrap_or(false)
        && seed_row
            .try_get::<String, _>("principal_ref")
            .map(|value| value == seed.principal_ref)
            .unwrap_or(false)
        && seed_row
            .try_get::<String, _>("approver_ref")
            .map(|value| value == seed.approver_ref)
            .unwrap_or(false)
        && seed_row
            .try_get::<Uuid, _>("role_assignment_id")
            .map(|value| value == seed.role_assignment_id)
            .unwrap_or(false);
    if !immutable_match {
        return Err(Status::failed_precondition(
            "tenant owner seed immutable references changed",
        ));
    }
    if stored_state == "seeded" || stored_recovery_issued_at.is_some() {
        transaction
            .commit()
            .await
            .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
        return Ok(TenantOwnerSeedReceipt {
            seeded: stored_state == "seeded",
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }

    let generated_principal_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:principal:v1:{}", seed.principal_ref).as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, created_at, updated_at)
         VALUES ($1, $2, 'recovery_required', $3, $3)
         ON CONFLICT (principal_ref) DO NOTHING",
    )
    .bind(generated_principal_id)
    .bind(&seed.principal_ref)
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner identity write unavailable"))?;
    let principal_row = sqlx::query(
        "SELECT id, status
         FROM erpx_identity.principals
         WHERE principal_ref = $1
         FOR UPDATE",
    )
    .bind(&seed.principal_ref)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner identity write unavailable"))?;
    let principal_id: Uuid = principal_row
        .try_get("id")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    let principal_status: String = principal_row
        .try_get("status")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    if principal_status == "suspended" {
        return Err(Status::failed_precondition("owner identity is suspended"));
    }
    let needs_recovery = principal_status == "recovery_required";
    if needs_recovery {
        sqlx::query("DELETE FROM erpx_identity.recovery_codes WHERE principal_id = $1")
            .bind(principal_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| Status::unavailable("owner recovery write unavailable"))?;
        for record in recovery_batch.set().records() {
            sqlx::query(
                "INSERT INTO erpx_identity.recovery_codes
                    (id, principal_id, code_hash, issued_at, consumed_at)
                 VALUES ($1, $2, $3, $4, NULL)",
            )
            .bind(record.id())
            .bind(principal_id)
            .bind(record.hash().as_str())
            .bind(recovery_batch.set().issued_at())
            .execute(&mut *transaction)
            .await
            .map_err(|_| Status::unavailable("owner recovery write unavailable"))?;
        }
    }

    sqlx::query(
        "INSERT INTO erpx_control.role_assignments
            (id, principal_ref, role_key, parent_assignment_id, valid_from,
             valid_until, assignment_version, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(role_assignment.id)
    .bind(&role_assignment.principal_ref)
    .bind(&role_assignment.role_key)
    .bind(role_assignment.parent_assignment_id)
    .bind(role_assignment.valid_from)
    .bind(role_assignment.valid_until)
    .bind(
        i32::try_from(role_assignment.assignment_version)
            .map_err(|_| Status::internal("owner assignment version is invalid"))?,
    )
    .bind(role_payload)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner role write unavailable"))?;
    sqlx::query(
        "INSERT INTO erpx_control.role_assignment_target_policies
            (assignment_id, policy_kind, environment_gate, configured_by)
         VALUES ($1, 'unrestricted', NULL, $2)
         ON CONFLICT (assignment_id) DO NOTHING",
    )
    .bind(role_assignment.id)
    .bind(grantor_ref)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner role target-policy write unavailable"))?;
    let stored_role_payload: Value =
        sqlx::query("SELECT payload FROM erpx_control.role_assignments WHERE id = $1")
            .bind(role_assignment.id)
            .fetch_one(&mut *transaction)
            .await
            .and_then(|row| row.try_get("payload"))
            .map_err(|_| Status::unavailable("owner role read unavailable"))?;
    let stored_assignment: RoleAssignment = serde_json::from_value(stored_role_payload)
        .map_err(|_| Status::failed_precondition("owner role payload is invalid"))?;
    if stored_assignment.canonical_bytes().ok() != role_assignment.canonical_bytes().ok() {
        return Err(Status::failed_precondition(
            "owner role assignment conflicts with existing state",
        ));
    }
    sqlx::query(
        "UPDATE erpx_control.tenant_owner_seeds
         SET state = 'seeded',
             recovery_issued_at = CASE WHEN $2 THEN $3 ELSE NULL END,
             updated_at = $4
         WHERE tenant_id = $1",
    )
    .bind(&seed.tenant_id)
    .bind(needs_recovery)
    .bind(if needs_recovery { Some(at) } else { None })
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("tenant owner seed update unavailable"))?;
    transaction
        .commit()
        .await
        .map_err(|_| Status::unavailable("tenant owner seed transaction unavailable"))?;
    Ok(TenantOwnerSeedReceipt {
        seeded: true,
        principal_ref: seed.principal_ref.clone(),
        recovery_code: if needs_recovery { recovery_code } else { None },
    })
}
