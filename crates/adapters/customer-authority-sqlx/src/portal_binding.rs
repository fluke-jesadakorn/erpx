use async_trait::async_trait;
use erpx_cqrs_sqlx::WritePool;
use erpx_customer_authority::{
    CustomerPortalBindingId, CustomerPortalBindingStatus, CustomerPortalPrincipalBinding,
    CustomerRecordAuthorityPort, PortError, PortalBindingAction, PortalBindingCommandUnitOfWork,
    PortalBindingCommit, PortalBindingWriteRepository, PortalPrincipalKind, PortalUseCase,
    StoredPortalBindingCommit,
};
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityEvidenceFact,
    AuthorityPortalBindingStatus, AuthorityPortalUseCase, AuthorityPrincipalKind, AuthorityRealm,
    CustomerPortalBindingAuthorityEvidence, RecordVersionRef,
};
use erpx_kernel::{AuthorityScope, CorrelationId, PrincipalId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CustomerPortalBindingSqlxAdapter {
    pool: WritePool,
}

impl CustomerPortalBindingSqlxAdapter {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
pub struct SqlxCustomerRecordAuthority {
    control_pool: PgPool,
}

impl SqlxCustomerRecordAuthority {
    pub const fn new(control_pool: PgPool) -> Self {
        Self { control_pool }
    }
}

#[async_trait]
impl CustomerRecordAuthorityPort for SqlxCustomerRecordAuthority {
    async fn ensure_active_customer_version(
        &self,
        customer: &RecordVersionRef,
        scope: &AuthorityScope,
    ) -> Result<(), PortError> {
        customer.validate().map_err(|_| PortError::Denied)?;
        if customer.record().owner_context()
            != erpx_integration_contracts::catalog::ContextId::CustomerAuthority
            || customer.record().record_type().as_str() != "customer.account"
        {
            return Err(PortError::Denied);
        }
        let row = sqlx::query(
            "SELECT aggregate_version,primary_holding_id,primary_company_id,
                    customer_status,merged_into_customer_id
             FROM erpx_customer_authority_write.account_guards
             WHERE customer_id=$1",
        )
        .bind(customer.record().id().as_uuid())
        .fetch_optional(&self.control_pool)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::NotFound)?;
        let version: i64 = row
            .try_get("aggregate_version")
            .map_err(|_| PortError::Internal)?;
        let status: String = row
            .try_get("customer_status")
            .map_err(|_| PortError::Internal)?;
        let merged: Option<Uuid> = row
            .try_get("merged_into_customer_id")
            .map_err(|_| PortError::Internal)?;
        let customer_scope = AuthorityScope::Company {
            holding_id: row
                .try_get("primary_holding_id")
                .map_err(|_| PortError::Internal)?,
            company_id: row
                .try_get("primary_company_id")
                .map_err(|_| PortError::Internal)?,
        };
        if u64::try_from(version).ok() != Some(customer.version())
            || status != "active"
            || merged.is_some()
            || !customer_scope.contains(scope)
        {
            return Err(PortError::Denied);
        }
        Ok(())
    }
}

#[async_trait]
impl PortalBindingWriteRepository for CustomerPortalBindingSqlxAdapter {
    async fn load_portal_binding(
        &self,
        authority_scope: &AuthorityScope,
        binding_id: CustomerPortalBindingId,
    ) -> Result<Option<CustomerPortalPrincipalBinding>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let value = load_binding(&mut transaction, binding_id, false).await?;
        transaction.commit().await.map_err(map_database)?;
        if value
            .as_ref()
            .is_some_and(|binding| !authority_scope.contains(binding.scope()))
        {
            return Err(PortError::Denied);
        }
        Ok(value)
    }
}

