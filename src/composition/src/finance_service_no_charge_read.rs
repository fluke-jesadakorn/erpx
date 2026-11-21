//! Native invoice History presentation without command/runtime dependencies.
//! Current owner source and selected authority remain on the caller's unit.
use crate::{
    finance_journal_actor::{command_actor, finance_actor},
    finance_service_invoice_flow::read_service_invoice_flow,
};
use erpx_server_finance::public::service_no_charge::*;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_NO_CHARGE_SOURCE_CHANGED",
        "Review the original fully covered Service billing source.",
    )
}
/// Called only inside the existing selected invoice history composition. The
/// opaque review key and canConfirm are current-source presentation, not grants.
pub(crate) async fn controls<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    settle: bool,
    clock: &C,
) -> Result<Option<ServiceNoChargeControls>, Error> {
    let Some(source) = read_optional_service_no_charge(c, &finance_actor(current), id).await?
    else {
        return Ok(None);
    };
    require_scope(&source, current)?;
    let independent = !current.same_person_user(&UserId::new(source.creator()?.to_owned())?)
        && !current.same_person_user(&UserId::new(source.releaser()?.to_owned())?);
    let actor = command_actor(current)?;
    let flow = read_service_invoice_flow(c, current, &source.record(), settle, clock).await?;
    let can = !source.confirmed()
        && independent
        && current.has_permission("finance.write")
        && current.has_permission("finance.approve")
        && settle
        && flow.view["canAct"] == true;
    let result = ServiceNoChargeControls {
        invoice_id: id.into(),
        version: source.version()?,
        context_key: source.context_key(&actor)?,
        can_confirm: can,
        confirmed: source.confirmed(),
    };
    source.require_current(c).await?;
    Ok(Some(result))
}

pub(crate) fn require_scope(
    source: &CurrentServiceNoCharge,
    current: &AuthorizedWorkContext,
) -> Result<(), BusinessError> {
    let scope = current.selection().workspace();
    if !source.scope_matches(
        scope.branch_id.as_ref().map(|b| b.as_str()),
        scope.department_id.as_ref().map(|d| d.as_str()),
    ) {
        return Err(changed());
    }
    Ok(())
}
