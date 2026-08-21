//! Control-plane worker composition for the Wave 5 bounded contexts.
//!
//! Context modules expose storage-agnostic factories. This composition root is
//! the only place that binds them to the control write/read pools and the
//! process-wide named worker catalog.

use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use erpx_collaboration_sqlx::{
    GLOBAL_RELATION_DRAFT_EVENT_TYPE, GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE,
};
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::SqlxControlPlane;
use erpx_integration_contracts::ITEM_DIRECTORY_OWNER_EVENT_TYPE;
use erpx_worker::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};
use sqlx::{PgPool, Row};

use crate::contexts::{
    brand::{BrandProjectionFactory, BrandProjectionKind, BrandProjectionRuntime},
    customer_authority::{CustomerAuthorityProjectionFactory, CustomerAuthorityProjectionRuntime},
    global_collaboration::{
        GlobalCollaborationProjectionFactory, GlobalCollaborationProjectionRuntime,
    },
    platform_administration::{
        PlatformAdministrationProjectionFactory, PlatformAdministrationProjectionKind,
        PlatformAdministrationProjectionRuntime,
    },
};

pub const WAVE5_CONTROL_PROJECTION_HANDLER_NAME: &str = "wave5_control_projections";

#[derive(Debug)]
pub struct Wave5ControlProjectionHandler {
    platform: Vec<PlatformAdministrationProjectionRuntime>,
    brand: BrandProjectionRuntime,
    customer_authority: CustomerAuthorityProjectionRuntime,
    global_collaboration: GlobalCollaborationProjectionRuntime,
    global_write: PgPool,
    global_read: PgPool,
    page_size: u32,
    rebuild_pending: AtomicBool,
}

impl Wave5ControlProjectionHandler {
    pub fn new(control: Arc<SqlxControlPlane>, page_size: u32) -> Result<Self, WorkerRuntimeError> {
        let global_write = control.pools().write.clone();
        let global_read = control.pools().read.clone();
        let write = WritePool::from_control_pg_pool(control.pools().write.clone());
        let read = ProjectionPool::from_control_pg_pool(control.pools().read.clone());
        let config = ProjectionRunnerConfig {
            page_size,
            ..ProjectionRunnerConfig::default()
        };

        let platform_factory = PlatformAdministrationProjectionFactory;
        for registration in platform_factory.registrations() {
            tracing::info!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Platform Administration CQRS projector"
            );
        }
        let platform = [
            PlatformAdministrationProjectionKind::Holdings,
            PlatformAdministrationProjectionKind::Connectors,
            PlatformAdministrationProjectionKind::AdminContexts,
            PlatformAdministrationProjectionKind::GovernanceRecords,
        ]
        .into_iter()
        .map(|kind| {
            platform_factory
                .build(kind, write.clone(), read.clone(), config)
                .map_err(configuration_error)
        })
        .collect::<Result<Vec<_>, _>>()?;

        let brand_factory = BrandProjectionFactory;
        for registration in brand_factory.registrations() {
            tracing::info!(
                projection = %registration.name,
                event_types = ?registration.event_types,
                "registered typed Brand CQRS projector"
            );
        }
        let brand = brand_factory
            .build(write.clone(), read.clone(), config)
            .map_err(configuration_error)?;

        let customer_factory = CustomerAuthorityProjectionFactory;
        let customer_registration = customer_factory.registration();
        tracing::info!(
            projection = %customer_registration.name,
            event_types = ?customer_registration.event_types,
            "registered typed Customer Authority CQRS projector"
        );
        let customer_authority = customer_factory
            .build(write.clone(), read.clone(), config)
            .map_err(configuration_error)?;

        let global_factory = GlobalCollaborationProjectionFactory;
        let global_registration = global_factory.registration();
        tracing::info!(
            projection = %global_registration.name,
            event_types = ?global_registration.event_types,
            "registered typed global Collaboration projector"
        );
        let global_collaboration = global_factory
            .build(write, read, config)
            .map_err(configuration_error)?;

        let rebuild_pending = match env::var("ERPX_WAVE5_PROJECTION_MODE")
            .unwrap_or_else(|_| "incremental".into())
            .as_str()
        {
            "incremental" => false,
            "rebuild" => true,
            _ => {
                return Err(WorkerRuntimeError::Configuration(
                    "ERPX_WAVE5_PROJECTION_MODE must be incremental or rebuild".into(),
                ));
            }
        };

