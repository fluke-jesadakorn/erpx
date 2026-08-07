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
use erpx_cqrs_sqlx::{ProjectionPool, ProjectionRun, ProjectionRunnerConfig, WritePool};
use erpx_infrastructure::SqlxControlPlane;
use erpx_worker::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

use crate::contexts::{
    brand::{BrandProjectionFactory, BrandProjectionKind, BrandProjectionRuntime},
    customer_authority::{CustomerAuthorityProjectionFactory, CustomerAuthorityProjectionRuntime},
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
    page_size: u32,
    rebuild_pending: AtomicBool,
}

impl Wave5ControlProjectionHandler {
    pub fn new(control: Arc<SqlxControlPlane>, page_size: u32) -> Result<Self, WorkerRuntimeError> {
        let write = WritePool::from_pg_pool(control.pools().write.clone());
        let read = ProjectionPool::from_pg_pool(control.pools().read.clone());
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
            PlatformAdministrationProjectionKind::Tenants,
            PlatformAdministrationProjectionKind::Connectors,
            PlatformAdministrationProjectionKind::AdminContexts,
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
            reports
        };

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
