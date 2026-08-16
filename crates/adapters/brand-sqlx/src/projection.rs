use std::collections::BTreeSet;

use async_trait::async_trait;
use erpx_brand::{
    BrandCategory, BrandLifecycle, BrandProfileView, BrandRevision, BrandScope, ResolvedBrandView,
};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, brand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, HISTORY_PROJECTION, PROFILE_PROJECTION,
    RESOLVED_PROJECTION, SNAPSHOT_EVENT_TYPE, profile_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandSnapshot {
    pub profile: BrandProfileView,
    pub reason: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
// This public serialized adapter payload is also constructed by command writers;
// boxing one variant would ripple through that API for no wire-size reduction.
#[allow(clippy::large_enum_variant)]
pub enum BrandOutboxPayload {
    Integration(brand::Event),
    Snapshot(BrandSnapshot),
    Directory(ItemDescriptorDraft),
}

macro_rules! projector {
    ($name:ident, $projection:expr, $apply:ident, $reset:ident) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            event_types: Vec<String>,
        }

        impl $name {
            pub fn new() -> Self {
                Self {
                    event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        #[async_trait]
        impl Projector for $name {
            fn name(&self) -> &str {
                $projection
            }

            fn event_types(&self) -> &[String] {
                &self.event_types
            }

            async fn apply(
                &self,
                transaction: &mut Transaction<'_, Postgres>,
                context: &ProjectorContext<'_>,
                event: &ProjectionEvent,
            ) -> Result<(), ProjectionFailure> {
                let snapshot = validate_event($projection, context, event)?;
                $apply(transaction, context, event, snapshot).await
            }

            async fn reset(
                &self,
                transaction: &mut Transaction<'_, Postgres>,
                context: &ProjectorContext<'_>,
            ) -> Result<(), ProjectionFailure> {
                $reset(transaction, context).await
            }
        }
    };
}

projector!(
    BrandProfileProjector,
    PROFILE_PROJECTION,
    apply_profile,
    reset_profiles
);
projector!(
    BrandResolvedProjector,
    RESOLVED_PROJECTION,
    apply_resolved,
    reset_resolved
);
projector!(
    BrandHistoryProjector,
    HISTORY_PROJECTION,
    apply_history,
    reset_history
);

async fn apply_profile(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    snapshot: BrandSnapshot,
) -> Result<(), ProjectionFailure> {
    let view = snapshot.profile;
    let state = serde_json::to_value(&view).map_err(projection_failure)?;
    let result = sqlx::query(
        "INSERT INTO erpx_brand_read.profile_projection
            (projection_name,generation,partition_key,profile_id,
             source_aggregate_version,source_global_sequence,tenant_scope,
             scope_key,draft_revision,published_revision,state,
             aggregate_created_at,aggregate_updated_at,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,now())
         ON CONFLICT (projection_name,generation,partition_key,profile_id)
         DO UPDATE SET
            source_aggregate_version=EXCLUDED.source_aggregate_version,
            source_global_sequence=EXCLUDED.source_global_sequence,
            tenant_scope=EXCLUDED.tenant_scope,
            scope_key=EXCLUDED.scope_key,
            draft_revision=EXCLUDED.draft_revision,
            published_revision=EXCLUDED.published_revision,
            state=EXCLUDED.state,
            aggregate_created_at=EXCLUDED.aggregate_created_at,
            aggregate_updated_at=EXCLUDED.aggregate_updated_at,
            projected_at=now()
         WHERE erpx_brand_read.profile_projection.source_aggregate_version
               < EXCLUDED.source_aggregate_version",
    )
    .bind(PROFILE_PROJECTION)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(view.profile_id.as_uuid())
    .bind(to_i64(view.aggregate_version)?)
    .bind(to_i64(event.global_sequence)?)
    .bind(
        serde_json::to_value(view.scope.authority_scope().map_err(projection_failure)?)
            .map_err(projection_failure)?,
    )
    .bind(view.scope.canonical_key())
    .bind(
        view.draft
            .as_ref()
            .map(BrandRevision::revision)
            .map(to_i64)
            .transpose()?,
    )
    .bind(
        view.published
            .as_ref()
            .map(BrandRevision::revision)
            .map(to_i64)
            .transpose()?,
    )
    .bind(state)
    .bind(view.created_at)
    .bind(view.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    if result.rows_affected() != 1 {
        return Err(ProjectionFailure::new(
            "brand profile aggregate version did not advance",
        ));
    }
    Ok(())
}

async fn apply_history(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    snapshot: BrandSnapshot,
) -> Result<(), ProjectionFailure> {
    let view = snapshot.profile;
    let draft = view.draft.as_ref().map(BrandRevision::revision);
    let published = view.published.as_ref().map(BrandRevision::revision);
    sqlx::query(
        "UPDATE erpx_brand_read.history_projection
         SET archived_at=$4,source_global_sequence=$5,projected_at=now()
         WHERE projection_name=$1 AND generation=$2 AND profile_id=$3
           AND archived_at IS NULL
           AND ($6::bigint IS NULL OR revision <> $6)
           AND ($7::bigint IS NULL OR revision <> $7)
           AND source_global_sequence < $5",
    )
    .bind(HISTORY_PROJECTION)
    .bind(context.generation)
    .bind(view.profile_id.as_uuid())
    .bind(view.updated_at)
    .bind(to_i64(event.global_sequence)?)
    .bind(draft.map(to_i64).transpose()?)
    .bind(published.map(to_i64).transpose()?)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    if let Some(revision) = &view.draft {
        upsert_history(
            transaction,
            context,
            event,
            &view,
            revision,
            &snapshot.reason,
        )
        .await?;
    }
    if let Some(revision) = &view.published {
        upsert_history(
            transaction,
            context,
            event,
            &view,
            revision,
            &snapshot.reason,
        )
        .await?;
    }
    Ok(())
}

async fn upsert_history(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    profile: &BrandProfileView,
    revision: &BrandRevision,
    reason: &str,
) -> Result<(), ProjectionFailure> {
    let published_at =
        (revision.lifecycle() == BrandLifecycle::Published).then_some(revision.updated_at());
    sqlx::query(
        "INSERT INTO erpx_brand_read.history_projection
            (projection_name,generation,partition_key,scope_key,profile_id,
             revision,lifecycle,definition,parent_published_revision,updated_by,
             reason,created_at,published_at,archived_at,source_global_sequence,projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NULL,$14,now())
         ON CONFLICT (projection_name,generation,partition_key,profile_id,revision)
         DO UPDATE SET
            lifecycle=EXCLUDED.lifecycle,
            definition=EXCLUDED.definition,
            parent_published_revision=EXCLUDED.parent_published_revision,
            updated_by=EXCLUDED.updated_by,
            reason=EXCLUDED.reason,
            published_at=COALESCE(erpx_brand_read.history_projection.published_at,
                                  EXCLUDED.published_at),
            archived_at=NULL,
            source_global_sequence=EXCLUDED.source_global_sequence,
            projected_at=now()
         WHERE erpx_brand_read.history_projection.source_global_sequence
               < EXCLUDED.source_global_sequence",
    )
    .bind(HISTORY_PROJECTION)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(profile.scope.canonical_key())
    .bind(profile.profile_id.as_uuid())
    .bind(to_i64(revision.revision())?)
    .bind(revision.lifecycle().as_str())
    .bind(serde_json::to_value(revision.definition()).map_err(projection_failure)?)
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
    .bind(to_i64(event.global_sequence)?)
    .execute(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    Ok(())
}

async fn apply_resolved(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    snapshot: BrandSnapshot,
) -> Result<(), ProjectionFailure> {
    let profiles = active_profiles(transaction, &snapshot.profile).await?;
    let targets = affected_targets(&profiles, &snapshot.profile.scope);
    for target in targets {
        let Some(resolved) = resolve_view(&profiles, &target)? else {
            continue;
        };
        let digest = digest(&resolved)?;
        let result = sqlx::query(
            "INSERT INTO erpx_brand_read.resolved_projection
                (projection_name,generation,partition_key,scope_key,
                 published_revision,definition,inherited_categories,
                 effective_locked_categories,digest,source_global_sequence,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())
             ON CONFLICT (projection_name,generation,partition_key,scope_key)
             DO UPDATE SET
                published_revision=EXCLUDED.published_revision,
                definition=EXCLUDED.definition,
                inherited_categories=EXCLUDED.inherited_categories,
                effective_locked_categories=EXCLUDED.effective_locked_categories,
                digest=EXCLUDED.digest,
                source_global_sequence=EXCLUDED.source_global_sequence,
                projected_at=now()
             WHERE erpx_brand_read.resolved_projection.source_global_sequence
                   < EXCLUDED.source_global_sequence",
        )
        .bind(RESOLVED_PROJECTION)
        .bind(context.generation)
        .bind(resolved_partition(&target))
        .bind(target.canonical_key())
        .bind(to_i64(resolved.published_revision)?)
        .bind(serde_json::to_value(&resolved.definition).map_err(projection_failure)?)
        .bind(serde_json::to_value(&resolved.inherited_categories).map_err(projection_failure)?)
        .bind(
            serde_json::to_value(&resolved.effective_locked_categories)
                .map_err(projection_failure)?,
        )
        .bind(&digest)
        .bind(to_i64(event.global_sequence)?)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "brand resolved projection sequence did not advance",
            ));
        }
    }
    Ok(())
}

async fn active_profiles(
    transaction: &mut Transaction<'_, Postgres>,
    current: &BrandProfileView,
) -> Result<Vec<BrandProfileView>, ProjectionFailure> {
    let rows = sqlx::query(
        "SELECT projection.state
         FROM erpx_brand_read.profile_projection projection
         JOIN erpx_cqrs_read.projection_generations generation
           ON generation.projection_name=projection.projection_name
          AND generation.generation=projection.generation
          AND generation.status='active'
         WHERE projection.projection_name=$1",
    )
    .bind(PROFILE_PROJECTION)
    .fetch_all(&mut **transaction)
    .await
    .map_err(projection_failure)?;
    let mut profiles = rows
        .into_iter()
        .map(|row| {
            let state: serde_json::Value = row.try_get("state").map_err(projection_failure)?;
            serde_json::from_value(state).map_err(projection_failure)
        })
        .collect::<Result<Vec<_>, _>>()?;
    profiles.retain(|profile: &BrandProfileView| profile.profile_id != current.profile_id);
    profiles.push(current.clone());
    Ok(profiles)
}

fn affected_targets(profiles: &[BrandProfileView], changed: &BrandScope) -> Vec<BrandScope> {
    let mut targets = profiles
        .iter()
        .filter(|profile| match changed {
            BrandScope::Platform => true,
            BrandScope::Holding {
                tenant_id,
                holding_id,
            } => {
                profile.scope == *changed
                    || matches!(
                        &profile.scope,
                        BrandScope::BusinessUnit {
                            tenant_id: child_tenant,
                            holding_id: child_holding,
                            ..
                        } if child_tenant == tenant_id && child_holding == holding_id
                    )
            }
            BrandScope::BusinessUnit { .. } => profile.scope == *changed,
        })
        .map(|profile| profile.scope.clone())
        .collect::<Vec<_>>();
    if !targets.contains(changed) {
        targets.push(changed.clone());
    }
    targets.sort();
    targets.dedup();
    targets
}

fn resolve_view(
    profiles: &[BrandProfileView],
    target: &BrandScope,
) -> Result<Option<ResolvedBrandView>, ProjectionFailure> {
    let platform = find_published(profiles, &BrandScope::Platform);
    let Some(platform) = platform else {
        return Ok(None);
    };
    let mut resolved = ResolvedBrandView {
        scope: BrandScope::Platform,
        published_revision: platform.revision(),
        definition: platform.definition().clone(),
        inherited_categories: BTreeSet::new(),
        effective_locked_categories: platform.definition().locked_categories.clone(),
        digest: String::new(),
    };
    match target {
        BrandScope::Platform => {}
        BrandScope::Holding { .. } => {
            let Some(holding) = find_published(profiles, target) else {
                return Ok(None);
            };
            apply_layer(&mut resolved, target, holding);
        }
        BrandScope::BusinessUnit {
            tenant_id,
            holding_id,
            ..
        } => {
            let holding_scope = BrandScope::Holding {
                tenant_id: tenant_id.clone(),
                holding_id: *holding_id,
            };
            let Some(holding) = find_published(profiles, &holding_scope) else {
                return Ok(None);
            };
            apply_layer(&mut resolved, &holding_scope, holding);
            let Some(unit) = find_published(profiles, target) else {
                return Ok(None);
            };
            apply_layer(&mut resolved, target, unit);
        }
    }
    resolved.digest = digest(&resolved)?;
    Ok(Some(resolved))
}

fn find_published<'a>(
    profiles: &'a [BrandProfileView],
    scope: &BrandScope,
) -> Option<&'a BrandRevision> {
    profiles
        .iter()
        .find(|profile| &profile.scope == scope)
        .and_then(|profile| profile.published.as_ref())
}

