//! Identity & Access projection runtime adapter.
//!
//! Composition supplies capability-separated pools. This module neither
//! creates connections nor depends on the Server runtime.

use erpx_cqrs_sqlx::{
    CqrsSqlxError, ProjectionPool, ProjectionRun, ProjectionRunner, ProjectionRunnerConfig,
    ProjectionSource, ProjectionStore, Projector, RebuildHandle, WritePool,
};
use erpx_identity_access_sqlx::{IdentityAccessPrincipalProjector, IdentityAccessRoleProjector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAccessProjectionRegistration {
    pub name: String,
    pub event_types: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityAccessProjectionFactory;

impl IdentityAccessProjectionFactory {
    pub fn registrations(self) -> Vec<IdentityAccessProjectionRegistration> {
        let principals = IdentityAccessPrincipalProjector::new();
        let roles = IdentityAccessRoleProjector::new();
        vec![
            IdentityAccessProjectionRegistration {
                name: principals.name().into(),
                event_types: principals.event_types().to_vec(),
            },
            IdentityAccessProjectionRegistration {
                name: roles.name().into(),
                event_types: roles.event_types().to_vec(),
            },
        ]
    }

    pub fn build(
        self,
        write: WritePool,
        projection: ProjectionPool,
        config: ProjectionRunnerConfig,
    ) -> Result<IdentityAccessProjectionRuntime, CqrsSqlxError> {
        let store = ProjectionStore::new(projection);
        let runner = ProjectionRunner::new(ProjectionSource::new(write), store.clone(), config)?;
        Ok(IdentityAccessProjectionRuntime {
            principals: IdentityAccessPrincipalProjector::new(),
            roles: IdentityAccessRoleProjector::new(),
            runner,
            store,
        })
    }
}

#[derive(Debug, Clone)]
pub struct IdentityAccessProjectionRuntime {
    principals: IdentityAccessPrincipalProjector,
    roles: IdentityAccessRoleProjector,
    runner: ProjectionRunner,
    store: ProjectionStore,
}

impl IdentityAccessProjectionRuntime {
    pub fn registrations(&self) -> Vec<IdentityAccessProjectionRegistration> {
        IdentityAccessProjectionFactory.registrations()
    }

    pub async fn run_principals_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.principals).await
    }

    pub async fn run_roles_once(&self) -> Result<ProjectionRun, CqrsSqlxError> {
        self.runner.run_active(&self.roles).await
    }

    pub async fn begin_principal_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.principals).await
    }

    pub async fn begin_role_rebuild(&self) -> Result<RebuildHandle, CqrsSqlxError> {
        self.store.begin_rebuild(&self.roles).await
    }

    pub async fn run_rebuild_page(
        &self,
        handle: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        if handle.projection == self.principals.name() {
            self.runner
                .run_rebuild(&self.principals, handle, after_sequence)
                .await
        } else if handle.projection == self.roles.name() {
            self.runner
                .run_rebuild(&self.roles, handle, after_sequence)
                .await
        } else {
            Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another projector",
            ))
        }
    }

    pub async fn activate_rebuild(&self, handle: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        if handle.projection != self.principals.name() && handle.projection != self.roles.name() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another projector",
            ));
        }
        self.store.activate_rebuild(handle).await
    }
}

#[cfg(test)]
mod tests {
    use erpx_identity_access_sqlx::{
        PRINCIPAL_PROJECTION_NAME, PRINCIPAL_SNAPSHOT_EVENT_TYPE, ROLE_PROJECTION_NAME,
        ROLE_SNAPSHOT_EVENT_TYPE,
    };

    use super::*;

    #[test]
    fn registrations_are_typed_and_snapshot_only() {
        let registrations = IdentityAccessProjectionFactory.registrations();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].name, PRINCIPAL_PROJECTION_NAME);
        assert_eq!(
            registrations[0].event_types,
            vec![PRINCIPAL_SNAPSHOT_EVENT_TYPE]
        );
        assert_eq!(registrations[1].name, ROLE_PROJECTION_NAME);
        assert_eq!(registrations[1].event_types, vec![ROLE_SNAPSHOT_EVENT_TYPE]);
    }

    #[tokio::test]
    async fn wrong_rebuild_handle_fails_before_database_access() {
        let write = WritePool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/identity_write").unwrap(),
        );
        let projection = ProjectionPool::from_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/identity_read").unwrap(),
        );
        let runtime = IdentityAccessProjectionFactory
            .build(write, projection, ProjectionRunnerConfig::default())
            .unwrap();
        let handle = RebuildHandle {
            projection: "another.projection".into(),
            generation: uuid::Uuid::new_v4(),
            replaces: None,
        };
        let result = runtime.run_rebuild_page(&handle, 0).await;
        assert!(matches!(
            result,
            Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another projector"
            ))
        ));
    }
}
