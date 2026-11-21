//! Same-Control retained source constraints for the ordinary Work coordinator.
use super::{
    authorized_work::{RetainedWorkControlSources, RetainedWorkHoldingSources, WorkControlSources},
    directory_postgres::{
        DirectoryRoute, PostgresDirectory, connection, identity, matches_descriptor, unavailable,
    },
    directory_work::DirectoryWorkPersistence,
};
use crate::{
    application::directory_refresh::{
        DirectoryObservationRetention, DirectoryRequestPreflight, PreparedDirectoryWork,
    },
    domain::{
        account_auth::AccountSession,
        directory::{ApprovedDirectoryIdentity, DirectoryConnectionDescriptor},
        work_context::{HoldingWorkspaceRequest, WorkContextRequest},
    },
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_shared::public::{BusinessError, FailureKind};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

pub(super) struct DirectoryControlSources {
    persistence: Arc<DirectoryWorkPersistence>,
    original: DirectoryRequestPreflight,
    retention: Arc<dyn DirectoryObservationRetention>,
}
impl DirectoryControlSources {
    pub(super) fn new(
        persistence: Arc<DirectoryWorkPersistence>,
        prepared: PreparedDirectoryWork,
    ) -> Result<Self, BusinessError> {
        let source = prepared
            .preflight
            .directory
            .as_ref()
            .ok_or_else(unavailable)?;
        let retention = prepared.retention.ok_or_else(unavailable)?;
        if retention.fingerprint() != source.descriptor.profile_fingerprint {
            return Err(unavailable());
        }
        Ok(Self {
            persistence,
            original: prepared.preflight,
            retention,
        })
    }
    pub(super) fn workspace(&self) -> &HoldingWorkspaceRequest {
        &self.original.workspace
    }
}
struct ControlRetention {
    original: DirectoryRequestPreflight,
    retention: Arc<dyn DirectoryObservationRetention>,
    route: Arc<DirectoryRoute>,
}
impl WorkControlSources for DirectoryControlSources {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            PostgresDirectory::lock_routes(c).await?;
            Ok(())
        })
    }
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        session: &'c AccountSession,
        input: &'c HoldingWorkspaceRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn RetainedWorkControlSources>, BusinessError> {
        Box::pin(async move {
            require_session(session, &self.original, now)?;
            require_workspace(input, &self.original.workspace)?;
            let source = self.original.directory.as_ref().ok_or_else(unavailable)?;
            if self.retention.fingerprint() != source.descriptor.profile_fingerprint {
                return Err(unavailable().into());
            }
            let route = self
                .persistence
                .directory
                .route(c, &source.descriptor.connection_id)
                .await?;
            route.matches(&source.descriptor)?;
            Ok(Arc::new(ControlRetention {
                original: self.original.clone(),
                retention: self.retention.clone(),
                route: Arc::new(route),
            }) as Arc<dyn RetainedWorkControlSources>)
        })
    }
}
impl RetainedWorkControlSources for ControlRetention {
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            sqlx::query("LOCK TABLE iam_mappings IN SHARE MODE")
                .execute(c)
                .await?;
            Ok(())
        })
    }
    fn retain_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        request: &'c WorkContextRequest,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn RetainedWorkHoldingSources>, BusinessError> {
        Box::pin(async move {
            if request.session_id != self.original.session_id
                || request.user_id != self.original.user_id
                || request.company_id != self.original.workspace.company_id
                || request.session_expires_at != self.original.session_expires_at
            {
                return Err(changed().into());
            }
            let source = self.original.directory.as_ref().ok_or_else(unavailable)?;
            matches_descriptor(
                &connection(c, &source.descriptor.connection_id, &self.route).await?,
                &source.descriptor,
            )?;
            let facts = identity(c, &source.descriptor, &source.external_id, now).await?;
            if facts.user_id != self.original.user_id.as_str()
                || facts.identity_id != source.identity_id
                || facts.member_version != source.member_version
                || facts.principal_id != source.principal_id
            {
                return Err(changed().into());
            }
            let assignment = request
                .expected
                .as_ref()
                .ok_or_else(changed)?
                .assignment_id
                .as_str();
            let mappings = mapping_witness(
                c,
                assignment,
                request.company_id.as_str(),
                request.user_id.as_str(),
                now,
            )
            .await?;
            let deadline = facts
                .observed_at
                .checked_add(300000)
                .ok_or_else(unavailable)?
                .min(self.original.request_deadline);
            if deadline <= now {
                return Err(changed().into());
            }
            Ok(Box::new(HoldingRetention {
                descriptor: source.descriptor.clone(),
                route: self.route.clone(),
                external_id: source.external_id.clone(),
                facts,
                assignment: assignment.to_owned(),
                mappings,
                deadline,
            }) as Box<dyn RetainedWorkHoldingSources>)
        })
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if now >= self.original.request_deadline {
                return Err(changed().into());
            }
            let source = self.original.directory.as_ref().ok_or_else(unavailable)?;
            if self.retention.fingerprint() != source.descriptor.profile_fingerprint {
                return Err(unavailable().into());
            }
            self.route.require_current(c).await
        })
    }
}
struct HoldingRetention {
    descriptor: DirectoryConnectionDescriptor,
    route: Arc<DirectoryRoute>,
    external_id: String,
    facts: ApprovedDirectoryIdentity,
    assignment: String,
    mappings: String,
    deadline: i64,
}
impl RetainedWorkHoldingSources for HoldingRetention {
    fn deadline(&self) -> i64 {
        self.deadline
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if now >= self.deadline {
                return Err(changed().into());
            }
            // No new pool or transaction: only the existing Holding lease is used.
            matches_descriptor(
                &connection(c, &self.descriptor.connection_id, self.route.as_ref()).await?,
                &self.descriptor,
            )?;
            let current = identity(c, &self.descriptor, &self.external_id, now).await?;
            if !same_identity(&self.facts, &current)
                || mapping_witness(
                    c,
                    &self.assignment,
                    &self.route.company_id,
                    &self.facts.user_id,
                    now,
                )
                .await?
                    != self.mappings
            {
                return Err(changed().into());
            }
            Ok(())
        })
    }
}
fn same_identity(a: &ApprovedDirectoryIdentity, b: &ApprovedDirectoryIdentity) -> bool {
    a.user_id == b.user_id
        && a.principal_id == b.principal_id
        && a.person_id == b.person_id
        && a.identity_id == b.identity_id
        && a.member_version == b.member_version
        && a.observed_at == b.observed_at
        && a.connection_version == b.connection_version
        && a.identity_enabled == b.identity_enabled
        && a.member_active == b.member_active
        && a.member_directory == b.member_directory
        && a.identity_user_id == b.identity_user_id
        && a.identity_company_id == b.identity_company_id
        && a.member_company_id == b.member_company_id
        && a.identity_connection_id == b.identity_connection_id
}
fn require_session(
    actual: &AccountSession,
    original: &DirectoryRequestPreflight,
    now: i64,
) -> Result<(), BusinessError> {
    if actual.id != original.session_id.as_str()
        || actual.user.id != original.user_id.as_str()
        || actual.authenticated_at != original.authenticated_at
        || actual.expires_at != original.session_expires_at
        || now >= original.request_deadline
    {
        return Err(changed());
    }
    Ok(())
}
fn require_workspace(
    a: &HoldingWorkspaceRequest,
    b: &HoldingWorkspaceRequest,
) -> Result<(), BusinessError> {
    let x = a.expected.as_ref().ok_or_else(changed)?;
    let y = b.expected.as_ref().ok_or_else(changed)?;
    if a.company_id != b.company_id
        || a.branch_slug != b.branch_slug
        || a.department_slug != b.department_slug
        || a.mutation != b.mutation
        || x.assignment_id != y.assignment_id
        || x.assignment_version != y.assignment_version
        || x.revision != y.revision
    {
        return Err(changed());
    }
    Ok(())
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your original account or work context changed. Review it before retrying.",
    )
}
async fn mapping_witness(
    c: &mut PgConnection,
    assignment: &str,
    company: &str,
    user: &str,
    now: i64,
) -> super::directory_postgres::Tx<String> {
    let mut rows = sqlx::query(MAPPINGS)
        .bind(assignment)
        .bind(company)
        .bind(user)
        .bind(now)
        .fetch(c);
    let (mut count, mut bytes) = (0usize, 0usize);
    let mut digest = Sha256::new();
    while let Some(row) = rows.try_next().await? {
        count += 1;
        let value: Option<String> = row.try_get("evidence")?;
        let value = value.ok_or_else(unavailable)?;
        bytes = bytes.checked_add(value.len()).ok_or_else(unavailable)?;
        if count > 32
            || bytes > 4 * 1024 * 1024
            || row.try_get::<Option<bool>, _>("eligible")? != Some(true)
        {
            return Err(changed().into());
        }
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    Ok(hex::encode(digest.finalize()))
}
const MAPPINGS: &str = "WITH RECURSIVE chain AS(SELECT a.*,ARRAY[a.id] path FROM assignments a WHERE a.id=$1 AND a.company_id=$2 AND a.user_id=$3 UNION ALL SELECT p.*,c.path||p.id FROM chain c JOIN assignments p ON p.id=c.parent_id WHERE cardinality(c.path)<32 AND NOT p.id=ANY(c.path)) SELECT (m.id IS NOT NULL AND m.status='active' AND m.valid_until>$4 AND a.valid_until<=m.valid_until AND m.parent_id=a.parent_id AND m.role_id=a.role AND m.scope_kind=a.scope_kind AND m.scope_node_id IS NOT DISTINCT FROM a.scope_node_id AND u.status='active' AND u.login_policy='directory' AND i.enabled AND i.observed_at<=$4 AND i.observed_at>$4-300000 AND c.status='active' AND i.groups_json @> jsonb_build_array(m.group_id)) eligible,CASE WHEN octet_length(jsonb_build_object('assignment',to_jsonb(a)-'path','mapping',to_jsonb(m),'member',to_jsonb(u),'identity',jsonb_build_object('id',i.id,'external',i.external_id,'observed',i.observed_at,'enabled',i.enabled),'connection',jsonb_build_object('id',c.id,'company',c.company_id,'version',c.version,'config',c.config))::text)<=131072 THEN jsonb_build_object('assignment',to_jsonb(a)-'path','mapping',to_jsonb(m),'member',to_jsonb(u),'identity',jsonb_build_object('id',i.id,'external',i.external_id,'observed',i.observed_at,'enabled',i.enabled),'connection',jsonb_build_object('id',c.id,'company',c.company_id,'version',c.version,'config',c.config))::text END evidence FROM chain a LEFT JOIN iam_mappings m ON m.id=a.source_id AND m.company_id=a.company_id LEFT JOIN iam_members u ON u.company_id=a.company_id AND u.user_id=a.user_id AND u.connection_id=m.connection_id LEFT JOIN iam_identities i ON i.id=u.identity_id AND i.company_id=u.company_id AND i.user_id=u.user_id AND i.connection_id=u.connection_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE a.source='directory' ORDER BY a.id";
