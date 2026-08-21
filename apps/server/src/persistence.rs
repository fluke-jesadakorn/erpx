//! SQL-backed adapters owned by the Server composition layer.
//!
//! Transport and orchestration stay in `lib.rs`; SQL statements and their
//! transaction boundaries stay here so bounded-context composition cannot
//! grow new persistence logic accidentally.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    access::{HOLDING_OWNER_ROLE, RiskTier, RoleAssignment},
    contracts::AuthorityScope as Scope,
    corporate_identity::{
        BUSINESS_UNIT_BRAND_MANAGE, BUSINESS_UNIT_BRAND_PUBLISH, BUSINESS_UNIT_BRAND_READ,
        HOLDING_BRAND_LOCK, HOLDING_BRAND_MANAGE, HOLDING_BRAND_PUBLISH, HOLDING_BRAND_READ,
    },
    data_plane::{
        BUSINESS_UNIT_CONFIGURATION_MANAGE, BUSINESS_UNIT_CONFIGURATION_READ, HOLDING_AGENT_ENROLL,
        HOLDING_AGENT_ROTATE, HOLDING_INFRASTRUCTURE_APPLY, HOLDING_INFRASTRUCTURE_APPROVE,
        HOLDING_INFRASTRUCTURE_PROPOSE, HOLDING_INFRASTRUCTURE_READ, HOLDING_ROOT_ACCESS_APPROVE,
        HOLDING_ROOT_ACCESS_READ, HOLDING_ROOT_ACCESS_REVOKE,
    },
};
use erpx_identity::{PasswordService, RecoveryCodeSet};
use erpx_infrastructure::{HoldingOwnerSeed, SqlxControlPlane};
use serde_json::Value;
use sqlx::Row;
use tonic::Status;
use uuid::Uuid;

pub(super) async fn directory_audit_customer(
    store: &SqlxControlPlane,
    holding_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT customer_id FROM erpx_control.customer_holding_bindings
         WHERE canonical_holding_id=$1",
    )
    .bind(holding_id)
    .fetch_optional(&store.pools().write)
    .await
}

pub(super) async fn branch_capacity_available(
    store: &SqlxControlPlane,
    holding_id: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM erpx_control.customer_holding_bindings b
             JOIN erpx_control.entitlement_allocation_books e
               ON e.customer_id=b.customer_id
             WHERE b.holding_id=$1 AND e.active
               AND (e.active_until IS NULL OR e.active_until > now())
               AND COALESCE(
                    CASE WHEN e.payload #>> '{capacity,branches}' ~ '^[0-9]+$'
                         THEN (e.payload #>> '{capacity,branches}')::BIGINT END,
                    0
               ) > (
                    SELECT count(*) FROM erpx_control.branch_database_routes r
                    WHERE r.holding_id=$1 AND r.state <> 'suspended'
               )
         )",
    )
    .bind(holding_id)
    .fetch_one(&store.pools().write)
    .await
}

#[derive(Debug)]
pub(super) struct HoldingOwnerSeedReceipt {
    pub(super) seeded: bool,
    pub(super) principal_ref: String,
    pub(super) recovery_code: Option<String>,
}

