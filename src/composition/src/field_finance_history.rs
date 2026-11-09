//! Same-unit Field lineage decision for generic Finance History.
//!
//! This is only one source family. It grants no IAM or purchase authority and
//! does not establish that any other Finance History source is absent. All
//! owner fences must be acquired before the first snapshot by the coordinator.
use erpx_server_field_service::public::finance_lineage::CurrentFieldHistoryJoin;
use erpx_server_finance::public::field_history_candidates::CurrentFieldHistoryFinanceCandidates;
use erpx_server_finance::public::field_history_money_lines::CurrentFieldHistoryMoneyLines;
use erpx_server_inventory::public::field_history_consumption::CurrentFieldHistoryConsumptionJoin;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::record_origin::CurrentFieldHistoryReceiptJoin;
use sqlx::{AssertSqlSafe, PgConnection};

type Error = PgTransactionError<BusinessError>;

// These joins deliberately mirror fieldServiceFinanceRecordSources. All CTE
// fragments originate in concrete owner witnesses; no request text is SQL.
const JOINS: &str = r#", owned_field_invoice AS (
 SELECT i.company_id,i.id,i.journal_id FROM field_history_billing_releases b
 JOIN field_history_orders s ON s.id=b.order_id AND s.agreement_id=b.agreement_id AND s.version>=b.source_order_version
 JOIN field_history_agreements g ON g.id=s.agreement_id AND g.company_id=s.company_id AND g.branch_id=s.branch_id AND g.department_id IS NOT DISTINCT FROM s.department_id AND g.customer_id=b.customer_id
 JOIN field_history_order_events e ON e.order_id=s.id AND e.version=b.source_order_version AND e.action='release-billing' AND e.status='billed' AND e.actor_id=b.released_by
 JOIN field_history_receipts r ON b.id='field-billing-'||r.command_id AND e.id='service-order-event-'||r.command_id AND r.operation='field.order.release-billing' AND r.record_id=s.id AND r.actor_id=b.released_by AND r.authority_json::jsonb->>'companyId'=s.company_id
 JOIN finance_history_invoices i ON i.id='field-invoice-'||r.command_id AND i.company_id=s.company_id AND i.kind='receivable' AND i.source_type='field_service' AND i.source_id=s.id AND i.party_id=b.customer_id AND i.currency=b.currency AND i.total_minor=b.amount_minor AND i.invoice_date=b.pricing_date
 JOIN field_history_finance_money m ON m.company_id=s.company_id AND m.source_type='field_service_billing_release' AND m.source_id=b.id AND m.amount_kind='invoice_total' AND m.currency=b.currency AND m.amount_units=b.amount_minor
 WHERE b.status='released'
), owned_field_payment AS (
 SELECT p.company_id,p.id FROM finance_history_payment_candidates p
 JOIN field_history_receipts r ON r.record_id=p.id AND r.operation='finance.payment.create' AND r.actor_id=p.created_by AND r.authority_json::jsonb->>'companyId'=p.company_id AND r.result_json::jsonb->>'id'=p.id
 JOIN owned_field_invoice source ON source.id=r.result_json::jsonb->>'settlementInvoiceId' AND source.company_id=p.company_id
 JOIN finance_history_invoices original ON original.id=source.id AND original.company_id=p.company_id AND original.party_id=p.party_id AND original.currency=p.currency
 WHERE p.kind='receipt'
 AND NOT EXISTS(SELECT 1 FROM finance_history_allocations a WHERE a.payment_id=p.id AND (a.company_id<>p.company_id OR a.invoice_id<>source.id))
 AND (p.status NOT IN ('settled','voided') OR EXISTS(SELECT 1 FROM finance_history_allocations a WHERE a.payment_id=p.id AND a.company_id=p.company_id AND a.invoice_id=source.id HAVING sum(a.amount_minor)=p.amount_minor))
), owned_field_parts AS (
 SELECT s.company_id,j.id FROM field_history_consumptions c
 JOIN field_history_orders s ON s.id=c.source_id AND s.company_id=c.company_id AND s.branch_id=c.branch_id AND s.department_id IS NOT DISTINCT FROM c.department_id AND s.version>=c.source_version
 JOIN field_history_order_events e ON e.order_id=s.id AND e.version=c.source_version AND e.action='issue-part' AND e.details_json->>'consumptionId'=c.id AND e.details_json->>'journalId'=c.journal_id
 JOIN field_history_part_reservations v ON v.id=e.details_json->>'reservationId' AND v.order_id=s.id AND v.warehouse_id=c.warehouse_id AND v.product_id=c.product_id
 JOIN field_history_receipts r ON c.id='operational-part-'||r.command_id AND e.id='service-order-event-'||r.command_id AND r.operation='field.order.issue-part' AND r.record_id=s.id AND r.actor_id=e.actor_id AND r.actor_id=c.created_by AND r.authority_json::jsonb->>'companyId'=s.company_id
 JOIN finance_history_root_journals j ON j.id=c.journal_id AND j.company_id=s.company_id AND j.source_type='field_service_parts' AND j.source_id=c.id AND j.currency=c.currency
 JOIN field_history_finance_money m ON m.company_id=s.company_id AND m.source_type='operational_inventory_consumption' AND m.source_id=c.id AND m.amount_kind='stock_cost' AND m.currency=c.currency AND m.amount_units=c.value_minor
 WHERE e.details_json->>'quantityMilli'=c.quantity_milli::text AND e.details_json->>'costMinor'=c.value_minor::text
 AND (SELECT sum(debit_minor) FROM field_history_finance_journal_lines WHERE journal_id=j.id)=c.value_minor
 AND (SELECT sum(credit_minor) FROM field_history_finance_journal_lines WHERE journal_id=j.id)=c.value_minor
)
"#;