fn apply_layer(resolved: &mut ResolvedBrandView, scope: &BrandScope, layer: &BrandRevision) {
    let definition = layer.definition();
    let active = definition
        .overridden_categories
        .difference(&resolved.effective_locked_categories)
        .copied()
        .collect::<BTreeSet<_>>();
    if active.contains(&BrandCategory::Identity) {
        resolved
            .definition
            .display_name
            .clone_from(&definition.display_name);
        resolved
            .definition
            .short_name
            .clone_from(&definition.short_name);
        resolved.definition.assets.clone_from(&definition.assets);
    }
    if active.contains(&BrandCategory::Color) {
        resolved
            .definition
            .tokens
            .light
            .clone_from(&definition.tokens.light);
        resolved
            .definition
            .tokens
            .dark
            .clone_from(&definition.tokens.dark);
    }
    if active.contains(&BrandCategory::Typography) {
        resolved.definition.tokens.font_pack = definition.tokens.font_pack;
        resolved.definition.tokens.heading_scale_percent = definition.tokens.heading_scale_percent;
        resolved.definition.tokens.body_scale_percent = definition.tokens.body_scale_percent;
        resolved.definition.tokens.line_height_percent = definition.tokens.line_height_percent;
    }
    if active.contains(&BrandCategory::Shape) {
        resolved.definition.tokens.radius = definition.tokens.radius;
    }
    if active.contains(&BrandCategory::Elevation) {
        resolved.definition.tokens.elevation = definition.tokens.elevation;
    }
    if active.contains(&BrandCategory::Density) {
        resolved.definition.tokens.density = definition.tokens.density;
    }
    if active.contains(&BrandCategory::Navigation) {
        resolved.definition.tokens.navigation = definition.tokens.navigation;
        resolved.definition.tokens.navigation_expanded_by_default =
            definition.tokens.navigation_expanded_by_default;
    }
    if active.contains(&BrandCategory::Motion) {
        resolved.definition.tokens.motion = definition.tokens.motion;
    }
    resolved
        .definition
        .overridden_categories
        .clone_from(&definition.overridden_categories);
    resolved
        .definition
        .locked_categories
        .clone_from(&definition.locked_categories);
    resolved.scope.clone_from(scope);
    resolved.published_revision = layer.revision();
    resolved
        .effective_locked_categories
        .extend(definition.locked_categories.iter().copied());
    resolved.inherited_categories = BrandCategory::ALL
        .into_iter()
        .filter(|category| !active.contains(category))
        .collect();
}