#[async_trait]
impl PortalBindingCommandUnitOfWork for CustomerPortalBindingSqlxAdapter {
    async fn find_portal_binding_receipt(
        &self,
        request: &erpx_customer_authority::PortalBindingCommandRequest,
    ) -> Result<Option<StoredPortalBindingCommit>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let receipt = find_receipt(&mut transaction, request).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(receipt)
    }

    async fn commit_portal_binding(
        &self,
        request: PortalBindingCommit,
    ) -> Result<StoredPortalBindingCommit, PortError> {
        validate_commit(&request)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        if let Some(receipt) = find_receipt(&mut transaction, &request.request).await? {
            transaction.commit().await.map_err(map_database)?;
            return Ok(receipt);
        }
        let current = load_binding(&mut transaction, request.aggregate.id(), true).await?;
        match (request.request.expected_version, current.as_ref()) {
            (None, None) if request.aggregate.version() == 1 => {}
            (Some(expected), Some(current))
                if current.version() == expected
                    && request.aggregate.version() == expected.saturating_add(1) => {}
            (Some(expected), Some(current)) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: current.version(),
                });
            }
            (Some(expected), None) => {
                return Err(PortError::Conflict {
                    expected,
                    actual: 0,
                });
            }
            (None, Some(current)) => {
                return Err(PortError::Conflict {
                    expected: 0,
                    actual: current.version(),
                });
            }
            (None, None) => return Err(PortError::Internal),
        }
        persist_binding(&mut transaction, &request.aggregate).await?;
        let fact = binding_fact(&request.aggregate)?;
        append_authority_event(&mut transaction, &request, &fact).await?;
        persist_audit(&mut transaction, &request).await?;
        persist_receipt(&mut transaction, &request).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(StoredPortalBindingCommit {
            command_id: request.request.identity.command_id,
            correlation_id: request.request.correlation_id,
            aggregate: request.aggregate,
            committed_at: request.changed_at,
            replayed: false,
        })
    }
}

async fn load_binding(
    transaction: &mut Transaction<'_, Postgres>,
    binding_id: CustomerPortalBindingId,
    lock: bool,
) -> Result<Option<CustomerPortalPrincipalBinding>, PortError> {
    let lock = if lock { " FOR UPDATE" } else { "" };
    let statement = format!(
        "SELECT binding_version,customer_record_id,customer_record_version,
                portal_principal_id,principal_kind,binding_scope,
                verified_identity_digest,verified_identity_version,verified_at,
                permitted_use_cases,valid_from,valid_until,proposed_by,approved_by,
                rejected_by,revoked_by,ended_at,decision_evidence_digest,binding_status,changed_at
         FROM erpx_authority_write.customer_portal_principal_bindings
         WHERE holding_id=erpx_authority_write.current_holding_id() AND binding_id=$1{lock}"
    );
    let row = sqlx::query(&statement)
        .bind(binding_id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database)?;
    row.map(|row| binding_from_row(binding_id, &row))
        .transpose()
}

fn binding_from_row(
    binding_id: CustomerPortalBindingId,
    row: &sqlx::postgres::PgRow,
) -> Result<CustomerPortalPrincipalBinding, PortError> {
    let customer_id: Uuid = row.try_get("customer_record_id").map_err(map_database)?;
    let principal_kind: String = row.try_get("principal_kind").map_err(map_database)?;
    let use_cases: Vec<String> = row.try_get("permitted_use_cases").map_err(map_database)?;
    let status: String = row.try_get("binding_status").map_err(map_database)?;
    let state = json!({
        "id": binding_id.as_uuid(),
        "version": row.try_get::<i64, _>("binding_version").map_err(map_database)?,
        "customer": {
            "record": {
                "id": customer_id,
                "owner_context": "customer_authority",
                "record_type": "customer.account"
            },
            "version": row.try_get::<i64, _>("customer_record_version").map_err(map_database)?
        },
        "portal_principal_id": row.try_get::<Uuid, _>("portal_principal_id").map_err(map_database)?,
        "principal_kind": principal_kind,
        "scope": row.try_get::<serde_json::Value, _>("binding_scope").map_err(map_database)?,
        "verified_identity": {
            "digest": row.try_get::<String, _>("verified_identity_digest").map_err(map_database)?,
            "version": row.try_get::<i64, _>("verified_identity_version").map_err(map_database)?,
            "verified_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("verified_at").map_err(map_database)?
        },
        "permitted_use_cases": use_cases,
        "valid_from": row.try_get::<chrono::DateTime<chrono::Utc>, _>("valid_from").map_err(map_database)?,
        "valid_until": row.try_get::<chrono::DateTime<chrono::Utc>, _>("valid_until").map_err(map_database)?,
        "proposed_by": row.try_get::<Uuid, _>("proposed_by").map_err(map_database)?,
        "approved_by": row.try_get::<Option<Uuid>, _>("approved_by").map_err(map_database)?,
        "rejected_by": row.try_get::<Option<Uuid>, _>("rejected_by").map_err(map_database)?,
        "revoked_by": row.try_get::<Option<Uuid>, _>("revoked_by").map_err(map_database)?,
        "ended_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at").map_err(map_database)?,
        "decision_evidence_digest": row.try_get::<Option<String>, _>("decision_evidence_digest").map_err(map_database)?,
        "status": status,
        "changed_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("changed_at").map_err(map_database)?
    });
    let value: CustomerPortalPrincipalBinding =
        serde_json::from_value(state).map_err(|_| PortError::Internal)?;
    value
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    Ok(value)
}

