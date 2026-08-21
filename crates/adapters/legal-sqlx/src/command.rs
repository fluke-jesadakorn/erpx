use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    BoundedContext, ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordRef, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use erpx_legal::{
    AccessEntry, AccessEntryId, CommandUnitOfWork, ContractParty, ContractStatus, DocumentVersion,
    LegalAggregate, LegalCommit, LegalContract, LegalContractId, LegalDocument, LegalDocumentId,
    LegalHold, LegalMatter, LegalMatterId, MatterStatus, PortError, RecordLink, RecordRelation,
    WriteRepository,
    ports::{CommandFingerprint, CommandIdentity, StoredCommit},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    ACCESS_AGGREGATE_TYPE, ACCESS_SNAPSHOT_EVENT, CONTEXT_NAME, CONTRACT_AGGREGATE_TYPE,
    CONTRACT_SNAPSHOT_EVENT, DOCUMENT_AGGREGATE_TYPE, DOCUMENT_SNAPSHOT_EVENT, EVENT_VERSION,
    MATTER_AGGREGATE_TYPE, MATTER_SNAPSHOT_EVENT, access_partition, contract_partition,
    document_partition, matter_partition,
    projection::{LegalOutboxPayload, LegalSnapshot, contract_status_name, matter_status_name},
};

#[derive(Debug, Clone)]
pub struct LegalWriteRepository {
    pool: WritePool,
}

impl LegalWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    async fn load_aggregate(
        &self,
        authority: &AuthorityScope,
        partition: &str,
        aggregate_type: &str,
        id: Uuid,
    ) -> Result<Option<LegalAggregate>, PortError> {
        let _ = partition;
        load_legal_relational(&self.pool, authority, aggregate_type, id).await
    }
}

async fn load_legal_relational(
    pool: &WritePool,
    authority: &AuthorityScope,
    aggregate_type: &str,
    id: Uuid,
) -> Result<Option<LegalAggregate>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let mut tx = pool.begin().await.map_err(map_store_error)?;
    let value = match aggregate_type {
        CONTRACT_AGGREGATE_TYPE => load_contract_relational(&mut tx, authority, id)
            .await?
            .map(LegalAggregate::Contract),
        DOCUMENT_AGGREGATE_TYPE => load_document_relational(&mut tx, authority, id)
            .await?
            .map(LegalAggregate::Document),
        ACCESS_AGGREGATE_TYPE => load_access_relational(&mut tx, authority, id)
            .await?
            .map(LegalAggregate::AccessEntry),
        MATTER_AGGREGATE_TYPE => load_matter_relational(&mut tx, authority, id)
            .await?
            .map(LegalAggregate::Matter),
        _ => return Err(PortError::Internal),
    };
    Ok(value)
}

