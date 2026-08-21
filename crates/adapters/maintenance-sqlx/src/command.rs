use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    IntegrationEventEnvelope, ItemDescriptorDraft, ItemDescriptorDraftV2, assets, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use erpx_maintenance::{
    AssetFactPort, CommandFingerprint, CommandIdentity, CommandUnitOfWork, MaintenanceCommit,
    MaintenanceRequest, MaintenanceStoredCommit, PortError, WorkOrder, WorkOrderId, WorkOrderState,
    WorkOrderStatus, WriteRepository,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE,
    projection::{MaintenanceOutboxPayload, MaintenanceSnapshot, condition_name, status_name},
    work_order_partition,
};

#[derive(Debug, Clone)]
pub struct MaintenanceWriteRepository {
    pool: WritePool,
}

impl MaintenanceWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WriteRepository for MaintenanceWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        work_order_id: WorkOrderId,
    ) -> Result<Option<WorkOrder>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let query = format!(
            "SELECT aggregate_version,work_order_code,holding_id,company_id,branch_id,
                    source_event_id,source_aggregate_version,asset_id,request_code,
                    asset_code,asset_name,asset_location_id,asset_condition,title,
                    work_order_status,scheduled_for,started_at,completed_at,created_at,updated_at
             FROM erpx_maintenance_write.work_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND work_order_id=$1{AUTHORITY_FILTER}"
        );
        let row = sqlx::query(&query)
            .bind(work_order_id.as_uuid())
            .bind(authority.mode)
            .bind(authority.holding_id)
            .bind(authority.company_id)
            .bind(authority.branch_id)
            .bind(&authority.company_ids)
            .bind(&authority.branch_ids)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let asset = assets::AssetRef::try_from_uuid(
            row.try_get("asset_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let downtime_rows = sqlx::query(
            "SELECT downtime_id,started_at,ended_at,reason
             FROM erpx_maintenance_write.downtime_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND work_order_id=$1
             ORDER BY started_at,downtime_id",
        )
        .bind(work_order_id.as_uuid())
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let downtime = downtime_rows
            .into_iter()
            .map(|row| {
                Ok(erpx_maintenance::Downtime {
                    downtime_id: row
                        .try_get("downtime_id")
                        .map_err(|_| PortError::Internal)?,
                    started_at: row.try_get("started_at").map_err(|_| PortError::Internal)?,
                    ended_at: row.try_get("ended_at").map_err(|_| PortError::Internal)?,
                    reason: row.try_get("reason").map_err(|_| PortError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        let work_order = WorkOrder::try_rehydrate(WorkOrderState {
            id: work_order_id,
            version: row_u64(&row, "aggregate_version")?,
            code: row
                .try_get("work_order_code")
                .map_err(|_| PortError::Internal)?,
            scope: exact_scope_from_row(&row)?,
            request: MaintenanceRequest::try_new(
                EventId::try_from_uuid(
                    row.try_get("source_event_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                row_u64(&row, "source_aggregate_version")?,
                asset,
                row.try_get::<String, _>("request_code")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            asset_fact: assets::AssetConditionFact {
                asset,
                asset_code: erpx_integration_contracts::ReferenceCode::try_new(
                    row.try_get::<String, _>("asset_code")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                name: erpx_integration_contracts::DisplayName::try_new(
                    row.try_get::<String, _>("asset_name")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                location: assets::AssetLocationRef::try_from_uuid(
                    row.try_get("asset_location_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                condition: parse_condition(
                    row.try_get::<String, _>("asset_condition")
                        .map_err(|_| PortError::Internal)?
                        .as_str(),
                )
                .ok_or(PortError::Internal)?,
            },
            title: row.try_get("title").map_err(|_| PortError::Internal)?,
            status: parse_work_order_status(
                row.try_get("work_order_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            scheduled_for: row
                .try_get("scheduled_for")
                .map_err(|_| PortError::Internal)?,
            started_at: row.try_get("started_at").map_err(|_| PortError::Internal)?,
            completed_at: row
                .try_get("completed_at")
                .map_err(|_| PortError::Internal)?,
            downtime,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(work_order))
    }
}

#[derive(Debug)]
struct AuthoritySqlScope {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthoritySqlScope {
    fn try_from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: vec![],
                branch_ids: vec![],
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

const AUTHORITY_FILTER: &str = "
 AND ($2='platform' OR ($2='holding' AND holding_id=$3)
   OR ($2='company' AND holding_id=$3 AND company_id=$4)
   OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
   OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
       AND ((branch_id IS NULL AND cardinality($7::uuid[])=0) OR branch_id=ANY($7))))";

/// Reads the local typed Assets fact inbox. It does not query Assets snapshots
/// or import the Assets core crate.
#[derive(Debug, Clone)]
pub struct MaintenanceAssetFactRepository {
    pool: PgPool,
}

impl MaintenanceAssetFactRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetFactPort for MaintenanceAssetFactRepository {
    async fn get_at_least(
        &self,
        asset: assets::AssetRef,
        source_aggregate_version: u64,
    ) -> Result<assets::AssetConditionFact, PortError> {
        let row = sqlx::query(
            "SELECT asset_code,asset_name,location_id,asset_condition
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2",
        )
        .bind(asset.as_uuid())
        .bind(to_i64_port(source_aggregate_version)?)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::AssetFactNotReady)?;
        decode_asset_fact(asset, &row)
    }
}

/// Idempotent inbox writer invoked by the Worker after consuming an Assets
/// condition event. Source aggregate version is monotonic and event IDs are
/// unique under the frozen DDL.
#[derive(Debug, Clone)]
pub struct MaintenanceAssetFactInbox {
    pool: PgPool,
}

impl MaintenanceAssetFactInbox {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ingest(
        &self,
        envelope: &IntegrationEventEnvelope<assets::Event>,
    ) -> Result<bool, PortError> {
        envelope.validate().map_err(|_| PortError::Internal)?;
        let assets::Event::AssetConditionChanged(fact) = envelope.payload() else {
            return Ok(false);
        };
        if fact.asset.as_uuid() != envelope.aggregate_id().as_uuid() {
            return Err(PortError::Internal);
        }
        let result = sqlx::query(
            "INSERT INTO erpx_maintenance_write.asset_fact_inbox
                (asset_id,source_aggregate_version,source_event_id,active_scope,
                 asset_code,asset_name,location_id,asset_condition,occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (asset_id,source_aggregate_version) DO NOTHING",
        )
        .bind(fact.asset.as_uuid())
        .bind(to_i64_port(envelope.aggregate_version())?)
        .bind(envelope.event_id().as_uuid())
        .bind(serde_json::to_value(envelope.active_scope()).map_err(|_| PortError::Internal)?)
        .bind(fact.asset_code.as_str())
        .bind(fact.name.as_str())
        .bind(fact.location.as_uuid())
        .bind(condition_name(fact.condition))
        .bind(*envelope.occurred_at())
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .and_then(|value| value.constraint())
                == Some("asset_fact_inbox_source_event_id_key")
            {
                PortError::IdempotencyConflict
            } else {
                PortError::Unavailable
            }
        })?;
        if result.rows_affected() == 1 {
            return Ok(true);
        }

        let row = sqlx::query(
            "SELECT source_event_id,active_scope,asset_code,asset_name,location_id,
                    asset_condition,occurred_at
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2",
        )
        .bind(fact.asset.as_uuid())
        .bind(to_i64_port(envelope.aggregate_version())?)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PortError::Unavailable)?
        .ok_or(PortError::Internal)?;
        let source_event_id: Uuid = row
            .try_get("source_event_id")
            .map_err(|_| PortError::Internal)?;
        let active_scope: serde_json::Value = row
            .try_get("active_scope")
            .map_err(|_| PortError::Internal)?;
        let asset_code: String = row.try_get("asset_code").map_err(|_| PortError::Internal)?;
        let asset_name: String = row.try_get("asset_name").map_err(|_| PortError::Internal)?;
        let location_id: Uuid = row
            .try_get("location_id")
            .map_err(|_| PortError::Internal)?;
        let condition: String = row
            .try_get("asset_condition")
            .map_err(|_| PortError::Internal)?;
        let occurred_at: chrono::DateTime<chrono::Utc> = row
            .try_get("occurred_at")
            .map_err(|_| PortError::Internal)?;
        let exact_replay = source_event_id == envelope.event_id().as_uuid()
            && active_scope
                == serde_json::to_value(envelope.active_scope())
                    .map_err(|_| PortError::Internal)?
            && asset_code == fact.asset_code.as_str()
            && asset_name == fact.name.as_str()
            && location_id == fact.location.as_uuid()
            && condition == condition_name(fact.condition)
            && occurred_at == *envelope.occurred_at();
        if exact_replay {
            Ok(false)
        } else {
            Err(PortError::IdempotencyConflict)
        }
    }
}

#[derive(Debug, Clone)]
pub struct MaintenanceCommandAdapter {
    store: CommandStore,
}

impl MaintenanceCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for MaintenanceCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<MaintenanceStoredCommit>, PortError> {
        let id = fingerprint_work_order_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<WorkOrder>(
                CONTEXT_NAME,
                &work_order_partition(id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(
        &self,
        request: MaintenanceCommit,
    ) -> Result<MaintenanceStoredCommit, PortError> {
        validate_commit(&request)?;
        let id = request.aggregate.id();
        let partition = work_order_partition(id);
        let occurred_at = request.aggregate.updated_at();
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
                payload: MaintenanceOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: MaintenanceOutboxPayload::Snapshot(MaintenanceSnapshot {
                work_order: (&request.aggregate).into(),
            }),
        });
        let directory_draft = maintenance_directory_draft(&request.aggregate)?;
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
            payload: MaintenanceOutboxPayload::Directory(directory_draft),
        });
        let guard = WorkOrderAtomicGuard {
            aggregate: request.aggregate.clone(),
            consumed_source_event: request.consumed_source_event,
        };
        let aggregate = request.aggregate;
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
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(id)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"maintenance-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "work_order_id": id.as_uuid(),
                    "expected_version": request.expected_version,
                    "source_event_id": guard.consumed_source_event.map(EventId::as_uuid),
                }),
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
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn maintenance_directory_draft(aggregate: &WorkOrder) -> Result<ItemDescriptorDraftV2, PortError> {
    let record = aggregate.record_ref().map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{} {}", aggregate.code(), aggregate.title()),
    };
    ItemDescriptorDraftV2::try_new(value, format!("{} {}", aggregate.code(), aggregate.title()))
        .map_err(|_| PortError::Internal)
}

struct WorkOrderAtomicGuard {
    aggregate: WorkOrder,
    consumed_source_event: Option<EventId>,
}

#[async_trait]
impl AtomicCommandGuard for WorkOrderAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_maintenance_write.work_order_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND work_order_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        value.map(from_i64_store).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        match (context.expected_version, self.consumed_source_event) {
            (None, Some(event)) if event == self.aggregate.request().source_event_id => {}
            (Some(_), None) => {}
            _ => {
                return Err(CqrsSqlxError::GuardRejected(
                    "consumed source-event guard mismatch",
                ));
            }
        }
        let fact = sqlx::query(
            "SELECT asset_code,asset_name,location_id,asset_condition,active_scope
             FROM erpx_maintenance_write.asset_fact_inbox
             WHERE asset_id=$1 AND source_aggregate_version=$2 FOR SHARE",
        )
        .bind(self.aggregate.asset_fact().asset.as_uuid())
        .bind(to_i64(self.aggregate.request().source_aggregate_version)?)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(CqrsSqlxError::GuardRejected("Assets fact is not ready"))?;
        let stored = decode_asset_fact_store(self.aggregate.asset_fact().asset, &fact)?;
        let active_scope: AuthorityScope = serde_json::from_value(fact.try_get("active_scope")?)?;
        if stored != *self.aggregate.asset_fact() || active_scope != *self.aggregate.scope() {
            return Err(CqrsSqlxError::GuardRejected(
                "Assets fact changed or belongs to another scope",
            ));
        }
        let downtime_rows = sqlx::query(
            "SELECT downtime_id FROM erpx_maintenance_write.downtime_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND work_order_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_all(&mut **transaction)
        .await?;
        if downtime_rows.iter().any(|row| {
            row.try_get::<Uuid, _>("downtime_id").ok().is_none_or(|id| {
                !self
                    .aggregate
                    .downtime()
                    .iter()
                    .any(|downtime| downtime.downtime_id == id)
            })
        }) {
            return Err(CqrsSqlxError::GuardRejected(
                "Maintenance downtime history cannot be removed",
            ));
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != work_order_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Maintenance applied guard context mismatch",
            ));
        }
        let (holding_id, company_id, branch_id) = exact_scope_ids(self.aggregate.scope())?;
        let result = sqlx::query(
            "INSERT INTO erpx_maintenance_write.work_order_authority
                (holding_id,work_order_id,aggregate_version,work_order_code,holding_id,company_id,
                 branch_id,source_event_id,source_aggregate_version,asset_id,request_code,
                 asset_code,asset_name,asset_location_id,asset_condition,title,work_order_status,
                 scheduled_for,started_at,completed_at,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                     $11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
             ON CONFLICT (holding_id,work_order_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,
                work_order_code=EXCLUDED.work_order_code,holding_id=EXCLUDED.holding_id,
                company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
                source_event_id=EXCLUDED.source_event_id,
                source_aggregate_version=EXCLUDED.source_aggregate_version,
                asset_id=EXCLUDED.asset_id,request_code=EXCLUDED.request_code,
                asset_code=EXCLUDED.asset_code,asset_name=EXCLUDED.asset_name,
                asset_location_id=EXCLUDED.asset_location_id,
                asset_condition=EXCLUDED.asset_condition,title=EXCLUDED.title,
                work_order_status=EXCLUDED.work_order_status,
                scheduled_for=EXCLUDED.scheduled_for,started_at=EXCLUDED.started_at,
                completed_at=EXCLUDED.completed_at,created_at=EXCLUDED.created_at,
                updated_at=EXCLUDED.updated_at
             WHERE erpx_maintenance_write.work_order_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_maintenance_write.work_order_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.code())
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(self.aggregate.request().source_event_id.as_uuid())
        .bind(to_i64(self.aggregate.request().source_aggregate_version)?)
        .bind(self.aggregate.asset_fact().asset.as_uuid())
        .bind(&self.aggregate.request().request_code)
        .bind(self.aggregate.asset_fact().asset_code.as_str())
        .bind(self.aggregate.asset_fact().name.as_str())
        .bind(self.aggregate.asset_fact().location.as_uuid())
        .bind(condition_name(self.aggregate.asset_fact().condition))
        .bind(self.aggregate.title())
        .bind(status_name(self.aggregate.status()))
        .bind(self.aggregate.scheduled_for())
        .bind(self.aggregate.started_at())
        .bind(self.aggregate.completed_at())
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        require_one_row(
            result.rows_affected(),
            "Maintenance work-order version fence",
        )?;
        for downtime in self.aggregate.downtime() {
            let result = sqlx::query(
                "INSERT INTO erpx_maintenance_write.downtime_authority
                    (holding_id,work_order_id,downtime_id,started_at,ended_at,reason,
                     work_order_aggregate_version)
                 VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6)
                 ON CONFLICT (holding_id,work_order_id,downtime_id) DO UPDATE SET
                    work_order_aggregate_version=EXCLUDED.work_order_aggregate_version
                 WHERE erpx_maintenance_write.downtime_authority.started_at=EXCLUDED.started_at
                   AND erpx_maintenance_write.downtime_authority.ended_at=EXCLUDED.ended_at
                   AND erpx_maintenance_write.downtime_authority.reason=EXCLUDED.reason",
            )
            .bind(self.aggregate.id().as_uuid())
            .bind(downtime.downtime_id)
            .bind(downtime.started_at)
            .bind(downtime.ended_at)
            .bind(&downtime.reason)
            .bind(to_i64(self.aggregate.version())?)
            .execute(&mut **transaction)
            .await?;
            require_one_row(
                result.rows_affected(),
                "Maintenance downtime immutable fields changed",
            )?;
        }
        Ok(())
    }
}

fn validate_guard_context(
    aggregate: &WorkOrder,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != work_order_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Maintenance command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &MaintenanceCommit) -> Result<(), PortError> {
    if request.aggregate.validate().is_err()
        || fingerprint_work_order_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn decode_asset_fact(
    asset: assets::AssetRef,
    row: &sqlx::postgres::PgRow,
) -> Result<assets::AssetConditionFact, PortError> {
    let code: String = row.try_get("asset_code").map_err(|_| PortError::Internal)?;
    let name: String = row.try_get("asset_name").map_err(|_| PortError::Internal)?;
    let location: Uuid = row
        .try_get("location_id")
        .map_err(|_| PortError::Internal)?;
    let condition: String = row
        .try_get("asset_condition")
        .map_err(|_| PortError::Internal)?;
    Ok(assets::AssetConditionFact {
        asset,
        asset_code: erpx_integration_contracts::ReferenceCode::try_new(code)
            .map_err(|_| PortError::Internal)?,
        name: erpx_integration_contracts::DisplayName::try_new(name)
            .map_err(|_| PortError::Internal)?,
        location: assets::AssetLocationRef::try_from_uuid(location)
            .map_err(|_| PortError::Internal)?,
        condition: parse_condition(&condition).ok_or(PortError::Internal)?,
    })
}

fn decode_asset_fact_store(
    asset: assets::AssetRef,
    row: &sqlx::postgres::PgRow,
) -> Result<assets::AssetConditionFact, CqrsSqlxError> {
    decode_asset_fact(asset, row)
        .map_err(|_| CqrsSqlxError::GuardRejected("stored Assets fact is invalid"))
}

fn parse_condition(value: &str) -> Option<assets::AssetCondition> {
    match value {
        "operational" => Some(assets::AssetCondition::Operational),
        "degraded" => Some(assets::AssetCondition::Degraded),
        "out_of_service" => Some(assets::AssetCondition::OutOfService),
        "retired" => Some(assets::AssetCondition::Retired),
        _ => None,
    }
}

fn parse_work_order_status(value: &str) -> Result<WorkOrderStatus, PortError> {
    match value {
        "planned" => Ok(WorkOrderStatus::Planned),
        "scheduled" => Ok(WorkOrderStatus::Scheduled),
        "in_progress" => Ok(WorkOrderStatus::InProgress),
        "blocked" => Ok(WorkOrderStatus::Blocked),
        "completed" => Ok(WorkOrderStatus::Completed),
        "cancelled" => Ok(WorkOrderStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}

fn exact_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<AuthorityScope, PortError> {
    let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
    let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
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
        _ => Err(CqrsSqlxError::GuardRejected(
            "Maintenance normalized authority requires exact scope",
        )),
    }
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn require_one_row(rows: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

fn aggregate_id(id: WorkOrderId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_work_order_id(fingerprint: &CommandFingerprint) -> WorkOrderId {
    match fingerprint {
        CommandFingerprint::Open { work_order_id, .. }
        | CommandFingerprint::Schedule { work_order_id, .. }
        | CommandFingerprint::Start { work_order_id }
        | CommandFingerprint::Block { work_order_id }
        | CommandFingerprint::Resume { work_order_id }
        | CommandFingerprint::Complete { work_order_id }
        | CommandFingerprint::Cancel { work_order_id }
        | CommandFingerprint::RecordDowntime { work_order_id, .. } => *work_order_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.maintenance.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Open { .. } => "maintenance.work_order.open",
        CommandFingerprint::Schedule { .. } => "maintenance.work_order.schedule",
        CommandFingerprint::Start { .. } => "maintenance.work_order.start",
        CommandFingerprint::Block { .. } => "maintenance.work_order.block",
        CommandFingerprint::Resume { .. } => "maintenance.work_order.resume",
        CommandFingerprint::Complete { .. } => "maintenance.work_order.complete",
        CommandFingerprint::Cancel { .. } => "maintenance.work_order.cancel",
        CommandFingerprint::RecordDowntime { .. } => "maintenance.work_order.record_downtime",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<WorkOrder>,
) -> Result<MaintenanceStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(MaintenanceStoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = event_type.as_bytes().to_vec();
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
        .expect("a digest-derived event ID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value)
        .map_err(|_| CqrsSqlxError::GuardRejected("Maintenance version is too large"))
}

fn to_i64_port(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
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
        CqrsSqlxError::GuardRejected("Assets fact is not ready") => PortError::AssetFactNotReady,
        CqrsSqlxError::Database(error) => match error
            .as_database_error()
            .and_then(|value| value.constraint())
        {
            Some("work_order_guards_source_event_id_key") => PortError::SourceEventAlreadyConsumed,
            _ => PortError::Unavailable,
        },
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_parser_accepts_only_frozen_values() {
        assert_eq!(
            parse_condition("out_of_service"),
            Some(assets::AssetCondition::OutOfService)
        );
        assert_eq!(parse_condition("draft"), None);
    }

    #[test]
    fn fingerprint_hash_is_stable() {
        let id = WorkOrderId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let fingerprint = CommandFingerprint::Start { work_order_id: id };
        assert_eq!(
            request_hash(&fingerprint).unwrap(),
            request_hash(&fingerprint).unwrap()
        );
    }
}
