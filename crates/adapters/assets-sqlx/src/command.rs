use async_trait::async_trait;
use erpx_assets::{
    Asset, AssetCommit, AssetId, AssetLifecycle, AssetScope, AssetState, AssetStatus,
    AssetStoredCommit, CommandFingerprint, CommandIdentity, CommandUnitOfWork, PortError,
    WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, assets::AssetCondition, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, asset_partition,
    projection::{AssetOutboxPayload, AssetSnapshot, condition_name, scope_key, status_name},
};

#[derive(Debug, Clone)]
pub struct AssetsWriteRepository {
    pool: WritePool,
}

impl AssetsWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
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
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
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
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
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

#[async_trait]
impl WriteRepository for AssetsWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        asset_id: AssetId,
    ) -> Result<Option<Asset>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,holding_id,company_id,branch_id,asset_code,asset_name,
                    location_id,asset_status,asset_condition,placed_in_service_at,retired_at,
                    created_at,updated_at
             FROM erpx_assets_write.asset_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND asset_id=$1
               AND (
                 $2='platform'
                 OR ($2='holding' AND holding_id=$3)
                 OR ($2='company' AND holding_id=$3 AND company_id=$4)
                 OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
                 OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
                     AND ((branch_id IS NULL AND cardinality($7::uuid[])=0)
                          OR branch_id=ANY($7)))
               )",
        )
        .bind(asset_id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let holding_id = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
        let company_id = row.try_get("company_id").map_err(|_| PortError::Internal)?;
        let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
        let scope = AssetScope::try_new(branch_id.map_or(
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            |branch_id| AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
        ))
        .map_err(|_| PortError::Internal)?;
        let state = AssetState {
            id: asset_id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            code: row.try_get("asset_code").map_err(|_| PortError::Internal)?,
            name: row.try_get("asset_name").map_err(|_| PortError::Internal)?,
            scope,
            location_id: row
                .try_get("location_id")
                .map_err(|_| PortError::Internal)?,
            status: parse_status(
                &row.try_get::<String, _>("asset_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            condition: parse_condition(
                &row.try_get::<String, _>("asset_condition")
                    .map_err(|_| PortError::Internal)?,
            )?,
            lifecycle: AssetLifecycle {
                placed_in_service_at: row
                    .try_get("placed_in_service_at")
                    .map_err(|_| PortError::Internal)?,
                retired_at: row.try_get("retired_at").map_err(|_| PortError::Internal)?,
            },
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        };
        let aggregate = Asset::try_rehydrate(state).map_err(|_| PortError::Internal)?;
        if !authority_scope.contains(aggregate.scope().active_scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(aggregate))
    }
}

#[derive(Debug, Clone)]
pub struct AssetsCommandAdapter {
    store: CommandStore,
}

