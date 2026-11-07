//! Scoped current Identity facts for a journal Flow roster, not a grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

pub async fn lock_journal_flow_participant_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(c).await
}
/// Company-wide journal has null branch/department, so only Holding scoped
/// assignments can cover it. All dates and principal state use the DB clock.
pub async fn eligible_journal_flow_participant(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &str,
    creator: &str,
) -> Result<bool, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
    }
    let allowed = sqlx::query_scalar("SELECT COALESCE(($2<>$3) AND EXISTS (SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND (iam_role_allows(a.role,a.company_id,'workflow.finance_approver') OR iam_role_allows(a.role,a.company_id,'workflow.holding_owner'))),false)")
        .bind(company.as_str()).bind(user).bind(creator).fetch_one(c).await?;
    Ok(allowed)
}
/// The B0 current-person mapping resolves a superseded account to its unique
/// active linked principal only when there is exactly one successor.
pub async fn current_journal_flow_principal(
    c: &mut PgConnection,
    company: &CompanyId,
    user: &str,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());
    }
    let values: Vec<String> = sqlx::query_scalar("SELECT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=$1 AND original.user_id=$2 AND original.status='superseded' ORDER BY linked.user_id LIMIT 2")
        .bind(company.as_str()).bind(user).fetch_all(c).await?;
    if values.len() == 1 {
        Ok(values.into_iter().next().expect("one successor"))
    } else {
        Ok(user.to_owned())
    }
}

/// Field invoice's two finite participant pools. The current People roster is
/// supplied by composition; this owner decides current IAM eligibility only.
pub async fn eligible_service_invoice_flow_participant(c:&mut PgConnection,company:&CompanyId,user:&str,excluded:&[String],treasury:bool)->Result<bool,Error>{
 if !c.is_in_transaction()||excluded.len()>10{return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());}
 let roles=if treasury{vec!["workflow.treasury"]}else{vec!["workflow.finance_approver","workflow.holding_owner"]};
 Ok(sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM unnest($3::text[]) e(user_id) WHERE e.user_id=$2 OR COALESCE(iam_same_person($2,e.user_id,$1),false)) AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND EXISTS(SELECT 1 FROM unnest($4::text[]) role(permission) WHERE iam_role_allows(a.role,a.company_id,role.permission)))").bind(company.as_str()).bind(user).bind(excluded).bind(roles).fetch_one(c).await?)
}
pub async fn service_invoice_flow_administrator(c:&mut PgConnection,current:&crate::public::effective_authority::AuthorizedWorkContext,now:i64)->Result<bool,Error>{
 if !c.is_in_transaction(){return Err(PersistenceError::new("IAM_OWNER_TRANSACTION_REQUIRED").into());}
 Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND iam_role_allows(role,company_id,'organization.manage') AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=$3 AND valid_until>$3)").bind(current.selection().assignment().id.as_str()).bind(current.selection().workspace().company_id.as_str()).bind(now).fetch_one(c).await?)
}
