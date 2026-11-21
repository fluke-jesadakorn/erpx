//! Enabled purchase-wrapper check against the original assignment and target.
use crate::domain::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RfqScopePermission {
    Read,
    Purchase,
    Approve,
}
impl RfqScopePermission {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "commerce.read",
            Self::Purchase => "commerce.purchase",
            Self::Approve => "commerce.approve",
        }
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROCUREMENT_ACCESS_CHANGED",
        "Choose a current authorized purchasing work role.",
    )
    .into()
}
/// Caller supplies scope from a current Procurement witness, not request data.
/// The selected context remains bound to its original protected transaction.
/// Invoke for enabled purchase modes only; disabled skips this TS wrapper.
pub async fn require_rfq_target_permissions(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[RfqScopePermission],
    now: i64,
) -> Result<(), Error> {
    if current.selected_unit.as_ref()
        != Some(&super::selected_command_authority::selected_unit(&mut *c).await?)
    {
        return Err(changed());
    }
    let assignment = current.selection().assignment();
    for permission in permissions {
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.assignments g WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(g.role,g.company_id,$5) AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$6 OR g.scope_kind='department' AND g.scope_node_id=$7))")
            .bind(assignment.id.as_str()).bind(current.user_id().as_str()).bind(&assignment.role).bind(company).bind(permission.name()).bind(branch).bind(department).fetch_one(&mut *c).await?;
        if !allowed {
            return Err(changed());
        }
    }
    super::selected_command_authority::require_selected_actor_current(c, current, now).await
}
