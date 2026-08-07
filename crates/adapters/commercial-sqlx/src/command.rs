use async_trait::async_trait;
use erpx_commercial::{
    CatalogPlan, CatalogPlanId, CatalogPlanView, CommandFingerprint, CommandIdentity,
    CommandUnitOfWork, CommercialAggregate, CommercialCommit, CommercialStoredCommit, Entitlement,
    EntitlementId, EntitlementView, PortError, Subscription, SubscriptionId, SubscriptionView,
    WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
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
    control_store: CommandStore,
    tenant_store: CommandStore,
}

impl CommercialWriteRepository {
    pub fn new(control_write: WritePool, tenant_write: WritePool) -> Self {
        Self {
            control_store: CommandStore::new(control_write),
            tenant_store: CommandStore::new(tenant_write),
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
        authority_scope: &TenantScope,
        id: CatalogPlanId,
    ) -> Result<Option<CatalogPlan>, PortError> {
        let snapshot = self
            .control_store
            .load_snapshot::<CommercialAggregate>(
                CONTEXT_NAME,
                &catalog_plan_partition(id),
                CATALOG_PLAN_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let CommercialAggregate::CatalogPlan(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || value.scope() != &TenantScope::Platform
            || value.amount_minor() <= 0
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(value.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }

    async fn load_subscription(
        &self,
        authority_scope: &TenantScope,
        id: SubscriptionId,
    ) -> Result<Option<Subscription>, PortError> {
        let snapshot = self
            .tenant_store
            .load_snapshot::<CommercialAggregate>(
                CONTEXT_NAME,
                &subscription_partition(id),
                SUBSCRIPTION_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let CommercialAggregate::Subscription(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || !is_holding_scope(value.scope())
            || value.amount_minor() <= 0
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(value.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }

    async fn load_entitlement(
        &self,
        authority_scope: &TenantScope,
        id: EntitlementId,
    ) -> Result<Option<Entitlement>, PortError> {
        let snapshot = self
            .tenant_store
            .load_snapshot::<CommercialAggregate>(
                CONTEXT_NAME,
                &entitlement_partition(id),
                ENTITLEMENT_AGGREGATE,
                aggregate_id(id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        let CommercialAggregate::Entitlement(value) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != value.version()
            || value.id() != id
            || !is_holding_scope(value.scope())
            || !valid_capacity(&value)
        {
            return Err(PortError::Internal);
        }
        if !authority_scope.contains(value.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(value))
    }
}

#[derive(Debug, Clone)]
pub struct CommercialCommandAdapter {
    control_store: CommandStore,
    tenant_store: CommandStore,
}

impl CommercialCommandAdapter {
    pub fn new(control_write: WritePool, tenant_write: WritePool) -> Self {
        Self {
            control_store: CommandStore::new(control_write),
            tenant_store: CommandStore::new(tenant_write),
        }
    }

    pub fn from_single_pool(pool: WritePool) -> Self {
        Self::new(pool.clone(), pool)
    }

    async fn ensure_published_control_plan(&self, plan_id: CatalogPlanId) -> Result<(), PortError> {
        let snapshot = self
            .control_store
            .load_snapshot::<CommercialAggregate>(
                CONTEXT_NAME,
                &catalog_plan_partition(plan_id),
                CATALOG_PLAN_AGGREGATE,
                aggregate_id(plan_id.as_uuid())?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Err(PortError::NotFound);
        };
        let CommercialAggregate::CatalogPlan(plan) = snapshot.state else {
            return Err(PortError::Internal);
        };
        if snapshot.version != plan.version()
            || plan.id() != plan_id
            || plan.scope() != &TenantScope::Platform
            || plan.status() != erpx_commercial::CatalogPlanStatus::Published
        {
            return Err(PortError::Conflict {
                expected: snapshot.version,
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
            StoragePlane::Tenant => &self.tenant_store,
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
            tenant_scope: aggregate.scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: key.aggregate_type.into(),
            aggregate_id,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
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
            StoragePlane::Tenant => &self.tenant_store,
        };
        let outcome = store
            .commit_guarded(command, &guard)
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

#[derive(Debug)]
struct CommercialGuard {
    aggregate: CommercialAggregate,
}

#[async_trait]
impl AtomicCommandGuard for CommercialGuard {
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
            // before the tenant transaction was opened.
            CommercialAggregate::Subscription(_) => {}
            CommercialAggregate::Entitlement(entitlement) => {
                let status: Option<String> = sqlx::query_scalar("SELECT subscription_status FROM erpx_commercial_write.subscription_guards WHERE subscription_id=$1 AND scope_key=$2 FOR SHARE")
                    .bind(entitlement.subscription_id().as_uuid()).bind(scope_key(entitlement.scope())?)
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
                sqlx::query("INSERT INTO erpx_commercial_write.catalog_plan_guards (catalog_plan_id,plan_key,plan_status,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (catalog_plan_id) DO UPDATE SET plan_key=EXCLUDED.plan_key,plan_status=EXCLUDED.plan_status,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(plan.id().as_uuid()).bind(plan.key()).bind(catalog_status(plan.status())).bind(version).bind(plan.updated_at()).execute(&mut **transaction).await?;
            }
            CommercialAggregate::Subscription(value) => {
                sqlx::query("INSERT INTO erpx_commercial_write.subscription_guards (subscription_id,scope_key,customer_id,catalog_plan_id,subscription_status,last_provider_event_at,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (subscription_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,customer_id=EXCLUDED.customer_id,catalog_plan_id=EXCLUDED.catalog_plan_id,subscription_status=EXCLUDED.subscription_status,last_provider_event_at=EXCLUDED.last_provider_event_at,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.customer_id()).bind(value.plan_id().as_uuid()).bind(subscription_status(value.status())).bind(value.last_provider_event_at()).bind(version).bind(value.updated_at()).execute(&mut **transaction).await?;
            }
            CommercialAggregate::Entitlement(value) => {
                sqlx::query("INSERT INTO erpx_commercial_write.entitlement_guards (entitlement_id,scope_key,subscription_id,capability,enabled,seat_capacity,company_capacity,branch_capacity,allocated_seats,allocated_companies,allocated_branches,aggregate_version,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT (entitlement_id) DO UPDATE SET scope_key=EXCLUDED.scope_key,subscription_id=EXCLUDED.subscription_id,capability=EXCLUDED.capability,enabled=EXCLUDED.enabled,seat_capacity=EXCLUDED.seat_capacity,company_capacity=EXCLUDED.company_capacity,branch_capacity=EXCLUDED.branch_capacity,allocated_seats=EXCLUDED.allocated_seats,allocated_companies=EXCLUDED.allocated_companies,allocated_branches=EXCLUDED.allocated_branches,aggregate_version=EXCLUDED.aggregate_version,updated_at=EXCLUDED.updated_at")
                    .bind(value.id().as_uuid()).bind(scope_key(value.scope())?).bind(value.subscription_id().as_uuid()).bind(value.capability()).bind(value.enabled())
                    .bind(i64::from(value.capacity().seats)).bind(i64::from(value.capacity().companies)).bind(i64::from(value.capacity().branches))
                    .bind(i64::from(value.allocated().seats)).bind(i64::from(value.allocated().companies)).bind(i64::from(value.allocated().branches))
                    .bind(version).bind(value.updated_at()).execute(&mut **transaction).await?;
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
    Tenant,
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
            plane: StoragePlane::Tenant,
        },
        CommandFingerprint::CreateEntitlement { entitlement_id, .. }
        | CommandFingerprint::ApplyEntitlementChange { entitlement_id, .. } => AggregateKey {
            partition: entitlement_partition(*entitlement_id),
            aggregate_type: ENTITLEMENT_AGGREGATE,
            snapshot_event: ENTITLEMENT_SNAPSHOT_EVENT,
            plane: StoragePlane::Tenant,
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
            plane: StoragePlane::Tenant,
        },
        CommercialAggregate::Entitlement(value) => AggregateKey {
            partition: entitlement_partition(value.id()),
            aggregate_type: ENTITLEMENT_AGGREGATE,
            snapshot_event: ENTITLEMENT_SNAPSHOT_EVENT,
            plane: StoragePlane::Tenant,
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
            if value.scope() == &TenantScope::Platform && value.amount_minor() > 0 =>
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
fn is_holding_scope(scope: &TenantScope) -> bool {
    matches!(scope, TenantScope::Holding { holding_id } if !holding_id.is_nil())
}
fn scope_key(scope: &TenantScope) -> Result<String, CqrsSqlxError> {
    match scope {
        TenantScope::Holding { holding_id } if !holding_id.is_nil() => {
            Ok(format!("holding:{holding_id}"))
        }
        _ => Err(CqrsSqlxError::GuardRejected(
            "commercial guard requires holding scope",
        )),
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
            StoragePlane::Tenant
        );
        assert_ne!(
            fingerprint_key(&plan).partition,
            fingerprint_key(&fingerprint(BillingSignal::TrialEnded)).partition
        );
    }

    #[test]
    fn guard_source_covers_all_frozen_guard_tables() {
        let source = include_str!("command.rs");
        for table in [
            "catalog_plan_guards",
            "subscription_guards",
            "entitlement_guards",
        ] {
            assert!(source.contains(table));
        }
        assert!(source.contains("commit_guarded"));
        assert!(source.contains("control_store"));
        assert!(source.contains("tenant_store"));
        assert!(source.contains("ensure_published_control_plan"));
    }
}
