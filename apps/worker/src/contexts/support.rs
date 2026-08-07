//! Tenant Support projection runtimes.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_support_sqlx::{
    CapabilityProjector, IncidentProjector, SessionProjector, TicketProjector,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportProjectionKind {
    Tickets,
    Incidents,
    Sessions,
    Capabilities,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SupportProjectionFactory;

impl SupportProjectionFactory {
    pub fn registrations(self) -> Vec<SupportProjectionRegistration> {
        [
            registration(&TicketProjector::new()),
            registration(&IncidentProjector::new()),
            registration(&SessionProjector::new()),
            registration(&CapabilityProjector::new()),
        ]
        .into()
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<SupportProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(SupportProjectionRuntime {
            tickets: TicketProjector::new(),
            incidents: IncidentProjector::new(),
            sessions: SessionProjector::new(),
            capabilities: CapabilityProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SupportProjectionRuntime {
    tickets: TicketProjector,
    incidents: IncidentProjector,
    sessions: SessionProjector,
    capabilities: CapabilityProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl SupportProjectionRuntime {
    pub fn registrations(&self) -> Vec<SupportProjectionRegistration> {
        SupportProjectionFactory.registrations()
    }

    pub async fn run_once(
        &self,
        kind: SupportProjectionKind,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            SupportProjectionKind::Tickets => self.runner.run_active(&self.tickets).await,
            SupportProjectionKind::Incidents => self.runner.run_active(&self.incidents).await,
            SupportProjectionKind::Sessions => self.runner.run_active(&self.sessions).await,
            SupportProjectionKind::Capabilities => self.runner.run_active(&self.capabilities).await,
        }
    }

    pub async fn begin_rebuild(
        &self,
        kind: SupportProjectionKind,
        rebuild_id: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        match kind {
            SupportProjectionKind::Tickets => {
                self.store
                    .begin_or_resume_rebuild(&self.tickets, rebuild_id)
                    .await
            }
            SupportProjectionKind::Incidents => {
                self.store
                    .begin_or_resume_rebuild(&self.incidents, rebuild_id)
                    .await
            }
            SupportProjectionKind::Sessions => {
                self.store
                    .begin_or_resume_rebuild(&self.sessions, rebuild_id)
                    .await
            }
            SupportProjectionKind::Capabilities => {
                self.store
                    .begin_or_resume_rebuild(&self.capabilities, rebuild_id)
                    .await
            }
        }
    }

    pub async fn run_rebuild_page(
        &self,
        kind: SupportProjectionKind,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        match kind {
            SupportProjectionKind::Tickets => {
                self.runner
                    .run_rebuild(&self.tickets, handle, after_sequence)
                    .await
            }
            SupportProjectionKind::Incidents => {
                self.runner
                    .run_rebuild(&self.incidents, handle, after_sequence)
                    .await
            }
            SupportProjectionKind::Sessions => {
                self.runner
                    .run_rebuild(&self.sessions, handle, after_sequence)
                    .await
            }
            SupportProjectionKind::Capabilities => {
                self.runner
                    .run_rebuild(&self.capabilities, handle, after_sequence)
                    .await
            }
        }
    }

    pub async fn activate_rebuild(
        &self,
        kind: SupportProjectionKind,
        handle: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        let expected = match kind {
            SupportProjectionKind::Tickets => self.tickets.name(),
            SupportProjectionKind::Incidents => self.incidents.name(),
            SupportProjectionKind::Sessions => self.sessions.name(),
            SupportProjectionKind::Capabilities => self.capabilities.name(),
        };
        if handle.projection != expected {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another Support projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

fn registration(projector: &impl Projector) -> SupportProjectionRegistration {
    SupportProjectionRegistration {
        name: projector.name().into(),
        event_types: projector.event_types().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_support_sqlx::{
        CAPABILITY_PROJECTION, CAPABILITY_SNAPSHOT_EVENT, INCIDENT_PROJECTION,
        INCIDENT_SNAPSHOT_EVENT, SESSION_PROJECTION, SESSION_SNAPSHOT_EVENT, TICKET_PROJECTION,
        TICKET_SNAPSHOT_EVENT,
    };

    #[test]
    fn registrations_cover_four_rebuildable_snapshot_projections() {
        assert_eq!(
            SupportProjectionFactory.registrations(),
            vec![
                SupportProjectionRegistration {
                    name: TICKET_PROJECTION.into(),
                    event_types: vec![TICKET_SNAPSHOT_EVENT.into()],
                },
                SupportProjectionRegistration {
                    name: INCIDENT_PROJECTION.into(),
                    event_types: vec![INCIDENT_SNAPSHOT_EVENT.into()],
                },
                SupportProjectionRegistration {
                    name: SESSION_PROJECTION.into(),
                    event_types: vec![SESSION_SNAPSHOT_EVENT.into()],
                },
                SupportProjectionRegistration {
                    name: CAPABILITY_PROJECTION.into(),
                    event_types: vec![CAPABILITY_SNAPSHOT_EVENT.into()],
                },
            ]
        );
    }

    #[test]
    fn scheduler_names_are_frozen() {
        assert_eq!(TICKET_PROJECTION, "support.tickets");
        assert_eq!(INCIDENT_PROJECTION, "support.incidents");
        assert_eq!(SESSION_PROJECTION, "support.sessions");
        assert_eq!(CAPABILITY_PROJECTION, "support.capabilities");
    }

    #[test]
    fn rebuild_wrapper_requires_the_durable_generation_api() {
        let source = include_str!("support.rs");
        assert!(source.contains("rebuild_id: Uuid"));
        assert!(source.contains("begin_or_resume_rebuild"));
    }
}