// Keep the parts supplement separate: legacy evaluates it only after the
// service-invoice capability has actually been selected. Similarly, a payment
// target must not force evaluation of unrelated journal branches.
const PAYMENT: &str = " SELECT EXISTS(SELECT 1 FROM owned_field_payment)";
const PARTS: &str = " SELECT EXISTS(SELECT 1 FROM owned_field_parts)";
const JOURNAL: &str = r#" SELECT EXISTS(
 SELECT company_id,id FROM owned_field_parts
 UNION SELECT i.company_id,r.id FROM owned_field_invoice i JOIN finance_history_roots r ON r.company_id=i.company_id AND r.id=i.journal_id
 UNION SELECT company_id,id FROM owned_field_payment
)"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldFinanceHistoryPhase {
    Inclusion,
    PartsSupplement,
}

/// Retains this one Field-source decision, not the other source families or any
/// personal authority. Revalidate after deferred constraints and all effects.
pub struct CurrentFieldFinanceHistory<'a, 'source> {
    finance: &'a CurrentFieldHistoryFinanceCandidates<'source>,
    money: &'a CurrentFieldHistoryMoneyLines<'a, 'source>,
    workflow: &'a CurrentFieldHistoryReceiptJoin,
    inventory: &'a CurrentFieldHistoryConsumptionJoin,
    field: &'a CurrentFieldHistoryJoin,
    phase: FieldFinanceHistoryPhase,
    decision: bool,
}
impl CurrentFieldFinanceHistory<'_, '_> {
    pub fn decision(&self) -> bool {
        self.decision
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if evaluate(
            &mut *c,
            self.finance,
            self.money,
            self.workflow,
            self.inventory,
            self.field,
            self.phase,
        )
        .await?
            != self.decision
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "FIELD_FINANCE_HISTORY_CHANGED",
                "The Field Finance History source changed.",
            )
            .into());
        }
        Ok(())
    }
}

pub async fn inspect_field_finance_history<'a, 'source>(
    c: &mut PgConnection,
    finance: &'a CurrentFieldHistoryFinanceCandidates<'source>,
    money: &'a CurrentFieldHistoryMoneyLines<'a, 'source>,
    workflow: &'a CurrentFieldHistoryReceiptJoin,
    inventory: &'a CurrentFieldHistoryConsumptionJoin,
    field: &'a CurrentFieldHistoryJoin,
    phase: FieldFinanceHistoryPhase,
) -> Result<CurrentFieldFinanceHistory<'a, 'source>, Error> {
    let decision = evaluate(c, finance, money, workflow, inventory, field, phase).await?;
    Ok(CurrentFieldFinanceHistory {
        finance,
        money,
        workflow,
        inventory,
        field,
        phase,
        decision,
    })
}

async fn evaluate(
    c: &mut PgConnection,
    finance: &CurrentFieldHistoryFinanceCandidates<'_>,
    money: &CurrentFieldHistoryMoneyLines<'_, '_>,
    workflow: &CurrentFieldHistoryReceiptJoin,
    inventory: &CurrentFieldHistoryConsumptionJoin,
    field: &CurrentFieldHistoryJoin,
    phase: FieldFinanceHistoryPhase,
) -> Result<bool, Error> {
    if !std::ptr::eq(money.source(), finance) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FIELD_FINANCE_HISTORY_BINDING",
            "The Finance History source changed.",
        )
        .into());
    }
    finance.require_current(&mut *c).await?;
    money.require_current(&mut *c).await?;
    workflow.require_current(&mut *c).await?;
    inventory.require_current(&mut *c).await?;
    field.require_current(&mut *c).await?;
    let selected = match (finance.kind_parameter(), phase) {
        ("payment", FieldFinanceHistoryPhase::Inclusion) => PAYMENT,
        ("journal", FieldFinanceHistoryPhase::Inclusion) => JOURNAL,
        ("journal", FieldFinanceHistoryPhase::PartsSupplement) => PARTS,
        _ => {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "FIELD_FINANCE_HISTORY_PHASE",
                "Choose a valid Finance History source phase.",
            )
            .into());
        }
    };
    let sql = format!(
        "{}, {}, {}, {}, {}{}{}",
        finance.cte(),
        workflow.cte_sql(),
        money.cte_sql(),
        inventory.cte_sql(),
        field.cte_sql(),
        JOINS,
        selected
    );
    let decision: bool = sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(finance.selected_id())
        .bind(finance.company_id())
        .bind(finance.kind_parameter())
        .fetch_one(&mut *c)
        .await?;
    finance.require_current(&mut *c).await?;
    money.require_current(&mut *c).await?;
    workflow.require_current(&mut *c).await?;
    inventory.require_current(&mut *c).await?;
    field.require_current(c).await?;
    Ok(decision)
}