        Ok(Self {
            platform,
            brand,
            customer_authority,
            global_collaboration,
            global_write,
            global_read,
            page_size,
            rebuild_pending: AtomicBool::new(rebuild_pending),
        })
    }

    async fn run_platform(&self) -> Result<Vec<ProjectionRun>, WorkerRuntimeError> {
        let mut reports = Vec::with_capacity(self.platform.len());
        for runtime in &self.platform {
            debug_assert!(
                PlatformAdministrationProjectionFactory
                    .registrations()
                    .contains(&runtime.registration())
            );
            reports.push(runtime.run_once().await.map_err(configuration_error)?);
        }
        Ok(reports)
    }

    async fn rebuild_all(&self) -> Result<Vec<ProjectionRun>, WorkerRuntimeError> {
        let mut reports = Vec::new();
        for runtime in &self.platform {
            let handle = runtime.begin_rebuild().await.map_err(configuration_error)?;
            reports.extend(
                run_rebuild_pages(self.page_size, |after| {
                    runtime.run_rebuild_page(&handle, after)
                })
                .await?,
            );
            runtime
                .activate_rebuild(&handle)
                .await
                .map_err(configuration_error)?;
        }

        // Resolved Brand rows depend on the active profile generation, so the
        // profile generation is rebuilt and activated before resolved/history.
        for kind in [
            BrandProjectionKind::Profiles,
            BrandProjectionKind::Resolved,
            BrandProjectionKind::History,
        ] {
            let handle = self
                .brand
                .begin_rebuild(kind)
                .await
                .map_err(configuration_error)?;
            reports.extend(
                run_rebuild_pages(self.page_size, |after| {
                    self.brand.run_rebuild_page(kind, &handle, after)
                })
                .await?,
            );
            self.brand
                .activate_rebuild(kind, &handle)
                .await
                .map_err(configuration_error)?;
        }

        let customer_handle = self
            .customer_authority
            .begin_rebuild()
            .await
            .map_err(configuration_error)?;
        reports.extend(
            run_rebuild_pages(self.page_size, |after| {
                self.customer_authority
                    .run_rebuild_page(&customer_handle, after)
            })
            .await?,
        );
        self.customer_authority
            .activate_rebuild(&customer_handle)
            .await
            .map_err(configuration_error)?;

        let global_handle = self
            .global_collaboration
            .begin_rebuild()
            .await
            .map_err(configuration_error)?;
        reports.extend(
            run_rebuild_pages(self.page_size, |after| {
                self.global_collaboration
                    .run_rebuild_page(&global_handle, after)
            })
            .await?,
        );
        self.global_collaboration
            .activate_rebuild(&global_handle)
            .await
            .map_err(configuration_error)?;
        Ok(reports)
    }
}

#[async_trait]
impl NamedWorkerHandler for Wave5ControlProjectionHandler {
    fn name(&self) -> &str {
        WAVE5_CONTROL_PROJECTION_HANDLER_NAME
    }

    async fn run_once(
        &self,
        _worker_id: uuid::Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let reports = if self.rebuild_pending.swap(false, Ordering::AcqRel) {
            self.rebuild_all().await?
        } else {
            let mut reports = self.run_platform().await?;
            debug_assert_eq!(
                self.brand.registrations(),
                BrandProjectionFactory.registrations()
            );
            for kind in [
                BrandProjectionKind::Profiles,
                BrandProjectionKind::Resolved,
                BrandProjectionKind::History,
            ] {
                reports.push(
                    self.brand
                        .run_once(kind)
                        .await
                        .map_err(configuration_error)?,
                );
            }
            debug_assert_eq!(
                self.customer_authority.registration(),
                CustomerAuthorityProjectionFactory.registration()
            );
            reports.push(
                self.customer_authority
                    .run_once()
                    .await
                    .map_err(configuration_error)?,
            );
            debug_assert_eq!(
                self.global_collaboration.registration(),
                GlobalCollaborationProjectionFactory.registration()
            );
            reports.push(
                self.global_collaboration
                    .run_once()
                    .await
                    .map_err(configuration_error)?,
            );
            reports
        };

        refresh_global_projection_state(&self.global_write, &self.global_read).await?;

        Ok(reports
            .into_iter()
            .fold(WorkerHandlerReport::default(), |mut total, run| {
                total.claimed = total.claimed.saturating_add(u64::from(run.fetched));
                total.processed = total
                    .processed
                    .saturating_add(u64::from(run.applied + run.duplicates));
                total.retried = total.retried.saturating_add(u64::from(run.retry_scheduled));
                total.dead_lettered = total
                    .dead_lettered
                    .saturating_add(u64::from(run.dead_lettered));
                total.has_work |= run.fetched != 0 || run.retry_scheduled;
                total
            }))
    }
}

