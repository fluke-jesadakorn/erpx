use async_trait::async_trait;
use erpx_commercial::{
    CatalogPlan, CatalogPlanId, CatalogPlanView, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, CommercialAggregate, CommercialCommit, CommercialStoredCommit, Entitlement,
    EntitlementCapacity, EntitlementId, EntitlementState, EntitlementView, PortError, Subscription,
    SubscriptionId, SubscriptionState, SubscriptionStatus, SubscriptionView, WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    CurrencyCode, ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CATALOG_PLAN_AGGREGATE, CATALOG_PLAN_SNAPSHOT_EVENT, CONTEXT_NAME, ENTITLEMENT_AGGREGATE,
    ENTITLEMENT_SNAPSHOT_EVENT, EVENT_VERSION, SUBSCRIPTION_AGGREGATE, SUBSCRIPTION_SNAPSHOT_EVENT,
    catalog_plan_partition, entitlement_partition,
    projection::{
        CommercialOutboxPayload, CommercialSnapshot, catalog_status, subscription_status,
    },
    subscription_partition,
};

#[derive(Debug, Clone)]
pub struct CommercialWriteRepository {
    control_write: WritePool,
    holding_write: WritePool,
}

impl CommercialWriteRepository {
    pub fn new(control_write: WritePool, holding_write: WritePool) -> Self {
        Self {
            control_write,
            holding_write,
        }
    }

    pub fn from_single_pool(pool: WritePool) -> Self {
        Self::new(pool.clone(), pool)
    }
}

#[async_trait]
impl WriteRepository for CommercialWriteRepository {
    async fn load_catalog_plan(
        &self,
        authority_scope: &AuthorityScope,
        id: CatalogPlanId,
    ) -> Result<Option<CatalogPlan>, PortError> {
        load_catalog_plan_relational(&self.control_write, authority_scope, id).await
    }

