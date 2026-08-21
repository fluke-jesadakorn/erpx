use async_trait::async_trait;
use erpx_brand::{
    ArchivedBrandRevision, BrandAssetKind, BrandAssetManifest, BrandAssetReference, BrandCategory,
    BrandCommit, BrandDefinition, BrandDensity, BrandElevation, BrandFontPack, BrandLifecycle,
    BrandMotion, BrandNavigation, BrandPalette, BrandProfile, BrandProfileId, BrandRadius,
    BrandScope, BrandStoredCommit, BrandTokenSet, BusinessUnitKey, CommandFingerprint,
    CommandIdentity, CommandUnitOfWork, HexColor, ParentAuthorityPort, ParentBrandWitness,
    PortError, WriteRepository,
};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, RecordTypeId, StableRecordRef, catalog::ContextId,
    record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId, RecordId};
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
    revision_pool: PgPool,
}

impl BrandWriteRepository {
    /// `revision_pool` points at the same control-write database but is used
    /// only for authoritative revision reads not exposed by CommandStore.
    pub fn new(_write: WritePool, revision_pool: PgPool) -> Self {
        Self { revision_pool }
    }
}

#[async_trait]
impl WriteRepository for BrandWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        profile_id: BrandProfileId,
    ) -> Result<Option<BrandProfile>, PortError> {
        load_brand_relational(&self.revision_pool, authority_scope, profile_id).await
    }

    async fn load_archived_revision(
        &self,
        authority_scope: &AuthorityScope,
        profile_id: BrandProfileId,
        revision: u64,
    ) -> Result<Option<ArchivedBrandRevision>, PortError> {
        let Some(profile) = self.load(authority_scope, profile_id).await? else {
            return Ok(None);
        };
        load_brand_revision(&self.revision_pool, profile.scope(), profile_id, revision)
            .await
            .map(|value| {
                value.map(|revision| ArchivedBrandRevision {
                    profile_id,
                    scope: profile.scope().clone(),
                    source_revision: revision.revision(),
                    definition: revision.definition().clone(),
                })
            })
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
            "SELECT guard.profile_id, guard.published_revision
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
        let profile_id = BrandProfileId::try_from_uuid(
            row.try_get("profile_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?;
        let definition = load_brand_revision(&self.pool, &parent_scope, profile_id, revision)
            .await?
            .ok_or(PortError::Internal)?
            .definition()
            .clone();
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
        for draft in brand_directory_drafts(&request.aggregate)? {
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
                payload: BrandOutboxPayload::Directory(draft),
            });
        }
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
            active_scope: target_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id: aggregate_id(profile_id)?,
            expected_version: request.expected_version,
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

fn brand_directory_drafts(
    aggregate: &BrandProfile,
) -> Result<Vec<ItemDescriptorDraftV2>, PortError> {
    let record_id =
        RecordId::try_from_uuid(aggregate.id().as_uuid()).map_err(|_| PortError::Internal)?;
    let record = StableRecordRef::try_new(
        record_id,
        ContextId::Brand,
        RecordTypeId::try_new("brand.profile").map_err(|_| PortError::Internal)?,
    )
    .map_err(|_| PortError::Internal)?;
    let scope = aggregate
        .scope()
        .authority_scope()
        .map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope,
        lifecycle_state: if aggregate.published().is_some() {
            "published".into()
        } else {
            "draft".into()
        },
        classification: DataClassification::Internal,
        descriptor_version: aggregate.version(),
        authorized_search_text: aggregate.scope().canonical_key(),
    };
    let value = ItemDescriptorDraftV2::try_new(
        value,
        format!("Brand profile {}", aggregate.scope().canonical_key()),
    )
    .map_err(|_| PortError::Internal)?;
    let mut values = vec![value];
    for revision in aggregate.draft().into_iter().chain(aggregate.published()) {
        for asset in revision.definition().assets.references() {
            let record = erpx_brand::integration::asset_record_ref(aggregate.id(), asset)
                .map_err(|_| PortError::Internal)?;
            let display_label = format!("{} {}", asset.kind.as_str(), asset.media_type);
            let value = ItemDescriptorDraft {
                authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
                record,
                scope: aggregate
                    .scope()
                    .authority_scope()
                    .map_err(|_| PortError::Internal)?,
                lifecycle_state: "stored".into(),
                classification: DataClassification::Internal,
                descriptor_version: aggregate.version(),
                authorized_search_text: format!(
                    "{} {} {}",
                    aggregate.scope().canonical_key(),
                    asset.kind.as_str(),
                    asset.media_type
                ),
            };
            let value = ItemDescriptorDraftV2::try_new(value, display_label)
                .map_err(|_| PortError::Internal)?;
            if !values
                .iter()
                .any(|current| current.draft.record == value.draft.record)
            {
                values.push(value);
            }
        }
    }
    if let Some(publication) = aggregate.published() {
        let record =
            erpx_brand::integration::publication_record_ref(aggregate.id(), publication.revision())
                .map_err(|_| PortError::Internal)?;
        let display_label = publication.definition().display_name.clone();
        let value = ItemDescriptorDraft {
            authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
            record,
            scope: aggregate
                .scope()
                .authority_scope()
                .map_err(|_| PortError::Internal)?,
            lifecycle_state: "published".into(),
            classification: DataClassification::Internal,
            descriptor_version: aggregate.version(),
            authorized_search_text: format!(
                "{} {} {}",
                aggregate.scope().canonical_key(),
                publication.definition().display_name,
                publication.revision()
            ),
        };
        let value = ItemDescriptorDraftV2::try_new(value, display_label)
            .map_err(|_| PortError::Internal)?;
        values.push(value);
    }
    Ok(values)
}