async fn find_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    request: &erpx_customer_authority::PortalBindingCommandRequest,
) -> Result<Option<StoredPortalBindingCommit>, PortError> {
    let identity = &request.identity;
    let row = sqlx::query(
        "SELECT command_id,request_hash,binding_id,binding_version,correlation_id,receipt,committed_at
         FROM erpx_authority_write.portal_binding_command_receipts
         WHERE holding_id=erpx_authority_write.current_holding_id()
           AND (idempotency_key=$1 OR command_id=$2) FOR UPDATE",
    )
    .bind(identity.idempotency_key.as_str())
    .bind(identity.command_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_database)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let hash: Vec<u8> = row.try_get("request_hash").map_err(map_database)?;
    if row.try_get::<Uuid, _>("command_id").map_err(map_database)? != identity.command_id.as_uuid()
        || hash.as_slice() != request_hash(request)?.as_slice()
    {
        return Err(PortError::IdempotencyConflict);
    }
    let aggregate: CustomerPortalPrincipalBinding =
        serde_json::from_value(row.try_get("receipt").map_err(map_database)?)
            .map_err(|_| PortError::Internal)?;
    aggregate
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if aggregate.id().as_uuid() != row.try_get::<Uuid, _>("binding_id").map_err(map_database)?
        || i64::try_from(aggregate.version()).ok()
            != Some(row.try_get("binding_version").map_err(map_database)?)
    {
        return Err(PortError::Internal);
    }
    Ok(Some(StoredPortalBindingCommit {
        command_id: identity.command_id,
        correlation_id: CorrelationId::try_from_uuid(
            row.try_get("correlation_id").map_err(map_database)?,
        )
        .map_err(|_| PortError::Internal)?,
        aggregate,
        committed_at: row.try_get("committed_at").map_err(map_database)?,
        replayed: true,
    }))
}