    async fn load_subscription(
        &self,
        authority_scope: &AuthorityScope,
        id: SubscriptionId,
    ) -> Result<Option<Subscription>, PortError> {
        let holding_id = authority_holding_id(authority_scope)?;
        let mut transaction = self.holding_write.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,holding_id,customer_id,catalog_plan_id,
                    subscription_status,amount_minor,currency,last_provider_event_id,
                    last_provider_event_at,grace_expires_at,created_at,updated_at
             FROM erpx_commercial_write.subscription_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND subscription_id=$1 AND holding_id=$2",
        )
        .bind(id.as_uuid())
        .bind(holding_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let currency_text: String = row.try_get("currency").map_err(|_| PortError::Internal)?;
        let value = Subscription::try_rehydrate(SubscriptionState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope: AuthorityScope::Holding { holding_id },
            customer_id: row
                .try_get("customer_id")
                .map_err(|_| PortError::Internal)?,
            plan_id: CatalogPlanId::try_from_uuid(
                row.try_get("catalog_plan_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            status: parse_subscription_status(
                row.try_get("subscription_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            amount_minor: row
                .try_get("amount_minor")
                .map_err(|_| PortError::Internal)?,
            currency: CurrencyCode::try_new(&currency_text).map_err(|_| PortError::Internal)?,
            last_provider_event_id: row
                .try_get("last_provider_event_id")
                .map_err(|_| PortError::Internal)?,
            last_provider_event_at: row
                .try_get("last_provider_event_at")
                .map_err(|_| PortError::Internal)?,
            grace_expires_at: row
                .try_get("grace_expires_at")
                .map_err(|_| PortError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(value))
    }

    async fn load_entitlement(
        &self,
        authority_scope: &AuthorityScope,
        id: EntitlementId,
    ) -> Result<Option<Entitlement>, PortError> {
        let holding_id = authority_holding_id(authority_scope)?;
        let mut transaction = self.holding_write.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,holding_id,subscription_id,capability,enabled,
                    seat_capacity,company_capacity,branch_capacity,allocated_seats,
                    allocated_companies,allocated_branches,last_provider_event_id,
                    last_provider_event_at,created_at,updated_at
             FROM erpx_commercial_write.entitlement_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND entitlement_id=$1 AND holding_id=$2",
        )
        .bind(id.as_uuid())
        .bind(holding_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let value = Entitlement::try_rehydrate(EntitlementState {
            id,
            version: row_u64(&row, "aggregate_version")?,
            scope: AuthorityScope::Holding { holding_id },
            subscription_id: SubscriptionId::try_from_uuid(
                row.try_get("subscription_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            capability: row.try_get("capability").map_err(|_| PortError::Internal)?,
            enabled: row.try_get("enabled").map_err(|_| PortError::Internal)?,
            capacity: capacity_from_row(
                &row,
                "seat_capacity",
                "company_capacity",
                "branch_capacity",
            )?,
            allocated: capacity_from_row(
                &row,
                "allocated_seats",
                "allocated_companies",
                "allocated_branches",
            )?,
            last_provider_event_id: row
                .try_get("last_provider_event_id")
                .map_err(|_| PortError::Internal)?,
            last_provider_event_at: row
                .try_get("last_provider_event_at")
                .map_err(|_| PortError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        })
        .map_err(|_| PortError::Internal)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        Ok(Some(value))
    }
}

#[derive(Debug, Clone)]
pub struct CommercialCommandAdapter {
    control_store: CommandStore,
    control_write: WritePool,
    holding_store: CommandStore,
}

impl CommercialCommandAdapter {
    pub fn new(control_write: WritePool, holding_write: WritePool) -> Self {
        Self {
            control_store: CommandStore::new(control_write.clone()),
            control_write,
            holding_store: CommandStore::new(holding_write),
        }
    }

    pub fn from_single_pool(pool: WritePool) -> Self {
        Self::new(pool.clone(), pool)
    }

    async fn ensure_published_control_plan(&self, plan_id: CatalogPlanId) -> Result<(), PortError> {
        let Some(plan) =
            load_catalog_plan_relational(&self.control_write, &AuthorityScope::Platform, plan_id)
                .await?
        else {
            return Err(PortError::NotFound);
        };
        if plan.status() != erpx_commercial::CatalogPlanStatus::Published {
            return Err(PortError::Conflict {
                expected: plan.version(),
                actual: plan.version(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl CommandUnitOfWork for CommercialCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<CommercialStoredCommit>, PortError> {
        let key = fingerprint_key(&identity.fingerprint);
        let store = match key.plane {
            StoragePlane::Control => &self.control_store,
            StoragePlane::Holding => &self.holding_store,
        };
        let receipt = store
            .find_receipt::<CommercialAggregate>(
                CONTEXT_NAME,
                &key.partition,
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: CommercialCommit) -> Result<CommercialStoredCommit, PortError> {
        validate_commit(&request)?;
        if let CommercialAggregate::Subscription(subscription) = &request.aggregate {
            self.ensure_published_control_plan(subscription.plan_id())
                .await?;
        }
        let key = aggregate_key(&request.aggregate);
        let plane = key.plane;
        let occurred_at = aggregate_updated_at(&request.aggregate);
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let audit_action = audit_action(&request.identity.fingerprint);
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: key.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: CommercialOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, key.snapshot_event, events.len()),
            event_type: key.snapshot_event.into(),
            event_version: EVENT_VERSION,
            partition: key.partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: CommercialOutboxPayload::Snapshot(snapshot(&request.aggregate)),
        });
        if let Some(directory_draft) = commercial_directory_draft(&request.aggregate)? {
            events.push(OutboxEventWrite {
                event_id: deterministic_event_id(
                    command_id,
                    ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                    events.len(),
                ),
                event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
                event_version: ITEM_DIRECTORY_FACT_VERSION,
                partition: key.partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: CommercialOutboxPayload::Directory(directory_draft),
            });
        }
        let aggregate_id = aggregate_id(request.aggregate.id_uuid())?;
        let aggregate = request.aggregate;
        let guard = CommercialGuard {
            aggregate: aggregate.clone(),
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition: key.partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash,
            active_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: key.aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"commercial-audit-v1"),
                action: audit_action.into(),
                occurred_at,
                details: json!({ "aggregate_id": aggregate_id.as_uuid(), "expected_version": request.expected_version }),
            },
            events,
        };
        let store = match plane {
            StoragePlane::Control => &self.control_store,
            StoragePlane::Holding => &self.holding_store,
        };
        let outcome = store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(value) | AtomicCommandOutcome::Replayed(value) => value,
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn commercial_directory_draft(
    aggregate: &CommercialAggregate,
) -> Result<Option<ItemDescriptorDraftV2>, PortError> {
    let (record_type, lifecycle_state, display_label) = match aggregate {
        CommercialAggregate::CatalogPlan(_) => return Ok(None),
        CommercialAggregate::Subscription(value) => (
            "commercial.subscription",
            subscription_status(value.status()).to_owned(),
            value.customer_id().to_string(),
        ),
        CommercialAggregate::Entitlement(value) => (
            "commercial.entitlement",
            if value.enabled() {
                "enabled"
            } else {
                "disabled"
            }
            .to_owned(),
            value.capability().to_owned(),
        ),
    };
    let record_id =
        RecordId::try_from_uuid(aggregate.id_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Commercial,
        RecordTypeId::try_new(record_type).map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state,
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: display_label.clone(),
    };
    ItemDescriptorDraftV2::try_new(value, display_label)
        .map(Some)
        .map_err(|_| PortError::Internal)
}

#[derive(Debug)]
struct CommercialGuard {
    aggregate: CommercialAggregate,
}

#[async_trait]
impl AtomicCommandGuard for CommercialGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.aggregate_type != self.aggregate.aggregate_type()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "commercial guard metadata mismatch",
            ));
        }
        let query = match &self.aggregate {
            CommercialAggregate::CatalogPlan(_) => {
                "SELECT aggregate_version FROM erpx_commercial_write.catalog_plan_guards WHERE catalog_plan_id=$1 FOR UPDATE"
            }
            CommercialAggregate::Subscription(_) => {
                "SELECT aggregate_version FROM erpx_commercial_write.subscription_authority
                 WHERE holding_id=erpx_cqrs_write.current_holding_id() AND subscription_id=$1 FOR UPDATE"
            }
            CommercialAggregate::Entitlement(_) => {
                "SELECT aggregate_version FROM erpx_commercial_write.entitlement_authority
                 WHERE holding_id=erpx_cqrs_write.current_holding_id() AND entitlement_id=$1 FOR UPDATE"
            }
        };
        let value: Option<i64> = sqlx::query_scalar(query)
            .bind(self.aggregate.id_uuid())
            .fetch_optional(&mut **transaction)
            .await?;
        value.map(from_i64).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_id.as_uuid() != self.aggregate.id_uuid()
            || context.aggregate_type != self.aggregate.aggregate_type()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "commercial guard metadata mismatch",
            ));
        }
        match &self.aggregate {
            CommercialAggregate::CatalogPlan(plan) => {
                let conflict: Option<Uuid> = sqlx::query_scalar("SELECT catalog_plan_id FROM erpx_commercial_write.catalog_plan_guards WHERE plan_key=$1 AND catalog_plan_id<>$2 FOR UPDATE")
                    .bind(plan.key()).bind(plan.id().as_uuid()).fetch_optional(&mut **transaction).await?;
                if conflict.is_some() {
                    return Err(CqrsSqlxError::GuardRejected(
                        "commercial plan key already exists",
                    ));
                }
            }
            // The immutable published plan was loaded from control storage
            // before the holding transaction was opened.
            CommercialAggregate::Subscription(_) => {}
            CommercialAggregate::Entitlement(entitlement) => {
                let status: Option<String> = sqlx::query_scalar("SELECT subscription_status FROM erpx_commercial_write.subscription_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND subscription_id=$1 AND holding_id=$2 FOR SHARE")
                    .bind(entitlement.subscription_id().as_uuid()).bind(entitlement.holding_id())
                    .fetch_optional(&mut **transaction).await?;
                if status.as_deref().is_none_or(|value| value == "ended") {
                    return Err(CqrsSqlxError::GuardRejected(
                        "commercial entitlement requires a live subscription",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        let version = to_i64(context.aggregate_version)?;
        match &self.aggregate {
            CommercialAggregate::CatalogPlan(plan) => {
                sqlx::query("INSERT INTO erpx_commercial_write.catalog_plan_guards (catalog_plan_id,plan_key,display_name,amount_minor,currency,plan_status,aggregate_version,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (catalog_plan_id) DO UPDATE SET plan_key=EXCLUDED.plan_key,display_name=EXCLUDED.display_name,amount_minor=EXCLUDED.amount_minor,currency=EXCLUDED.currency,plan_status=EXCLUDED.plan_status,aggregate_version=EXCLUDED.aggregate_version,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at")
                    .bind(plan.id().as_uuid()).bind(plan.key()).bind(plan.display_name()).bind(plan.amount_minor()).bind(plan.currency().as_str()).bind(catalog_status(plan.status())).bind(version).bind(plan.created_at()).bind(plan.updated_at()).execute(&mut **transaction).await?;
            }
            CommercialAggregate::Subscription(value) => {
                let result = sqlx::query("INSERT INTO erpx_commercial_write.subscription_authority (holding_id,subscription_id,aggregate_version,holding_id,customer_id,catalog_plan_id,subscription_status,amount_minor,currency,last_provider_event_id,last_provider_event_at,grace_expires_at,created_at,updated_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (holding_id,subscription_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,holding_id=EXCLUDED.holding_id,customer_id=EXCLUDED.customer_id,catalog_plan_id=EXCLUDED.catalog_plan_id,subscription_status=EXCLUDED.subscription_status,amount_minor=EXCLUDED.amount_minor,currency=EXCLUDED.currency,last_provider_event_id=EXCLUDED.last_provider_event_id,last_provider_event_at=EXCLUDED.last_provider_event_at,grace_expires_at=EXCLUDED.grace_expires_at,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_commercial_write.subscription_authority.holding_id=erpx_cqrs_write.current_holding_id() AND erpx_commercial_write.subscription_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(version).bind(value.holding_id()).bind(value.customer_id()).bind(value.plan_id().as_uuid()).bind(subscription_status(value.status())).bind(value.amount_minor()).bind(value.currency().as_str()).bind(value.last_provider_event_id()).bind(value.last_provider_event_at()).bind(value.grace_expires_at()).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
                require_one_row(
                    result.rows_affected(),
                    "commercial subscription version fence",
                )?;
            }
            CommercialAggregate::Entitlement(value) => {
                let result = sqlx::query("INSERT INTO erpx_commercial_write.entitlement_authority (holding_id,entitlement_id,aggregate_version,holding_id,subscription_id,capability,enabled,seat_capacity,company_capacity,branch_capacity,allocated_seats,allocated_companies,allocated_branches,last_provider_event_id,last_provider_event_at,created_at,updated_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16) ON CONFLICT (holding_id,entitlement_id) DO UPDATE SET aggregate_version=EXCLUDED.aggregate_version,holding_id=EXCLUDED.holding_id,subscription_id=EXCLUDED.subscription_id,capability=EXCLUDED.capability,enabled=EXCLUDED.enabled,seat_capacity=EXCLUDED.seat_capacity,company_capacity=EXCLUDED.company_capacity,branch_capacity=EXCLUDED.branch_capacity,allocated_seats=EXCLUDED.allocated_seats,allocated_companies=EXCLUDED.allocated_companies,allocated_branches=EXCLUDED.allocated_branches,last_provider_event_id=EXCLUDED.last_provider_event_id,last_provider_event_at=EXCLUDED.last_provider_event_at,created_at=EXCLUDED.created_at,updated_at=EXCLUDED.updated_at WHERE erpx_commercial_write.entitlement_authority.holding_id=erpx_cqrs_write.current_holding_id() AND erpx_commercial_write.entitlement_authority.aggregate_version + 1=EXCLUDED.aggregate_version")
                    .bind(value.id().as_uuid()).bind(version).bind(value.holding_id()).bind(value.subscription_id().as_uuid()).bind(value.capability()).bind(value.enabled())
                    .bind(i64::from(value.capacity().seats)).bind(i64::from(value.capacity().companies)).bind(i64::from(value.capacity().branches))
                    .bind(i64::from(value.allocated().seats)).bind(i64::from(value.allocated().companies)).bind(i64::from(value.allocated().branches))
                    .bind(value.last_provider_event_id()).bind(value.last_provider_event_at()).bind(value.created_at()).bind(value.updated_at()).execute(&mut **transaction).await?;
                require_one_row(
                    result.rows_affected(),
                    "commercial entitlement version fence",
                )?;
            }
        }
        Ok(())
    }
}

struct AggregateKey {
    partition: String,
    aggregate_type: &'static str,
    snapshot_event: &'static str,
    plane: StoragePlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoragePlane {
    Control,
    Holding,
}

fn fingerprint_key(value: &CommandFingerprint) -> AggregateKey {
    match value {
        CommandFingerprint::CreateCatalogPlan { plan_id, .. }
        | CommandFingerprint::PublishCatalogPlan { plan_id } => AggregateKey {
            partition: catalog_plan_partition(*plan_id),
            aggregate_type: CATALOG_PLAN_AGGREGATE,
            snapshot_event: CATALOG_PLAN_SNAPSHOT_EVENT,
            plane: StoragePlane::Control,
        },
        CommandFingerprint::CreateSubscription {
            subscription_id, ..
        }
        | CommandFingerprint::ApplyBillingEvent {
            subscription_id, ..
        } => AggregateKey {
            partition: subscription_partition(*subscription_id),
            aggregate_type: SUBSCRIPTION_AGGREGATE,
            snapshot_event: SUBSCRIPTION_SNAPSHOT_EVENT,
            plane: StoragePlane::Holding,
        },
        CommandFingerprint::CreateEntitlement { entitlement_id, .. }
        | CommandFingerprint::ApplyEntitlementChange { entitlement_id, .. } => AggregateKey {
            partition: entitlement_partition(*entitlement_id),
            aggregate_type: ENTITLEMENT_AGGREGATE,
            snapshot_event: ENTITLEMENT_SNAPSHOT_EVENT,
            plane: StoragePlane::Holding,
        },
    }
}

fn aggregate_key(value: &CommercialAggregate) -> AggregateKey {
    match value {
        CommercialAggregate::CatalogPlan(plan) => AggregateKey {
            partition: catalog_plan_partition(plan.id()),
            aggregate_type: CATALOG_PLAN_AGGREGATE,
            snapshot_event: CATALOG_PLAN_SNAPSHOT_EVENT,
            plane: StoragePlane::Control,
        },
        CommercialAggregate::Subscription(value) => AggregateKey {
            partition: subscription_partition(value.id()),
            aggregate_type: SUBSCRIPTION_AGGREGATE,
            snapshot_event: SUBSCRIPTION_SNAPSHOT_EVENT,
            plane: StoragePlane::Holding,
        },
        CommercialAggregate::Entitlement(value) => AggregateKey {
            partition: entitlement_partition(value.id()),
            aggregate_type: ENTITLEMENT_AGGREGATE,
            snapshot_event: ENTITLEMENT_SNAPSHOT_EVENT,
            plane: StoragePlane::Holding,
        },
    }
}

fn validate_commit(request: &CommercialCommit) -> Result<(), PortError> {
    let fingerprint = fingerprint_key(&request.identity.fingerprint);
    let aggregate = aggregate_key(&request.aggregate);
    if fingerprint.partition != aggregate.partition
        || fingerprint.aggregate_type != aggregate.aggregate_type
        || !request.authority_scope.contains(request.aggregate.scope())
    {
        return Err(PortError::Internal);
    }
    match &request.aggregate {
        CommercialAggregate::CatalogPlan(value)
            if value.scope() == &AuthorityScope::Platform && value.amount_minor() > 0 =>
        {
            Ok(())
        }
        CommercialAggregate::Subscription(value)
            if is_holding_scope(value.scope()) && value.amount_minor() > 0 =>
        {
            Ok(())
        }
        CommercialAggregate::Entitlement(value)
            if is_holding_scope(value.scope()) && valid_capacity(value) =>
        {
            Ok(())
        }
        _ => Err(PortError::Internal),
    }
}

fn snapshot(value: &CommercialAggregate) -> CommercialSnapshot {
    match value {
        CommercialAggregate::CatalogPlan(value) => {
            CommercialSnapshot::CatalogPlan(CatalogPlanView::from(value))
        }
        CommercialAggregate::Subscription(value) => {
            CommercialSnapshot::Subscription(SubscriptionView::from(value))
        }
        CommercialAggregate::Entitlement(value) => {
            CommercialSnapshot::Entitlement(EntitlementView::from(value))
        }
    }
}

fn aggregate_updated_at(value: &CommercialAggregate) -> chrono::DateTime<chrono::Utc> {
    match value {
        CommercialAggregate::CatalogPlan(value) => value.updated_at(),
        CommercialAggregate::Subscription(value) => value.updated_at(),
        CommercialAggregate::Entitlement(value) => value.updated_at(),
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<CommercialAggregate>,
) -> Result<CommercialStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != receipt.response.aggregate_type()
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id_uuid()
        || receipt.aggregate.version != receipt.response.version()
    {
        return Err(PortError::Internal);
    }
    Ok(CommercialStoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.commercial.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn aggregate_id(value: Uuid) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(value).map_err(|_| PortError::Internal)
}
fn is_holding_scope(scope: &AuthorityScope) -> bool {
    matches!(scope, AuthorityScope::Holding { holding_id } if !holding_id.is_nil())
}

fn authority_holding_id(scope: &AuthorityScope) -> Result<Uuid, PortError> {
    scope.validate().map_err(|_| PortError::Denied)?;
    match scope {
        AuthorityScope::Holding { holding_id } if !holding_id.is_nil() => Ok(*holding_id),
        _ => Err(PortError::Denied),
    }
}

fn valid_capacity(value: &Entitlement) -> bool {
    let cap = value.capacity();
    let used = value.allocated();
    cap.seats > 0
        && cap.companies > 0
        && cap.branches > 0
        && used.seats <= cap.seats
        && used.companies <= cap.companies
        && used.branches <= cap.branches
}

fn audit_action(value: &CommandFingerprint) -> &'static str {
    match value {
        CommandFingerprint::CreateCatalogPlan { .. } => "commercial.catalog_plan.create",
        CommandFingerprint::PublishCatalogPlan { .. } => "commercial.catalog_plan.publish",
        CommandFingerprint::CreateSubscription { .. } => "commercial.subscription.create",
        CommandFingerprint::ApplyBillingEvent { .. } => {
            "commercial.subscription.apply_billing_event"
        }
        CommandFingerprint::CreateEntitlement { .. } => "commercial.entitlement.create",
        CommandFingerprint::ApplyEntitlementChange { .. } => {
            "commercial.entitlement.apply_provider_change"
        }
    }
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::with_capacity(event_type.len() + 16);
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("derived UUID is non-nil")
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
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PortError> {
    from_i64_port(row.try_get(column).map_err(|_| PortError::Internal)?)
}

fn parse_subscription_status(value: &str) -> Result<SubscriptionStatus, PortError> {
    match value {
        "trial" => Ok(SubscriptionStatus::Trial),
        "active" => Ok(SubscriptionStatus::Active),
        "past_due" => Ok(SubscriptionStatus::PastDue),
        "suspended" => Ok(SubscriptionStatus::Suspended),
        "ended" => Ok(SubscriptionStatus::Ended),
        _ => Err(PortError::Internal),
    }
}

fn capacity_from_row(
    row: &sqlx::postgres::PgRow,
    seats: &str,
    companies: &str,
    branches: &str,
) -> Result<EntitlementCapacity, PortError> {
    Ok(EntitlementCapacity {
        seats: u32::try_from(
            row.try_get::<i64, _>(seats)
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        companies: u32::try_from(
            row.try_get::<i64, _>(companies)
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        branches: u32::try_from(
            row.try_get::<i64, _>(branches)
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
    })
}

fn require_one_row(rows: u64, message: &'static str) -> Result<(), CqrsSqlxError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(CqrsSqlxError::GuardRejected(message))
    }
}

async fn load_catalog_plan_relational(
    write: &WritePool,
    authority_scope: &AuthorityScope,
    id: CatalogPlanId,
) -> Result<Option<CatalogPlan>, PortError> {
    if !authority_scope.contains(&AuthorityScope::Platform) {
        return Err(PortError::Denied);
    }
    let mut transaction = write.begin().await.map_err(map_store_error)?;
    let row = sqlx::query(
        "SELECT aggregate_version,plan_key,display_name,amount_minor,currency,
                plan_status,created_at,updated_at
         FROM erpx_commercial_write.catalog_plan_guards
         WHERE catalog_plan_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let status = match row
        .try_get::<String, _>("plan_status")
        .map_err(|_| PortError::Internal)?
        .as_str()
    {
        "draft" => erpx_commercial::CatalogPlanStatus::Draft,
        "published" => erpx_commercial::CatalogPlanStatus::Published,
        _ => return Err(PortError::Internal),
    };
    let currency = CurrencyCode::try_new(
        &row.try_get::<String, _>("currency")
            .map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    CatalogPlan::try_rehydrate(
        id,
        from_i64_port(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        row.try_get("plan_key").map_err(|_| PortError::Internal)?,
        row.try_get("display_name")
            .map_err(|_| PortError::Internal)?,
        row.try_get("amount_minor")
            .map_err(|_| PortError::Internal)?,
        currency,
        status,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

fn from_i64_port(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
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
    use erpx_commercial::BillingSignal;
    use erpx_integration_contracts::CurrencyCode;

    use super::*;

    fn fingerprint(signal: BillingSignal) -> CommandFingerprint {
        CommandFingerprint::ApplyBillingEvent {
            subscription_id: SubscriptionId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            provider: "stripe".into(),
            provider_event_id: "evt-1".into(),
            occurred_at: chrono::DateTime::UNIX_EPOCH,
            signal,
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_payload_sensitive_and_secret_free() {
        let first = request_hash(&fingerprint(BillingSignal::TrialEnded)).unwrap();
        assert_eq!(
            first,
            request_hash(&fingerprint(BillingSignal::TrialEnded)).unwrap()
        );
        assert_ne!(
            first,
            request_hash(&fingerprint(BillingSignal::PaymentFailed)).unwrap()
        );
        let serialized = serde_json::to_string(&fingerprint(BillingSignal::TrialEnded)).unwrap();
        assert!(!serialized.contains("verification_token"));
    }

    #[test]
    fn aggregate_keys_are_namespaced() {
        let plan = CommandFingerprint::CreateCatalogPlan {
            plan_id: CatalogPlanId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            key: "starter".into(),
            display_name: "Starter".into(),
            amount_minor: 1,
            currency: CurrencyCode::try_new("THB").unwrap(),
        };
        assert_eq!(
            fingerprint_key(&plan).aggregate_type,
            CATALOG_PLAN_AGGREGATE
        );
        assert_eq!(fingerprint_key(&plan).plane, StoragePlane::Control);
        assert_eq!(
            fingerprint_key(&fingerprint(BillingSignal::TrialEnded)).plane,
            StoragePlane::Holding
        );
        assert_ne!(
            fingerprint_key(&plan).partition,
            fingerprint_key(&fingerprint(BillingSignal::TrialEnded)).partition
        );
    }

    #[test]
    fn guard_source_covers_all_frozen_guard_tables() {
        let source = include_str!("command.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for table in [
            "catalog_plan_guards",
            "subscription_authority",
            "entitlement_authority",
        ] {
            assert!(source.contains(table));
        }
        assert!(!source.contains("commit_guarded"));
        assert!(source.contains("control_store"));
        assert!(source.contains("holding_store"));
        assert!(source.contains("ensure_published_control_plan"));
    }

    #[test]
    fn every_plane_uses_normalized_authority_without_snapshot_fallback() {
        let production = include_str!("command.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("load_snapshot"));
        assert!(!production.contains("commit_guarded"));
        assert!(production.contains("load_catalog_plan_relational"));
        assert!(production.contains("commit_normalized"));
    }
}