async fn refresh_global_projection_state(
    write: &PgPool,
    read: &PgPool,
) -> Result<(), WorkerRuntimeError> {
    let source_high_watermark = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(global_sequence),0)::BIGINT
         FROM erpx_cqrs_write.outbox_events WHERE
           (event_type=$1 AND context_name='platform_administration')
           OR (event_type=ANY($2::TEXT[]) AND context_name='collaboration')",
    )
    .bind(ITEM_DIRECTORY_OWNER_EVENT_TYPE)
    .bind([
        GLOBAL_RELATION_DRAFT_EVENT_TYPE,
        GLOBAL_THREAD_SNAPSHOT_EVENT_TYPE,
    ])
    .fetch_one(write)
    .await
    .map_err(database_configuration_error)?;
    let applied_position = sqlx::query(
        "SELECT GREATEST(
           COALESCE((SELECT MAX(source_event_position)
                     FROM erpx_global_collaboration_read.descriptors),0),
           COALESCE((SELECT MAX(committed_sequence)
                     FROM erpx_global_collaboration_read.relations),0),
           COALESCE((SELECT MAX(source_event_position)
                     FROM erpx_global_collaboration_read.thread_projection),0)
         )::BIGINT AS applied_position",
    )
    .fetch_one(read)
    .await
    .map_err(database_configuration_error)?
    .try_get::<i64, _>("applied_position")
    .map_err(database_configuration_error)?;
    let state = if applied_position == source_high_watermark {
        "ready"
    } else {
        "rebuilding"
    };
    sqlx::query(
        "INSERT INTO erpx_global_collaboration_read.projection_state
         (singleton,rebuild_generation,source_high_watermark,applied_position,state,
          last_success_at,updated_at)
         VALUES (TRUE,1,$1,$2,$3,
                 CASE WHEN $3='ready' THEN now() ELSE NULL END,now())
         ON CONFLICT (singleton) DO UPDATE SET
           source_high_watermark=EXCLUDED.source_high_watermark,
           applied_position=EXCLUDED.applied_position,state=EXCLUDED.state,
           last_success_at=CASE WHEN EXCLUDED.state='ready' THEN now()
                                ELSE erpx_global_collaboration_read.projection_state.last_success_at END,
           updated_at=now()",
    )
    .bind(source_high_watermark)
    .bind(applied_position)
    .bind(state)
    .execute(read)
    .await
    .map_err(database_configuration_error)?;
    sqlx::query(
        "UPDATE erpx_global_collaboration.relation_publication_drafts draft
         SET committed_sequence=outbox.global_sequence,projected_at=now()
         FROM erpx_cqrs_write.outbox_events outbox
         WHERE draft.event_id=outbox.event_id
           AND draft.committed_sequence IS NULL
           AND outbox.event_type=$1",
    )
    .bind(GLOBAL_RELATION_DRAFT_EVENT_TYPE)
    .execute(write)
    .await
    .map_err(database_configuration_error)?;
    Ok(())
}

fn database_configuration_error(error: sqlx::Error) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(error.to_string())
}

async fn run_rebuild_pages<'a, F, Fut>(
    page_size: u32,
    mut run_page: F,
) -> Result<Vec<ProjectionRun>, WorkerRuntimeError>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<ProjectionRun, erpx_cqrs_sqlx::CqrsSqlxError>> + 'a,
{
    let mut reports = Vec::new();
    let mut after = 0;
    loop {
        let run = run_page(after).await.map_err(configuration_error)?;
        if run.retry_scheduled || run.dead_lettered != 0 {
            return Err(WorkerRuntimeError::Configuration(
                "Wave 5 projection rebuild scheduled a retry or dead-letter".into(),
            ));
        }
        after = run.cursor;
        let complete = run.fetched < page_size;
        reports.push(run);
        if complete {
            return Ok(reports);
        }
    }
}

fn configuration_error(error: erpx_cqrs_sqlx::CqrsSqlxError) -> WorkerRuntimeError {
    WorkerRuntimeError::Configuration(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_catalog_name_is_stable() {
        assert_eq!(
            WAVE5_CONTROL_PROJECTION_HANDLER_NAME,
            "wave5_control_projections"
        );
    }
}