fn digest(value: &ResolvedBrandView) -> Result<String, ProjectionFailure> {
    let mut canonical = value.clone();
    canonical.digest.clear();
    let bytes = serde_json::to_vec(&canonical).map_err(projection_failure)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_event(
    projection: &str,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<BrandSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "brand projector received an incompatible event contract",
        ));
    }
    let payload: BrandOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let BrandOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "brand snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.profile;
    let scope = view.scope.authority_scope().map_err(projection_failure)?;
    if view.aggregate_version != event.aggregate_version
        || view.profile_id.as_uuid() != event.aggregate_id.as_uuid()
        || scope != event.tenant_scope
        || event.partition != profile_partition(view.profile_id)
        || snapshot.reason.trim().is_empty()
    {
        return Err(ProjectionFailure::new(
            "brand snapshot metadata does not match its outbox envelope",
        ));
    }
    Ok(snapshot)
}

async fn reset_profiles(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
) -> Result<(), ProjectionFailure> {
    reset_table(transaction, context, "erpx_brand_read.profile_projection").await
}

async fn reset_resolved(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
) -> Result<(), ProjectionFailure> {
    reset_table(transaction, context, "erpx_brand_read.resolved_projection").await
}

async fn reset_history(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
) -> Result<(), ProjectionFailure> {
    reset_table(transaction, context, "erpx_brand_read.history_projection").await
}