struct BrandAtomicGuard {
    aggregate: BrandProfile,
    ensure_scope_vacant: bool,
    parent_guard: Option<ParentBrandWitness>,
    reason: String,
}

#[async_trait]
impl AtomicCommandGuard for BrandAtomicGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version FROM erpx_brand_write.profile_guards
             WHERE profile_id=$1 FOR UPDATE",
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
        let (scope_kind, data_profile_key, holding_id, business_unit) =
            brand_scope_parts(self.aggregate.scope());
        sqlx::query(
            "INSERT INTO erpx_brand_write.profile_guards
                (profile_id, scope_key, draft_revision, published_revision,
                 parent_published_revision, aggregate_version, updated_at,
                 scope_kind,data_profile_key,holding_id,business_unit,created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (profile_id) DO UPDATE SET
                scope_key=EXCLUDED.scope_key,
                draft_revision=EXCLUDED.draft_revision,
                published_revision=EXCLUDED.published_revision,
                parent_published_revision=EXCLUDED.parent_published_revision,
                aggregate_version=EXCLUDED.aggregate_version,
                updated_at=EXCLUDED.updated_at,
                scope_kind=EXCLUDED.scope_kind,
                data_profile_key=EXCLUDED.data_profile_key,
                holding_id=EXCLUDED.holding_id,
                business_unit=EXCLUDED.business_unit,
                created_at=EXCLUDED.created_at",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(self.aggregate.scope().canonical_key())
        .bind(draft_revision.map(to_i64).transpose()?)
        .bind(published_revision.map(to_i64).transpose()?)
        .bind(parent_published_revision.map(to_i64).transpose()?)
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.updated_at())
        .bind(scope_kind)
        .bind(data_profile_key)
        .bind(holding_id)
        .bind(business_unit)
        .bind(self.aggregate.created_at())
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
        "SELECT guard.profile_id, guard.published_revision
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
    let profile_id: Uuid = row.try_get("profile_id")?;
    let locked: Vec<String> = sqlx::query_scalar(
        "SELECT category FROM erpx_brand_write.profile_revision_categories
         WHERE profile_id=$1 AND revision=$2 AND category_role='locked'
         ORDER BY category",
    )
    .bind(profile_id)
    .bind(to_i64(revision)?)
    .fetch_all(&mut **transaction)
    .await?;
    let locked = locked
        .into_iter()
        .map(|value| parse_brand_category_store(&value))
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    if revision != witness.published_revision || locked != witness.locked_categories {
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
            (profile_id,revision,lifecycle,parent_published_revision,
             updated_by,reason,created_at,updated_at,published_at,archived_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,NULL)
         ON CONFLICT (profile_id,revision) DO UPDATE SET
            lifecycle=EXCLUDED.lifecycle,
            parent_published_revision=EXCLUDED.parent_published_revision,
            updated_by=EXCLUDED.updated_by,
            reason=EXCLUDED.reason,
            updated_at=EXCLUDED.updated_at,
            published_at=COALESCE(erpx_brand_write.profile_revisions.published_at,
                                  EXCLUDED.published_at),
            archived_at=NULL",
    )
    .bind(profile_id.as_uuid())
    .bind(to_i64(revision.revision())?)
    .bind(revision.lifecycle().as_str())
    .bind(
        revision
            .parent_published_revision()
            .map(to_i64)
            .transpose()?,
    )
    .bind(revision.updated_by())
    .bind(reason)
    .bind(revision.updated_at())
    .bind(revision.updated_at())
    .bind(published_at)
    .execute(&mut **transaction)
    .await?;
    upsert_revision_definition(transaction, profile_id, revision).await?;
    Ok(())
}

