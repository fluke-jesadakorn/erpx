//! Real RFQ target/Identity binding, not a command or purchase grant.
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    rfq_target_authority::{RfqScopePermission, require_rfq_target_permissions},
};
use erpx_server_procurement::public::{
    rfq_target_source::CurrentRfqTarget, rfq_transition::RfqTransitionAction,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// Enabled-mode wrapper check; caller retains actual purchase and Workflow
/// policy, uses the same protected transaction and revalidates after effects.
pub(crate) async fn require_enabled_rfq_target(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &CurrentRfqTarget,
    action: RfqTransitionAction,
    now: i64,
) -> Result<(), Error> {
    target.require_current(&mut *c).await?;
    let f = target.facts();
    if !f.within_selected_scope(current) {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "PROCUREMENT_SOURCE_NOT_FOUND",
            "The original purchasing source was not found in this workspace.",
        )
        .into());
    }
    let mut permissions = vec![RfqScopePermission::Read, RfqScopePermission::Purchase];
    if matches!(
        action,
        RfqTransitionAction::Approve | RfqTransitionAction::Return
    ) {
        permissions.push(RfqScopePermission::Approve);
    }
    require_rfq_target_permissions(
        &mut *c,
        current,
        &f.company_id,
        Some(&f.branch_id),
        f.department_id.as_deref(),
        &permissions,
        now,
    )
    .await?;
    target.require_current(c).await
}
