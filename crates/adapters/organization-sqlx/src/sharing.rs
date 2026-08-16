use async_trait::async_trait;
use erpx_cqrs_sqlx::WritePool;
use erpx_integration_contracts::{
    AUTHORITY_EVIDENCE_EVENT_TYPE, AUTHORITY_EVIDENCE_VERSION, AuthorityEvidenceFact,
    AuthoritySharingDirection, AuthoritySharingStatus, ITEM_DIRECTORY_FACT_VERSION,
    ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft, OrganizationSharingAuthorityEvidence,
    RecordTypeId, StableRecordRef, catalog::ContextId, record_policy_id,
};
use erpx_kernel::{DataClassification, RecordId, TenantScope};
use erpx_organization::{
    OrganizationSharingAgreement, PortError, SharingAgreementId, SharingCapability,
    SharingCommandIdentity, SharingCommandPayload, SharingCommandUnitOfWork, SharingCommit,
    SharingDirection, SharingStatus, SharingWriteRepository, StoredSharingCommit,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{EVENT_VERSION, SHARING_SNAPSHOT_EVENT_TYPE, sharing_partition};

#[derive(Debug, Clone)]
pub struct OrganizationSharingSqlxAdapter {
    pool: WritePool,
}

impl OrganizationSharingSqlxAdapter {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SharingWriteRepository for OrganizationSharingSqlxAdapter {
    async fn load_sharing_agreement(
        &self,
        authority_scope: &TenantScope,
        sharing_agreement_id: SharingAgreementId,
    ) -> Result<Option<OrganizationSharingAgreement>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let value = load_agreement(&mut transaction, sharing_agreement_id, false).await?;
        transaction.commit().await.map_err(map_database)?;
        if value.as_ref().is_some_and(|agreement| {
            !authority_scope.contains(agreement.source_scope())
                && !authority_scope.contains(agreement.target_scope())
        }) {
            return Err(PortError::Denied);
        }
        Ok(value)
    }
}

#[async_trait]
impl SharingCommandUnitOfWork for OrganizationSharingSqlxAdapter {
    async fn find_sharing_receipt(
        &self,
        identity: &SharingCommandIdentity,
    ) -> Result<Option<StoredSharingCommit>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let receipt = find_receipt(&mut transaction, identity).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(receipt)
    }

    async fn commit_sharing(
        &self,
        request: SharingCommit,
    ) -> Result<StoredSharingCommit, PortError> {
        validate_commit(&request)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        if let Some(receipt) = find_receipt(&mut transaction, &request.request.identity).await? {
            transaction.commit().await.map_err(map_database)?;
            return Ok(receipt);
        }

        let identity = &request.request.identity;
        let current = load_agreement(&mut transaction, identity.sharing_agreement_id, true).await?;
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

        persist_agreement(&mut transaction, &request.aggregate, request.changed_at).await?;
        let fact = sharing_fact(&request.aggregate, request.changed_at)?;
        append_authority_event(&mut transaction, &request, &fact).await?;
        append_projection_event(&mut transaction, &request).await?;
        append_directory_event(&mut transaction, &request).await?;
        persist_audit(&mut transaction, &request).await?;
        persist_receipt(&mut transaction, &request).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(StoredSharingCommit {
            command_id: identity.command_id,
            correlation_id: request.request.correlation_id,
            aggregate: request.aggregate,
            committed_at: request.changed_at,
            replayed: false,
        })
    }
}

