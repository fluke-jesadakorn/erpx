//! Reviewed staff invitations retain a logically read-only Holding lease through
//! the Control credential commit. This adapter never fabricates a Work actor.
use crate::{
    adapters::{
        permission_catalog::permission_catalog,
        postgres_effective_authority::{
            current_iam_authority_facts, lock_effective_authority_sources,
        },
        work_admission::work_module_catalog,
    },
    application::{
        account_credentials::{
            InvitationCredentialAuthority, InvitationCredentialLease, InvitationCredentialQuery,
        },
        account_ports::AccountFuture,
        effective_authority::evaluate_current_iam_authority,
    },
    domain::{
        account_credentials::{
            InvitationCredentialFacts, signup_unavailable, validate_invitation_credential,
        },
        effective_authority::IamAuthorityQuery,
        invitation_authority::{
            ReviewedInvitationGrant, StaffInvitationGrant, invitation_scope_covers,
        },
    },
};
use erpx_server_organization::public::PgOrganizationReader;
use erpx_server_people_payroll::public::identity_facts::PgPeopleAuthorityReader;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use sqlx::{PgConnection, Postgres, Row, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Composition supplies trusted, already-resolved Holding writer bindings. A
/// request can select only an exact configured Holding; no ambient DB fallback.
pub struct PostgresInvitationCredentialAuthority {
    holdings: BTreeMap<HoldingId, DatabasePool>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresInvitationCredentialAuthority {
    pub fn new(
        holdings: BTreeMap<HoldingId, DatabasePool>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self { holdings, clock }
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVITATION_AUTHORITY_UNAVAILABLE",
        "Unable to verify current invitation authority.",
    )
}
fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(_) => signup_unavailable(),
        PgTransactionError::Persistence(_) => unavailable(),
    }
}
pub(crate) async fn inspect_current(
    connection: &mut PgConnection,
    request: &InvitationCredentialQuery,
    now: i64,
    allow_same_user: bool,
) -> Result<InvitationCredentialFacts, PgTransactionError<BusinessError>> {
    let reject = signup_unavailable;
    if !request.authorizer_control_active
        || !request.holding_binding_active
        || request.created_at > now
        || request.now > now
        || request.expires_at <= now
    {
        return Err(reject().into());
    }
    let grant = StaffInvitationGrant::parse(&request.role_payload)?;
    if !work_module_catalog()?
        .definitions()
        .iter()
        .any(|role| role.id == grant.role)
        || !grant.supports_scope(&request.scope_kind)
    {
        return Err(reject().into());
    }
    let company = CompanyId::new(request.company_id.clone())?;
    let holding = HoldingId::new(request.holding_id.clone())?;
    let person_id = RecordId::new(request.person_id.clone())?;
    let user_id = UserId::new(request.created_by.clone())?;
    let branch = request.branch_id.clone().map(RecordId::new).transpose()?;
    let department = request
        .department_id
        .clone()
        .map(RecordId::new)
        .transpose()?;
    let scope_node = match request.scope_kind.as_str() {
        "holding" if branch.is_none() && department.is_none() => None,
        "branch" if branch.is_some() && department.is_none() => request.branch_id.as_deref(),
        "department" if department.is_some() => request.department_id.as_deref(),
        _ => return Err(reject().into()),
    };
    PgOrganizationReader::new(holding.clone(), &mut *connection)
        .scope_context(&company, branch.as_ref(), department.as_ref())
        .await?;
    let person = PgPeopleAuthorityReader::new(&mut *connection)
        .person_subject(&company, &person_id)
        .await?
        .ok_or_else(reject)?;
    let current_engagement = person.active_engagements.iter().any(|engagement| {
        branch
            .as_ref()
            .is_none_or(|branch| engagement.branch_id.as_ref() == Some(branch))
            && department
                .as_ref()
                .is_none_or(|department| engagement.department_id.as_ref() == Some(department))
    });
    let target_user = format!("usr_invite_{}", request.invitation_id);
    let parent = sqlx::query("SELECT role,scope_kind,scope_node_id,branch_id,valid_until,version::bigint version FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND iam_assignment_active(id,$4)")
        .bind(&request.assignment_id).bind(&request.created_by).bind(&request.company_id).bind(now).fetch_optional(&mut *connection).await?.ok_or_else(reject)?;
    let parent_kind: String = parent.try_get("scope_kind")?;
    let parent_node: Option<String> = parent.try_get("scope_node_id")?;
    let parent_branch: Option<String> = parent.try_get("branch_id")?;
    let (parent_branch_id, parent_department_id) = match parent_kind.as_str() {
        "holding" if parent_node.is_none() => (None, None),
        "branch" if parent_node.is_some() => (parent_node.clone(), None),
        "department" if parent_node.is_some() => (parent_branch, parent_node.clone()),
        _ => return Err(reject().into()),
    };
    PgOrganizationReader::new(holding, &mut *connection)
        .scope_context(
            &company,
            parent_branch_id.map(RecordId::new).transpose()?.as_ref(),
            parent_department_id
                .map(RecordId::new)
                .transpose()?
                .as_ref(),
        )
        .await?;
    let author = current_iam_authority_facts(
        connection,
        &IamAuthorityQuery {
            company_id: company,
            user_id,
            role: parent.try_get("role")?,
            session_id: None,
        },
        now,
    )
    .await?;
    let evaluated = evaluate_current_iam_authority(&author, now)?;
    let permissions: BTreeSet<_> = evaluated.permissions.into_iter().collect();
    let allowed = author.role.scopes.contains(&parent_kind)
        && permission_catalog()?.allows_current_permissions(
            "iam.manage",
            &parent_kind,
            true,
            &permissions,
        );
    let roles = sqlx::query("SELECT r.published_revision::bigint revision,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=$1 AND r.company_id IN ('*',$2) AND r.status='active'")
        .bind(&grant.role).bind(&request.company_id).fetch_all(&mut *connection).await?;
    if roles.len() != 1 {
        return Err(reject().into());
    }
    let role = &roles[0];
    let target_permissions: Vec<String> =
        serde_json::from_value(role.try_get("permissions_json")?).map_err(|_| reject())?;
    let target_grantable: Vec<String> =
        serde_json::from_value(role.try_get("grantable_json")?).map_err(|_| reject())?;
    let target_scopes: Vec<String> =
        serde_json::from_value(role.try_get("scopes_json")?).map_err(|_| reject())?;
    let ceiling = target_permissions
        .iter()
        .chain(target_grantable.iter())
        .all(|permission| author.role.grantable_permissions.contains(permission));
    let reviews = sqlx::query("SELECT payload,created_by,created_person,created_at,version::bigint version,reviewed_by,reviewed_at FROM iam_reviews WHERE company_id=$1 AND kind='invitation.authorize' AND status='approved' AND payload->>'invitationId'=$2")
        .bind(&request.company_id).bind(&request.invitation_id).fetch_all(&mut *connection).await?;
    if reviews.len() != 1 {
        return Err(reject().into());
    }
    let review = &reviews[0];
    let reviewed: ReviewedInvitationGrant =
        serde_json::from_value(review.try_get("payload")?).map_err(|_| reject())?;
    let reviewer: String = review
        .try_get::<Option<String>, _>("reviewed_by")?
        .ok_or_else(reject)?;
    let author_person = author
        .member
        .as_ref()
        .and_then(|member| member.person_id.as_ref())
        .or(author.fallback_person_id.as_ref())
        .map(|id| id.as_str())
        .unwrap_or(&request.created_by);
    let same_person: bool = sqlx::query_scalar("SELECT COALESCE(iam_same_person($1,$2,$3),false) OR EXISTS(SELECT 1 FROM iam_members WHERE company_id=$3 AND user_id=$2 AND person_id=$4)")
        .bind(&request.created_by).bind(&reviewer).bind(&request.company_id).bind(&request.person_id).fetch_one(&mut *connection).await?;
    let reviewed_at: Option<i64> = review.try_get("reviewed_at")?;
    let review_created: i64 = review.try_get("created_at")?;
    let independent = reviewer != request.created_by
        && reviewer != target_user
        && !same_person
        && review.try_get::<String, _>("created_by")? == request.created_by
        && review.try_get::<String, _>("created_person")? == author_person
        && author_person != request.person_id
        && review.try_get::<i64, _>("version")? >= 2
        && review_created >= request.created_at
        && reviewed_at.is_some_and(|time| time >= review_created && time <= now)
        && reviewed.invitation_id == request.invitation_id
        && reviewed.user_id == target_user
        && reviewed.role_id == grant.role
        && reviewed.expected_role_revision == role.try_get::<i64, _>("revision")?
        && reviewed.scope_kind == request.scope_kind
        && reviewed.scope_node_id.as_deref() == scope_node
        && reviewed.valid_until == grant.expires
        && reviewed.parent_id == request.assignment_id
        && reviewed.authorizing_assignment_id == request.assignment_id;
    let target_identity: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM iam_members WHERE company_id=$1 AND (person_id=$2 OR user_id=$3))")
        .bind(&request.company_id).bind(&request.person_id).bind(&target_user).fetch_one(&mut *connection).await?;
    let facts = InvitationCredentialFacts {
        invitation_id: request.invitation_id.clone(),
        holding_id: request.holding_id.clone(),
        company_id: request.company_id.clone(),
        person_id: request.person_id.clone(),
        email: request.email.clone(),
        expected_email: request.email.clone(),
        now,
        expires_at: request.expires_at,
        pending: true,
        independently_reviewed: independent,
        published_role_current: ceiling && target_scopes.contains(&request.scope_kind),
        person_active: person.active_hr_subject() && current_engagement,
        person_unlinked: person.user_id.is_none()
            || allow_same_user
                && person
                    .user_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == target_user),
        authorizer_current: parent.try_get::<i64, _>("version")? > 0,
        authorizer_can_manage: allowed,
        scope_within_parent: invitation_scope_covers(
            &parent_kind,
            parent_node.as_deref(),
            &request.scope_kind,
            request.branch_id.as_deref(),
            request.department_id.as_deref(),
        ) && grant.expires <= parent.try_get::<i64, _>("valid_until")?,
        grant_expires_at: grant.expires,
        directory_only: target_identity,
        existing_user: false,
    };
    validate_invitation_credential(&facts)?;
    Ok(facts)
}
struct CurrentInvitationLease {
    // SQLx owns BEGIN/rollback state and rolls back on drop, including cancellation
    // or a failed outer commit. No Holding business effects are written.
    connection: Transaction<'static, Postgres>,
    request: InvitationCredentialQuery,
    facts: InvitationCredentialFacts,
}
impl InvitationCredentialLease for CurrentInvitationLease {
    fn facts(&self) -> &InvitationCredentialFacts {
        &self.facts
    }
    fn recheck(&mut self, now: i64) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            self.facts = inspect_current(&mut self.connection, &self.request, now, false)
                .await
                .map_err(persistence)?;
            Ok(())
        })
    }
}
impl InvitationCredentialAuthority for PostgresInvitationCredentialAuthority {
    fn inspect<'a>(
        &'a self,
        request: &'a InvitationCredentialQuery,
    ) -> AccountFuture<'a, Box<dyn InvitationCredentialLease>> {
        Box::pin(async move {
            let holding =
                HoldingId::new(request.holding_id.clone()).map_err(|_| signup_unavailable())?;
            let pool = self.holdings.get(&holding).ok_or_else(signup_unavailable)?;
            let mut connection = pool
                .raw()
                .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
                .await
                .map_err(|_| unavailable())?;
            sqlx::query("SET LOCAL lock_timeout='2s'")
                .execute(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            sqlx::query("SET LOCAL statement_timeout='5s'")
                .execute(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            lock_effective_authority_sources(&mut connection)
                .await
                .map_err(persistence)?;
            sqlx::query("LOCK TABLE iam_reviews IN SHARE MODE")
                .execute(&mut *connection)
                .await
                .map_err(|_| unavailable())?;
            let now = self.clock.now().get();
            if now < request.now {
                return Err(signup_unavailable());
            }
            let facts = inspect_current(&mut connection, request, now, false)
                .await
                .map_err(persistence)?;
            Ok(Box::new(CurrentInvitationLease {
                connection,
                request: request.clone(),
                facts,
            }) as Box<dyn InvitationCredentialLease>)
        })
    }
}
