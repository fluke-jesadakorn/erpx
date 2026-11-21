//! Eligible Company choices require a live native Holding account, not a
//! preselected Company or a serialized account/grant list supplied by Next.
use super::{
    account_work_roles::lock_account_work_roles, holding_work_context::HoldingAccountScope,
};
use erpx_server_organization::public::{PgOrganizationReader, read_company_context};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct AccountCompanyChoice {
    pub id: String,
    pub slug: String,
    pub name: String,
}
pub async fn lock_account_company_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_account_work_roles(c, false).await
}
pub async fn read_account_companies(
    c: &mut PgConnection,
    account: &HoldingAccountScope,
    now: i64,
) -> Result<Vec<AccountCompanyChoice>, Error> {
    account.require_current(c, now).await?;
    // This is the original Company-picker rule: current Holding-scoped grants,
    // current published Holding scope and a current principal. Branch-only grants
    // do not silently expose the whole Company in a Company picker.
    let companies:Vec<String>=sqlx::query_scalar("SELECT DISTINCT a.company_id FROM assignments a WHERE a.user_id=$1 AND a.scope_kind='holding' AND a.revoked_at IS NULL AND a.valid_from<=$2 AND a.valid_until>$2 AND iam_assignment_active(a.id,$2) AND iam_principal_active(a.user_id,a.company_id,$2) AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=$3 AND r.user_id=a.user_id AND r.company_id=a.company_id) AND EXISTS(SELECT 1 FROM iam_roles role JOIN iam_role_revisions revision ON revision.company_id=role.company_id AND revision.role_id=role.id AND revision.revision=role.published_revision WHERE role.id=a.role AND role.company_id IN('*',a.company_id) AND role.status='active' AND revision.scopes_json @> '[\"holding\"]'::jsonb) ORDER BY a.company_id LIMIT 257")
 .bind(account.user_id().as_str()).bind(now).bind(account.session_id().as_str()).fetch_all(&mut *c).await?;
    if companies.len() > 256 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_COMPANIES_UNAVAILABLE",
            "The Company selection exceeds this bounded read.",
        )
        .into());
    }
    let mut choices = Vec::new();
    for company in companies {
        let id = CompanyId::new(company)?;
        let context = read_company_context(
            &mut PgOrganizationReader::new(account.holding_id().clone(), c),
            &id,
        )
        .await;
        match context {
            Ok(context) => {
                let n = context.company();
                choices.push(AccountCompanyChoice {
                    id: id.as_str().into(),
                    slug: n.slug.clone(),
                    name: n.display_name.clone(),
                });
            }
            Err(Error::Business(e)) if e.kind == FailureKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    choices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    account.require_current(c, now).await?;
    Ok(choices)
}
