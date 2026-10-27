//! Short current-source preflight. Nothing borrowed here survives into provider I/O.
use super::{
    account_sessions::{lock_session_reads, verify_session_in_transaction},
    directory_postgres::{PostgresDirectory, Tx, connection, finish, limits, unavailable},
    directory_work::DirectoryWorkPersistence,
};
use crate::{
    application::{
        account_ports::DeviceProof,
        directory_refresh::{
            DirectoryRefreshSource, DirectoryRequestPreflight, DirectoryWorkspaceInput,
        },
    },
    domain::{
        account_auth::RequestAudience,
        directory_refresh::{
            DirectoryRefreshDisposition, DirectoryRefreshMemberFacts,
            DirectoryRefreshSelectionFacts, classify_member, selection_fence,
        },
        work_context::*,
    },
};
use erpx_server_organization::public::{
    PgOrganizationReader, WorkspaceRouteQuery, lock_hierarchy_sources, read_company_context,
    resolve_workspace,
};
use erpx_server_runtime::public::postgres::PgRequestLease;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, FailureKind, RecordId, UserId,
};
use serde::Deserialize;
use sqlx::{PgConnection, Row};

#[derive(Deserialize)]
struct SelectionRow {
    session_id: String,
    user_id: String,
    company_id: String,
    assignment_id: Option<String>,
    assignment_version: Option<i64>,
    revision: i64,
    expires_at: i64,
}
#[derive(Deserialize)]
struct AssignmentRow {
    id: String,
    user_id: String,
    company_id: String,
    role: String,
    modules_json: String,
    scope_kind: String,
    scope_node_id: Option<String>,
    version: i64,
    valid_from: i64,
    valid_until: i64,
    revoked_at: Option<i64>,
}
#[derive(Deserialize)]
struct RoleRow {
    id: String,
    company_id: String,
    published_revision: i64,
    status: String,
    scopes: Vec<String>,
}
fn scope(value: &str) -> Result<WorkScopeKind, BusinessError> {
    match value {
        "holding" => Ok(WorkScopeKind::Holding),
        "branch" => Ok(WorkScopeKind::Branch),
        "department" => Ok(WorkScopeKind::Department),
        _ => Err(unavailable()),
    }
}
fn decode<T: serde::de::DeserializeOwned>(row: &sqlx::postgres::PgRow, key: &str) -> Tx<T> {
    let raw = row
        .try_get::<Option<String>, _>(key)?
        .ok_or_else(unavailable)?;
    serde_json::from_str(&raw).map_err(|_| unavailable().into())
}
fn unauthenticated() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "UNAUTHENTICATED",
        "Sign in to continue.",
    )
}
fn revoked() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "SESSION_REVOKED",
        "Sign in again to access this company.",
    )
}

