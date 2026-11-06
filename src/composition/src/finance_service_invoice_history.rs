//! Shared same-unit history authority for the Field invoice page and its native
//! notification consumers. This is not an HTTP read or a deserialized grant.
use crate::field_service_source_record::SourceRecordContext;
use erpx_server_commercial::public::purchases::*;
use erpx_server_field_service::public::finance_lineage::*;
use erpx_server_finance::public::{FinanceActor, field_service_invoice_read::*};
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}
pub(crate) async fn lock_service_invoice_history_sources(
    c: &mut PgConnection,
) -> Result<(), Error> {
    lock_service_invoice_sources(c).await?;
    lock_commercial_authority_sources(c).await?;
    erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
    lock_field_service_invoice_finance_sources(c).await
}
fn actor<C: Clock>(ctx: &SourceRecordContext<'_, C>) -> Result<FinanceActor, BusinessError> {
    let current = ctx.actor;
    current.require_permission("finance.read")?;
    current.require_permission("module.finance.read")?;
    if current.holding_id() != ctx.holding
        || &current.selection().workspace().company_id != ctx.company
        || !current.modules().iter().any(|m| m == "finance")
        || ctx.clock.now().get() >= current.expires_at()
    {
        return Err(changed());
    }
    Ok(FinanceActor {
        holding_id: ctx.holding.clone(),
        company_id: ctx.company.clone(),
        user_id: current.user_id().clone(),
        assignment_id: current.selection().assignment().id.clone(),
    })
}
pub(crate) struct CurrentServiceInvoiceHistory {
    finance: CurrentFinanceServiceInvoiceWorkspace,
    lineage: CurrentFieldServiceInvoiceFinanceLineage,
    base: PurchaseWitness,
    selected: PurchaseWitness,
    pub source_only: bool,
}
impl CurrentServiceInvoiceHistory {
    pub fn data(&self) -> &FinanceServiceInvoiceWorkspace {
        self.finance.data()
    }
    pub fn require_payment_evidence(&self) -> Result<(), BusinessError> { self.finance.require_payment_evidence() }
    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        actor(ctx)?;
        require_selected_actor_current(c, ctx.actor, ctx.clock.now().get()).await?;
        self.finance.require_current(c).await?;
        self.lineage.require_current(c).await?;
        let mut reader = ctx.purchase_reader(c);
        if reader
            .revalidate(&self.base, ctx.clock.now())
            .await?
            .require_allowed()
            .is_err()
            != self.source_only
        {
            return Err(changed().into());
        }
        reader
            .revalidate(&self.selected, ctx.clock.now())
            .await?
            .require_allowed()?;
        Ok(())
    }
}
pub(crate) async fn read_service_invoice_history<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    id: &str,
) -> Result<CurrentServiceInvoiceHistory, Error> {
    let finance = read_service_invoice_workspace(c, &actor(ctx)?, id).await?;
    let data = finance.data();
    let src = &data.source;
    let text = |key| {
        data.invoice
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(changed)
    };
    let lineage = read_field_service_invoice_finance_lineage(
        c,
        FieldServiceInvoiceFinanceRequest {
            company_id: ctx.company.clone(),
            order_id: src.order_id.clone(),
            release_id: src.release_id.clone(),
            key: src.command_key.clone(),
            actor_id: src.actor_id.clone(),
            currency: src.currency.clone(),
            amount_units: src.amount_units.clone(),
            customer_id: text("party_id")?,
            pricing_date: text("invoice_date")?,
        },
    )
    .await?
    .ok_or_else(changed)?;
    let mut reader =
        ctx.purchase_reader(c);
    let req = |cap: &str| PurchaseRequest {
        capabilities: vec![cap.into()],
        purpose: CapabilityPurpose::History,
        mode: PackageEnforcementMode::Enforced,
    };
    let base = reader
        .inspect(&req("finance.accounting"), ctx.clock.now())
        .await?;
    let source_only = base.require_allowed().is_err();
    let selected = if source_only {
        let included = reader
            .inspect(&req("finance.service-invoice"), ctx.clock.now())
            .await?;
        included.require_allowed()?;
        included.witness
    } else {
        base.witness.clone()
    };
    let result = CurrentServiceInvoiceHistory {
        finance,
        lineage,
        base: base.witness,
        selected,
        source_only,
    };
    result.require_current(c, ctx).await?;
    Ok(result)
}
