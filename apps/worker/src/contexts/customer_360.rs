//! Customer 360 control-source and holding fan-in projection runtime adapters.
//!
//! Composition owns pool discovery and scheduler registration. This module
//! never compares global sequences from different holding write databases.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, MultiSourceProjectionRun, MultiSourceProjectionRunner, ProjectionPool,
    ProjectionRun, ProjectionRunner, ProjectionRunnerConfig, ProjectionSource,
    ProjectionSourceFence, ProjectionSourceKey, ProjectionStore, Projector, RebuildHandle,
    WritePool,
};
use erpx_customer_360_sqlx::{ControlCustomerProjector, HoldingActivityProjector};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360ProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
    pub multi_source: bool,
}

#[derive(Debug, Clone)]
pub struct HoldingActivitySource {
    pub holding_id: Uuid,
    pub write_pool: WritePool,
}

impl HoldingActivitySource {
    fn into_runner_source(self) -> Result<(ProjectionSourceKey, ProjectionSource), CqrsSqlxError> {
        if self.holding_id.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "Customer 360 holding source id is nil",
            ));
        }
        Ok((
            ProjectionSourceKey::try_new(format!("holding:{}", self.holding_id))?,
            ProjectionSource::new(self.write_pool),
        ))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Customer360ProjectionFactory;

impl Customer360ProjectionFactory {
    pub fn registrations(self) -> Vec<Customer360ProjectionRegistration> {
        let control = ControlCustomerProjector::new();
        let holding = HoldingActivityProjector::new();
        vec![
            Customer360ProjectionRegistration {
                name: control.name().into(),
                event_types: control.event_types().to_vec(),
                multi_source: false,
            },
            Customer360ProjectionRegistration {
                name: holding.name().into(),
                event_types: holding.event_types().to_vec(),
                multi_source: true,
            },
        ]
    }

    pub fn build_control(
        self,
        control_write: WritePool,
        control_projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<Customer360ControlProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(control_projection);
        let runner =
            ProjectionRunner::new(ProjectionSource::new(control_write), store.clone(), config)?;
        Ok(Customer360ControlProjectionRuntime {
            projector: ControlCustomerProjector::new(),
            runner,
            store,
        })
    }

    pub fn build_holding(
        self,
        control_projection: ProjectionPool,
        holding_sources: Vec<HoldingActivitySource>,
        config: ProjectionRunnerConfig,
    ) -> Result<Customer360HoldingProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(control_projection);
        let sources = holding_sources
            .into_iter()
            .map(HoldingActivitySource::into_runner_source)
            .collect::<Result<Vec<_>, _>>()?;
        let runner = MultiSourceProjectionRunner::new(sources, store.clone(), config)?;
        Ok(Customer360HoldingProjectionRuntime {
            projector: HoldingActivityProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Customer360ControlProjectionRuntime {
    projector: ControlCustomerProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl Customer360ControlProjectionRuntime {
    pub async fn run_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    pub async fn begin_or_resume_rebuild(
        &self,
        rebuild_generation: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store
            .begin_or_resume_rebuild(&self.projector, rebuild_generation)
            .await
    }

    pub async fn rebuild_cursor(&self, generation: Uuid) -> Result<Option<u64>, CqrsSqlxError> {
        self.store
            .generation(self.projector.name(), generation)
            .await
            .map(|state| state.map(|state| state.source_sequence))
    }

    pub async fn generation_is_active(&self, generation: Uuid) -> Result<bool, CqrsSqlxError> {
        self.store
            .generation(self.projector.name(), generation)
            .await
            .map(|state| state.is_some_and(|state| state.active))
    }

    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner
            .run_rebuild(&self.projector, handle, after_sequence)
            .await
    }

    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        if handle.projection != self.projector.name() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Customer 360 projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

#[derive(Debug, Clone)]
pub struct Customer360HoldingProjectionRuntime {
    projector: HoldingActivityProjector,
    runner: MultiSourceProjectionRunner,
    store: ProjectionStore,
}

impl Customer360HoldingProjectionRuntime {
    pub fn source_keys(&self) -> Vec<String> {
        self.runner
            .source_keys()
            .map(|key| key.as_str().to_owned())
            .collect()
    }

    pub async fn run_once(&self) -> Result<MultiSourceProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.projector).await
    }

    pub async fn capture_rebuild_fences(
        &self,
    ) -> Result<Vec<ProjectionSourceFence>, CqrsSqlxError> {
        self.runner.capture_source_fences(&self.projector).await
    }

    pub async fn persisted_rebuild_fences(
        &self,
        generation: Uuid,
    ) -> Result<Vec<ProjectionSourceFence>, CqrsSqlxError> {
        self.store
            .source_rebuild_fences(self.projector.name(), generation)
            .await
    }

    pub async fn generation_is_active(&self, generation: Uuid) -> Result<bool, CqrsSqlxError> {
        self.store
            .generation(self.projector.name(), generation)
            .await
            .map(|state| state.is_some_and(|state| state.active))
    }

    /// The scheduler must persist `fences` beside its durable rebuild id and
    /// pass the exact same set on retry. Recapturing would move the rebuild
    /// target and is intentionally not hidden by this API.
    pub async fn begin_or_resume_rebuild(
        &self,
        rebuild_generation: Uuid,
        fences: &[ProjectionSourceFence],
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store
            .begin_or_resume_multi_source_rebuild(&self.projector, rebuild_generation, fences)
            .await
    }

    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
    ) -> Result<MultiSourceProjectionRun, CqrsSqlxError> {
        self.runner.run_rebuild(&self.projector, handle).await
    }

    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        if handle.projection != self.projector.name() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Customer 360 projector",
            ));
        }
        self.store.activate_multi_source_rebuild(handle).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_customer_360_sqlx::{
        CONTROL_CUSTOMERS_PROJECTION_NAME, CONTROL_PROFILE_EVENT, HOLDING_ACTIVITY_PROJECTION_NAME,
        OPPORTUNITY_EVENT, SALES_ORDER_EVENT, SUBSCRIPTION_EVENT, SUPPORT_CASE_EVENT,
    };

    #[test]
    fn registration_inventory_freezes_both_source_topologies() {
        assert_eq!(
            Customer360ProjectionFactory.registrations(),
            vec![
                Customer360ProjectionRegistration {
                    name: CONTROL_CUSTOMERS_PROJECTION_NAME.into(),
                    event_types: vec![CONTROL_PROFILE_EVENT.into()],
                    multi_source: false,
                },
                Customer360ProjectionRegistration {
                    name: HOLDING_ACTIVITY_PROJECTION_NAME.into(),
                    event_types: vec![
                        SUBSCRIPTION_EVENT.into(),
                        OPPORTUNITY_EVENT.into(),
                        SALES_ORDER_EVENT.into(),
                        SUPPORT_CASE_EVENT.into(),
                    ],
                    multi_source: true,
                },
            ]
        );
    }

    #[test]
    fn runtime_source_uses_frozen_fences_and_multi_source_activation() {
        let source = include_str!("customer_360.rs");
        for required in [
            "capture_source_fences",
            "begin_or_resume_multi_source_rebuild",
            "run_rebuild(&self.projector",
            "activate_multi_source_rebuild",
            "source_rebuild_fences",
        ] {
            assert!(source.contains(required), "missing {required}");
        }
        assert!(!source.contains(concat!("Uuid::", "new_v4()")));
    }
}