async fn upsert_revision_definition(
    transaction: &mut Transaction<'_, Postgres>,
    profile_id: BrandProfileId,
    revision: &erpx_brand::BrandRevision,
) -> Result<(), CqrsSqlxError> {
    let definition = revision.definition();
    let light = &definition.tokens.light;
    let dark = &definition.tokens.dark;
    sqlx::query(
        "INSERT INTO erpx_brand_write.profile_revision_tokens
         (profile_id,revision,display_name,short_name,
          light_primary,light_on_primary,light_accent,light_on_accent,light_canvas,
          light_surface,light_surface_raised,light_border,light_text,light_text_muted,light_link,light_focus,
          dark_primary,dark_on_primary,dark_accent,dark_on_accent,dark_canvas,
          dark_surface,dark_surface_raised,dark_border,dark_text,dark_text_muted,dark_link,dark_focus,
          font_pack,heading_scale_percent,body_scale_percent,line_height_percent,density,radius,
          elevation,navigation,navigation_expanded_by_default,motion)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35,$36,$37,$38)
         ON CONFLICT (profile_id,revision) DO UPDATE SET
          display_name=EXCLUDED.display_name,short_name=EXCLUDED.short_name,
          light_primary=EXCLUDED.light_primary,light_on_primary=EXCLUDED.light_on_primary,
          light_accent=EXCLUDED.light_accent,light_on_accent=EXCLUDED.light_on_accent,
          light_canvas=EXCLUDED.light_canvas,light_surface=EXCLUDED.light_surface,
          light_surface_raised=EXCLUDED.light_surface_raised,light_border=EXCLUDED.light_border,
          light_text=EXCLUDED.light_text,light_text_muted=EXCLUDED.light_text_muted,
          light_link=EXCLUDED.light_link,light_focus=EXCLUDED.light_focus,
          dark_primary=EXCLUDED.dark_primary,dark_on_primary=EXCLUDED.dark_on_primary,
          dark_accent=EXCLUDED.dark_accent,dark_on_accent=EXCLUDED.dark_on_accent,
          dark_canvas=EXCLUDED.dark_canvas,dark_surface=EXCLUDED.dark_surface,
          dark_surface_raised=EXCLUDED.dark_surface_raised,dark_border=EXCLUDED.dark_border,
          dark_text=EXCLUDED.dark_text,dark_text_muted=EXCLUDED.dark_text_muted,
          dark_link=EXCLUDED.dark_link,dark_focus=EXCLUDED.dark_focus,
          font_pack=EXCLUDED.font_pack,heading_scale_percent=EXCLUDED.heading_scale_percent,
          body_scale_percent=EXCLUDED.body_scale_percent,line_height_percent=EXCLUDED.line_height_percent,
          density=EXCLUDED.density,radius=EXCLUDED.radius,elevation=EXCLUDED.elevation,
          navigation=EXCLUDED.navigation,navigation_expanded_by_default=EXCLUDED.navigation_expanded_by_default,
          motion=EXCLUDED.motion",
    )
    .bind(profile_id.as_uuid()).bind(to_i64(revision.revision())?)
    .bind(&definition.display_name).bind(&definition.short_name)
    .bind(light.primary.as_str()).bind(light.on_primary.as_str()).bind(light.accent.as_str()).bind(light.on_accent.as_str())
    .bind(light.canvas.as_str()).bind(light.surface.as_str()).bind(light.surface_raised.as_str()).bind(light.border.as_str())
    .bind(light.text.as_str()).bind(light.text_muted.as_str()).bind(light.link.as_str()).bind(light.focus.as_str())
    .bind(dark.primary.as_str()).bind(dark.on_primary.as_str()).bind(dark.accent.as_str()).bind(dark.on_accent.as_str())
    .bind(dark.canvas.as_str()).bind(dark.surface.as_str()).bind(dark.surface_raised.as_str()).bind(dark.border.as_str())
    .bind(dark.text.as_str()).bind(dark.text_muted.as_str()).bind(dark.link.as_str()).bind(dark.focus.as_str())
    .bind(definition.tokens.font_pack.as_str()).bind(i32::from(definition.tokens.heading_scale_percent))
    .bind(i32::from(definition.tokens.body_scale_percent)).bind(i32::from(definition.tokens.line_height_percent))
    .bind(definition.tokens.density.as_str()).bind(definition.tokens.radius.as_str())
    .bind(definition.tokens.elevation.as_str()).bind(definition.tokens.navigation.as_str())
    .bind(definition.tokens.navigation_expanded_by_default).bind(definition.tokens.motion.as_str())
    .execute(&mut **transaction).await?;

    sqlx::query("DELETE FROM erpx_brand_write.profile_revision_categories WHERE profile_id=$1 AND revision=$2")
        .bind(profile_id.as_uuid()).bind(to_i64(revision.revision())?).execute(&mut **transaction).await?;
    for (role, categories) in [
        ("overridden", &definition.overridden_categories),
        ("locked", &definition.locked_categories),
    ] {
        for category in categories {
            sqlx::query("INSERT INTO erpx_brand_write.profile_revision_categories (profile_id,revision,category,category_role) VALUES ($1,$2,$3,$4)")
                .bind(profile_id.as_uuid()).bind(to_i64(revision.revision())?).bind(category.as_str()).bind(role).execute(&mut **transaction).await?;
        }
    }
    sqlx::query(
        "DELETE FROM erpx_brand_write.profile_revision_assets WHERE profile_id=$1 AND revision=$2",
    )
    .bind(profile_id.as_uuid())
    .bind(to_i64(revision.revision())?)
    .execute(&mut **transaction)
    .await?;
    for asset in definition.assets.references() {
        sqlx::query("INSERT INTO erpx_brand_write.profile_revision_assets (profile_id,revision,asset_kind,object_key,sha256,media_type,byte_length,width,height) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(profile_id.as_uuid()).bind(to_i64(revision.revision())?).bind(asset.kind.as_str()).bind(&asset.object_key).bind(&asset.sha256).bind(&asset.media_type)
            .bind(to_i64(asset.byte_length)?).bind(asset.width.map(i64::from)).bind(asset.height.map(i64::from)).execute(&mut **transaction).await?;
    }
    Ok(())
}

