use async_trait::async_trait;
use erpx_brand::{
    ArchivedBrandRevision, BrandCommit, BrandDefinition, BrandProfile, BrandProfileId, BrandScope,
    BrandStoredCommit, CommandFingerprint, CommandIdentity, CommandUnitOfWork, ParentAuthorityPort,
    ParentBrandWitness, PortError, WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::IntegrationEvent;
use erpx_kernel::{AggregateId, CommandId, EventId, TenantScope};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, profile_partition,
    projection::{BrandOutboxPayload, BrandSnapshot},
};

#[derive(Debug, Clone)]
pub struct BrandWriteRepository {
    store: CommandStore,
    revision_pool: PgPool,
}

impl BrandWriteRepository {
    /// `revision_pool` points at the same control-write database but is used
    /// only for authoritative revision reads not exposed by CommandStore.
    pub fn new(write: WritePool, revision_pool: PgPool) -> Self {
        Self {
            store: CommandStore::new(write),
            revision_pool,
        }
    }
}

#[async_trait]
impl WriteRepository for BrandWriteRepository {
    async fn load(
        &self,
        authority_scope: &TenantScope,
        profile_id: BrandProfileId,
    ) -> Result<Option<BrandProfile>, PortError> {
        let snapshot = self
            .store
            .load_snapshot::<BrandProfile>(
                CONTEXT_NAME,
                &profile_partition(profile_id),
                AGGREGATE_TYPE,
                aggregate_id(profile_id)?,
            )
            .await
            .map_err(map_store_error)?;
        let Some(snapshot) = snapshot else {
            return Ok(None);
        };
        if snapshot.version != snapshot.state.version()
            || snapshot.state.id() != profile_id
            || snapshot.state.validate().is_err()
        {
            return Err(PortError::Internal);
        }
        let target = snapshot
            .state
            .scope()
            .authority_scope()
            .map_err(|_| PortError::Internal)?;
        if !authority_scope.contains(&target) {
            return Err(PortError::Denied);
        }
        Ok(Some(snapshot.state))
    }

