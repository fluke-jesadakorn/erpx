//! Current Field-parts journal history for native notification consumers.
//! All source and purchase witnesses are retained in the caller's transaction.
use crate::{
    field_service_source_record::SourceRecordContext,
    finance_field_service_lineage::FieldServicePartsSource,
};
use erpx_server_commercial::public::purchases::*;
use erpx_server_finance::public::{FinanceActor, journal_detail::*, journal_review::*};
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}
pub(crate) async fn lock_field_journal_history_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_commercial_authority_sources(c).await?;
    lock_journal_review_sources(c).await?;
    erpx_server_inventory::public::field_service_consumption_finance::lock_consumption_finance_sources(c).await?;
    erpx_server_workflow::public::record_origin::lock_record_origin_sources(c).await?;
    erpx_server_field_service::public::finance_lineage::lock_field_service_parts_finance_sources(c)
        .await?;
    lock_journal_detail_sources(c).await
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
pub(crate) struct CurrentFieldJournalHistory {
    finance: CurrentFieldServiceJournalDetail,
    lineage: FieldServicePartsSource,
    base: PurchaseWitness,
    selected: PurchaseWitness,
    source_only: bool,
}
impl CurrentFieldJournalHistory {
    #[cfg(feature = "finance-field-write")]
    pub(crate) fn version(&self) -> Result<i64, BusinessError> {
        use erpx_server_finance::public::journal_detail::FinanceLedgerValue;
        match self.finance.detail().journal.get("version") {
            Some(FinanceLedgerValue::Integer(value)) if value.get() >= 1 => Ok(value.get()),
            _ => Err(changed()),
        }
    }
    #[cfg(feature = "finance-field-write")]
    pub(crate) async fn after_evidence<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
        id: &str,
        version: i64,
        updated_at: i64,
    ) -> Result<Self, Error> {
        use erpx_server_finance::public::journal_detail::{FinanceLedgerValue, FinanceReadInteger};
        actor(ctx)?;
        require_selected_actor_current(c, ctx.actor, ctx.clock.now().get()).await?;
        Box::pin(self.lineage.require_current(c)).await?;
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
        if self.version()?.checked_add(1) != Some(version) {
            return Err(changed().into());
        }
        let next = Box::pin(read_field_journal_history(c, ctx, id)).await?;
        let mut expected = self.finance.detail().clone();
        expected.journal.insert(
            "version".into(),
            FinanceLedgerValue::Integer(FinanceReadInteger::new(version)?),
        );
        expected.journal.insert(
            "updated_at".into(),
            FinanceLedgerValue::Integer(FinanceReadInteger::new(updated_at)?),
        );
        if &expected != next.finance.detail() {
            return Err(changed().into());
        }
        Box::pin(next.require_current(c, ctx)).await?;
        Ok(next)
    }
    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        actor(ctx)?;
        require_selected_actor_current(c, ctx.actor, ctx.clock.now().get()).await?;
        Box::pin(self.finance.require_current(c)).await?;
        Box::pin(self.lineage.require_current(c)).await?;
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
pub(crate) async fn read_field_journal_history<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    id: &str,
) -> Result<CurrentFieldJournalHistory, Error> {
    let actor = actor(ctx)?;
    let id = RecordId::new(id)?;
    let inspect = JournalReviewCommand {
        key: CommandKey::new("field-journal-inbox-source")?,
        journal_id: id.clone(),
        input: JournalReviewInput::Post {
            expected_version: 1,
        },
    };
    let target = Box::pin(inspect_field_service_parts_journal_target(
        c, &actor, &inspect,
    ))
    .await?
    .ok_or_else(changed)?;
    let finance = Box::pin(read_field_service_journal_detail(c, &actor, &id, &target)).await?;
    let lineage = Box::pin(FieldServicePartsSource::retain(c, target)).await?;
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
    let result = CurrentFieldJournalHistory {
        finance,
        lineage,
        base: base.witness,
        selected,
        source_only,
    };
    Box::pin(result.require_current(c, ctx)).await?;
    Ok(result)
}