async fn persist_binding(
    transaction: &mut Transaction<'_, Postgres>,
    value: &CustomerPortalPrincipalBinding,
) -> Result<(), PortError> {
    let result = sqlx::query(
        "INSERT INTO erpx_authority_write.customer_portal_principal_bindings
            (holding_id,binding_id,binding_version,customer_record_id,customer_record_version,
             portal_principal_id,principal_kind,portal_realm,binding_scope,
             verified_identity_digest,verified_identity_version,verified_at,permitted_use_cases,
             valid_from,valid_until,proposed_by,approved_by,rejected_by,revoked_by,ended_at,
             decision_evidence_digest,binding_status,changed_at)
         VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,$5,$6,'customer_portal',
                 $7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
         ON CONFLICT (holding_id,binding_id) DO UPDATE SET
            binding_version=EXCLUDED.binding_version,approved_by=EXCLUDED.approved_by,
            rejected_by=EXCLUDED.rejected_by,revoked_by=EXCLUDED.revoked_by,
            ended_at=EXCLUDED.ended_at,
            decision_evidence_digest=EXCLUDED.decision_evidence_digest,
            binding_status=EXCLUDED.binding_status,changed_at=EXCLUDED.changed_at
         WHERE erpx_authority_write.customer_portal_principal_bindings.holding_id
               = erpx_authority_write.current_holding_id()
           AND erpx_authority_write.customer_portal_principal_bindings.binding_version + 1
               = EXCLUDED.binding_version",
    )
    .bind(value.id().as_uuid())
    .bind(to_i64(value.version())?)
    .bind(value.customer().record().id().as_uuid())
    .bind(to_i64(value.customer().version())?)
    .bind(value.portal_principal_id().as_uuid())
    .bind(principal_kind_name(value.principal_kind()))
    .bind(serde_json::to_value(value.scope()).map_err(|_| PortError::Internal)?)
    .bind(value.verified_identity().digest())
    .bind(to_i64(value.verified_identity().version())?)
    .bind(value.verified_identity().verified_at())
    .bind(
        value
            .permitted_use_cases()
            .iter()
            .copied()
            .map(use_case_name)
            .collect::<Vec<_>>(),
    )
    .bind(value.valid_from())
    .bind(value.valid_until())
    .bind(value.proposed_by().as_uuid())
    .bind(value.approved_by().map(PrincipalId::as_uuid))
    .bind(value.rejected_by().map(PrincipalId::as_uuid))
    .bind(value.revoked_by().map(PrincipalId::as_uuid))
    .bind(value.ended_at())
    .bind(value.decision_evidence_digest())
    .bind(status_name(value.status()))
    .bind(value.changed_at())
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    if result.rows_affected() != 1 {
        return Err(PortError::Conflict {
            expected: value.version().saturating_sub(1),
            actual: value.version(),
        });
    }
    Ok(())
}

fn binding_fact(
    value: &CustomerPortalPrincipalBinding,
) -> Result<AuthorityEvidenceFact, PortError> {
    let fact =
        AuthorityEvidenceFact::CustomerPortalBinding(CustomerPortalBindingAuthorityEvidence {
            binding_id: value.id().as_uuid(),
            binding_version: value.version(),
            customer: value.customer().clone(),
            portal_principal_id: value.portal_principal_id(),
            principal_kind: match value.principal_kind() {
                PortalPrincipalKind::Human => AuthorityPrincipalKind::Human,
                PortalPrincipalKind::Service => AuthorityPrincipalKind::Service,
            },
            realm: AuthorityRealm::CustomerPortal,
            scope: value.scope().clone(),
            verified_identity_digest: value.verified_identity().digest().to_owned(),
            verified_identity_version: value.verified_identity().version(),
            permitted_use_cases: value
                .permitted_use_cases()
                .iter()
                .map(|value| match value {
                    PortalUseCase::FieldServiceRecordAccess => {
                        AuthorityPortalUseCase::FieldServiceRecordAccess
                    }
                })
                .collect(),
            valid_from: value.valid_from(),
            valid_until: value.valid_until(),
            proposed_by: value.proposed_by(),
            approved_by: value.approved_by(),
            ended_at: value.ended_at(),
            status: match value.status() {
                CustomerPortalBindingStatus::Proposed => AuthorityPortalBindingStatus::Proposed,
                CustomerPortalBindingStatus::Active => AuthorityPortalBindingStatus::Active,
                CustomerPortalBindingStatus::Rejected => AuthorityPortalBindingStatus::Rejected,
                CustomerPortalBindingStatus::Revoked => AuthorityPortalBindingStatus::Revoked,
                CustomerPortalBindingStatus::Expired => AuthorityPortalBindingStatus::Expired,
            },
            changed_at: value.changed_at(),
        });
    fact.validate().map_err(|_| PortError::Internal)?;
    Ok(fact)
}