impl AssetsCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for AssetsCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<AssetStoredCommit>, PortError> {
        let asset_id = fingerprint_asset_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<Asset>(
                CONTEXT_NAME,
                &asset_partition(asset_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: AssetCommit) -> Result<AssetStoredCommit, PortError> {
        validate_commit(&request)?;
        let asset_id = request.aggregate.id();
        let partition = asset_partition(asset_id);
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
                payload: AssetOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: AssetOutboxPayload::Snapshot(AssetSnapshot {
                asset: (&request.aggregate).into(),
            }),
        });
        let directory_draft = asset_directory_draft(&request.aggregate)?;
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
            payload: AssetOutboxPayload::Directory(directory_draft),
        });
        let guard = AssetAtomicGuard {
            aggregate: request.aggregate.clone(),
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            active_scope: aggregate.scope().active_scope().clone(),
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(asset_id)?,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"assets-audit-v1"),
                action: audit_action(&request.identity.fingerprint).into(),
                occurred_at,
                details: json!({
                    "asset_id": asset_id.as_uuid(),
                    "expected_version": request.expected_version,
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

fn asset_directory_draft(aggregate: &Asset) -> Result<ItemDescriptorDraftV2, PortError> {
    let record = aggregate.record_ref().map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().active_scope().clone(),
        lifecycle_state: status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: format!("{} {}", aggregate.code(), aggregate.name()),
    };
    ItemDescriptorDraftV2::try_new(value, format!("{} {}", aggregate.code(), aggregate.name()))
        .map_err(|_| PortError::Internal)
}

struct AssetAtomicGuard {
    aggregate: Asset,
}

#[async_trait]
impl AtomicCommandGuard for AssetAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_assets_write.asset_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND asset_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        current.map(from_i64_store).transpose()
    }

    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_assets_write.asset_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND asset_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected("asset already exists"));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected("asset guard is missing"));
            }
            _ => {}
        }
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT asset_id FROM erpx_assets_write.asset_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND scope_key=$1 AND asset_code=$2 AND asset_id<>$3
             ORDER BY asset_id LIMIT 1 FOR UPDATE",
        )
        .bind(guard_scope_key(self.aggregate.scope().active_scope())?)
        .bind(self.aggregate.code())
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "asset code already exists in scope",
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
            || context.partition != asset_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "assets applied guard context mismatch",
            ));
        }
        let result = sqlx::query(
            "INSERT INTO erpx_assets_write.asset_authority
                (holding_id,asset_id,aggregate_version,scope_key,holding_id,company_id,branch_id,
                 asset_code,asset_name,location_id,asset_status,asset_condition,
                 placed_in_service_at,retired_at,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,
                     $11,$12,$13,$14,$15)
             ON CONFLICT (holding_id,asset_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,
                scope_key=EXCLUDED.scope_key,asset_code=EXCLUDED.asset_code,
                holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,
                branch_id=EXCLUDED.branch_id,asset_name=EXCLUDED.asset_name,
                location_id=EXCLUDED.location_id,asset_status=EXCLUDED.asset_status,
                asset_condition=EXCLUDED.asset_condition,
                placed_in_service_at=EXCLUDED.placed_in_service_at,
                retired_at=EXCLUDED.retired_at,created_at=EXCLUDED.created_at,
                updated_at=EXCLUDED.updated_at
             WHERE erpx_assets_write.asset_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_assets_write.asset_authority.aggregate_version + 1=
                       EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(to_i64(self.aggregate.version())?)
        .bind(guard_scope_key(self.aggregate.scope().active_scope())?)
        .bind(scope_holding_id(self.aggregate.scope().active_scope())?)
        .bind(scope_company_id(self.aggregate.scope().active_scope())?)
        .bind(scope_branch_id(self.aggregate.scope().active_scope())?)
        .bind(self.aggregate.code())
        .bind(self.aggregate.name())
        .bind(self.aggregate.location_id())
        .bind(status_name(self.aggregate.status()))
        .bind(condition_name(self.aggregate.condition()))
        .bind(self.aggregate.lifecycle().placed_in_service_at)
        .bind(self.aggregate.lifecycle().retired_at)
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "asset normalized version did not advance exactly once",
            ));
        }
        Ok(())
    }
}

