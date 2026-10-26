//! Selected grant facts only. Effective IAM permission, HR/emergency authority and owner policies remain mandatory.
use crate::{
    application::work_context::{CurrentWorkContextPort, WorkContextFuture},
    domain::work_context::*,
};
use erpx_server_organization::public::{
    PgOrganizationReader, WorkspaceRouteQuery, lock_hierarchy_sources, read_company_context,
    resolve_workspace,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde::Deserialize;
use sqlx::{PgConnection, Row};

pub struct PgWorkContextReader<'c> {
    holding_id: HoldingId,
    connection: &'c mut PgConnection,
}
impl<'c> PgWorkContextReader<'c> {
    pub fn new(holding_id: HoldingId, connection: &'c mut PgConnection) -> Self {
        Self {
            holding_id,
            connection,
        }
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_ACCESS_UNAVAILABLE",
        "Unable to read current account access.",
    )
}
fn scope(value: &str) -> Result<WorkScopeKind, BusinessError> {
    match value {
        "holding" => Ok(WorkScopeKind::Holding),
        "branch" => Ok(WorkScopeKind::Branch),
        "department" => Ok(WorkScopeKind::Department),
        _ => Err(unavailable()),
    }
}
#[derive(Deserialize)]
struct SelectionSql {
    session_id: String,
    user_id: String,
    company_id: String,
    assignment_id: Option<String>,
    assignment_version: Option<i64>,
    revision: i64,
    expires_at: i64,
}
#[derive(Deserialize)]
struct AssignmentSql {
    id: String,
    user_id: String,
    company_id: String,
    role: String,
    scope_kind: String,
    scope_node_id: Option<String>,
    version: i64,
    valid_from: i64,
    valid_until: i64,
    revoked_at: Option<i64>,
}
#[derive(Deserialize)]
struct RoleSql {
    id: String,
    company_id: String,
    published_revision: i64,
    status: String,
    scopes: Vec<String>,
}
fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, BusinessError> {
    serde_json::from_value(value).map_err(|_| unavailable())
}
impl CurrentWorkContextPort for PgWorkContextReader<'_> {
    type Error = PgTransactionError<BusinessError>;
    fn holding_id(&self) -> &HoldingId {
        &self.holding_id
    }
    fn snapshot<'a>(
        &'a mut self,
        request: &'a WorkContextRequest,
        now: i64,
    ) -> WorkContextFuture<'a, WorkContextSnapshot, Self::Error> {
        Box::pin(async move {
            // Retain own authoritative grant/role/selection sources before reading a snapshot.
            // People/UM-derived source locks are a later full-permission adapter obligation.
            sqlx::query("LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,active_work_contexts IN SHARE MODE").execute(&mut *self.connection).await?;
            lock_hierarchy_sources(&mut *self.connection).await?;
            let row=sqlx::query("WITH requested AS (SELECT $1::text company_id,$2::text user_id,$3::text session_id,$4::bigint observed_now),
     selected AS MATERIALIZED (SELECT s.* FROM requested q JOIN active_work_contexts s ON s.company_id=q.company_id AND s.user_id=q.user_id AND s.session_id=q.session_id),
     grant_row AS MATERIALIZED (SELECT a.* FROM requested q JOIN selected s ON true JOIN assignments a ON a.id=s.assignment_id AND a.company_id=q.company_id AND a.user_id=q.user_id)
     SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=q.company_id AND r.user_id=q.user_id AND r.session_id=q.session_id) session_revoked,
      EXISTS(SELECT 1 FROM iam_members m JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id WHERE m.company_id=q.company_id AND m.user_id=q.user_id AND m.status='active' AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=q.observed_now-300000) directory_stale,
      (SELECT to_jsonb(s) FROM selected s) selection,
      (SELECT to_jsonb(a) FROM grant_row a) assignment,
      COALESCE((SELECT iam_assignment_active(a.id,q.observed_now) FROM grant_row a),false) assignment_active,
      iam_principal_active(q.user_id,q.company_id,q.observed_now) principal_active,
      COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'company_id',r.company_id,'published_revision',r.published_revision,'status',r.status,'scopes',v.scopes_json))
       FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
       WHERE r.company_id IN ('*',q.company_id) AND r.id=(SELECT role FROM grant_row)),'[]'::jsonb) roles
     FROM requested q")
     .bind(request.company_id.as_str()).bind(request.user_id.as_str()).bind(request.session_id.as_str()).bind(now).fetch_one(&mut *self.connection).await?;
            let selection: Option<serde_json::Value> = row.try_get("selection")?;
            let selection = selection
                .map(|value| -> Result<_, BusinessError> {
                    let s: SelectionSql = decode(value)?;
                    Ok(ActiveSelectionFact {
                        session_id: RecordId::new(s.session_id)?,
                        user_id: UserId::new(s.user_id)?,
                        company_id: CompanyId::new(s.company_id)?,
                        assignment_id: s.assignment_id.map(AssignmentId::new).transpose()?,
                        assignment_version: s.assignment_version,
                        revision: s.revision,
                        expires_at: s.expires_at,
                    })
                })
                .transpose()?;
            let assignment: Option<serde_json::Value> = row.try_get("assignment")?;
            let assignment = assignment
                .map(|value| -> Result<_, BusinessError> {
                    let a: AssignmentSql = decode(value)?;
                    Ok(WorkAssignmentFact {
                        id: AssignmentId::new(a.id)?,
                        user_id: UserId::new(a.user_id)?,
                        company_id: CompanyId::new(a.company_id)?,
                        role: a.role,
                        scope_kind: scope(&a.scope_kind)?,
                        scope_node_id: a.scope_node_id.map(RecordId::new).transpose()?,
                        version: a.version,
                        valid_from: a.valid_from,
                        valid_until: a.valid_until,
                        revoked: a.revoked_at.is_some(),
                    })
                })
                .transpose()?;
            let roles: Vec<RoleSql> = decode(row.try_get("roles")?)?;
            let role = if roles.len() == 1 {
                let r = roles.into_iter().next().expect("one checked role");
                Some(PublishedRoleFact {
                    id: r.id,
                    company_id: r.company_id,
                    revision: r.published_revision,
                    active: r.status == "active",
                    scopes: r
                        .scopes
                        .iter()
                        .map(|s| scope(s))
                        .collect::<Result<_, _>>()?,
                })
            } else {
                None
            };
            let session_revoked: bool = row.try_get("session_revoked")?;
            let workspace = if session_revoked {
                None
            } else {
                let mut organization =
                    PgOrganizationReader::new(self.holding_id.clone(), &mut *self.connection);
                let context = if request.branch_slug.is_none() && request.department_slug.is_none()
                {
                    Some(read_company_context(&mut organization, &request.company_id).await?)
                } else {
                    let query = WorkspaceRouteQuery::new(
                        request.branch_slug.clone(),
                        request.department_slug.clone(),
                        Some(request.company_id.as_str().to_owned()),
                    )?;
                    resolve_workspace(&mut organization, &query).await?
                };
                context.map(|context| WorkspaceLineageFact {
                    company_id: context.company().company_id.clone(),
                    branch_id: context.branch().map(|n| n.id.clone()),
                    department_id: context.department().map(|n| n.id.clone()),
                    branch_slug: context.branch().map(|n| n.slug.clone()),
                    department_slug: context.department().map(|n| n.slug.clone()),
                })
            };
            Ok(WorkContextSnapshot {
                workspace,
                selection,
                assignment,
                role,
                session_revoked,
                directory_stale: row.try_get("directory_stale")?,
                principal_active: row.try_get("principal_active")?,
                assignment_active: row.try_get("assignment_active")?,
            })
        })
    }
}