async fn reset_table(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    table: &'static str,
) -> Result<(), ProjectionFailure> {
    let statement = format!("DELETE FROM {table} WHERE projection_name=$1 AND generation=$2");
    sqlx::query(&statement)
        .bind(context.projection)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
    Ok(())
}

pub fn resolved_partition(scope: &BrandScope) -> String {
    let digest = Sha256::digest(scope.canonical_key().as_bytes());
    format!("brand.resolved:{digest:x}")
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("brand numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_projectors_consume_private_snapshot_events_only() {
        for projector in [
            Box::new(BrandProfileProjector::new()) as Box<dyn Projector>,
            Box::new(BrandResolvedProjector::new()),
            Box::new(BrandHistoryProjector::new()),
        ] {
            assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
        }
    }

    #[test]
    fn resolved_partitions_are_stable_and_scope_specific() {
        let platform = resolved_partition(&BrandScope::Platform);
        let holding = resolved_partition(&BrandScope::Holding {
            tenant_id: "tenant-acme".into(),
            holding_id: uuid::Uuid::from_u128(1),
        });
        assert!(platform.starts_with("brand.resolved:"));
        assert_ne!(platform, holding);
    }

    #[test]
    fn affected_targets_include_descendants_but_not_siblings() {
        let changed = BrandScope::Holding {
            tenant_id: "tenant-a".into(),
            holding_id: uuid::Uuid::from_u128(1),
        };
        let profiles = vec![BrandProfileView {
            profile_id: erpx_brand::BrandProfileId::try_from_uuid(uuid::Uuid::from_u128(2))
                .unwrap(),
            aggregate_version: 1,
            scope: BrandScope::BusinessUnit {
                tenant_id: "tenant-a".into(),
                holding_id: uuid::Uuid::from_u128(1),
                business_unit: erpx_brand::BusinessUnitKey::Finance,
            },
            draft: None,
            published: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];
        let targets = affected_targets(&profiles, &changed);
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn history_contract_keeps_archival_and_rebuild_columns() {
        let ddl = crate::CONTROL_READ_CONTEXT_DDL;
        for field in [
            "history_projection",
            "archived_at",
            "source_global_sequence",
            "generation",
        ] {
            assert!(ddl.contains(field));
        }
        let _ = std::mem::size_of::<erpx_brand::BrandHistoryEntry>();
    }
}
