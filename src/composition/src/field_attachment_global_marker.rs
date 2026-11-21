//! Source-only expansion of the installed Field attachment ancestry marker.
//! The caller must acquire every owner lock before the first snapshot and must
//! retain selected actor, target authority and purchase evidence independently.
use erpx_server_files::public::field_attachment_routing::CurrentGlobalAttachmentAncestry;
use erpx_server_finance::public::field_attachment_routing::{
    CurrentGlobalFieldJournalFinanceSource, CurrentGlobalFieldPaymentFinanceSources,
};
use erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin;
use sqlx::{AssertSqlSafe, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
pub(crate) fn owned(kind: &str, id: &str) -> String {
    format!(
        "COALESCE({kind}='field_service_visit' OR ({kind}='finance_payment' AND (EXISTS(SELECT 1 FROM field_history_receipts w JOIN finance_field_service_invoices i ON i.id=w.result_json::jsonb->>'settlementInvoiceId' WHERE w.operation='finance.payment.create' AND (w.record_id={id} OR w.result_json::jsonb->>'id'={id})) OR EXISTS(SELECT 1 FROM finance_field_payment_allocations p WHERE p.payment_id={id}))) OR ({kind}='finance_journal' AND (EXISTS(SELECT 1 FROM finance_field_service_journals j WHERE j.id={id}) OR EXISTS(SELECT 1 FROM field_global_journal_consumptions q WHERE q.journal_id={id}))),false)"
    )
}
pub struct FieldAttachmentMarkerSources<'a> {
    pub files: &'a CurrentGlobalAttachmentAncestry<'a>,
    pub workflow: &'a CurrentFieldHistoryReceiptJoin,
    pub payments: &'a CurrentGlobalFieldPaymentFinanceSources,
    pub journals: &'a CurrentGlobalFieldJournalFinanceSource,
    pub inventory: &'a CurrentFieldHistoryConsumptionJoin,
}
impl FieldAttachmentMarkerSources<'_> {
    async fn require_owners(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.files.require_current(&mut *c).await?;
        self.workflow.require_current(&mut *c).await?;
        self.payments.require_current(&mut *c).await?;
        self.journals.require_current(&mut *c).await?;
        self.inventory.require_current(c).await
    }
    async fn evaluate(&self, c: &mut PgConnection) -> Result<bool, Error> {
        self.require_owners(&mut *c).await?;
        let sql = format!(
            "WITH {}, {}, {}, {}, {} SELECT EXISTS(SELECT 1 FROM file_global_attachment_source a WHERE {} OR {} OR {}) OR EXISTS(SELECT 1 FROM file_global_related_links l WHERE {} OR {} OR {}) AS marker",
            self.files.cte_sql(),
            self.workflow.cte_sql(),
            self.payments.cte_sql(),
            self.journals.cte_sql(),
            self.inventory.global_journal_marker_cte_sql(),
            owned("a.attachment_owner_type", "a.attachment_owner_id"),
            owned("a.upload_owner_type", "a.upload_owner_id"),
            owned("a.upload_set_type", "a.upload_set_id"),
            owned("l.set_type", "l.set_id"),
            owned("l.upload_owner_type", "l.upload_owner_id"),
            owned("l.upload_set_type", "l.upload_set_id")
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(self.files.selected_link_id())
            .bind(self.files.company_id())
            .fetch_one(&mut *c)
            .await?;
        let result = row.try_get("marker")?;
        self.require_owners(c).await?;
        Ok(result)
    }
    pub async fn inspect<'a>(
        &'a self,
        c: &mut PgConnection,
    ) -> Result<CurrentFieldAttachmentMarker<'a, 'a>, Error> {
        Ok(CurrentFieldAttachmentMarker {
            sources: self,
            marker: self.evaluate(c).await?,
        })
    }
}
pub struct CurrentFieldAttachmentMarker<'a, 's> {
    sources: &'a FieldAttachmentMarkerSources<'s>,
    marker: bool,
}
impl CurrentFieldAttachmentMarker<'_, '_> {
    pub fn is_field_marker(&self) -> bool {
        self.marker
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if self.sources.evaluate(c).await? != self.marker {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_ATTACHMENT_MARKER_CHANGED",
                "The attachment owner source changed.",
            )
            .into());
        }
        Ok(())
    }
}
