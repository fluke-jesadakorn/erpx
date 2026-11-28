//! Identity-owned assignment predicate used by Support request visibility.
//! This read alone is not a selected-work-context or RFQ command grant.
use crate::domain::effective_authority::AuthorizedWorkContext;
use erpx_platform_runtime_server::public::postgres::PersistenceError;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;

/// Prove only that an opaque selected actor belongs to this SERIALIZABLE
/// Holding unit. This is for enabled-mode composition; it neither rereads
/// authorization nor supplies disabled-mode transaction semantics.
pub async fn require_rfq_request_actor_unit(
    c: &mut PgConnection,
    actor: &AuthorizedWorkContext,
) -> Result<(), PgTransactionError<BusinessError>> {
    if actor.selected_unit.as_ref()
        != Some(&super::selected_command_authority::selected_unit(c).await?)
    {
        return Err(PersistenceError::new("RFQ_REQUEST_ACTOR_UNIT_MISMATCH").into());
    }
    Ok(())
}

/// Exact serviceMemberSql predicate: any unrevoked, time-valid assignment
/// in Company and matching holding/branch/department scope. Unlike other IAM
/// authorization predicates, the source does not check principal or role.
pub async fn rfq_request_service_member(
    c: &mut PgConnection,
    company: &str,
    user: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> Result<bool, PgTransactionError<BusinessError>> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL AND a.valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) OR (a.scope_kind='department' AND a.scope_node_id=$4)))")
        .bind(company).bind(user).bind(branch).bind(department).fetch_one(c).await?)
}

/// The Identity-owned `companyAuthority(actor,["holding_owner"])` source
/// used by serviceOwner. `policy_now` must come from the original policy clock,
/// not a caller-controlled request field. This fact is not a selected context.
pub async fn rfq_request_service_owner(
    c: &mut PgConnection,
    assignment_id: &str,
    company: &str,
    policy_now: i64,
) -> Result<bool, PgTransactionError<BusinessError>> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND iam_role_allows(role,company_id,'manage_erpx.access') AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=$3 AND valid_until>$3)")
        .bind(assignment_id).bind(company).bind(policy_now).fetch_one(c).await?)
}

/// Resolve superseded team-member IDs exactly as currentPersonPrincipals:
/// only an unambiguous active successor replaces the original ID. No result
/// grants membership and the caller still evaluates each member's scope.
pub async fn rfq_request_current_principals(
    c: &mut PgConnection,
    company: &str,
    ids: &[String],
) -> Result<Vec<(String, String)>, PgTransactionError<BusinessError>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT original.user_id,linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=$1 AND original.status='superseded' AND original.user_id=ANY($2::text[])")
        .bind(company).bind(ids).fetch_all(c).await?;
    Ok(ids
        .iter()
        .map(|id| {
            let mut matches = rows.iter().filter(|(original, _)| original == id);
            let first = matches.next();
            let current = match (first, matches.next()) {
                (Some((_, current)), None) => current.clone(),
                _ => id.clone(),
            };
            (id.clone(), current)
        })
        .collect())
}