impl DirectoryWorkPersistence {
    pub(super) async fn read_preflight_locked(
        &self,
        control: &mut PgConnection,
        host: String,
        proof: DeviceProof,
        input: DirectoryWorkspaceInput,
        audience: RequestAudience,
    ) -> Tx<DirectoryRequestPreflight> {
        let this = self;
        // These are utility locks only, before the first Control SELECT.
        lock_session_reads(control).await?;
        PostgresDirectory::lock_routes(control).await?;
        let session = verify_session_in_transaction(
            control,
            &proof,
            &audience,
            &this.policy.environment,
            this.directory.clock.now().get(),
            &this.directory.registry,
        )
        .await?
        .ok_or_else(unauthenticated)?;
        let mut input = input()?;
        let binding = this
            .directory
            .registry
            .read_current(control, &audience)
            .await?;
        let mut holding = this
            .holding
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
            .await?;
        limits(&mut holding).await?;
        sqlx::query("LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_mappings,iam_revoked_sessions,active_work_contexts IN SHARE MODE").execute(&mut *holding).await?;
        lock_hierarchy_sources(&mut holding).await?;
        let user = UserId::new(session.user.id)?;
        let session_id = RecordId::new(session.id)?;
        // This read and its failure precede malformed optional source or
        // profile/config loading. Never perform external work for a tombstone.
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE company_id=$1 AND user_id=$2 AND session_id=$3)").bind(input.company_id.as_str()).bind(user.as_str()).bind(session_id.as_str()).fetch_one(&mut *holding).await?;
        if blocked {
            return Err(revoked().into());
        }
        let row = sqlx::query(SELECTION)
            .bind(input.company_id.as_str())
            .bind(user.as_str())
            .bind(session_id.as_str())
            .fetch_one(&mut *holding)
            .await?;
        if !row.try_get::<bool, _>("selection_present")? {
            return Err(context_error("WORK_ROLE_REQUIRED").into());
        }
        if !row.try_get::<bool, _>("assignment_present")? {
            return Err(context_error("WORK_ROLE_UNAVAILABLE").into());
        }
        let selected: SelectionRow = decode(&row, "selection")?;
        let assignment: AssignmentRow = decode(&row, "assignment")?;
        let roles: Vec<RoleRow> = decode(&row, "roles")?;
        if roles.len() != 1 {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ROLE_UNAVAILABLE",
                "This role is unavailable.",
            )
            .into());
        }
        let role = roles.into_iter().next().ok_or_else(unavailable)?;
        let mut org = PgOrganizationReader::new(this.holding_id.clone(), &mut holding);
        let context = if input.branch_slug.is_none() && input.department_slug.is_none() {
            read_company_context(&mut org, &input.company_id).await?
        } else {
            let route = WorkspaceRouteQuery::new(
                input.branch_slug.clone(),
                input.department_slug.clone(),
                Some(input.company_id.as_str().to_owned()),
            )?;
            resolve_workspace(&mut org, &route).await?.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "WORKSPACE_NOT_FOUND",
                    "Workspace not found.",
                )
            })?
        };
        let facts = DirectoryRefreshSelectionFacts {
            revoked: false,
            workspace: WorkspaceLineageFact {
                company_id: context.company().company_id.clone(),
                branch_id: context.branch().map(|n| n.id.clone()),
                department_id: context.department().map(|n| n.id.clone()),
                branch_slug: context.branch().map(|n| n.slug.clone()),
                department_slug: context.department().map(|n| n.slug.clone()),
            },
            selection: ActiveSelectionFact {
                session_id: RecordId::new(selected.session_id)?,
                user_id: UserId::new(selected.user_id)?,
                company_id: CompanyId::new(selected.company_id)?,
                assignment_id: selected.assignment_id.map(AssignmentId::new).transpose()?,
                assignment_version: selected.assignment_version,
                revision: selected.revision,
                expires_at: selected.expires_at,
            },
            assignment: WorkAssignmentFact {
                id: AssignmentId::new(assignment.id)?,
                user_id: UserId::new(assignment.user_id)?,
                company_id: CompanyId::new(assignment.company_id)?,
                role: assignment.role,
                modules: serde_json::from_str(&assignment.modules_json)
                    .map_err(|_| unavailable())?,
                scope_kind: scope(&assignment.scope_kind)?,
                scope_node_id: assignment.scope_node_id.map(RecordId::new).transpose()?,
                version: assignment.version,
                valid_from: assignment.valid_from,
                valid_until: assignment.valid_until,
                revoked: assignment.revoked_at.is_some(),
            },
            role: PublishedRoleFact {
                id: role.id,
                company_id: role.company_id,
                revision: role.published_revision,
                active: role.status == "active",
                scopes: role
                    .scopes
                    .iter()
                    .map(|s| scope(s))
                    .collect::<Result<_, _>>()?,
            },
        };
        let expected = selection_fence(
            &input,
            &session_id,
            &user,
            session.expires_at,
            &facts,
            this.directory.clock.now().get(),
        )?;
        input.expected = Some(expected);
        let deadline = session
            .expires_at
            .min(facts.selection.expires_at)
            .min(facts.assignment.valid_until);
        let member = sqlx::query(MEMBER)
            .bind(input.company_id.as_str())
            .bind(user.as_str())
            .fetch_optional(&mut *holding)
            .await?;
        let mut source = None;
        let mut stale = false;
        if let Some(member) = member {
            let raw: MemberRow = decode(&member, "member")?;
            if raw.status != "active" || raw.version <= 0 {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "IDENTITY_SUSPENDED",
                    "This identity no longer has access to this company.",
                )
                .into());
            }
            match raw.login_policy.as_str() {
                "local" | "emergency" => {}
                "directory" => {
                    if member
                        .try_get::<Option<String>, _>("connection_status")?
                        .as_deref()
                        != Some("active")
                        || !member.try_get::<bool, _>("identity_present")?
                    {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "DIRECTORY_ACCESS_UNAVAILABLE",
                            "Your directory access needs current approved verification.",
                        )
                        .into());
                    }
                    let conn = raw.connection_id.ok_or_else(unavailable)?;
                    let route = this.directory.route(control, &conn).await?;
                    if route.holding_id.as_ref() != Some(&this.holding_id)
                        || route.company_id != input.company_id.as_str()
                    {
                        return Err(unavailable().into());
                    }
                    let descriptor = connection(&mut holding, &conn, &route).await?;
                    let identity: IdentityRow = decode(&member, "identity")?;
                    let current = DirectoryRefreshMemberFacts {
                        active: true,
                        version: raw.version,
                        connection_active: true,
                        link_matches: raw.identity_id.as_deref() == Some(identity.id.as_str())
                            && identity.company_id == input.company_id.as_str()
                            && identity.user_id.as_deref() == Some(user.as_str())
                            && identity.connection_id == conn,
                        identity_enabled: identity.enabled,
                        observed_at: identity.observed_at,
                    };
                    stale = classify_member(&current, this.directory.clock.now().get())?
                        == DirectoryRefreshDisposition::Observe;
                    source = Some(DirectoryRefreshSource {
                        descriptor,
                        identity_id: identity.id,
                        member_version: raw.version,
                        external_id: identity.external_id,
                        principal_id: raw.principal_id,
                    });
                }
                _ => return Err(unavailable().into()),
            }
        }
        if this.directory.clock.now().get() >= deadline {
            return Err(unauthenticated().into());
        }
        binding.require_current(control).await?;
        holding.rollback().await?;
        Ok(DirectoryRequestPreflight {
            host,
            session_id,
            user_id: user,
            authenticated_at: session.authenticated_at,
            session_expires_at: session.expires_at,
            request_deadline: deadline,
            workspace: input,
            directory: source,
            stale,
        })
    }
    pub(super) async fn read_preflight(
        &self,
        host: &str,
        proof: DeviceProof,
        input: DirectoryWorkspaceInput,
    ) -> Result<DirectoryRequestPreflight, BusinessError> {
        let audience = self.audience(host)?;
        let this = self.clone();
        let host = host.to_owned();
        let result = PgRequestLease::new(self.directory.control.clone())
            .run(move |control| {
                Box::pin(async move {
                    this.read_preflight_locked(control, host, proof, input, audience)
                        .await
                })
            })
            .await;
        let mut preflight = finish(result)?;
        // Fingerprint is resolved only after ALL database leases have ended.
        if let Some(source) = &mut preflight.directory {
            source.descriptor.profile_fingerprint = self
                .directory
                .profiles
                .fingerprint(
                    &source.descriptor.config.secret_profile,
                    &source.descriptor.connection_id,
                )
                .await?;
        }
        Ok(preflight)
    }
}
#[derive(Deserialize)]
struct MemberRow {
    principal_id: String,
    status: String,
    login_policy: String,
    connection_id: Option<String>,
    identity_id: Option<String>,
    version: i64,
}
#[derive(Deserialize)]
struct IdentityRow {
    id: String,
    company_id: String,
    connection_id: String,
    user_id: Option<String>,
    external_id: String,
    enabled: bool,
    observed_at: i64,
}
const SELECTION: &str = "WITH selected AS MATERIALIZED(SELECT s.* FROM active_work_contexts s WHERE s.company_id=$1 AND s.user_id=$2 AND s.session_id=$3),grant_row AS MATERIALIZED(SELECT a.* FROM selected s JOIN assignments a ON a.id=s.assignment_id AND a.company_id=$1 AND a.user_id=$2) SELECT EXISTS(SELECT 1 FROM selected) selection_present,EXISTS(SELECT 1 FROM grant_row) assignment_present,(SELECT CASE WHEN octet_length(to_jsonb(s)::text)<=8192 THEN to_jsonb(s)::text END FROM selected s) selection,(SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=65536 THEN to_jsonb(a)::text END FROM grant_row a) assignment,(SELECT CASE WHEN octet_length(v::text)<=8192 THEN v::text END FROM (SELECT COALESCE(jsonb_agg(jsonb_build_object('id',r.id,'company_id',r.company_id,'published_revision',r.published_revision,'status',r.status,'scopes',v.scopes_json)),'[]'::jsonb) v FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=(SELECT role FROM grant_row)) q) roles";
const MEMBER: &str = "SELECT c.status connection_status,i.id IS NOT NULL identity_present,CASE WHEN octet_length(to_jsonb(m)::text)<=8192 THEN to_jsonb(m)::text END member,CASE WHEN octet_length(jsonb_build_object('id',i.id,'company_id',i.company_id,'connection_id',i.connection_id,'user_id',i.user_id,'external_id',i.external_id,'enabled',i.enabled,'observed_at',i.observed_at)::text)<=16384 THEN jsonb_build_object('id',i.id,'company_id',i.company_id,'connection_id',i.connection_id,'user_id',i.user_id,'external_id',i.external_id,'enabled',i.enabled,'observed_at',i.observed_at)::text END identity FROM iam_members m LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.connection_id=m.connection_id AND i.user_id=m.user_id WHERE m.company_id=$1 AND m.user_id=$2";