async fn append_authority_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &PortalBindingCommit,
    fact: &AuthorityEvidenceFact,
) -> Result<(), PortError> {
    let command_id = request.request.identity.command_id;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (holding_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,active_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_holding_id(),$1,'customer_authority',$2,$3,$4,$5,
                 'customer_portal_principal_binding',$6,$7,$8,$9,$10,NULL,$11,$12,now())",
    )
    .bind(deterministic_uuid(
        command_id.as_uuid(),
        b"customer-portal-binding-authority-v2",
    ))
    .bind(format!(
        "customer_authority.portal_binding:{}",
        request.aggregate.id().as_uuid().simple()
    ))
    .bind(command_id.as_uuid())
    .bind(AUTHORITY_EVIDENCE_EVENT_TYPE)
    .bind(i32::try_from(AUTHORITY_EVIDENCE_VERSION).map_err(|_| PortError::Internal)?)
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(serde_json::to_value(request.aggregate.scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(fact).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_audit(
    transaction: &mut Transaction<'_, Postgres>,
    request: &PortalBindingCommit,
) -> Result<(), PortError> {
    let command_id = request.request.identity.command_id;
    sqlx::query(
        "INSERT INTO erpx_authority_write.portal_binding_audit_log
            (holding_id,audit_id,command_id,correlation_id,actor,authority_scope,binding_id,
             binding_version,customer_record_id,portal_principal_id,action,evidence_digest,occurred_at)
         VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(deterministic_uuid(command_id.as_uuid(), b"customer-portal-binding-audit-v1"))
    .bind(command_id.as_uuid())
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.authority_scope).map_err(|_| PortError::Internal)?)
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(request.aggregate.customer().record().id().as_uuid())
    .bind(request.aggregate.portal_principal_id().as_uuid())
    .bind(action_name(request.request.identity.payload.action()))
    .bind(request.aggregate.decision_evidence_digest())
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    request: &PortalBindingCommit,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_authority_write.portal_binding_command_receipts
            (holding_id,idempotency_key,command_id,request_hash,binding_id,binding_version,
             correlation_id,receipt,committed_at)
         VALUES (erpx_authority_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(request.request.identity.idempotency_key.as_str())
    .bind(request.request.identity.command_id.as_uuid())
    .bind(request_hash(&request.request)?.as_slice())
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.aggregate).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            PortError::IdempotencyConflict
        } else {
            PortError::Unavailable
        }
    })?;
    Ok(())
}

fn validate_commit(request: &PortalBindingCommit) -> Result<(), PortError> {
    request
        .aggregate
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if request.request.identity.payload.binding_id() != request.aggregate.id()
        || !request
            .request
            .authority_scope
            .contains(request.aggregate.scope())
        || request.changed_at != request.aggregate.changed_at()
    {
        return Err(PortError::Denied);
    }
    Ok(())
}

fn request_hash(
    request: &erpx_customer_authority::PortalBindingCommandRequest,
) -> Result<[u8; 32], PortError> {
    // Idempotency is bound to every authority-affecting input. Correlation is
    // intentionally excluded so an exact transport retry can preserve the
    // original committed correlation from the receipt.
    let value = serde_json::to_vec(&(
        &request.identity.payload,
        &request.actor,
        request.actor_principal_id,
        &request.authority_scope,
        request.expected_version,
    ))
    .map_err(|_| PortError::Internal)?;
    let digest = Sha256::digest(value);
    let mut result = [0_u8; 32];
    result.copy_from_slice(&digest);
    Ok(result)
}

fn deterministic_uuid(namespace: Uuid, name: &[u8]) -> Uuid {
    Uuid::new_v5(&namespace, name)
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

const fn principal_kind_name(value: PortalPrincipalKind) -> &'static str {
    match value {
        PortalPrincipalKind::Human => "human",
        PortalPrincipalKind::Service => "service",
    }
}

const fn use_case_name(value: PortalUseCase) -> &'static str {
    match value {
        PortalUseCase::FieldServiceRecordAccess => "field_service_record_access",
    }
}

