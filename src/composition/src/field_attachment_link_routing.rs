//! Ordered Field file-link routing source, not target authority or purchase.
//! All owner fences must be acquired before the first transaction snapshot.
use crate::field_attachment_global_marker::owned;
use erpx_server_files::public::field_attachment_routing::CurrentUnscopedGlobalAttachmentAncestry;
use erpx_server_finance::public::field_attachment_routing::{
    CurrentGlobalFieldJournalFinanceSource, CurrentGlobalFieldPaymentFinanceSources,
};
use erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin;
use sqlx::{AssertSqlSafe, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
/// Validate the request selector after normal authentication but before any
/// owner lock or PostgreSQL bind, matching the legacy routing precondition.
pub fn validate_field_link_selector(id: &str) -> Result<(), BusinessError> {
    if id.is_empty() || id.contains('\0') {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "ATTACHMENT_NOT_FOUND",
            "File evidence not found.",
        ));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldLinkRoute {
    Field,
    Other,
}
pub struct FieldLinkRoutingSources<'a> {
    pub files: &'a CurrentUnscopedGlobalAttachmentAncestry<'a>,
    pub workflow: &'a CurrentFieldHistoryReceiptJoin,
    pub payments: &'a CurrentGlobalFieldPaymentFinanceSources,
    pub journals: &'a CurrentGlobalFieldJournalFinanceSource,
    pub inventory: &'a CurrentFieldHistoryConsumptionJoin,
}
fn payment(id: &str) -> String {
    format!(
        "(EXISTS(SELECT 1 FROM field_history_receipts w JOIN finance_field_service_invoices i ON i.id=w.result_json::jsonb->>'settlementInvoiceId' WHERE w.operation='finance.payment.create' AND (w.record_id={id} OR w.result_json::jsonb->>'id'={id})) OR EXISTS(SELECT 1 FROM finance_field_payment_allocations p WHERE p.payment_id={id}))"
    )
}
fn journal(id: &str) -> String {
    format!(
        "(EXISTS(SELECT 1 FROM finance_field_service_journals j WHERE j.id={id}) OR EXISTS(SELECT 1 FROM field_global_journal_consumptions q WHERE q.journal_id={id}))"
    )
}
fn direct(typ: &str, kind: &str, id: &str) -> String {
    let f = if typ == "finance_payment" {
        payment(id)
    } else {
        journal(id)
    };
    format!("({kind}='{typ}' AND {f})")
}
impl FieldLinkRoutingSources<'_> {
    async fn require_owners(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.files.require_current(&mut *c).await?;
        self.workflow.require_current(&mut *c).await?;
        self.payments.require_current(&mut *c).await?;
        self.journals.require_current(&mut *c).await?;
        self.inventory.require_current(c).await
    }
    fn ctes(&self) -> String {
        format!(
            "WITH {}, {}, {}, {}, {}, {}",
            self.files.source_cte_sql(),
            self.files.cte_sql(),
            self.workflow.cte_sql(),
            self.payments.cte_sql(),
            self.journals.cte_sql(),
            self.inventory.global_journal_marker_cte_sql()
        )
    }
    async fn query(&self, c: &mut PgConnection, expr: &str) -> Result<(bool, bool), Error> {
        let sql = format!(
            "{} SELECT EXISTS(SELECT 1 FROM file_unscoped_link l WHERE {expr}) AS marker,(SELECT scoped FROM file_link_scope) AS scoped",
            self.ctes()
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(self.files.selected_link_id())
            .bind(self.files.company_id())
            .fetch_one(c)
            .await?;
        Ok((row.try_get("marker")?, row.try_get("scoped")?))
    }
    async fn evaluate(&self, c: &mut PgConnection) -> Result<FieldLinkRoute, Error> {
        self.require_owners(&mut *c).await?;
        validate_field_link_selector(self.files.selected_link_id())?;
        for typ in ["finance_payment", "finance_journal"] {
            let expr = [
                direct(typ, "l.set_type", "l.set_target"),
                direct(typ, "l.upload_type", "l.upload_target"),
                direct(typ, "l.upload_set_type", "l.upload_set_target"),
                direct(typ, "l.attachment_type", "l.attachment_target"),
            ]
            .join(" OR ");
            let (marker, scoped) = self.query(&mut *c, &expr).await?;
            if marker {
                self.require_owners(&mut *c).await?;
                if !scoped {
                    return Err(BusinessError::new(
                        FailureKind::NotFound,
                        "ATTACHMENT_NOT_FOUND",
                        "File evidence not found.",
                    )
                    .into());
                }
                return Ok(FieldLinkRoute::Field);
            }
        }
        let attachment = format!(
            "(EXISTS(SELECT 1 FROM file_global_attachment_source a WHERE {} OR {} OR {}) OR EXISTS(SELECT 1 FROM file_global_related_links r WHERE {} OR {} OR {}))",
            owned("a.attachment_owner_type", "a.attachment_owner_id"),
            owned("a.upload_owner_type", "a.upload_owner_id"),
            owned("a.upload_set_type", "a.upload_set_id"),
            owned("r.set_type", "r.set_id"),
            owned("r.upload_owner_type", "r.upload_owner_id"),
            owned("r.upload_set_type", "r.upload_set_id")
        );
        let expr = format!(
            "l.set_type='field_service_visit' OR l.upload_type='field_service_visit' OR l.upload_set_type='field_service_visit' OR {attachment}"
        );
        let (marker, scoped) = self.query(&mut *c, &expr).await?;
        self.require_owners(c).await?;
        if marker && !scoped {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "ATTACHMENT_NOT_FOUND",
                "File evidence not found.",
            )
            .into());
        }
        Ok(if marker {
            FieldLinkRoute::Field
        } else {
            FieldLinkRoute::Other
        })
    }
    pub async fn inspect<'a>(
        &'a self,
        c: &mut PgConnection,
    ) -> Result<CurrentFieldLinkRoute<'a, 'a>, Error> {
        Ok(CurrentFieldLinkRoute {
            route: self.evaluate(c).await?,
            sources: self,
        })
    }
}
pub struct CurrentFieldLinkRoute<'a, 's> {
    sources: &'a FieldLinkRoutingSources<'s>,
    route: FieldLinkRoute,
}
impl CurrentFieldLinkRoute<'_, '_> {
    pub fn route(&self) -> FieldLinkRoute {
        self.route
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let changed = || {
            BusinessError::new(
                FailureKind::Conflict,
                "FIELD_LINK_ROUTE_CHANGED",
                "The file routing source changed.",
            )
        };
        match self.sources.evaluate(c).await {
            Ok(route) if route == self.route => Ok(()),
            Ok(_) => Err(changed().into()),
            Err(PgTransactionError::Business(error)) if error.code == "ATTACHMENT_NOT_FOUND" => {
                Err(changed().into())
            }
            Err(error) => Err(error),
        }
    }
}