fn validate_guard_context(
    aggregate: &Asset,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != asset_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "assets command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &AssetCommit) -> Result<(), PortError> {
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_asset_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request
            .authority_scope
            .contains(request.aggregate.scope().active_scope())
        || request.aggregate.version() != next
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn guard_scope_key(scope: &AuthorityScope) -> Result<String, CqrsSqlxError> {
    scope_key(scope).map_err(|_| CqrsSqlxError::GuardRejected("asset scope is invalid"))
}

fn aggregate_id(asset_id: AssetId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(asset_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_asset_id(fingerprint: &CommandFingerprint) -> AssetId {
    match fingerprint {
        CommandFingerprint::Create { asset_id, .. }
        | CommandFingerprint::PlaceInService { asset_id }
        | CommandFingerprint::MarkDegraded { asset_id }
        | CommandFingerprint::BeginMaintenance { asset_id, .. }
        | CommandFingerprint::ReturnToService { asset_id }
        | CommandFingerprint::Retire { asset_id } => *asset_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.assets.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "assets.asset.create",
        CommandFingerprint::PlaceInService { .. } => "assets.asset.place_in_service",
        CommandFingerprint::MarkDegraded { .. } => "assets.asset.mark_degraded",
        CommandFingerprint::BeginMaintenance { .. } => "assets.asset.begin_maintenance",
        CommandFingerprint::ReturnToService { .. } => "assets.asset.return_to_service",
        CommandFingerprint::Retire { .. } => "assets.asset.retire",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<Asset>,
) -> Result<AssetStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(AssetStoredCommit {
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
    i64::try_from(value).map_err(|_| CqrsSqlxError::GuardRejected("asset version is too large"))
}

fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn scope_holding_id(scope: &AuthorityScope) -> Result<Uuid, CqrsSqlxError> {
    match scope {
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. } => Ok(*holding_id),
        AuthorityScope::Platform | AuthorityScope::Subtree { .. } => Err(
            CqrsSqlxError::GuardRejected("asset authority requires company or branch scope"),
        ),
    }
}

fn scope_company_id(scope: &AuthorityScope) -> Result<Uuid, CqrsSqlxError> {
    match scope {
        AuthorityScope::Company { company_id, .. } | AuthorityScope::Branch { company_id, .. } => {
            Ok(*company_id)
        }
        AuthorityScope::Platform
        | AuthorityScope::Holding { .. }
        | AuthorityScope::Subtree { .. } => Err(CqrsSqlxError::GuardRejected(
            "asset authority requires company or branch scope",
        )),
    }
}

fn scope_branch_id(scope: &AuthorityScope) -> Result<Option<Uuid>, CqrsSqlxError> {
    match scope {
        AuthorityScope::Branch { branch_id, .. } => Ok(Some(*branch_id)),
        AuthorityScope::Company { .. } => Ok(None),
        AuthorityScope::Platform
        | AuthorityScope::Holding { .. }
        | AuthorityScope::Subtree { .. } => Err(CqrsSqlxError::GuardRejected(
            "asset authority requires company or branch scope",
        )),
    }
}

fn parse_status(value: &str) -> Result<AssetStatus, PortError> {
    match value {
        "draft" => Ok(AssetStatus::Draft),
        "in_service" => Ok(AssetStatus::InService),
        "under_maintenance" => Ok(AssetStatus::UnderMaintenance),
        "retired" => Ok(AssetStatus::Retired),
        _ => Err(PortError::Internal),
    }
}

fn parse_condition(value: &str) -> Result<AssetCondition, PortError> {
    match value {
        "operational" => Ok(AssetCondition::Operational),
        "degraded" => Ok(AssetCondition::Degraded),
        "out_of_service" => Ok(AssetCondition::OutOfService),
        "retired" => Ok(AssetCondition::Retired),
        _ => Err(PortError::Internal),
    }
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
        CqrsSqlxError::Database(database) => match database
            .as_database_error()
            .and_then(|error| error.constraint().map(str::to_owned))
        {
            Some(name) if name == "asset_guards_scope_key_asset_code_key" => {
                PortError::IdempotencyConflict
            }
            _ => PortError::Unavailable,
        },
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_assets::{AssetScope, AssetStatus};
    use erpx_integration_contracts::assets;

    fn fingerprint(code: &str) -> CommandFingerprint {
        CommandFingerprint::Create {
            asset_id: AssetId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            code: code.into(),
            name: "Packing line".into(),
            scope: AssetScope::try_new(AuthorityScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            })
            .unwrap(),
            location_id: Uuid::from_u128(4),
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        assert_eq!(
            request_hash(&fingerprint("AST-1")).unwrap(),
            request_hash(&fingerprint("AST-1")).unwrap()
        );
        assert_ne!(
            request_hash(&fingerprint("AST-1")).unwrap(),
            request_hash(&fingerprint("AST-2")).unwrap()
        );
    }

    #[test]
    fn status_and_condition_names_match_frozen_ddl() {
        assert_eq!(
            status_name(AssetStatus::UnderMaintenance),
            "under_maintenance"
        );
        assert_eq!(
            condition_name(assets::AssetCondition::OutOfService),
            "out_of_service"
        );
    }
}
