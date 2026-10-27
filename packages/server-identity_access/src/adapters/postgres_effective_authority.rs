//! Current authority facts. Only authorized_work owns the fresh transaction that
//! is allowed to mint a sealed context; these readers remain crate-private.
use crate::{
    adapters::postgres_work_context::PgWorkContextReader,
    application::work_context::inspect_current_work_selection,
    domain::{effective_authority::*, work_context::WorkContextRequest},
};
use erpx_server_manage_erpx::public::{
    BetaPolicyReadPort, PgBetaPolicyReader, lock_beta_policy_sources,
};
use erpx_server_organization::public::{
    PgOrganizationAuthorityReader, lock_organization_authority_sources,
};
use erpx_server_people_payroll::public::identity_facts::{
    PgPeopleAuthorityReader, lock_people_authority_sources,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, FailureKind, RecordId, TimestampMillis, UserId,
};
use sqlx::{PgConnection, Row};

/// Must be the first operation of the coordinator's new SERIALIZABLE transaction.
/// LOCK utility statements do not establish its data snapshot. Holding every
/// policy source first covers inserted denials as well as updated known rows.
pub(crate) async fn lock_effective_authority_sources(
    connection: &mut PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query("LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,active_work_contexts,iam_access_subjects,iam_access_invitations,um_attributes,um_values,um_groups,um_memberships,um_policies,um_attachments,um_candidates IN SHARE MODE").execute(&mut *connection).await?;
    super::directory_source_postgres::lock_sources(connection).await?;
    lock_organization_authority_sources(&mut *connection).await?;
    lock_people_authority_sources(&mut *connection).await?;
    lock_beta_policy_sources(connection).await?;
    Ok(())
}
struct SnapshotClock(i64);
impl Clock for SnapshotClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_AUTHORITY_UNAVAILABLE",
        "Unable to read current authority.",
    )
}
pub(crate) async fn current_authority_facts(
    connection: &mut PgConnection,
    request: &WorkContextRequest,
    now: i64,
) -> Result<EffectiveAuthorityFacts, PgTransactionError<BusinessError>> {
    let selection = inspect_current_work_selection(
        &mut PgWorkContextReader::new(request.holding_id.clone(), &mut *connection),
        &SnapshotClock(now),
        request,
    )
    .await?;
    let iam = current_iam_authority_facts(
        connection,
        &IamAuthorityQuery {
            company_id: request.company_id.clone(),
            user_id: request.user_id.clone(),
            role: selection.assignment().role.clone(),
            session_id: Some(request.session_id.clone()),
        },
        now,
    )
    .await?;
    let beta = PgBetaPolicyReader::new(&mut *connection)
        .company_beta_policy(&request.company_id)
        .await?;
    if beta.company_id != request.company_id {
        return Err(unavailable().into());
    }
    Ok(EffectiveAuthorityFacts {
        selection,
        iam,
        beta: beta.enabled,
    })
}
/// Current IAM facts only, with no selected Work context or business actor.
/// The caller must first retain the full policy source locks and prove its
/// original owner/assignment/review scope. Persistence errors remain retryable.
pub(crate) async fn current_iam_authority_facts(
    connection: &mut PgConnection,
    request: &IamAuthorityQuery,
    now: i64,
) -> Result<CurrentIamAuthorityFacts, PgTransactionError<BusinessError>> {
    let roles=sqlx::query("SELECT r.id,r.published_revision::bigint revision,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'")
        .bind(request.company_id.as_str()).bind(&request.role).fetch_all(&mut *connection).await?;
    if roles.len() != 1 {
        return Err(unavailable().into());
    }
    let role = &roles[0];
    let role = EffectiveRoleFact {
        id: role.try_get("id")?,
        revision: role.try_get("revision")?,
        permissions: serde_json::from_value(role.try_get("permissions_json")?)
            .map_err(|_| unavailable())?,
        grantable_permissions: serde_json::from_value(role.try_get("grantable_json")?)
            .map_err(|_| unavailable())?,
        scopes: serde_json::from_value(role.try_get("scopes_json")?).map_err(|_| unavailable())?,
    };
    let member=sqlx::query("SELECT m.connection_id,m.status,m.login_policy,m.version::bigint version,m.principal_id,m.person_id,COALESCE(i.enabled,false) enabled,COALESCE(i.observed_at,0)::bigint observed_at,COALESCE(c.status='active',false) connection_active FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.user_id=m.user_id AND i.connection_id=m.connection_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE m.company_id=$1 AND m.user_id=$2")
        .bind(request.company_id.as_str()).bind(request.user_id.as_str()).fetch_optional(&mut *connection).await?;
    let source_current = if let Some(m) = member.as_ref() {
        if m.try_get::<&str, _>("login_policy")? == "directory" {
            let id = m
                .try_get::<Option<String>, _>("connection_id")?
                .ok_or_else(unavailable)?;
            Some(
                super::directory_source_postgres::require_member_observation(
                    connection,
                    &id,
                    request.company_id.as_str(),
                    m.try_get("observed_at")?,
                    now,
                )
                .await?,
            )
        } else {
            None
        }
    } else {
        None
    };
    let member = member
        .map(|m| -> Result<_, PgTransactionError<BusinessError>> {
            Ok(EffectiveMemberFact {
                active: m.try_get::<String, _>("status")? == "active",
                login_policy: match m.try_get::<String, _>("login_policy")?.as_str() {
                    "local" => LoginPolicy::Local,
                    "directory" => LoginPolicy::Directory,
                    "emergency" => LoginPolicy::Emergency,
                    _ => return Err(unavailable().into()),
                },
                version: m.try_get("version")?,
                principal_id: m.try_get("principal_id")?,
                person_id: m
                    .try_get::<Option<String>, _>("person_id")?
                    .map(RecordId::new)
                    .transpose()?,
                directory_enabled: m.try_get("enabled")?,
                directory_provenance_current: source_current,
                directory_connection_active: m.try_get("connection_active")?,
                directory_observed_at: m.try_get("observed_at")?,
            })
        })
        .transpose()?;
    let mut people = PgPeopleAuthorityReader::new(&mut *connection);
    let fallback_person = if member.as_ref().and_then(|m| m.person_id.as_ref()).is_none() {
        people
            .person_for_user(&request.company_id, &request.user_id)
            .await?
            .map(|p| p.person_id)
    } else {
        None
    };
    let person = member
        .as_ref()
        .and_then(|m| m.person_id.as_ref())
        .or(fallback_person.as_ref());
    let aliases = if let Some(person) = person {
        sqlx::query_scalar::<_, String>(
            "SELECT user_id FROM iam_members WHERE company_id=$1 AND person_id=$2 ORDER BY user_id",
        )
        .bind(request.company_id.as_str())
        .bind(person.as_str())
        .fetch_all(&mut *connection)
        .await?
        .into_iter()
        .map(UserId::new)
        .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let emergency_until=sqlx::query_scalar::<_,i64>("SELECT expires_at FROM iam_emergency_sessions WHERE company_id=$1 AND user_id=$2 AND session_id=$3").bind(request.company_id.as_str()).bind(request.user_id.as_str()).bind(request.session_id.as_ref().map(|id|id.as_str()).unwrap_or("")).fetch_optional(&mut *connection).await?;
    let seat = PgOrganizationAuthorityReader::new(&mut *connection)
        .current_hr_seat(&request.company_id, TimestampMillis::new(now))
        .await?;
    let hr = if let Some(seat) = seat {
        let person = PgPeopleAuthorityReader::new(&mut *connection)
            .person_subject(&request.company_id, &seat.person_id)
            .await?;
        if let Some(person) = person {
            let same = if let Some(user) = &person.user_id {
                sqlx::query_scalar::<_, bool>("SELECT COALESCE(iam_same_person($1,$2,$3),false)")
                    .bind(user.as_str())
                    .bind(request.user_id.as_str())
                    .bind(request.company_id.as_str())
                    .fetch_one(&mut *connection)
                    .await?
            } else {
                false
            };
            Some(HrAppointmentFact {
                version: seat.version,
                active_person: person.status == "active",
                unmerged_person: person.merged_into_id.is_none(),
                active_engagement: !person.active_engagements.is_empty(),
                person_user_id: person.user_id,
                same_person_as_principal: same,
            })
        } else {
            None
        }
    } else {
        None
    };
    Ok(CurrentIamAuthorityFacts {
        role,
        member,
        fallback_person_id: fallback_person,
        aliases,
        hr,
        emergency_until,
    })
}