async fn load_contract_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<LegalContract>, PortError> {
    let row = authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,reference,title,object_ref,
                content_sha256,retention_until,contract_status,created_by,approved_by,created_at,updated_at
         FROM erpx_legal_write.contract_guards WHERE contract_id=$1",
        "erpx_legal_write.contract_guards",
        "contract_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let party_rows = sqlx::query(
        "SELECT party_id,role FROM erpx_legal_write.contract_party_guards
         WHERE contract_id=$1 ORDER BY party_id",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let parties = party_rows
        .into_iter()
        .map(|party| {
            ContractParty::try_new(
                party.try_get("party_id").map_err(|_| PortError::Internal)?,
                party.try_get("role").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    LegalContract::try_rehydrate(
        LegalContractId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        exact_scope(&row)?,
        row.try_get("reference").map_err(|_| PortError::Internal)?,
        row.try_get("title").map_err(|_| PortError::Internal)?,
        row.try_get("object_ref").map_err(|_| PortError::Internal)?,
        row.try_get("content_sha256")
            .map_err(|_| PortError::Internal)?,
        row.try_get("retention_until")
            .map_err(|_| PortError::Internal)?,
        parse_contract_status(
            row.try_get("contract_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        parties,
        row.try_get("created_by").map_err(|_| PortError::Internal)?,
        row.try_get("approved_by")
            .map_err(|_| PortError::Internal)?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_document_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<LegalDocument>, PortError> {
    let row = authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,title,archived,created_at,updated_at
         FROM erpx_legal_write.document_guards WHERE document_id=$1",
        "erpx_legal_write.document_guards",
        "document_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let rows = sqlx::query(
        "SELECT document_version,object_ref,content_sha256,created_at,created_by
         FROM erpx_legal_write.document_version_guards
         WHERE document_id=$1 ORDER BY document_version",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let versions = rows
        .into_iter()
        .map(|version| {
            Ok(DocumentVersion {
                version: row_u64(&version, "document_version")?,
                object_ref: version
                    .try_get("object_ref")
                    .map_err(|_| PortError::Internal)?,
                content_sha256: version
                    .try_get("content_sha256")
                    .map_err(|_| PortError::Internal)?,
                created_at: version
                    .try_get("created_at")
                    .map_err(|_| PortError::Internal)?,
                created_by: version
                    .try_get("created_by")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    LegalDocument::try_rehydrate(
        LegalDocumentId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        exact_scope(&row)?,
        row.try_get("title").map_err(|_| PortError::Internal)?,
        versions,
        row.try_get("archived").map_err(|_| PortError::Internal)?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_access_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<AccessEntry>, PortError> {
    let row = authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,document_id,document_version,
                actor_ref,action,purpose,observed_at
         FROM erpx_legal_write.access_entry_guards WHERE access_entry_id=$1",
        "erpx_legal_write.access_entry_guards",
        "access_entry_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    AccessEntry::try_rehydrate(
        AccessEntryId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        exact_scope(&row)?,
        LegalDocumentId::try_from_uuid(
            row.try_get("document_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        row_u64(&row, "document_version")?,
        row.try_get("actor_ref").map_err(|_| PortError::Internal)?,
        row.try_get("action").map_err(|_| PortError::Internal)?,
        row.try_get("purpose").map_err(|_| PortError::Internal)?,
        row.try_get("observed_at")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_matter_relational(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    id: Uuid,
) -> Result<Option<LegalMatter>, PortError> {
    let row = authorized_root(
        tx,
        authority,
        "SELECT aggregate_version,holding_id,company_id,branch_id,reference,title,matter_status,
                hold_active,hold_reason,hold_placed_by,hold_placed_at,hold_released_by,
                hold_released_at,created_by,created_at,updated_at
         FROM erpx_legal_write.matter_guards WHERE matter_id=$1",
        "erpx_legal_write.matter_guards",
        "matter_id",
        id,
    )
    .await?;
    let Some(row) = row else { return Ok(None) };
    let link_rows = sqlx::query(
        "SELECT relation,target_context,target_aggregate_kind,target_aggregate_id,
                target_aggregate_version,target_holding_id,target_company_id,target_branch_id,
                target_digest,linked_by,linked_at
         FROM erpx_legal_write.matter_link_guards WHERE matter_id=$1 ORDER BY link_ordinal",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let links = link_rows
        .into_iter()
        .map(|link| {
            let target_scope = exact_scope_named(
                link.try_get("target_holding_id")
                    .map_err(|_| PortError::Internal)?,
                link.try_get("target_company_id")
                    .map_err(|_| PortError::Internal)?,
                link.try_get("target_branch_id")
                    .map_err(|_| PortError::Internal)?,
            )?;
            Ok(RecordLink {
                relation: parse_record_relation(
                    link.try_get("relation").map_err(|_| PortError::Internal)?,
                )?,
                target: RecordRef::try_new(
                    parse_bounded_context(
                        link.try_get("target_context")
                            .map_err(|_| PortError::Internal)?,
                    )?,
                    link.try_get::<String, _>("target_aggregate_kind")
                        .map_err(|_| PortError::Internal)?,
                    link.try_get("target_aggregate_id")
                        .map_err(|_| PortError::Internal)?,
                    row_u64(&link, "target_aggregate_version")?,
                )
                .map_err(|_| PortError::Internal)?,
                target_scope,
                target_digest: link
                    .try_get("target_digest")
                    .map_err(|_| PortError::Internal)?,
                linked_by: link.try_get("linked_by").map_err(|_| PortError::Internal)?,
                linked_at: link.try_get("linked_at").map_err(|_| PortError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>, PortError>>()?;
    let legal_hold = row
        .try_get::<Option<bool>, _>("hold_active")
        .map_err(|_| PortError::Internal)?
        .map(|active| {
            Ok(LegalHold {
                active,
                reason: row
                    .try_get("hold_reason")
                    .map_err(|_| PortError::Internal)?,
                placed_by: row
                    .try_get("hold_placed_by")
                    .map_err(|_| PortError::Internal)?,
                placed_at: row
                    .try_get("hold_placed_at")
                    .map_err(|_| PortError::Internal)?,
                released_by: row
                    .try_get("hold_released_by")
                    .map_err(|_| PortError::Internal)?,
                released_at: row
                    .try_get("hold_released_at")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()?;
    LegalMatter::try_rehydrate(
        LegalMatterId::try_from_uuid(id).map_err(|_| PortError::Internal)?,
        row_u64(&row, "aggregate_version")?,
        exact_scope(&row)?,
        row.try_get("reference").map_err(|_| PortError::Internal)?,
        row.try_get("title").map_err(|_| PortError::Internal)?,
        parse_matter_status(
            row.try_get("matter_status")
                .map_err(|_| PortError::Internal)?,
        )?,
        links,
        legal_hold,
        row.try_get("created_by").map_err(|_| PortError::Internal)?,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

#[async_trait]
impl WriteRepository for LegalWriteRepository {
    async fn load_contract(
        &self,
        authority: &AuthorityScope,
        id: LegalContractId,
    ) -> Result<Option<LegalContract>, PortError> {
        match self
            .load_aggregate(
                authority,
                &contract_partition(id),
                CONTRACT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Contract(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_document(
        &self,
        authority: &AuthorityScope,
        id: LegalDocumentId,
    ) -> Result<Option<LegalDocument>, PortError> {
        match self
            .load_aggregate(
                authority,
                &document_partition(id),
                DOCUMENT_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Document(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }

    async fn load_matter(
        &self,
        authority: &AuthorityScope,
        id: LegalMatterId,
    ) -> Result<Option<LegalMatter>, PortError> {
        match self
            .load_aggregate(
                authority,
                &matter_partition(id),
                MATTER_AGGREGATE_TYPE,
                id.as_uuid(),
            )
            .await?
        {
            Some(LegalAggregate::Matter(value)) => Ok(Some(value)),
            Some(_) => Err(PortError::Internal),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LegalCommandAdapter {
    store: CommandStore,
}
impl LegalCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for LegalCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let (partition, aggregate_type) = fingerprint_target(&identity.fingerprint);
        self.store
            .find_receipt::<LegalAggregate>(
                CONTEXT_NAME,
                &partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?
            .map(|receipt| receipt_to_stored(receipt, aggregate_type))
            .transpose()
    }

    async fn commit(&self, request: LegalCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let partition = aggregate_partition(&request.aggregate);
        let aggregate_type = request.aggregate.aggregate_type();
        let occurred_at = request.aggregate.occurred_at();
        let command_id = request.identity.command_id;
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: LegalOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        let snapshot_event = snapshot_event(&request.aggregate);
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, snapshot_event, events.len()),
            event_type: snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: LegalOutboxPayload::Snapshot(LegalSnapshot::from(&request.aggregate)),
        });
        for directory_draft in legal_directory_drafts(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: LegalOutboxPayload::Directory(directory_draft),
            });
        }
        let guard = LegalAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let aggregate_id =
            AggregateId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"legal-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({"aggregate_id": aggregate_id.as_uuid(), "aggregate_type": aggregate_type, "expected_version": request.expected_version}),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        let mut stored = receipt_to_stored(receipt, aggregate_type)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn legal_directory_drafts(
    aggregate: &LegalAggregate,
) -> Result<Vec<ItemDescriptorDraftV2>, PortError> {
    let (record, lifecycle_state, display_label) = match aggregate {
        LegalAggregate::Contract(value) => (
            value.record_ref().map_err(|_| PortError::Internal)?,
            contract_status_name(value.status()),
            format!("{} {}", value.reference(), value.title()),
        ),
        LegalAggregate::Matter(value) => (
            value.record_ref().map_err(|_| PortError::Internal)?,
            matter_status_name(value.status()),
            format!("{} {}", value.reference(), value.title()),
        ),
        // Document versions and access entries are governed children/evidence,
        // not separate route families in the first-class record catalog.
        LegalAggregate::Document(_) | LegalAggregate::AccessEntry(_) => return Ok(Vec::new()),
    };
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: lifecycle_state.into(),
        classification: DataClassification::Restricted,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    let value =
        ItemDescriptorDraftV2::try_new(value, display_label).map_err(|_| PortError::Internal)?;
    let mut values = vec![value];
    if let LegalAggregate::Matter(matter) = aggregate
        && let Some(hold) = matter.legal_hold()
    {
        let record = matter
            .legal_hold_record_ref()
            .map_err(|_| PortError::Internal)?
            .ok_or(PortError::Internal)?;
        let display_label = format!("{} legal hold", matter.reference());
        let value = ItemDescriptorDraft {
            authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
            record,
            scope: matter.scope().clone(),
            lifecycle_state: if hold.active { "active" } else { "released" }.into(),
            classification: DataClassification::Restricted,
            descriptor_version: matter.version(),
            authorized_search_text: display_label.clone(),
        };
        let value = ItemDescriptorDraftV2::try_new(value, display_label)
            .map_err(|_| PortError::Internal)?;
        values.push(value);
    }
    Ok(values)
}

struct LegalAtomicGuard {
    aggregate: LegalAggregate,
}

#[async_trait]
impl AtomicCommandGuard for LegalAtomicGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        guard_version(tx, &self.aggregate)
            .await?
            .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = guard_version(tx, &self.aggregate).await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Legal aggregate already exists",
                ));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "Legal aggregate guard is missing",
                ));
            }
            _ => {}
        }
        match &self.aggregate {
            LegalAggregate::Contract(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT contract_id FROM erpx_legal_write.contract_guards WHERE scope_key=$1 AND reference=$2 FOR UPDATE")
                    .bind(scope_key(value.scope())?).bind(value.reference()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal contract reference already exists",
                    ));
                }
            }
            LegalAggregate::Matter(value) if context.expected_version.is_none() => {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT matter_id FROM erpx_legal_write.matter_guards WHERE scope_key=$1 AND reference=$2 FOR UPDATE")
                    .bind(scope_key(value.scope())?).bind(value.reference()).fetch_optional(&mut **tx).await?;
                if existing.is_some_and(|id| id != value.id().as_uuid()) {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal matter reference already exists",
                    ));
                }
                validate_existing_links(tx, value).await?;
            }
            LegalAggregate::Document(value) => validate_existing_versions(tx, value).await?,
            LegalAggregate::AccessEntry(value) => {
                let row = sqlx::query("SELECT d.scope_key FROM erpx_legal_write.document_guards d JOIN erpx_legal_write.document_version_guards v ON v.document_id=d.document_id WHERE d.document_id=$1 AND v.document_version=$2 FOR SHARE")
                    .bind(value.document_id().as_uuid()).bind(to_i64(value.document_version())?)
                    .fetch_optional(&mut **tx).await?;
                let Some(row) = row else {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal document version is missing",
                    ));
                };
                let document_scope: String = row.try_get("scope_key")?;
                if document_scope != scope_key(value.scope())? {
                    return Err(CqrsSqlxError::GuardRejected(
                        "Legal access document is cross-scope",
                    ));
                }
            }
            LegalAggregate::Matter(value) => validate_existing_links(tx, value).await?,
            LegalAggregate::Contract(_) => {}
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != self.aggregate.aggregate_type()
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.partition != aggregate_partition(&self.aggregate)
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Legal applied guard metadata mismatch",
            ));
        }
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            LegalAggregate::Contract(value) => {
                let changed = sqlx::query("INSERT INTO erpx_legal_write.contract_guards (contract_id,scope_key,reference,title,object_ref,content_sha256,retention_until,contract_status,created_by,approved_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (contract_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,object_ref=EXCLUDED.object_ref,content_sha256=EXCLUDED.content_sha256,retention_until=EXCLUDED.retention_until,contract_status=EXCLUDED.contract_status,created_by=EXCLUDED.created_by,approved_by=EXCLUDED.approved_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_legal_write.contract_guards.aggregate_version + 1=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.reference()).bind(value.title())
                    .bind(value.object_ref()).bind(value.content_sha256()).bind(value.retention_until())
                    .bind(contract_status_name(value.status())).bind(value.created_by()).bind(value.approved_by())
                    .bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?.rows_affected();
                require_single_write(changed, "Legal contract write conflict")?;
                write_exact_scope(
                    tx,
                    "erpx_legal_write.contract_guards",
                    "contract_id",
                    value.id().as_uuid(),
                    value.scope(),
                )
                .await?;
                sqlx::query(
                    "DELETE FROM erpx_legal_write.contract_party_guards WHERE contract_id=$1",
                )
                .bind(value.id().as_uuid())
                .execute(&mut **tx)
                .await?;
                for party in value.parties() {
                    sqlx::query("INSERT INTO erpx_legal_write.contract_party_guards (contract_id,party_id,role) VALUES ($1,$2,$3)")
                        .bind(value.id().as_uuid()).bind(party.party_id).bind(&party.role).execute(&mut **tx).await?;
                }
            }
            LegalAggregate::Document(value) => {
                let changed = sqlx::query("INSERT INTO erpx_legal_write.document_guards (document_id,scope_key,title,current_document_version,archived,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (document_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,title=EXCLUDED.title,current_document_version=EXCLUDED.current_document_version,archived=EXCLUDED.archived,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_legal_write.document_guards.aggregate_version + 1=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.title())
                    .bind(to_i64(value.current_version())?).bind(value.archived()).bind(version)
                    .bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?.rows_affected();
                require_single_write(changed, "Legal document write conflict")?;
                write_exact_scope(
                    tx,
                    "erpx_legal_write.document_guards",
                    "document_id",
                    value.id().as_uuid(),
                    value.scope(),
                )
                .await?;
                let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_legal_write.document_version_guards WHERE document_id=$1")
                    .bind(value.id().as_uuid()).fetch_one(&mut **tx).await?;
                let existing =
                    usize::try_from(existing).map_err(|_| CqrsSqlxError::NumericRange)?;
                for child in value.versions().iter().skip(existing) {
                    sqlx::query("INSERT INTO erpx_legal_write.document_version_guards (document_id,document_version,object_ref,content_sha256,created_at,created_by) VALUES ($1,$2,$3,$4,$5,$6)")
                        .bind(value.id().as_uuid()).bind(to_i64(child.version)?).bind(&child.object_ref)
                        .bind(&child.content_sha256).bind(child.created_at).bind(&child.created_by)
                        .execute(&mut **tx).await?;
                }
            }
            LegalAggregate::AccessEntry(value) => {
                sqlx::query("INSERT INTO erpx_legal_write.access_entry_guards (access_entry_id,scope_key,document_id,document_version,actor_ref,action,purpose,observed_at,aggregate_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.document_id().as_uuid())
                    .bind(to_i64(value.document_version())?).bind(value.actor_ref()).bind(value.action())
                    .bind(value.purpose()).bind(value.observed_at()).bind(version).execute(&mut **tx).await?;
                write_exact_scope(
                    tx,
                    "erpx_legal_write.access_entry_guards",
                    "access_entry_id",
                    value.id().as_uuid(),
                    value.scope(),
                )
                .await?;
            }
            LegalAggregate::Matter(value) => {
                let changed = sqlx::query("INSERT INTO erpx_legal_write.matter_guards (matter_id,scope_key,reference,title,matter_status,legal_hold,created_by,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (matter_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,reference=EXCLUDED.reference,title=EXCLUDED.title,matter_status=EXCLUDED.matter_status,legal_hold=EXCLUDED.legal_hold,created_by=EXCLUDED.created_by,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_legal_write.matter_guards.aggregate_version + 1=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.reference()).bind(value.title())
                    .bind(matter_status_name(value.status())).bind(serde_json::to_value(value.legal_hold()).map_err(|_| CqrsSqlxError::InvalidInput("Legal hold cannot be encoded"))?)
                    .bind(value.created_by()).bind(version).bind(value.created_at()).bind(value.updated_at()).execute(&mut **tx).await?.rows_affected();
                require_single_write(changed, "Legal matter write conflict")?;
                write_exact_scope(
                    tx,
                    "erpx_legal_write.matter_guards",
                    "matter_id",
                    value.id().as_uuid(),
                    value.scope(),
                )
                .await?;
                write_legal_hold_columns(tx, value).await?;
                let existing: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM erpx_legal_write.matter_link_guards WHERE matter_id=$1",
                )
                .bind(value.id().as_uuid())
                .fetch_one(&mut **tx)
                .await?;
                let existing =
                    usize::try_from(existing).map_err(|_| CqrsSqlxError::NumericRange)?;
                for (ordinal, link) in value.links().iter().enumerate().skip(existing) {
                    sqlx::query("INSERT INTO erpx_legal_write.matter_link_guards (matter_id,link_ordinal,relation,target_context,target_aggregate_kind,target_aggregate_id,target_aggregate_version,target_scope,target_scope_key,target_digest,linked_by,linked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
                        .bind(value.id().as_uuid()).bind(to_i64(u64::try_from(ordinal + 1).map_err(|_| CqrsSqlxError::NumericRange)?)?)
                        .bind(record_relation_name(link.relation)).bind(bounded_context_name(link.target.context)?)
                        .bind(link.target.aggregate_kind.as_str()).bind(link.target.aggregate_id).bind(to_i64(link.target.aggregate_version)?)
                        .bind(serde_json::to_value(&link.target_scope).map_err(|_| CqrsSqlxError::InvalidInput("target scope cannot be encoded"))?)
                        .bind(scope_key(&link.target_scope)?).bind(&link.target_digest).bind(&link.linked_by).bind(link.linked_at)
                        .execute(&mut **tx).await?;
                    write_link_scope_columns(
                        tx,
                        value.id().as_uuid(),
                        ordinal + 1,
                        &link.target_scope,
                    )
                    .await?;
                }
                if let Some(hold) = value.legal_hold() {
                    let hold_ref = value
                        .legal_hold_record_ref()
                        .map_err(|_| CqrsSqlxError::InvalidInput("Legal hold record is invalid"))?
                        .ok_or(CqrsSqlxError::InvalidInput("Legal hold record is missing"))?;
                    sqlx::query("INSERT INTO erpx_legal_write.legal_hold_records
                        (holding_id,legal_hold_id,matter_id,source_matter_version,active_scope,
                         scope_key,hold_active,reason,placed_by,placed_at,released_by,released_at,
                         changed_at)
                        VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                        ON CONFLICT (holding_id,legal_hold_id) DO UPDATE SET
                         source_matter_version=EXCLUDED.source_matter_version,
                         hold_active=EXCLUDED.hold_active,released_by=EXCLUDED.released_by,
                         released_at=EXCLUDED.released_at,changed_at=EXCLUDED.changed_at
                        WHERE erpx_legal_write.legal_hold_records.holding_id
                              = erpx_cqrs_write.current_holding_id()
                          AND erpx_legal_write.legal_hold_records.source_matter_version + 1
                              = EXCLUDED.source_matter_version")
                        .bind(hold_ref.id().as_uuid()).bind(value.id().as_uuid()).bind(version)
                        .bind(serde_json::to_value(value.scope()).map_err(|_| CqrsSqlxError::InvalidInput("Legal hold scope cannot be encoded"))?)
                        .bind(scope_key(value.scope())?).bind(hold.active).bind(&hold.reason)
                        .bind(&hold.placed_by).bind(hold.placed_at).bind(&hold.released_by)
                        .bind(hold.released_at).bind(value.updated_at())
                        .execute(&mut **tx).await?;
                }
            }
        }
        Ok(())
    }
}

async fn validate_existing_versions(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalDocument,
) -> Result<(), CqrsSqlxError> {
    let rows = sqlx::query("SELECT document_version,object_ref,content_sha256,created_at,created_by FROM erpx_legal_write.document_version_guards WHERE document_id=$1 ORDER BY document_version FOR SHARE")
        .bind(value.id().as_uuid()).fetch_all(&mut **tx).await?;
    if rows.len() > value.versions().len() {
        return Err(CqrsSqlxError::GuardRejected(
            "Legal document versions cannot be removed",
        ));
    }
    for (row, expected) in rows.iter().zip(value.versions()) {
        let version: i64 = row.try_get("document_version")?;
        let object_ref: String = row.try_get("object_ref")?;
        let digest: String = row.try_get("content_sha256")?;
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at")?;
        let created_by: String = row.try_get("created_by")?;
        if u64::try_from(version).ok() != Some(expected.version)
            || object_ref != expected.object_ref
            || digest != expected.content_sha256
            || created_at.timestamp_micros() != expected.created_at.timestamp_micros()
            || created_by != expected.created_by
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Legal document version evidence changed",
            ));
        }
    }
    Ok(())
}

async fn validate_existing_links(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalMatter,
) -> Result<(), CqrsSqlxError> {
    let rows = sqlx::query("SELECT relation,target_context,target_aggregate_kind,target_aggregate_id,target_aggregate_version,target_scope,target_digest,linked_by,linked_at FROM erpx_legal_write.matter_link_guards WHERE matter_id=$1 ORDER BY link_ordinal FOR SHARE")
        .bind(value.id().as_uuid()).fetch_all(&mut **tx).await?;
    if rows.len() > value.links().len() {
        return Err(CqrsSqlxError::GuardRejected(
            "Legal matter links cannot be removed",
        ));
    }
    for (row, expected) in rows.iter().zip(value.links()) {
        let relation: String = row.try_get("relation")?;
        let target_context: String = row.try_get("target_context")?;
        let target_kind: String = row.try_get("target_aggregate_kind")?;
        let target_id: Uuid = row.try_get("target_aggregate_id")?;
        let target_version: i64 = row.try_get("target_aggregate_version")?;
        let target_scope: serde_json::Value = row.try_get("target_scope")?;
        let target_digest: String = row.try_get("target_digest")?;
        let linked_by: String = row.try_get("linked_by")?;
        let linked_at: chrono::DateTime<chrono::Utc> = row.try_get("linked_at")?;
        if relation != record_relation_name(expected.relation)
            || target_context != bounded_context_name(expected.target.context)?
            || target_kind != expected.target.aggregate_kind.as_str()
            || target_id != expected.target.aggregate_id
            || u64::try_from(target_version).ok() != Some(expected.target.aggregate_version)
            || target_scope
                != serde_json::to_value(&expected.target_scope)
                    .map_err(|_| CqrsSqlxError::InvalidInput("target scope cannot be encoded"))?
            || target_digest != expected.target_digest
            || linked_by != expected.linked_by
            || linked_at.timestamp_micros() != expected.linked_at.timestamp_micros()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Legal matter link evidence changed",
            ));
        }
    }
    Ok(())
}

async fn guard_version(
    tx: &mut Transaction<'_, Postgres>,
    value: &LegalAggregate,
) -> Result<Option<i64>, CqrsSqlxError> {
    let (sql, id) = match value {
        LegalAggregate::Contract(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.contract_guards WHERE contract_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::Document(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.document_guards WHERE document_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::AccessEntry(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.access_entry_guards WHERE access_entry_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
        LegalAggregate::Matter(v) => (
            "SELECT aggregate_version FROM erpx_legal_write.matter_guards WHERE matter_id=$1 FOR UPDATE",
            v.id().as_uuid(),
        ),
    };
    Ok(sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?)
}

fn validate_guard_context(
    value: &LegalAggregate,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != value.aggregate_type()
        || context.aggregate_id.as_uuid() != value.id_uuid()
        || context.partition != aggregate_partition(value)
    {
        Err(CqrsSqlxError::GuardRejected(
            "Legal guard metadata mismatch",
        ))
    } else {
        Ok(())
    }
}
fn validate_commit(request: &LegalCommit) -> Result<(), PortError> {
    let (partition, aggregate_type) = fingerprint_target(&request.identity.fingerprint);
    if request.aggregate.validate().is_err()
        || partition != aggregate_partition(&request.aggregate)
        || aggregate_type != request.aggregate.aggregate_type()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        Err(PortError::Internal)
    } else {
        Ok(())
    }
}
fn fingerprint_target(value: &CommandFingerprint) -> (String, &'static str) {
    match value {
        CommandFingerprint::CreateContract { contract_id, .. }
        | CommandFingerprint::AddContractParty { contract_id, .. }
        | CommandFingerprint::TransitionContract { contract_id, .. } => {
            (contract_partition(*contract_id), CONTRACT_AGGREGATE_TYPE)
        }
        CommandFingerprint::CreateDocument { document_id, .. }
        | CommandFingerprint::AddDocumentVersion { document_id, .. }
        | CommandFingerprint::ArchiveDocument { document_id } => {
            (document_partition(*document_id), DOCUMENT_AGGREGATE_TYPE)
        }
        CommandFingerprint::RecordDocumentAccess {
            access_entry_id, ..
        } => (access_partition(*access_entry_id), ACCESS_AGGREGATE_TYPE),
        CommandFingerprint::CreateMatter { matter_id, .. }
        | CommandFingerprint::LinkMatterRecord { matter_id, .. }
        | CommandFingerprint::PlaceLegalHold { matter_id, .. }
        | CommandFingerprint::ReleaseLegalHold { matter_id }
        | CommandFingerprint::CloseMatter { matter_id } => {
            (matter_partition(*matter_id), MATTER_AGGREGATE_TYPE)
        }
    }
}
fn aggregate_partition(value: &LegalAggregate) -> String {
    match value {
        LegalAggregate::Contract(v) => contract_partition(v.id()),
        LegalAggregate::Document(v) => document_partition(v.id()),
        LegalAggregate::AccessEntry(v) => access_partition(v.id()),
        LegalAggregate::Matter(v) => matter_partition(v.id()),
    }
}
fn snapshot_event(value: &LegalAggregate) -> &'static str {
    match value {
        LegalAggregate::Contract(_) => CONTRACT_SNAPSHOT_EVENT,
        LegalAggregate::Document(_) => DOCUMENT_SNAPSHOT_EVENT,
        LegalAggregate::AccessEntry(_) => ACCESS_SNAPSHOT_EVENT,
        LegalAggregate::Matter(_) => MATTER_SNAPSHOT_EVENT,
    }
}
fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateContract { .. } => "legal.contract.create",
        CommandFingerprint::AddContractParty { .. } => "legal.contract.add_party",
        CommandFingerprint::TransitionContract { .. } => "legal.contract.transition",
        CommandFingerprint::CreateDocument { .. } => "legal.document.create",
        CommandFingerprint::AddDocumentVersion { .. } => "legal.document.add_version",
        CommandFingerprint::ArchiveDocument { .. } => "legal.document.archive",
        CommandFingerprint::RecordDocumentAccess { .. } => "legal.access.record",
        CommandFingerprint::CreateMatter { .. } => "legal.matter.create",
        CommandFingerprint::LinkMatterRecord { .. } => "legal.matter.link_record",
        CommandFingerprint::PlaceLegalHold { .. } => "legal.matter.place_hold",
        CommandFingerprint::ReleaseLegalHold { .. } => "legal.matter.release_hold",
        CommandFingerprint::CloseMatter { .. } => "legal.matter.close",
    }
}

fn record_relation_name(value: erpx_legal::RecordRelation) -> &'static str {
    match value {
        erpx_legal::RecordRelation::GovernedBy => "governed_by",
        erpx_legal::RecordRelation::Supports => "supports",
        erpx_legal::RecordRelation::BilledUnder => "billed_under",
        erpx_legal::RecordRelation::AttributedTo => "attributed_to",
        erpx_legal::RecordRelation::Disputes => "disputes",
        erpx_legal::RecordRelation::SettlementFor => "settlement_for",
        erpx_legal::RecordRelation::Supersedes => "supersedes",
        erpx_legal::RecordRelation::AccountingFor => "accounting_for",
    }
}

fn bounded_context_name(
    value: erpx_integration_contracts::BoundedContext,
) -> Result<String, CqrsSqlxError> {
    let encoded = serde_json::to_value(value)
        .map_err(|_| CqrsSqlxError::InvalidInput("bounded context cannot be encoded"))?;
    encoded
        .as_str()
        .map(str::to_owned)
        .ok_or(CqrsSqlxError::InvalidInput(
            "bounded context must encode as text",
        ))
}

async fn authorized_root(
    tx: &mut Transaction<'_, Postgres>,
    authority: &AuthorityScope,
    base_sql: &str,
    table: &str,
    id_column: &str,
    id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, PortError> {
    let (kind, holding_id, company_id, branch_id, company_ids, branch_ids) =
        authority_parameters(authority)?;
    let sql = format!(
        "{base_sql} AND (
          $2='platform' OR ($2='holding' AND holding_id=$3)
          OR ($2='company' AND holding_id=$3 AND company_id=$4)
          OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
          OR ($2='subtree' AND holding_id=$3 AND (
             (branch_id IS NULL AND cardinality($7::uuid[])=0 AND company_id=ANY($6::uuid[]))
             OR (branch_id IS NOT NULL AND company_id=ANY($6::uuid[]) AND branch_id=ANY($7::uuid[]))
          )))"
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .bind(kind)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(company_ids)
        .bind(branch_ids)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if row.is_some() {
        return Ok(row);
    }
    let exists_sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=$1)");
    let exists: bool = sqlx::query_scalar(&exists_sql)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
    if exists {
        Err(PortError::Denied)
    } else {
        Ok(None)
    }
}

type AuthorityParameters = (
    &'static str,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Vec<Uuid>,
    Vec<Uuid>,
);

fn authority_parameters(scope: &AuthorityScope) -> Result<AuthorityParameters, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    Ok(match scope {
        AuthorityScope::Platform => ("platform", None, None, None, vec![], vec![]),
        AuthorityScope::Holding { holding_id } => {
            ("holding", Some(*holding_id), None, None, vec![], vec![])
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => (
            "company",
            Some(*holding_id),
            Some(*company_id),
            None,
            vec![],
            vec![],
        ),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => (
            "branch",
            Some(*holding_id),
            Some(*company_id),
            Some(*branch_id),
            vec![],
            vec![],
        ),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => (
            "subtree",
            Some(*holding_id),
            None,
            None,
            vec![*organization_node_id],
            Vec::new(),
        ),
    })
}

fn exact_scope(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    exact_scope_named(
        row.try_get("holding_id").map_err(|_| PortError::Internal)?,
        row.try_get("company_id").map_err(|_| PortError::Internal)?,
        row.try_get("branch_id").map_err(|_| PortError::Internal)?,
    )
}

fn exact_scope_named(
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<AuthorityScope, PortError> {
    let scope = match branch_id {
        Some(branch_id) => AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        None => AuthorityScope::Company {
            holding_id,
            company_id,
        },
    };
    scope.validate().map_err(|_| PortError::Internal)?;
    Ok(scope)
}

fn exact_scope_ids(scope: &AuthorityScope) -> Result<(Uuid, Uuid, Option<Uuid>), CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(CqrsSqlxError::InvalidInput("Legal scope must be exact")),
    }
}

async fn write_exact_scope(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    id: Uuid,
    scope: &AuthorityScope,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = exact_scope_ids(scope)?;
    let sql =
        format!("UPDATE {table} SET holding_id=$2,company_id=$3,branch_id=$4 WHERE {id_column}=$1");
    let changed = sqlx::query(&sql)
        .bind(id)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    require_single_write(changed, "Legal scope authority write conflict")
}

async fn write_legal_hold_columns(
    tx: &mut Transaction<'_, Postgres>,
    matter: &LegalMatter,
) -> Result<(), CqrsSqlxError> {
    let hold = matter.legal_hold();
    let changed = sqlx::query(
        "UPDATE erpx_legal_write.matter_guards SET
           hold_active=$2,hold_reason=$3,hold_placed_by=$4,hold_placed_at=$5,
           hold_released_by=$6,hold_released_at=$7 WHERE matter_id=$1",
    )
    .bind(matter.id().as_uuid())
    .bind(hold.map(|value| value.active))
    .bind(hold.map(|value| value.reason.as_str()))
    .bind(hold.map(|value| value.placed_by.as_str()))
    .bind(hold.map(|value| value.placed_at))
    .bind(hold.and_then(|value| value.released_by.as_deref()))
    .bind(hold.and_then(|value| value.released_at))
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require_single_write(changed, "Legal hold authority write conflict")
}

async fn write_link_scope_columns(
    tx: &mut Transaction<'_, Postgres>,
    matter_id: Uuid,
    ordinal: usize,
    scope: &AuthorityScope,
) -> Result<(), CqrsSqlxError> {
    let (holding_id, company_id, branch_id) = exact_scope_ids(scope)?;
    let changed = sqlx::query(
        "UPDATE erpx_legal_write.matter_link_guards SET
           target_holding_id=$3,target_company_id=$4,target_branch_id=$5
         WHERE matter_id=$1 AND link_ordinal=$2",
    )
    .bind(matter_id)
    .bind(to_i64(
        u64::try_from(ordinal).map_err(|_| CqrsSqlxError::NumericRange)?,
    )?)
    .bind(holding_id)
    .bind(company_id)
    .bind(branch_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    require_single_write(changed, "Legal link authority write conflict")
}

fn require_single_write(changed: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn parse_contract_status(value: &str) -> Result<ContractStatus, PortError> {
    match value {
        "draft" => Ok(ContractStatus::Draft),
        "review" => Ok(ContractStatus::Review),
        "approved" => Ok(ContractStatus::Approved),
        "active" => Ok(ContractStatus::Active),
        "expired" => Ok(ContractStatus::Expired),
        "terminated" => Ok(ContractStatus::Terminated),
        "archived" => Ok(ContractStatus::Archived),
        _ => Err(PortError::Internal),
    }
}

fn parse_matter_status(value: &str) -> Result<MatterStatus, PortError> {
    match value {
        "open" => Ok(MatterStatus::Open),
        "closed" => Ok(MatterStatus::Closed),
        _ => Err(PortError::Internal),
    }
}

fn parse_record_relation(value: &str) -> Result<RecordRelation, PortError> {
    match value {
        "governed_by" => Ok(RecordRelation::GovernedBy),
        "supports" => Ok(RecordRelation::Supports),
        "billed_under" => Ok(RecordRelation::BilledUnder),
        "attributed_to" => Ok(RecordRelation::AttributedTo),
        "disputes" => Ok(RecordRelation::Disputes),
        "settlement_for" => Ok(RecordRelation::SettlementFor),
        "supersedes" => Ok(RecordRelation::Supersedes),
        "accounting_for" => Ok(RecordRelation::AccountingFor),
        _ => Err(PortError::Internal),
    }
}

fn parse_bounded_context(value: &str) -> Result<BoundedContext, PortError> {
    match value {
        "identity_access" => Ok(BoundedContext::IdentityAccess),
        "organization" => Ok(BoundedContext::Organization),
        "commercial" => Ok(BoundedContext::Commercial),
        "platform_administration" => Ok(BoundedContext::PlatformAdministration),
        "brand" => Ok(BoundedContext::Brand),
        "customer" => Ok(BoundedContext::Customer),
        "marketing" => Ok(BoundedContext::Marketing),
        "crm" => Ok(BoundedContext::Crm),
        "procurement" => Ok(BoundedContext::Procurement),
        "inventory" => Ok(BoundedContext::Inventory),
        "assets" => Ok(BoundedContext::Assets),
        "maintenance" => Ok(BoundedContext::Maintenance),
        "quality" => Ok(BoundedContext::Quality),
        "sales" => Ok(BoundedContext::Sales),
        "people_payroll" => Ok(BoundedContext::PeoplePayroll),
        "pos" => Ok(BoundedContext::Pos),
        "finance" => Ok(BoundedContext::Finance),
        "legal" => Ok(BoundedContext::Legal),
        "support" => Ok(BoundedContext::Support),
        "operations" => Ok(BoundedContext::Operations),
        _ => Err(PortError::Internal),
    }
}
fn request_hash(value: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let mut digest = Sha256::new();
    digest.update(b"erpx.legal.command\0");
    digest.update(serde_json::to_vec(value).map_err(|_| PortError::Internal)?);
    Ok(RequestHash::new(digest.finalize().into()))
}
fn deterministic_event_id(command: CommandId, kind: &str, index: usize) -> EventId {
    let mut input = kind.as_bytes().to_vec();
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command.as_uuid(), &input)).expect("derived event id")
}
fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("h:{holding_id}/c:{company_id}/b:{branch_id}")),
        _ => Err(CqrsSqlxError::InvalidInput("Legal scope must be exact")),
    }
}
fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}
fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<LegalAggregate>,
    aggregate_type: &str,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != aggregate_type
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}
fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::GuardRejected(_) => PortError::Conflict {
            expected: 0,
            actual: 0,
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_legal::AccessEntryId;
    #[test]
    fn stable_hash_and_targets_cover_access_evidence() {
        let entry = AccessEntryId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let document = LegalDocumentId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let value = CommandFingerprint::RecordDocumentAccess {
            access_entry_id: entry,
            document_id: document,
            document_version: 1,
            action: "download".into(),
            purpose: "review".into(),
            observed_at: chrono::Utc::now(),
        };
        assert_eq!(request_hash(&value).unwrap(), request_hash(&value).unwrap());
        assert_eq!(
            fingerprint_target(&value),
            (access_partition(entry), ACCESS_AGGREGATE_TYPE)
        );
    }

    #[test]
    fn append_only_sql_uses_insert_and_prefix_comparison_only() {
        let source = include_str!("command.rs");
        assert!(source.contains("Legal document version evidence changed"));
        assert!(source.contains("value.versions().iter().skip(existing)"));
        assert!(!source.contains(concat!(
            "DELETE FROM erpx_legal_write.document_",
            "version_guards"
        )));
    }

    #[test]
    fn legal_hold_authority_and_descriptor_share_the_matter_uow() {
        let source = include_str!("command.rs");
        assert!(source.contains("legal_hold_records"));
        assert!(source.contains("legal_directory_drafts"));
        assert!(source.contains("LegalOutboxPayload::Directory"));
        assert!(source.contains("source_matter_version + 1"));
    }
}
