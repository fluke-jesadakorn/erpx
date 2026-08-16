//! Operations read-model runtimes and typed source-event process manager.

use std::{future::Future, pin::Pin};

use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_operations::{
    OperationsApplicationError, PublishedSourceEvent, SourceFactMessage, SourceHandlingContext,
    SourceHandlingResult, SourcePosition,
};
use erpx_operations_sqlx::{
    OperationsExceptionProjector, OperationsLifecycleRecordProjector,
    OperationsSourceSignalProjector, OperationsWorkQueuePolicyProjector,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsProjectionKind {
    Exceptions,
    SourceSignals,
    WorkQueuePolicies,
    LifecycleRecords,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OperationsProjectionFactory;

impl OperationsProjectionFactory {
    pub fn registrations(self) -> Vec<OperationsProjectionRegistration> {
        [
            registration(&OperationsExceptionProjector::new()),
            registration(&OperationsSourceSignalProjector::new()),
            registration(&OperationsWorkQueuePolicyProjector::new()),
            registration(&OperationsLifecycleRecordProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<OperationsProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(OperationsProjectionRuntime {
            exceptions: OperationsExceptionProjector::new(),
            source_signals: OperationsSourceSignalProjector::new(),
            policies: OperationsWorkQueuePolicyProjector::new(),
            lifecycle_records: OperationsLifecycleRecordProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OperationsProjectionRuntime {
    exceptions: OperationsExceptionProjector,
    source_signals: OperationsSourceSignalProjector,
    policies: OperationsWorkQueuePolicyProjector,
    lifecycle_records: OperationsLifecycleRecordProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl OperationsProjectionRuntime {
    pub fn registrations(&self) -> Vec<OperationsProjectionRegistration> {
        OperationsProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: OperationsProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            OperationsProjectionKind::Exceptions => self.runner.run_active(&self.exceptions).await,
            OperationsProjectionKind::SourceSignals => {
                self.runner.run_active(&self.source_signals).await
            }
            OperationsProjectionKind::WorkQueuePolicies => {
                self.runner.run_active(&self.policies).await
            }
            OperationsProjectionKind::LifecycleRecords => {
                self.runner.run_active(&self.lifecycle_records).await
            }
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: OperationsProjectionKind,
        rebuild_id: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            OperationsProjectionKind::Exceptions => {
                self.store
                    .begin_or_resume_rebuild(&self.exceptions, rebuild_id)
                    .await
            }
            OperationsProjectionKind::SourceSignals => {
                self.store
                    .begin_or_resume_rebuild(&self.source_signals, rebuild_id)
                    .await
            }
            OperationsProjectionKind::WorkQueuePolicies => {
                self.store
                    .begin_or_resume_rebuild(&self.policies, rebuild_id)
                    .await
            }
            OperationsProjectionKind::LifecycleRecords => {
                self.store
                    .begin_or_resume_rebuild(&self.lifecycle_records, rebuild_id)
                    .await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: OperationsProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            OperationsProjectionKind::Exceptions => {
                self.runner
                    .run_rebuild(&self.exceptions, handle, after_sequence)
                    .await
            }
            OperationsProjectionKind::SourceSignals => {
                self.runner
                    .run_rebuild(&self.source_signals, handle, after_sequence)
                    .await
            }
            OperationsProjectionKind::WorkQueuePolicies => {
                self.runner
                    .run_rebuild(&self.policies, handle, after_sequence)
                    .await
            }
            OperationsProjectionKind::LifecycleRecords => {
                self.runner
                    .run_rebuild(&self.lifecycle_records, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn rebuild_cursor(
        &self,
        kind: OperationsProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<u64, CqrsSqlxError> {
        let expected = match kind {
            OperationsProjectionKind::Exceptions => self.exceptions.name(),
            OperationsProjectionKind::SourceSignals => self.source_signals.name(),
            OperationsProjectionKind::WorkQueuePolicies => self.policies.name(),
            OperationsProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Operations projector",
            ));
        }
        self.store
            .generation(expected, handle.generation)
            .await?
            .filter(|state| !state.active)
            .map(|state| state.source_sequence)
            .ok_or(CqrsSqlxError::MissingGeneration)
    }

    pub async fn activate_rebuild(
        &self,
        kind: OperationsProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            OperationsProjectionKind::Exceptions => self.exceptions.name(),
            OperationsProjectionKind::SourceSignals => self.source_signals.name(),
            OperationsProjectionKind::WorkQueuePolicies => self.policies.name(),
            OperationsProjectionKind::LifecycleRecords => self.lifecycle_records.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Operations projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> OperationsProjectionRegistration {
    OperationsProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

/// Supplies trusted actor/scope/audit state at the internal worker boundary.
/// The source fact itself always comes from the typed integration envelope.
#[async_trait]
pub trait OperationsSourceContextFactory: Send + Sync {
    async fn build(
        &self,
        source: SourceFactMessage,
        position: SourcePosition,
    ) -> Result<SourceHandlingContext, OperationsApplicationError>;
}

#[async_trait]
pub trait OperationsSourceApplication: Send + Sync {
    async fn consume(
        &self,
        context: SourceHandlingContext,
    ) -> Result<SourceHandlingResult, OperationsApplicationError>;
}

/// Small composition adapter used to wrap the concrete
/// `OperationsSourceHandler<OperationsSourceAdapter, ..>` without erasing its
/// typed result or exposing persistence to the process manager.
pub struct OperationsSourceApplicationFn<F> {
    dispatch: F,
}

pub type BoxedOperationsSourceFuture = Pin<
    Box<
        dyn Future<Output = Result<SourceHandlingResult, OperationsApplicationError>>
            + Send
            + 'static,
    >,
>;
pub type BoxedOperationsSourceDispatch =
    Box<dyn Fn(SourceHandlingContext) -> BoxedOperationsSourceFuture + Send + Sync + 'static>;
pub type BoxedOperationsSourceApplication =
    OperationsSourceApplicationFn<BoxedOperationsSourceDispatch>;

impl<F> OperationsSourceApplicationFn<F> {
    pub const fn new(dispatch: F) -> Self {
        Self { dispatch }
    }
}

#[async_trait]
impl<F, Fut> OperationsSourceApplication for OperationsSourceApplicationFn<F>
where
    F: Fn(SourceHandlingContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<SourceHandlingResult, OperationsApplicationError>> + Send,
{
    async fn consume(
        &self,
        context: SourceHandlingContext,
    ) -> Result<SourceHandlingResult, OperationsApplicationError> {
        (self.dispatch)(context).await
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OperationsSourceProcessOutcome {
    Irrelevant,
    Applied(Box<SourceHandlingResult>),
}

pub struct OperationsSourceProcessManager<A, C> {
    application: A,
    context_factory: C,
}

impl<A, C> OperationsSourceProcessManager<A, C> {
    pub const fn new(application: A, context_factory: C) -> Self {
        Self {
            application,
            context_factory,
        }
    }
}

impl<A, C> OperationsSourceProcessManager<A, C>
where
    A: OperationsSourceApplication,
    C: OperationsSourceContextFactory,
{
    pub async fn handle(
        &self,
        event: PublishedSourceEvent,
        position: SourcePosition,
    ) -> Result<OperationsSourceProcessOutcome, OperationsApplicationError> {
        let Some(source) = event.try_into_message()? else {
            return Ok(OperationsSourceProcessOutcome::Irrelevant);
        };
        source.validate()?;
        position.validate(&source)?;
        let context = self
            .context_factory
            .build(source.clone(), position.clone())
            .await?;
        let actor = serde_json::to_value(&context.actor)
            .map_err(|_| OperationsApplicationError::Port(erpx_operations::PortError::Denied))?;
        if context.source != source
            || context.position != position
            || actor.get("kind").and_then(serde_json::Value::as_str) != Some("system_worker")
        {
            return Err(OperationsApplicationError::Port(
                erpx_operations::PortError::Denied,
            ));
        }
        let result = self.application.consume(context).await?;
        Ok(OperationsSourceProcessOutcome::Applied(Box::new(result)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_operations_sqlx::{
        EXCEPTION_PROJECTION, EXCEPTION_SNAPSHOT_EVENT, LIFECYCLE_RECORD_PROJECTION_NAME,
        LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE, POLICY_PROJECTION, POLICY_SNAPSHOT_EVENT,
        SOURCE_SIGNAL_PROJECTION,
    };

    #[test]
    fn registrations_cover_all_rebuildable_operations_projections() {
        assert_eq!(
            OperationsProjectionFactory.registrations(),
            vec![
                OperationsProjectionRegistration {
                    name: EXCEPTION_PROJECTION.into(),
                    event_types: vec![EXCEPTION_SNAPSHOT_EVENT.into()],
                },
                OperationsProjectionRegistration {
                    name: SOURCE_SIGNAL_PROJECTION.into(),
                    event_types: vec![EXCEPTION_SNAPSHOT_EVENT.into()],
                },
                OperationsProjectionRegistration {
                    name: POLICY_PROJECTION.into(),
                    event_types: vec![POLICY_SNAPSHOT_EVENT.into()],
                },
                OperationsProjectionRegistration {
                    name: LIFECYCLE_RECORD_PROJECTION_NAME.into(),
                    event_types: vec![LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE.into()],
                },
            ]
        );
    }

    #[test]
    fn source_process_manager_has_no_generic_json_or_public_rpc_path() {
        let source = include_str!("operations.rs");
        assert!(source.contains("PublishedSourceEvent"));
        assert!(source.contains("SourcePosition"));
        assert!(source.contains("try_into_message"));
        assert!(source.contains("system_worker"));
        assert!(!source.contains(concat!("payload", "_json")));
        assert!(!source.contains(concat!("CreateOperationsException", "FromSource")));
    }
}
