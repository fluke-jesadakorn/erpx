//! Native Billing workspace read use case.
//!
//! The repository scopes every query to the target Holding. The application
//! then revalidates the current actor, exact Holding, bounded collections,
//! references, money and ordering before presentation. A foreign or malformed
//! row fails closed instead of being filtered into a plausible view.
use super::BillingFuture;
use crate::domain::actor::BillingActor;
use crate::domain::workspace::{
    BillingAccountView, BillingAuditView, BillingCostView, BillingEnrollmentView,
    BillingInvoiceView, BillingJobView, BillingLegacyView, BillingPeriodView, BillingPlanView,
    BillingWorkspace, validate_workspace,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

#[derive(Clone, Debug, Default)]
pub struct WorkspaceRows {
    pub account: Option<BillingAccountView>,
    pub enrollment: Option<BillingEnrollmentView>,
    pub plans: Vec<BillingPlanView>,
    pub periods: Vec<BillingPeriodView>,
    pub costs: Vec<BillingCostView>,
    pub invoices: Vec<BillingInvoiceView>,
    pub audit: Vec<BillingAuditView>,
    pub jobs: Vec<BillingJobView>,
    pub legacy: Option<BillingLegacyView>,
}

pub trait BillingWorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn load<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, WorkspaceRows, Self::Error>;
}

pub async fn billing_workspace<R: BillingWorkspaceRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<BillingWorkspace, R::Error> {
    actor.require_read(target, now)?;
    let rows = repository.load(target).await?;
    validate_workspace(
        actor,
        target,
        BillingWorkspace {
            sandbox: true,
            now: now.get(),
            account: rows.account,
            enrollment: rows.enrollment,
            plans: rows.plans,
            periods: rows.periods,
            costs: rows.costs,
            invoices: rows.invoices,
            audit: rows.audit,
            jobs: rows.jobs,
            legacy: rows.legacy,
        },
        now,
    )
    .map_err(Into::into)
}