    async fn load_archived_revision(
        &self,
        authority_scope: &TenantScope,
        profile_id: BrandProfileId,
        revision: u64,
    ) -> Result<Option<ArchivedBrandRevision>, PortError> {
        let Some(profile) = self.load(authority_scope, profile_id).await? else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT definition
             FROM erpx_brand_write.profile_revisions
             WHERE profile_id = $1 AND revision = $2",
        )
        .bind(profile_id.as_uuid())
        .bind(to_i64_port(revision)?)
        .fetch_optional(&self.revision_pool)
        .await
        .map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            let definition: serde_json::Value =
                row.try_get("definition").map_err(|_| PortError::Internal)?;
            Ok(ArchivedBrandRevision {
                profile_id,
                scope: profile.scope().clone(),
                source_revision: revision,
                definition: serde_json::from_value(definition).map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()
    }
}

#[derive(Debug, Clone)]
pub struct BrandParentAuthorityRepository {
    pool: PgPool,
}

impl BrandParentAuthorityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ParentAuthorityPort for BrandParentAuthorityRepository {
    async fn current_parent(
        &self,
        child: &BrandScope,
    ) -> Result<Option<ParentBrandWitness>, PortError> {
        let Some(parent_scope) = parent_scope(child)? else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT guard.published_revision, revision.definition
             FROM erpx_brand_write.profile_guards guard
             JOIN erpx_brand_write.profile_revisions revision
               ON revision.profile_id = guard.profile_id
              AND revision.revision = guard.published_revision
             WHERE guard.scope_key = $1 AND revision.archived_at IS NULL",
        )
        .bind(parent_scope.canonical_key())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let revision = from_i64(
            row.try_get("published_revision")
                .map_err(|_| PortError::Internal)?,
        )?;
        let definition: BrandDefinition =
            serde_json::from_value(row.try_get("definition").map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?;
        Ok(Some(ParentBrandWitness {
            scope: parent_scope,
            published_revision: revision,
            locked_categories: definition.locked_categories,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct BrandCommandAdapter {
    store: CommandStore,
}

impl BrandCommandAdapter {
    pub fn new(write: WritePool) -> Self {
        Self {
            store: CommandStore::new(write),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for BrandCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<BrandStoredCommit>, PortError> {
        let profile_id = fingerprint_profile_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<BrandProfile>(
                CONTEXT_NAME,
                &profile_partition(profile_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: BrandCommit) -> Result<BrandStoredCommit, PortError> {
        validate_commit(&request)?;
        let profile_id = request.aggregate.id();
        let partition = profile_partition(profile_id);
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let action = audit_action(&request.identity.fingerprint);
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
                payload: BrandOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: BrandOutboxPayload::Snapshot(BrandSnapshot {
                profile: (&request.aggregate).into(),
                reason: request.reason.clone(),
                action: action.into(),
            }),
        });
        let target_scope = request
            .aggregate
            .scope()
            .authority_scope()
            .map_err(|_| PortError::Internal)?;
        let guard = BrandAtomicGuard {
            aggregate: request.aggregate.clone(),
            ensure_scope_vacant: request.ensure_scope_vacant,
            parent_guard: request.parent_guard.clone(),
            reason: request.reason.clone(),
        };
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key: request.identity.idempotency_key,
            request_hash: request_hash(&request.identity.fingerprint)?,
            tenant_scope: target_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(profile_id)?,
            expected_version: request.expected_version,
            aggregate_state: serde_json::to_value(&aggregate).map_err(|_| PortError::Internal)?,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"brand-audit-v1"),
                action: action.into(),
                occurred_at,
                details: json!({
                    "profile_id": profile_id.as_uuid(),
                    "scope_key": guard.aggregate.scope().canonical_key(),
                    "reason": &guard.reason,
                    "expected_version": request.expected_version,
                    "parent_published_revision": guard.parent_guard.as_ref().map(|value| value.published_revision),
                }),
            },
            events,
        };
        let outcome = self
            .store
            .commit_guarded(command, &guard)
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

struct BrandAtomicGuard {
    aggregate: BrandProfile,
    ensure_scope_vacant: bool,
    parent_guard: Option<ParentBrandWitness>,
    reason: String,
}

#[async_trait]
impl AtomicCommandGuard for BrandAtomicGuard {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current = sqlx::query(
            "SELECT profile_id, scope_key, aggregate_version
             FROM erpx_brand_write.profile_guards
             WHERE profile_id = $1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        match (context.expected_version, current.as_ref()) {
            (None, Some(_)) => {
                return Err(CqrsSqlxError::GuardRejected("brand profile already exists"));
            }
            (Some(_), None) => {
                return Err(CqrsSqlxError::GuardRejected(
                    "brand profile guard is missing",
                ));
            }
            _ => {}
        }
        if self.ensure_scope_vacant != context.expected_version.is_none() {
            return Err(CqrsSqlxError::GuardRejected(
                "brand scope vacancy guard mismatch",
            ));
        }
        let scope_key = self.aggregate.scope().canonical_key();
        let duplicate: Option<Uuid> = sqlx::query_scalar(
            "SELECT profile_id FROM erpx_brand_write.profile_guards
             WHERE scope_key = $1 AND profile_id <> $2 FOR UPDATE",
        )
        .bind(&scope_key)
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        if duplicate.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "brand scope already has a profile",
            ));
        }
        validate_parent_guard(
            transaction,
            self.aggregate.scope(),
            self.parent_guard.as_ref(),
        )
        .await
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != profile_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "brand applied guard context mismatch",
            ));
        }
        let draft_revision = self.aggregate.draft().map(|value| value.revision());
        let published_revision = self.aggregate.published().map(|value| value.revision());
        let parent_published_revision = self
            .aggregate
            .draft()
            .and_then(|value| value.parent_published_revision())
            .or_else(|| {
                self.aggregate
                    .published()
                    .and_then(|value| value.parent_published_revision())
            });
        sqlx::query(
            "INSERT INTO erpx_brand_write.profile_guards
                (profile_id, scope_key, draft_revision, published_revision,
                 parent_published_revision, aggregate_version, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (profile_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,
                draft_revision=EXCLUDED.draft_revision,
                published_revision=EXCLUDED.published_revision,
                parent_published_revision=EXCLUDED.parent_published_revision,
                aggregate_version=EXCLUDED.aggregate_version,
                updated_at=EXCLUDED.updated_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(self.aggregate.scope().canonical_key())
        .bind(draft_revision.map(to_i64).transpose()?)
        .bind(published_revision.map(to_i64).transpose()?)
        .bind(parent_published_revision.map(to_i64).transpose()?)
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;

        sqlx::query(
            "UPDATE erpx_brand_write.profile_revisions
             SET archived_at = $2
             WHERE profile_id = $1 AND archived_at IS NULL
               AND ($3::bigint IS NULL OR revision <> $3)
               AND ($4::bigint IS NULL OR revision <> $4)",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(self.aggregate.updated_at())
        .bind(draft_revision.map(to_i64).transpose()?)
        .bind(published_revision.map(to_i64).transpose()?)
        .execute(&mut **transaction)
        .await?;

        if let Some(draft) = self.aggregate.draft() {
            upsert_revision(transaction, self.aggregate.id(), draft, &self.reason).await?;
        }
        if let Some(published) = self.aggregate.published() {
            upsert_revision(transaction, self.aggregate.id(), published, &self.reason).await?;
        }
        Ok(())
    }
}

async fn validate_parent_guard(
    transaction: &mut Transaction<'_, Postgres>,
    child: &BrandScope,
    witness: Option<&ParentBrandWitness>,
) -> Result<(), CqrsSqlxError> {
    let expected_parent = parent_scope(child)
        .map_err(|_| CqrsSqlxError::GuardRejected("brand parent scope is invalid"))?;
    match (expected_parent, witness) {
        (None, None) => return Ok(()),
        (None, Some(_)) | (Some(_), None) => {
            return Err(CqrsSqlxError::GuardRejected(
                "brand parent witness is missing or unexpected",
            ));
        }
        (Some(expected), Some(witness)) if expected != witness.scope => {
            return Err(CqrsSqlxError::GuardRejected(
                "brand parent witness scope mismatch",
            ));
        }
        (Some(_), Some(_)) => {}
    }
    let witness = witness.expect("matched parent witness");
    let row = sqlx::query(
        "SELECT guard.published_revision, revision.definition
         FROM erpx_brand_write.profile_guards guard
         JOIN erpx_brand_write.profile_revisions revision
           ON revision.profile_id = guard.profile_id
          AND revision.revision = guard.published_revision
         WHERE guard.scope_key = $1 FOR UPDATE OF guard, revision",
    )
    .bind(witness.scope.canonical_key())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Err(CqrsSqlxError::GuardRejected(
            "brand published parent is missing",
        ));
    };
    let revision = from_i64_store(row.try_get("published_revision")?)?;
    let definition: BrandDefinition = serde_json::from_value(row.try_get("definition")?)?;
    if revision != witness.published_revision
        || definition.locked_categories != witness.locked_categories
    {
        return Err(CqrsSqlxError::GuardRejected("brand parent witness changed"));
    }
    Ok(())
}

async fn upsert_revision(
    transaction: &mut Transaction<'_, Postgres>,
    profile_id: BrandProfileId,
    revision: &erpx_brand::BrandRevision,
    reason: &str,
) -> Result<(), CqrsSqlxError> {
    let published_at = (revision.lifecycle() == erpx_brand::BrandLifecycle::Published)
        .then_some(revision.updated_at());
    sqlx::query(
        "INSERT INTO erpx_brand_write.profile_revisions
            (profile_id,revision,lifecycle,definition,parent_published_revision,
             updated_by,reason,created_at,published_at,archived_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NULL)
         ON CONFLICT (profile_id,revision) DO UPDATE SET
            lifecycle=EXCLUDED.lifecycle,
            definition=EXCLUDED.definition,
            parent_published_revision=EXCLUDED.parent_published_revision,
            updated_by=EXCLUDED.updated_by,
            reason=EXCLUDED.reason,
            published_at=COALESCE(erpx_brand_write.profile_revisions.published_at,
                                  EXCLUDED.published_at),
            archived_at=NULL",
    )
    .bind(profile_id.as_uuid())
    .bind(to_i64(revision.revision())?)
    .bind(revision.lifecycle().as_str())
    .bind(serde_json::to_value(revision.definition())?)
    .bind(
        revision
            .parent_published_revision()
            .map(to_i64)
            .transpose()?,
    )
    .bind(revision.updated_by())
    .bind(reason)
    .bind(revision.updated_at())
    .bind(published_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_guard_context(
    aggregate: &BrandProfile,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != profile_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "brand command guard context mismatch",
        ));
    }
    Ok(())
}

fn validate_commit(request: &BrandCommit) -> Result<(), PortError> {
    let target = request
        .aggregate
        .scope()
        .authority_scope()
        .map_err(|_| PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_profile_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(&target)
        || request.reason.trim().is_empty()
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn parent_scope(child: &BrandScope) -> Result<Option<BrandScope>, PortError> {
    child.validate().map_err(|_| PortError::Internal)?;
    Ok(match child {
        BrandScope::Platform => None,
        BrandScope::Holding { .. } => Some(BrandScope::Platform),
        BrandScope::BusinessUnit {
            tenant_id,
            holding_id,
            ..
        } => Some(BrandScope::Holding {
            tenant_id: tenant_id.clone(),
            holding_id: *holding_id,
        }),
    })
}

fn fingerprint_profile_id(fingerprint: &CommandFingerprint) -> BrandProfileId {
    match fingerprint {
        CommandFingerprint::CreateDraft { profile_id, .. }
        | CommandFingerprint::SaveDraft { profile_id, .. }
        | CommandFingerprint::Publish { profile_id, .. }
        | CommandFingerprint::Rollback { profile_id, .. }
        | CommandFingerprint::SetCategoryLocks { profile_id, .. } => *profile_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.brand.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::CreateDraft { .. } => "brand.draft.create",
        CommandFingerprint::SaveDraft { .. } => "brand.draft.save",
        CommandFingerprint::Publish { .. } => "brand.draft.publish",
        CommandFingerprint::Rollback { .. } => "brand.rollback",
        CommandFingerprint::SetCategoryLocks { .. } => "brand.category_locks.set",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<BrandProfile>,
) -> Result<BrandStoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(BrandStoredCommit {
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
        CqrsSqlxError::GuardRejected("brand scope already has a profile")
        | CqrsSqlxError::GuardRejected("brand profile already exists") => PortError::ScopeConflict,
        CqrsSqlxError::GuardRejected(message) if message.contains("parent") => {
            PortError::ParentConflict
        }
        CqrsSqlxError::Database(error)
            if error
                .as_database_error()
                .and_then(|error| error.constraint())
                == Some("profile_guards_scope_key_key") =>
        {
            PortError::ScopeConflict
        }
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

fn aggregate_id(profile_id: BrandProfileId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(profile_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut label = Vec::with_capacity(event_type.len() + 16);
    label.extend_from_slice(event_type.as_bytes());
    label.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &label))
        .expect("a SHA-256-derived UUID is non-nil")
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

fn to_i64_port(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_fingerprint_has_a_stable_profile_owner() {
        let id = BrandProfileId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let fingerprint = CommandFingerprint::Publish {
            profile_id: id,
            reason: "publish".into(),
        };
        assert_eq!(fingerprint_profile_id(&fingerprint), id);
        assert_eq!(audit_action(&fingerprint), "brand.draft.publish");
        assert_eq!(request_hash(&fingerprint), request_hash(&fingerprint));
    }

    #[test]
    fn parent_scope_is_exact_for_each_brand_layer() {
        assert_eq!(parent_scope(&BrandScope::Platform).unwrap(), None);
        let holding = BrandScope::Holding {
            tenant_id: "tenant-acme".into(),
            holding_id: Uuid::from_u128(2),
        };
        assert_eq!(parent_scope(&holding).unwrap(), Some(BrandScope::Platform));
    }
}