const fn status_name(value: CustomerPortalBindingStatus) -> &'static str {
    match value {
        CustomerPortalBindingStatus::Proposed => "proposed",
        CustomerPortalBindingStatus::Active => "active",
        CustomerPortalBindingStatus::Rejected => "rejected",
        CustomerPortalBindingStatus::Revoked => "revoked",
        CustomerPortalBindingStatus::Expired => "expired",
    }
}

const fn action_name(value: PortalBindingAction) -> &'static str {
    match value {
        PortalBindingAction::Propose => "propose",
        PortalBindingAction::Approve => "approve",
        PortalBindingAction::Reject => "reject",
        PortalBindingAction::Revoke => "revoke",
        PortalBindingAction::Expire => "expire",
    }
}

fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use erpx_customer_authority::{
        PortalBindingCommandIdentity, PortalBindingCommandPayload, PortalBindingCommandRequest,
    };
    use erpx_kernel::{ActorAttribution, ActorKind, CommandId};

    use super::*;

    #[test]
    fn portal_binding_adapter_is_normalized_holding_exact_and_secret_free() {
        let source = include_str!("portal_binding.rs");
        let source = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(source.contains("customer_portal_principal_bindings"));
        assert!(source.contains("holding_id=erpx_authority_write.current_holding_id()"));
        assert!(source.contains("AUTHORITY_EVIDENCE_EVENT_TYPE"));
        assert!(!source.contains("support.internal"));
        assert!(!source.contains("comment_body"));
        assert!(!source.contains("aggregate_snapshots"));
    }

    #[test]
    fn request_hash_distinguishes_payload_changes() {
        let id = CustomerPortalBindingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let principal = PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap();
        let request = |digest: &str| PortalBindingCommandRequest {
            identity: PortalBindingCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::new_v4()).unwrap(),
                idempotency_key: erpx_cqrs::IdempotencyKey::try_new("binding-test").unwrap(),
                payload: PortalBindingCommandPayload::Revoke {
                    binding_id: id,
                    decision_evidence_digest: digest.repeat(64),
                },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).unwrap(),
            actor: ActorAttribution {
                principal_ref: "principal:test".into(),
                kind: ActorKind::PlatformUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            actor_principal_id: principal,
            authority_scope: AuthorityScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            expected_version: Some(2),
        };
        assert_ne!(
            request_hash(&request("a")).unwrap(),
            request_hash(&request("b")).unwrap()
        );
    }

    #[test]
    fn request_hash_binds_actor_scope_and_expected_version() {
        let id = CustomerPortalBindingId::try_from_uuid(Uuid::new_v4()).unwrap();
        let mut request = PortalBindingCommandRequest {
            identity: PortalBindingCommandIdentity {
                command_id: CommandId::try_from_uuid(Uuid::new_v4()).unwrap(),
                idempotency_key: erpx_cqrs::IdempotencyKey::try_new("binding-authority-test")
                    .unwrap(),
                payload: PortalBindingCommandPayload::Expire { binding_id: id },
            },
            correlation_id: CorrelationId::try_from_uuid(Uuid::new_v4()).unwrap(),
            actor: ActorAttribution {
                principal_ref: "principal:first".into(),
                kind: ActorKind::PlatformUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            actor_principal_id: PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap(),
            authority_scope: AuthorityScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            expected_version: Some(3),
        };
        let baseline = request_hash(&request).unwrap();
        let mut changed_actor = request.clone();
        changed_actor.actor_principal_id = PrincipalId::try_from_uuid(Uuid::new_v4()).unwrap();
        assert_ne!(baseline, request_hash(&changed_actor).unwrap());
        let mut changed_scope = request.clone();
        changed_scope.authority_scope = AuthorityScope::Holding {
            holding_id: Uuid::new_v4(),
        };
        assert_ne!(baseline, request_hash(&changed_scope).unwrap());
        request.expected_version = Some(4);
        assert_ne!(baseline, request_hash(&request).unwrap());
    }
}