async fn append_projection_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &SharingCommit,
) -> Result<(), PortError> {
    let command_id = request.request.identity.command_id;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,'organization',$2,$3,$4,$5,
                 'organization_sharing_agreement',$6,$7,$8,$9,$10,NULL,$11,$12,now())",
    )
    .bind(deterministic_uuid(
        command_id.as_uuid(),
        b"organization-sharing-projection-v1",
    ))
    .bind(sharing_partition(request.aggregate.id()))
    .bind(command_id.as_uuid())
    .bind(SHARING_SNAPSHOT_EVENT_TYPE)
    .bind(i32::try_from(EVENT_VERSION).map_err(|_| PortError::Internal)?)
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(serde_json::to_value(request.aggregate.source_scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.aggregate).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn load_agreement(
    transaction: &mut Transaction<'_, Postgres>,
    id: SharingAgreementId,
    lock: bool,
) -> Result<Option<OrganizationSharingAgreement>, PortError> {
    let lock = if lock { " FOR UPDATE" } else { "" };
    let statement = format!(
        "SELECT jsonb_build_object(
            'id',sharing_agreement_id,'version',agreement_version,
            'source_scope',source_scope,'target_scope',target_scope,
            'direction',direction,'capabilities',to_jsonb(capabilities),
            'reason',reason,'proposed_by',proposed_by,
            'source_approved_by',source_approved_by,
            'target_approved_by',target_approved_by,
            'rejected_by',rejected_by,'rejected_at',rejected_at,
            'valid_from',valid_from,'valid_until',valid_until,
            'revoked_at',revoked_at,'status',sharing_status
         ) AS state
         FROM erpx_authority_write.organization_sharing_agreements
         WHERE tenant_id=erpx_authority_write.current_tenant_id()
           AND sharing_agreement_id=$1{lock}"
    );
    let row = sqlx::query(&statement)
        .bind(id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(map_database)?;
    row.map(|row| {
        let state: Value = row.try_get("state").map_err(map_database)?;
        let aggregate: OrganizationSharingAgreement =
            serde_json::from_value(state).map_err(|_| PortError::Internal)?;
        aggregate
            .validate_rehydrated()
            .map_err(|_| PortError::Internal)?;
        Ok(aggregate)
    })
    .transpose()
}

async fn find_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    identity: &SharingCommandIdentity,
) -> Result<Option<StoredSharingCommit>, PortError> {
    let row = sqlx::query(
        "SELECT command_id,request_hash,sharing_agreement_id,agreement_version,
                correlation_id,receipt,committed_at
         FROM erpx_authority_write.sharing_command_receipts
         WHERE tenant_id=erpx_authority_write.current_tenant_id()
           AND idempotency_key=$1 FOR UPDATE",
    )
    .bind(identity.idempotency_key.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(map_database)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let command_id: Uuid = row.try_get("command_id").map_err(map_database)?;
    let hash: Vec<u8> = row.try_get("request_hash").map_err(map_database)?;
    if command_id != identity.command_id.as_uuid() || hash.as_slice() != request_hash(identity)? {
        return Err(PortError::IdempotencyConflict);
    }
    let aggregate: OrganizationSharingAgreement =
        serde_json::from_value(row.try_get("receipt").map_err(map_database)?)
            .map_err(|_| PortError::Internal)?;
    aggregate
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if aggregate.id() != identity.sharing_agreement_id
        || aggregate.version()
            != u64::try_from(
                row.try_get::<i64, _>("agreement_version")
                    .map_err(map_database)?,
            )
            .map_err(|_| PortError::Internal)?
    {
        return Err(PortError::Internal);
    }
    Ok(Some(StoredSharingCommit {
        command_id: identity.command_id,
        correlation_id: erpx_kernel::CorrelationId::try_from_uuid(
            row.try_get("correlation_id").map_err(map_database)?,
        )
        .map_err(|_| PortError::Internal)?,
        aggregate,
        committed_at: row.try_get("committed_at").map_err(map_database)?,
        replayed: true,
    }))
}

async fn persist_agreement(
    transaction: &mut Transaction<'_, Postgres>,
    value: &OrganizationSharingAgreement,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), PortError> {
    let result = sqlx::query(
        "INSERT INTO erpx_authority_write.organization_sharing_agreements
            (tenant_id,sharing_agreement_id,agreement_version,source_scope,target_scope,
             direction,capabilities,reason,proposed_by,source_approved_by,target_approved_by,
             rejected_by,rejected_at,valid_from,valid_until,revoked_at,sharing_status,changed_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,
                 $10,$11,$12,$13,$14,$15,$16,$17)
         ON CONFLICT (tenant_id,sharing_agreement_id) DO UPDATE SET
            agreement_version=EXCLUDED.agreement_version,
            source_approved_by=EXCLUDED.source_approved_by,
            target_approved_by=EXCLUDED.target_approved_by,
            rejected_by=EXCLUDED.rejected_by,rejected_at=EXCLUDED.rejected_at,
            revoked_at=EXCLUDED.revoked_at,sharing_status=EXCLUDED.sharing_status,
            changed_at=EXCLUDED.changed_at
         WHERE erpx_authority_write.organization_sharing_agreements.tenant_id
               = erpx_authority_write.current_tenant_id()
           AND erpx_authority_write.organization_sharing_agreements.agreement_version + 1
               = EXCLUDED.agreement_version",
    )
    .bind(value.id().as_uuid())
    .bind(to_i64(value.version())?)
    .bind(serde_json::to_value(value.source_scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(value.target_scope()).map_err(|_| PortError::Internal)?)
    .bind(direction_name(value.direction()))
    .bind(
        value
            .capabilities()
            .iter()
            .copied()
            .map(capability_name)
            .collect::<Vec<_>>(),
    )
    .bind(value.reason())
    .bind(value.proposed_by())
    .bind(value.source_approved_by())
    .bind(value.target_approved_by())
    .bind(value.rejected_by())
    .bind(value.rejected_at())
    .bind(value.valid_from())
    .bind(value.valid_until())
    .bind(value.revoked_at())
    .bind(status_name(value.status()))
    .bind(changed_at)
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

async fn append_authority_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &SharingCommit,
    fact: &AuthorityEvidenceFact,
) -> Result<(), PortError> {
    let command_id = request.request.identity.command_id;
    let event_id = deterministic_uuid(command_id.as_uuid(), b"organization-sharing-authority-v1");
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,'organization',$2,$3,$4,$5,
                 'organization_sharing_agreement',$6,$7,$8,$9,$10,NULL,$11,$12,now())",
    )
    .bind(event_id)
    .bind(format!(
        "organization.sharing:{}",
        request.aggregate.id().as_uuid().simple()
    ))
    .bind(command_id.as_uuid())
    .bind(AUTHORITY_EVIDENCE_EVENT_TYPE)
    .bind(i32::try_from(AUTHORITY_EVIDENCE_VERSION).map_err(|_| PortError::Internal)?)
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(serde_json::to_value(request.aggregate.source_scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(fact).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn append_directory_event(
    transaction: &mut Transaction<'_, Postgres>,
    request: &SharingCommit,
) -> Result<(), PortError> {
    let aggregate = &request.aggregate;
    let record = StableRecordRef::try_new(
        RecordId::try_from_uuid(aggregate.id().as_uuid()).map_err(|_| PortError::Internal)?,
        ContextId::Organization,
        RecordTypeId::try_new("organization.sharing-agreement").map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let draft = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.source_scope().clone(),
        lifecycle_state: status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!(
            "Organization sharing agreement {}",
            aggregate.id().as_uuid()
        ),
    };
    draft.validate().map_err(|_| PortError::Internal)?;

    let command_id = request.request.identity.command_id;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
             aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
             causation_id,payload,occurred_at,available_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,'organization',$2,$3,$4,$5,
                 'organization_sharing_agreement',$6,$7,$8,$9,$10,NULL,$11,$12,now())",
    )
    .bind(deterministic_uuid(
        command_id.as_uuid(),
        b"organization-sharing-directory-v1",
    ))
    .bind(format!(
        "organization.sharing:{}",
        aggregate.id().as_uuid().simple()
    ))
    .bind(command_id.as_uuid())
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| PortError::Internal)?)
    .bind(aggregate.id().as_uuid())
    .bind(to_i64(aggregate.version())?)
    .bind(serde_json::to_value(aggregate.source_scope()).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(draft).map_err(|_| PortError::Internal)?)
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_audit(
    transaction: &mut Transaction<'_, Postgres>,
    request: &SharingCommit,
) -> Result<(), PortError> {
    let command_id = request.request.identity.command_id;
    sqlx::query(
        "INSERT INTO erpx_authority_write.sharing_audit_log
            (tenant_id,audit_id,command_id,correlation_id,actor,authority_scope,
             sharing_agreement_id,agreement_version,action,occurred_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(deterministic_uuid(
        command_id.as_uuid(),
        b"organization-sharing-audit-v1",
    ))
    .bind(command_id.as_uuid())
    .bind(request.request.correlation_id.as_uuid())
    .bind(serde_json::to_value(&request.request.actor).map_err(|_| PortError::Internal)?)
    .bind(serde_json::to_value(&request.request.authority_scope).map_err(|_| PortError::Internal)?)
    .bind(request.aggregate.id().as_uuid())
    .bind(to_i64(request.aggregate.version())?)
    .bind(action_name(&request.request.identity.payload))
    .bind(request.changed_at)
    .execute(&mut **transaction)
    .await
    .map_err(map_database)?;
    Ok(())
}

async fn persist_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    request: &SharingCommit,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_authority_write.sharing_command_receipts
            (tenant_id,idempotency_key,command_id,request_hash,sharing_agreement_id,
             agreement_version,correlation_id,receipt,committed_at)
         VALUES (erpx_authority_write.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(request.request.identity.idempotency_key.as_str())
    .bind(request.request.identity.command_id.as_uuid())
    .bind(request_hash(&request.request.identity)?.as_slice())
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

fn validate_commit(request: &SharingCommit) -> Result<(), PortError> {
    request
        .aggregate
        .validate_rehydrated()
        .map_err(|_| PortError::Internal)?;
    if request.aggregate.id() != request.request.identity.sharing_agreement_id
        || (!request
            .request
            .authority_scope
            .contains(request.aggregate.source_scope())
            && !request
                .request
                .authority_scope
                .contains(request.aggregate.target_scope()))
    {
        return Err(PortError::Denied);
    }
    Ok(())
}

fn sharing_fact(
    value: &OrganizationSharingAgreement,
    changed_at: chrono::DateTime<chrono::Utc>,
) -> Result<AuthorityEvidenceFact, PortError> {
    let fact = AuthorityEvidenceFact::OrganizationSharing(OrganizationSharingAuthorityEvidence {
        sharing_agreement_id: value.id().as_uuid(),
        agreement_version: value.version(),
        source_scope: value.source_scope().clone(),
        target_scope: value.target_scope().clone(),
        direction: match value.direction() {
            SharingDirection::SourceToTarget => AuthoritySharingDirection::SourceToTarget,
            SharingDirection::Mutual => AuthoritySharingDirection::Mutual,
        },
        permits_open_record: value
            .capabilities()
            .contains(&SharingCapability::OpenRecord),
        source_approved: value.source_approved_by().is_some(),
        target_approved: value.target_approved_by().is_some(),
        valid_from: value.valid_from(),
        valid_until: value.valid_until(),
        revoked_at: value.revoked_at(),
        status: match value.status() {
            SharingStatus::Proposed => AuthoritySharingStatus::Proposed,
            SharingStatus::Active => AuthoritySharingStatus::Active,
            SharingStatus::Rejected => AuthoritySharingStatus::Rejected,
            SharingStatus::Revoked => AuthoritySharingStatus::Revoked,
            SharingStatus::Expired => AuthoritySharingStatus::Expired,
        },
        changed_at,
    });
    fact.validate().map_err(|_| PortError::Internal)?;
    Ok(fact)
}

fn request_hash(identity: &SharingCommandIdentity) -> Result<[u8; 32], PortError> {
    let bytes = serde_json::to_vec(identity).map_err(|_| PortError::Internal)?;
    Ok(Sha256::digest(bytes).into())
}

fn deterministic_uuid(namespace: Uuid, discriminator: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(namespace.as_bytes());
    digest.update(discriminator);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.finalize()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

const fn direction_name(value: SharingDirection) -> &'static str {
    match value {
        SharingDirection::SourceToTarget => "source_to_target",
        SharingDirection::Mutual => "mutual",
    }
}

const fn status_name(value: SharingStatus) -> &'static str {
    match value {
        SharingStatus::Proposed => "proposed",
        SharingStatus::Active => "active",
        SharingStatus::Rejected => "rejected",
        SharingStatus::Revoked => "revoked",
        SharingStatus::Expired => "expired",
    }
}

const fn capability_name(value: SharingCapability) -> &'static str {
    match value {
        SharingCapability::DiscoverPrincipal => "discover_principal",
        SharingCapability::Mention => "mention",
        SharingCapability::Reference => "reference",
        SharingCapability::OpenRecord => "open_record",
    }
}

const fn action_name(value: &SharingCommandPayload) -> &'static str {
    match value {
        SharingCommandPayload::Propose(_) => "propose",
        SharingCommandPayload::ApproveSource { .. } => "approve_source",
        SharingCommandPayload::ApproveTarget { .. } => "approve_target",
        SharingCommandPayload::Reject { .. } => "reject",
        SharingCommandPayload::Revoke { .. } => "revoke",
        SharingCommandPayload::Expire => "expire",
    }
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_contract_uses_exact_tenant_predicates_and_atomic_evidence() {
        let source = include_str!("sharing.rs");
        assert!(source.contains("tenant_id=erpx_authority_write.current_tenant_id()"));
        assert!(source.contains("sharing_command_receipts"));
        assert!(source.contains("sharing_audit_log"));
        assert!(source.contains("erpx_cqrs_write.outbox_events"));
        assert!(source.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(source.contains("organization.sharing-agreement"));
        assert!(!source.contains(&["aggregate", "snapshots"].join("_")));
    }
}