/// Persist the identity, recovery code, finite holding-owner assignment, and
/// owner-seed state in one control-write transaction. The plaintext recovery
/// code is retained only in the returned value and is never logged or stored.
pub(super) async fn seed_holding_owner(
    store: &SqlxControlPlane,
    seed: &HoldingOwnerSeed,
    grantor_ref: &str,
    at: DateTime<Utc>,
) -> Result<HoldingOwnerSeedReceipt, Status> {
    if seed.state == "seeded" {
        return Ok(HoldingOwnerSeedReceipt {
            seeded: true,
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }
    if seed.state != "pending" {
        return Err(Status::failed_precondition(
            "holding owner seed is in an invalid state",
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
            super::HOLDING_DIRECTORY_ACTIVATE_PERMISSION.into(),
            super::HOLDING_DIRECTORY_MANAGE_PERMISSION.into(),
            super::HOLDING_DIRECTORY_READ_PERMISSION.into(),
            "role.assignment.recover".into(),
            HOLDING_AGENT_ENROLL.into(),
            HOLDING_AGENT_ROTATE.into(),
            HOLDING_BRAND_LOCK.into(),
            HOLDING_BRAND_MANAGE.into(),
            HOLDING_BRAND_PUBLISH.into(),
            HOLDING_BRAND_READ.into(),
            HOLDING_INFRASTRUCTURE_APPLY.into(),
            HOLDING_INFRASTRUCTURE_APPROVE.into(),
            HOLDING_INFRASTRUCTURE_PROPOSE.into(),
            HOLDING_INFRASTRUCTURE_READ.into(),
            HOLDING_ROOT_ACCESS_READ.into(),
            HOLDING_ROOT_ACCESS_APPROVE.into(),
            HOLDING_ROOT_ACCESS_REVOKE.into(),
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
        .map_err(|_| Status::unavailable("holding owner seed transaction unavailable"))?;
    let seed_row = sqlx::query(
        "SELECT data_profile_key, holding_id, principal_ref, approver_ref,
                role_assignment_id, state, recovery_issued_at
         FROM erpx_control.holding_owner_seeds
         WHERE holding_id = $1
         FOR UPDATE",
    )
    .bind(seed.holding_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("holding owner seed transaction unavailable"))?
    .ok_or_else(|| Status::failed_precondition("holding owner seed is missing"))?;
    let stored_state: String = seed_row
        .try_get("state")
        .map_err(|_| Status::internal("holding owner seed record is invalid"))?;
    let stored_recovery_issued_at: Option<DateTime<Utc>> =
        seed_row
            .try_get("recovery_issued_at")
            .map_err(|_| Status::internal("holding owner seed record is invalid"))?;
    let immutable_match = seed_row
        .try_get::<String, _>("data_profile_key")
        .map(|value| value == seed.data_profile_key)
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
            "holding owner seed immutable references changed",
        ));
    }
    if stored_state == "seeded" || stored_recovery_issued_at.is_some() {
        transaction
            .commit()
            .await
            .map_err(|_| Status::unavailable("holding owner seed transaction unavailable"))?;
        return Ok(HoldingOwnerSeedReceipt {
            seeded: stored_state == "seeded",
            principal_ref: seed.principal_ref.clone(),
            recovery_code: None,
        });
    }

    let generated_principal_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:principal:{}", seed.principal_ref).as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_identity.principals
            (id, principal_ref, status, authenticated_realm, principal_kind,
             created_at, updated_at)
         VALUES ($1, $2, 'recovery_required', 'internal_workforce', 'human', $3, $3)
         ON CONFLICT (principal_ref) DO NOTHING",
    )
    .bind(generated_principal_id)
    .bind(&seed.principal_ref)
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("owner identity write unavailable"))?;
    let principal_row = sqlx::query(
        "SELECT id, status, authenticated_realm, principal_kind
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
    let principal_realm: String = principal_row
        .try_get("authenticated_realm")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    let principal_kind: String = principal_row
        .try_get("principal_kind")
        .map_err(|_| Status::internal("owner identity record is invalid"))?;
    if principal_realm != "internal_workforce" || principal_kind != "human" {
        return Err(Status::failed_precondition(
            "holding owner authenticated authority is contradictory",
        ));
    }
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
        "UPDATE erpx_control.holding_owner_seeds
         SET state = 'seeded',
             recovery_issued_at = CASE WHEN $2 THEN $3 ELSE NULL END,
             updated_at = $4
         WHERE holding_id = $1",
    )
    .bind(seed.holding_id)
    .bind(needs_recovery)
    .bind(if needs_recovery { Some(at) } else { None })
    .bind(at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| Status::unavailable("holding owner seed update unavailable"))?;
    transaction
        .commit()
        .await
        .map_err(|_| Status::unavailable("holding owner seed transaction unavailable"))?;
    Ok(HoldingOwnerSeedReceipt {
        seeded: true,
        principal_ref: seed.principal_ref.clone(),
        recovery_code: if needs_recovery { recovery_code } else { None },
    })
}
