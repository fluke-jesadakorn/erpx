//! Native generic Field Finance History read for journals.
//!
//! This mounts the committed `field_finance_history` decision into a real
//! Finance journal detail/workspace read. It retains the Finance target, Field,
//! Workflow, Inventory and Finance Money witnesses in the caller's selected
//! Holding unit and never turns a source label into authority or a purchase.
use crate::{
    field_finance_history::{FieldFinanceHistoryPhase, inspect_field_finance_history},
    field_service_source_record::SourceRecordContext,
};
use erpx_server_finance::public::{
    FinanceActor,
    attachment_target_source::{
        CurrentCanonicalFinanceAttachmentObservation, FinanceAttachmentTargetKind,
        inspect_canonical_finance_history_journal_roots, lock_canonical_finance_attachment_targets,
        lock_canonical_finance_history_book_roots, observe_canonical_finance_attachment_target,
    },
    field_history_candidates::{
        inspect_field_history_finance_candidates, lock_field_history_finance_candidate_sources,
    },
    field_history_money_lines::{
        inspect_field_history_money_lines, lock_field_history_money_line_sources,
    },
    journal_detail::{
        CurrentFieldServiceJournalDetail, FinanceLedgerRow, lock_journal_detail_sources,
        read_field_finance_journal_detail,
    },
    journal_workspace::{
        CurrentFieldServiceJournalWorkspace, read_field_finance_journal_workspace,
    },
};
use erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, RecordId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "Journal not found.",
    )
}
/// Acquire every generic Field History owner fence before the first snapshot.
/// The order follows the owner source modules: Finance target, book roots,
/// invoice/allocation/reversal candidates, Money and journal lines, Workflow
/// receipts, Inventory consumption, Field joins, then the retained journal
/// payload. A conflict must retry the whole outer unit.
pub(crate) async fn lock_generic_field_finance_history_sources(
    c: &mut PgConnection,
) -> Result<(), Error> {
    lock_canonical_finance_attachment_targets(c).await?;
    lock_canonical_finance_history_book_roots(c).await?;
    lock_field_history_finance_candidate_sources(c).await?;
    lock_field_history_money_line_sources(c).await?;
    erpx_server_workflow::public::record_origin::lock_field_history_receipt_join_sources(c).await?;
    erpx_server_inventory::public::field_history_consumption::lock_field_history_consumption_join_sources(c)
        .await?;
    erpx_server_field_service::public::finance_lineage::lock_field_history_join_sources(c).await?;
    lock_journal_detail_sources(c).await
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GenericHistoryReadKind {
    Detail,
    Workspace,
}
enum Payload {
    Detail(CurrentFieldServiceJournalDetail),
    Workspace(CurrentFieldServiceJournalWorkspace),
}
/// Retains one joined Field Finance History decision and the exact bounded
/// Finance journal payload. This is read data, never a posting grant.
pub(crate) struct CurrentGenericFieldFinanceHistory {
    company: CompanyId,
    target: CurrentCanonicalFinanceAttachmentObservation,
    workflow: erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin,
    inventory:
        erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin,
    field: erpx_server_field_service::public::finance_lineage::CurrentFieldHistoryJoin,
    inclusion: bool,
    parts_supplement: bool,
    payload: Payload,
}
impl CurrentGenericFieldFinanceHistory {
    pub(crate) fn detail(&self) -> Option<&CurrentFieldServiceJournalDetail> {
        match &self.payload {
            Payload::Detail(value) => Some(value),
            Payload::Workspace(_) => None,
        }
    }
    pub(crate) fn workspace(&self) -> Option<&CurrentFieldServiceJournalWorkspace> {
        match &self.payload {
            Payload::Detail(_) => None,
            Payload::Workspace(value) => Some(value),
        }
    }
    pub(crate) fn journal(&self) -> &FinanceLedgerRow {
        match &self.payload {
            Payload::Detail(value) => &value.detail().journal,
            Payload::Workspace(value) => &value.data().journal,
        }
    }
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        let _ = actor(ctx)?;
        require_selected_actor_current(c, ctx.actor, ctx.clock.now().get()).await?;
        if ctx.company != &self.company {
            return Err(changed().into());
        }
        match &self.payload {
            Payload::Detail(value) => value.require_current(c).await?,
            Payload::Workspace(value) => value.require_current(c).await?,
        }
        self.target.require_current(c).await?;
        let roots = inspect_canonical_finance_history_journal_roots(c, &self.target).await?;
        let candidates =
            inspect_field_history_finance_candidates(c, &self.target, Some(&roots)).await?;
        let money = inspect_field_history_money_lines(c, &candidates).await?;
        let inclusion = inspect_field_finance_history(
            c,
            &candidates,
            &money,
            &self.workflow,
            &self.inventory,
            &self.field,
            FieldFinanceHistoryPhase::Inclusion,
        )
        .await?
        .decision();
        if inclusion != self.inclusion {
            return Err(changed().into());
        }
        let parts = inspect_field_finance_history(
            c,
            &candidates,
            &money,
            &self.workflow,
            &self.inventory,
            &self.field,
            FieldFinanceHistoryPhase::PartsSupplement,
        )
        .await?
        .decision();
        if parts != self.parts_supplement {
            return Err(changed().into());
        }
        Ok(())
    }
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
/// Read the selected Field-owned journal through the generic History sources.
/// `None` means the joined Field decision is absent; it is not a global
/// negative for another source family and must fall through to the ordinary
/// Finance read policy.
pub(crate) async fn read_generic_field_finance_history<C: Clock>(
    c: &mut PgConnection,
    ctx: &SourceRecordContext<'_, C>,
    kind: GenericHistoryReadKind,
    id: &str,
) -> Result<Option<CurrentGenericFieldFinanceHistory>, Error> {
    let actor = actor(ctx)?;
    let record = RecordId::new(id)?;
    let target = observe_canonical_finance_attachment_target(
        c,
        FinanceAttachmentTargetKind::Journal,
        ctx.company,
        id,
    )
    .await?;
    if target.version().is_none() {
        return Ok(None);
    }
    let roots = inspect_canonical_finance_history_journal_roots(c, &target).await?;
    let candidates = inspect_field_history_finance_candidates(c, &target, Some(&roots)).await?;
    let money = inspect_field_history_money_lines(c, &candidates).await?;
    let workflow =
        erpx_server_workflow::public::record_origin::inspect_field_history_receipt_join(c).await?;
    let inventory =
        erpx_server_inventory::public::field_history_consumption::inspect_field_history_consumption_join(c)
            .await?;
    let field =
        erpx_server_field_service::public::finance_lineage::inspect_field_history_join(c).await?;
    let inclusion = inspect_field_finance_history(
        c,
        &candidates,
        &money,
        &workflow,
        &inventory,
        &field,
        FieldFinanceHistoryPhase::Inclusion,
    )
    .await?
    .decision();
    if !inclusion {
        return Ok(None);
    }
    let parts_supplement = inspect_field_finance_history(
        c,
        &candidates,
        &money,
        &workflow,
        &inventory,
        &field,
        FieldFinanceHistoryPhase::PartsSupplement,
    )
    .await?
    .decision();
    let payload = match kind {
        GenericHistoryReadKind::Detail => {
            Payload::Detail(read_field_finance_journal_detail(c, &actor, &record).await?)
        }
        GenericHistoryReadKind::Workspace => Payload::Workspace(
            read_field_finance_journal_workspace(c, &actor, &record).await?,
        ),
    };
    let result = CurrentGenericFieldFinanceHistory {
        company: ctx.company.clone(),
        target,
        workflow,
        inventory,
        field,
        inclusion,
        parts_supplement,
        payload,
    };
    result.require_current(c, ctx).await?;
    Ok(Some(result))
}