async fn load_brand_relational(
    pool: &PgPool,
    authority: &AuthorityScope,
    profile_id: BrandProfileId,
) -> Result<Option<BrandProfile>, PortError> {
    authority.validate().map_err(|_| PortError::Denied)?;
    let row = sqlx::query(
        "SELECT aggregate_version,scope_kind,data_profile_key,holding_id,business_unit,
                draft_revision,published_revision,created_at,updated_at
         FROM erpx_brand_write.profile_guards WHERE profile_id=$1",
    )
    .bind(profile_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let scope = brand_scope_from_row(&row)?;
    let target = scope.authority_scope().map_err(|_| PortError::Internal)?;
    if !authority.contains(&target) {
        return Err(PortError::Denied);
    }
    let draft_revision = optional_port_u64(
        row.try_get("draft_revision")
            .map_err(|_| PortError::Internal)?,
    )?;
    let published_revision = optional_port_u64(
        row.try_get("published_revision")
            .map_err(|_| PortError::Internal)?,
    )?;
    let draft = match draft_revision {
        Some(revision) => load_brand_revision(pool, &scope, profile_id, revision).await?,
        None => None,
    };
    let published = match published_revision {
        Some(revision) => load_brand_revision(pool, &scope, profile_id, revision).await?,
        None => None,
    };
    BrandProfile::try_rehydrate(
        profile_id,
        from_i64(
            row.try_get("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )?,
        scope,
        draft,
        published,
        row.try_get("created_at").map_err(|_| PortError::Internal)?,
        row.try_get("updated_at").map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_brand_revision(
    pool: &PgPool,
    scope: &BrandScope,
    profile_id: BrandProfileId,
    revision: u64,
) -> Result<Option<erpx_brand::BrandRevision>, PortError> {
    let metadata = sqlx::query(
        "SELECT lifecycle,parent_published_revision,updated_by,updated_at
         FROM erpx_brand_write.profile_revisions
         WHERE profile_id=$1 AND revision=$2",
    )
    .bind(profile_id.as_uuid())
    .bind(to_i64_port(revision)?)
    .fetch_optional(pool)
    .await
    .map_err(|_| PortError::Unavailable)?;
    let Some(metadata) = metadata else {
        return Ok(None);
    };
    let tokens = sqlx::query("SELECT * FROM erpx_brand_write.profile_revision_tokens WHERE profile_id=$1 AND revision=$2")
        .bind(profile_id.as_uuid()).bind(to_i64_port(revision)?).fetch_optional(pool).await.map_err(|_| PortError::Unavailable)?.ok_or(PortError::Internal)?;
    let categories = sqlx::query("SELECT category,category_role FROM erpx_brand_write.profile_revision_categories WHERE profile_id=$1 AND revision=$2 ORDER BY category_role,category")
        .bind(profile_id.as_uuid()).bind(to_i64_port(revision)?).fetch_all(pool).await.map_err(|_| PortError::Unavailable)?;
    let mut overridden_categories = std::collections::BTreeSet::new();
    let mut locked_categories = std::collections::BTreeSet::new();
    for row in categories {
        let category = parse_brand_category(
            &row.try_get::<String, _>("category")
                .map_err(|_| PortError::Internal)?,
        )?;
        match row
            .try_get::<String, _>("category_role")
            .map_err(|_| PortError::Internal)?
            .as_str()
        {
            "overridden" => {
                overridden_categories.insert(category);
            }
            "locked" => {
                locked_categories.insert(category);
            }
            _ => return Err(PortError::Internal),
        }
    }
    let assets = load_brand_assets(pool, profile_id, revision).await?;
    let definition = BrandDefinition {
        display_name: tokens
            .try_get("display_name")
            .map_err(|_| PortError::Internal)?,
        short_name: tokens
            .try_get("short_name")
            .map_err(|_| PortError::Internal)?,
        tokens: BrandTokenSet {
            light: palette_from_row(&tokens, "light")?,
            dark: palette_from_row(&tokens, "dark")?,
            font_pack: parse_font_pack(
                tokens
                    .try_get("font_pack")
                    .map_err(|_| PortError::Internal)?,
            )?,
            heading_scale_percent: port_u16(
                tokens
                    .try_get("heading_scale_percent")
                    .map_err(|_| PortError::Internal)?,
            )?,
            body_scale_percent: port_u16(
                tokens
                    .try_get("body_scale_percent")
                    .map_err(|_| PortError::Internal)?,
            )?,
            line_height_percent: port_u16(
                tokens
                    .try_get("line_height_percent")
                    .map_err(|_| PortError::Internal)?,
            )?,
            density: parse_density(tokens.try_get("density").map_err(|_| PortError::Internal)?)?,
            radius: parse_radius(tokens.try_get("radius").map_err(|_| PortError::Internal)?)?,
            elevation: parse_elevation(
                tokens
                    .try_get("elevation")
                    .map_err(|_| PortError::Internal)?,
            )?,
            navigation: parse_navigation(
                tokens
                    .try_get("navigation")
                    .map_err(|_| PortError::Internal)?,
            )?,
            navigation_expanded_by_default: tokens
                .try_get("navigation_expanded_by_default")
                .map_err(|_| PortError::Internal)?,
            motion: parse_motion(tokens.try_get("motion").map_err(|_| PortError::Internal)?)?,
        },
        assets,
        overridden_categories,
        locked_categories,
    };
    erpx_brand::BrandRevision::try_rehydrate(
        scope,
        revision,
        match metadata
            .try_get::<String, _>("lifecycle")
            .map_err(|_| PortError::Internal)?
            .as_str()
        {
            "draft" => BrandLifecycle::Draft,
            "published" => BrandLifecycle::Published,
            _ => return Err(PortError::Internal),
        },
        definition,
        optional_port_u64(
            metadata
                .try_get("parent_published_revision")
                .map_err(|_| PortError::Internal)?,
        )?,
        metadata
            .try_get::<String, _>("updated_by")
            .map_err(|_| PortError::Internal)?,
        metadata
            .try_get("updated_at")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn load_brand_assets(
    pool: &PgPool,
    profile_id: BrandProfileId,
    revision: u64,
) -> Result<BrandAssetManifest, PortError> {
    let rows = sqlx::query("SELECT asset_kind,object_key,sha256,media_type,byte_length,width,height FROM erpx_brand_write.profile_revision_assets WHERE profile_id=$1 AND revision=$2 ORDER BY asset_kind")
        .bind(profile_id.as_uuid()).bind(to_i64_port(revision)?).fetch_all(pool).await.map_err(|_| PortError::Unavailable)?;
    let mut manifest = BrandAssetManifest::default();
    for row in rows {
        let kind = match row
            .try_get::<String, _>("asset_kind")
            .map_err(|_| PortError::Internal)?
            .as_str()
        {
            "logo_mark" => BrandAssetKind::LogoMark,
            "wordmark" => BrandAssetKind::Wordmark,
            "favicon" => BrandAssetKind::Favicon,
            "login_artwork" => BrandAssetKind::LoginArtwork,
            _ => return Err(PortError::Internal),
        };
        let asset = BrandAssetReference {
            kind,
            object_key: row.try_get("object_key").map_err(|_| PortError::Internal)?,
            sha256: row.try_get("sha256").map_err(|_| PortError::Internal)?,
            media_type: row.try_get("media_type").map_err(|_| PortError::Internal)?,
            byte_length: u64::try_from(
                row.try_get::<i64, _>("byte_length")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            width: optional_port_u32(row.try_get("width").map_err(|_| PortError::Internal)?)?,
            height: optional_port_u32(row.try_get("height").map_err(|_| PortError::Internal)?)?,
        };
        let slot = match kind {
            BrandAssetKind::LogoMark => &mut manifest.logo_mark,
            BrandAssetKind::Wordmark => &mut manifest.wordmark,
            BrandAssetKind::Favicon => &mut manifest.favicon,
            BrandAssetKind::LoginArtwork => &mut manifest.login_artwork,
        };
        if slot.replace(asset).is_some() {
            return Err(PortError::Internal);
        }
    }
    Ok(manifest)
}

fn palette_from_row(row: &sqlx::postgres::PgRow, prefix: &str) -> Result<BrandPalette, PortError> {
    let color = |name: &str| -> Result<HexColor, PortError> {
        let column = format!("{prefix}_{name}");
        HexColor::try_new(
            row.try_get::<String, _>(column.as_str())
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)
    };
    Ok(BrandPalette {
        primary: color("primary")?,
        on_primary: color("on_primary")?,
        accent: color("accent")?,
        on_accent: color("on_accent")?,
        canvas: color("canvas")?,
        surface: color("surface")?,
        surface_raised: color("surface_raised")?,
        border: color("border")?,
        text: color("text")?,
        text_muted: color("text_muted")?,
        link: color("link")?,
        focus: color("focus")?,
    })
}

fn brand_scope_parts(
    scope: &BrandScope,
) -> (
    &'static str,
    Option<String>,
    Option<Uuid>,
    Option<&'static str>,
) {
    match scope {
        BrandScope::Platform => ("platform", None, None, None),
        BrandScope::Holding { holding_id } => (
            "holding",
            Some(holding_id.to_string()),
            Some(*holding_id),
            None,
        ),
        BrandScope::BusinessUnit {
            holding_id,
            business_unit,
        } => (
            "business_unit",
            Some(holding_id.to_string()),
            Some(*holding_id),
            Some(business_unit.as_str()),
        ),
    }
}

fn brand_scope_from_row(row: &sqlx::postgres::PgRow) -> Result<BrandScope, PortError> {
    let kind: String = row.try_get("scope_kind").map_err(|_| PortError::Internal)?;
    let _partition: Option<String> = row
        .try_get("data_profile_key")
        .map_err(|_| PortError::Internal)?;
    let holding: Option<Uuid> = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
    let unit: Option<String> = row
        .try_get("business_unit")
        .map_err(|_| PortError::Internal)?;
    match (kind.as_str(), holding, unit) {
        ("platform", None, None) => Ok(BrandScope::Platform),
        ("holding", Some(holding_id), None) => Ok(BrandScope::Holding { holding_id }),
        ("business_unit", Some(holding_id), Some(unit)) => Ok(BrandScope::BusinessUnit {
            holding_id,
            business_unit: parse_business_unit(&unit)?,
        }),
        _ => Err(PortError::Internal),
    }
}

fn parse_business_unit(value: &str) -> Result<BusinessUnitKey, PortError> {
    match value {
        "operations" => Ok(BusinessUnitKey::Operations),
        "finance" => Ok(BusinessUnitKey::Finance),
        "sales" => Ok(BusinessUnitKey::Sales),
        "procurement" => Ok(BusinessUnitKey::Procurement),
        "inventory" => Ok(BusinessUnitKey::Inventory),
        "assets" => Ok(BusinessUnitKey::Assets),
        "maintenance" => Ok(BusinessUnitKey::Maintenance),
        "quality" => Ok(BusinessUnitKey::Quality),
        "marketing-crm" => Ok(BusinessUnitKey::MarketingCrm),
        "hr-payroll" => Ok(BusinessUnitKey::HrPayroll),
        "point-of-sale" => Ok(BusinessUnitKey::PointOfSale),
        "legal" => Ok(BusinessUnitKey::Legal),
        _ => Err(PortError::Internal),
    }
}
fn parse_brand_category(value: &str) -> Result<BrandCategory, PortError> {
    parse_brand_category_store(value).map_err(|_| PortError::Internal)
}
fn parse_brand_category_store(value: &str) -> Result<BrandCategory, CqrsSqlxError> {
    match value {
        "identity" => Ok(BrandCategory::Identity),
        "color" => Ok(BrandCategory::Color),
        "typography" => Ok(BrandCategory::Typography),
        "shape" => Ok(BrandCategory::Shape),
        "elevation" => Ok(BrandCategory::Elevation),
        "density" => Ok(BrandCategory::Density),
        "navigation" => Ok(BrandCategory::Navigation),
        "motion" => Ok(BrandCategory::Motion),
        _ => Err(CqrsSqlxError::GuardRejected(
            "invalid brand category authority",
        )),
    }
}
fn parse_font_pack(value: String) -> Result<BrandFontPack, PortError> {
    match value.as_str() {
        "erpx_sans" => Ok(BrandFontPack::ErpxSans),
        "corporate_sans" => Ok(BrandFontPack::CorporateSans),
        "thai_formal" => Ok(BrandFontPack::ThaiFormal),
        "system_ui" => Ok(BrandFontPack::SystemUi),
        _ => Err(PortError::Internal),
    }
}
fn parse_density(value: String) -> Result<BrandDensity, PortError> {
    match value.as_str() {
        "compact" => Ok(BrandDensity::Compact),
        "comfortable" => Ok(BrandDensity::Comfortable),
        "spacious" => Ok(BrandDensity::Spacious),
        _ => Err(PortError::Internal),
    }
}
fn parse_radius(value: String) -> Result<BrandRadius, PortError> {
    match value.as_str() {
        "square" => Ok(BrandRadius::Square),
        "soft" => Ok(BrandRadius::Soft),
        "rounded" => Ok(BrandRadius::Rounded),
        _ => Err(PortError::Internal),
    }
}
fn parse_elevation(value: String) -> Result<BrandElevation, PortError> {
    match value.as_str() {
        "flat" => Ok(BrandElevation::Flat),
        "subtle" => Ok(BrandElevation::Subtle),
        "elevated" => Ok(BrandElevation::Elevated),
        _ => Err(PortError::Internal),
    }
}
fn parse_navigation(value: String) -> Result<BrandNavigation, PortError> {
    match value.as_str() {
        "solid" => Ok(BrandNavigation::Solid),
        "tinted" => Ok(BrandNavigation::Tinted),
        _ => Err(PortError::Internal),
    }
}
fn parse_motion(value: String) -> Result<BrandMotion, PortError> {
    match value.as_str() {
        "none" => Ok(BrandMotion::None),
        "subtle" => Ok(BrandMotion::Subtle),
        "standard" => Ok(BrandMotion::Standard),
        _ => Err(PortError::Internal),
    }
}
fn optional_port_u64(value: Option<i64>) -> Result<Option<u64>, PortError> {
    value.map(from_i64).transpose()
}
fn optional_port_u32(value: Option<i64>) -> Result<Option<u32>, PortError> {
    value
        .map(|value| u32::try_from(value).map_err(|_| PortError::Internal))
        .transpose()
}
fn port_u16(value: i32) -> Result<u16, PortError> {
    u16::try_from(value).map_err(|_| PortError::Internal)
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
        BrandScope::BusinessUnit { holding_id, .. } => Some(BrandScope::Holding {
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

#[cfg(test)]
mod derived_record_tests {
    #[test]
    fn derived_facts_are_emitted_inside_the_profile_owner_uow() {
        let source = include_str!("command.rs");
        assert!(source.contains("brand_directory_drafts(&request.aggregate)"));
        assert!(source.contains("integration::asset_record_ref"));
        assert!(source.contains("integration::publication_record_ref"));
        assert!(!source.contains(concat!("asset_", "snapshots")));
        assert!(!source.contains(concat!("publication_", "snapshots")));
    }
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
            holding_id: Uuid::from_u128(2),
        };
        assert_eq!(parent_scope(&holding).unwrap(), Some(BrandScope::Platform));
    }

    #[test]
    fn command_authority_is_relational_and_snapshot_free() {
        let production = include_str!("command.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("load_snapshot"));
        assert!(!production.contains("serde_json::from_value"));
        assert!(!production.contains("definition JSONB"));
        assert!(production.contains("profile_revision_tokens"));
        assert!(production.contains("commit_normalized"));
    }
}
