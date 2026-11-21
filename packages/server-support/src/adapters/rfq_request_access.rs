//! Support-owned facts used by Records request access. These do not establish
//! Identity membership, current principals, or a request-access decision.
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// Fixed owner projection for composition with People names and Identity
/// principal resolution. The caller controls the original transaction/order.
pub fn rfq_request_team_sources_sql() -> &'static str {
    "rfq_request_teams AS (SELECT id,company_id,active FROM public.service_teams), rfq_request_team_members AS (SELECT team_id,company_id,user_id,responsibility FROM public.service_team_members)"
}

/// Mirrors inServiceWorkspace's independently optional branch and department.
pub fn rfq_in_service_workspace(
    actor_company: &str,
    actor_branch: Option<&str>,
    actor_department: Option<&str>,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
) -> bool {
    actor_company == company
        && actor_branch.is_none_or(|value| value.is_empty() || Some(value) == branch)
        && actor_department.is_none_or(|value| value.is_empty() || Some(value) == department)
}

/// Execute at employeeRequestDetail's original team-read point. Team
/// activity is deliberately not filtered here: the source only checks ID and
/// Company. This is not the separately filtered `teamHandlers` read.
pub async fn rfq_request_team_name(
    c: &mut PgConnection,
    company: &str,
    team_id: &str,
) -> Result<Option<String>, Error> {
    Ok(
        sqlx::query_scalar("SELECT name FROM service_teams WHERE id=$1 AND company_id=$2")
            .bind(team_id)
            .bind(company)
            .fetch_optional(c)
            .await?,
    )
}
