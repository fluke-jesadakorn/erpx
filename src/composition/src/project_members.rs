//! Composition backend for the native Projects member roster.
use erpx_server_projects::public::project_members::{
    PgProjectMembers, ProjectMemberRow, ProjectMemberScope, read_project_members,
};
use erpx_server_projects::public::project_members_http::{
    ProjectMemberHttpBackend, ProjectMemberHttpFuture, ProjectMemberHttpRequest,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;

pub struct ProjectMemberBackend {
    pool: DatabasePool,
}

impl ProjectMemberBackend {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ProjectMemberHttpBackend for ProjectMemberBackend {
    fn read<'a>(
        &'a self,
        request: ProjectMemberHttpRequest<'a>,
    ) -> ProjectMemberHttpFuture<'a, Vec<ProjectMemberRow>> {
        Box::pin(async move {
            let scope = ProjectMemberScope::new(request.company, request.project_id)?;
            let repository = PgProjectMembers::new(self.pool.clone());
            read_project_members(&scope, &repository).await
        })
    }
}

pub fn target(
    host: String,
    holding_slug: String,
    pool: DatabasePool,
) -> erpx_server_projects::public::project_members_http::ProjectMemberHttpTarget {
    erpx_server_projects::public::project_members_http::ProjectMemberHttpTarget {
        origin: format!("https://{host}"),
        host,
        holding_slug,
        backend: Arc::new(ProjectMemberBackend::new(pool)),
    }
}
