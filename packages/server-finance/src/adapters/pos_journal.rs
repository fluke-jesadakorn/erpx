//! POS owner journal posting on the Finance unit. POS proves its own register
//! session command authority; this adapter rechecks the exact captured draft
//! and writes the canonical Finance journal inside the same serializable
//! transaction. A caller-supplied journal id is derived from the POS source key
//! so its deferred posting cannot silently change identity.
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
};
use crate::domain::journal::{JournalError, JournalLineInput};
use crate::domain::money::units_to_decimal;
use crate::public::{FinanceCollation, PgJournalPreparation, PgJournalStore, embedded_currency_rules};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosJournalLine {
    pub account_id: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

/// Exact POS-owned posting facts. The POS command supplies them from its own
/// retained settings/session read; they are not provider or request authority.
#[derive(Clone, Debug)]
pub struct PosJournalPosting {
    pub journal_id: String,
    pub actor: FinanceActor,
    pub chart_id: String,
    pub currency: String,
    pub business_date: String,
    pub source_key: String,
    pub source_id: String,
    pub memo: String,
    pub lines: Vec<PosJournalLine>,
}

struct BoundAuthority {
    actor: FinanceActor,
    draft: JournalDraft,
}

impl JournalAuthority for BoundAuthority {
    type Error = PgTransactionError<JournalError>;

    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        let lines = draft.lines.len() == self.draft.lines.len()
            && draft.lines.iter().zip(&self.draft.lines).all(|(a, b)| {
                a.account_id == b.account_id
                    && a.debit == b.debit
                    && a.credit == b.credit
                    && a.description == b.description
                    && a.dimensions == b.dimensions
            });
        if actor != &self.actor
            || !lines
            || draft.chart_id != self.draft.chart_id
            || draft.period_id != self.draft.period_id
            || draft.entry_date != self.draft.entry_date
            || draft.currency != self.draft.currency
            || draft.fx_rate != self.draft.fx_rate
            || draft.memo != self.draft.memo
            || draft.source_type != self.draft.source_type
            || draft.source_id != self.draft.source_id
            || draft.is_opening
        {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "POS_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized POS posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn journal_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => {
            BusinessError::new(error.kind, error.code, "The POS Finance posting is unavailable.").into()
        }
    }
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

/// Post the POS journal on the caller's open Finance transaction. The entry
/// date must fall in an open period; the canonical preparation revalidates the
/// chart, accounts, currency and functional FX evidence. Never commits.
pub async fn post_pos_journal<C: Clock + Sync>(
    c: &mut PgConnection,
    clock: &C,
    posting: &PosJournalPosting,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "POS_FINANCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    let period_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2 ORDER BY start_date DESC, id LIMIT 1",
    )
    .bind(posting.actor.company_id.as_str())
    .bind(&posting.business_date)
    .fetch_optional(&mut *c)
    .await?;
    let Some(period_id) = period_id else {
        return Err(invalid(
            "POS_PERIOD_CLOSED",
            "An open accounting period is required for this date.",
        ));
    };
    let lines = posting
        .lines
        .iter()
        .map(|line| {
            Ok(JournalLineInput {
                account_id: line.account_id.clone(),
                description: None,
                debit: units_to_decimal(&BigInt::from(line.debit_minor), 2)
                    .map_err(|_| invalid("POS_FINANCE_LINE_INVALID", "The POS posting amount is invalid."))?,
                credit: units_to_decimal(&BigInt::from(line.credit_minor), 2)
                    .map_err(|_| invalid("POS_FINANCE_LINE_INVALID", "The POS posting amount is invalid."))?,
                dimensions: None,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let draft = JournalDraft {
        chart_id: posting.chart_id.clone(),
        period_id,
        entry_date: posting.business_date.clone(),
        currency: posting.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo: posting.memo.clone(),
        source_type: "pos".into(),
        source_id: Some(posting.source_id.clone()),
        lines,
    };
    let key = CommandKey::new(posting.source_key.clone()).map_err(|_| {
        invalid(
            "POS_FINANCE_SOURCE_KEY_INVALID",
            "The POS posting source key is invalid.",
        )
    })?;
    let mut authority = BoundAuthority {
        actor: posting.actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "POS_FINANCE_CURRENCY_UNAVAILABLE",
            "The Finance currency rules are unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid(
            "POS_FINANCE_COLLATION_UNAVAILABLE",
            "The Finance collation is unavailable.",
        )
    })?;
    let plan = {
        let mut ports = PgJournalPreparation::new(
            c,
            &posting.actor,
            &mut authority,
            clock,
            &rules,
            &collation,
        )
        .map_err(journal_error)?;
        prepare_journal(&mut ports, &posting.actor, &draft, &key, JournalStatus::Posted)
            .await
            .map_err(journal_error)?
    };
    if plan.journal_id() != posting.journal_id {
        return Err(invalid(
            "POS_FINANCE_JOURNAL_ID_CHANGED",
            "The POS journal identity changed before posting.",
        ));
    }
    PgJournalStore::new(
        posting.actor.holding_id.clone(),
        posting.actor.company_id.clone(),
        c,
    )
    .persist(&plan)
    .await
    .map_err(journal_error)?;
    let persisted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND company_id=$2 AND status='posted')",
    )
    .bind(plan.journal_id())
    .bind(posting.actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    if !persisted {
        return Err(invalid(
            "POS_FINANCE_JOURNAL_UNAVAILABLE",
            "The POS journal was not persisted.",
        ));
    }
    Ok(plan.journal_id().to_owned())
}

/// Exact Finance receipt facts supplied by the authorized POS payment command.
#[derive(Clone, Debug)]
pub struct PosReceiptRecord {
    pub finance_payment_id: String,
    pub company_id: String,
    pub party_id: Option<String>,
    pub payment_number: String,
    pub payment_date: String,
    pub currency: String,
    pub amount_minor: i64,
    pub account_id: String,
    pub journal_id: String,
    pub actor_id: String,
    pub now: i64,
}

/// Write the settled receipt row on the caller's open Finance transaction. The
/// journal keeps the deterministic id and is posted separately by
/// `post_pos_journal`; the payment row remains the owner receipt evidence.
pub async fn insert_pos_receipt(
    c: &mut PgConnection,
    receipt: &PosReceiptRecord,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "POS_FINANCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    if receipt.amount_minor <= 0
        || receipt.company_id.is_empty()
        || receipt.payment_number.is_empty()
        || receipt.journal_id.is_empty()
    {
        return Err(invalid(
            "POS_FINANCE_RECEIPT_INVALID",
            "The POS receipt facts are invalid.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) \
         VALUES($1,$2,$3,'receipt',$4,$5,$6,$7,0,$8,'settled',$9,$10,$10,$11,$11)",
    )
    .bind(&receipt.finance_payment_id)
    .bind(&receipt.company_id)
    .bind(&receipt.party_id)
    .bind(&receipt.payment_number)
    .bind(&receipt.payment_date)
    .bind(&receipt.currency)
    .bind(receipt.amount_minor)
    .bind(&receipt.account_id)
    .bind(&receipt.journal_id)
    .bind(&receipt.actor_id)
    .bind(receipt.now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Exact Finance invoice facts supplied by the authorized POS completion.
#[derive(Clone, Debug)]
pub struct PosInvoiceRecord {
    pub invoice_id: String,
    pub company_id: String,
    pub party_id: Option<String>,
    pub number: String,
    pub invoice_date: String,
    pub currency: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub receivable_account_id: String,
    pub revenue_account_id: String,
    pub tax_account_id: String,
    pub journal_id: String,
    pub source_id: String,
    pub actor_id: String,
    pub now: i64,
}

/// Write the paid POS receivable invoice on the caller's open Finance
/// transaction. The deterministic sale journal must already be posted.
pub async fn insert_pos_invoice(
    c: &mut PgConnection,
    invoice: &PosInvoiceRecord,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "POS_FINANCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    if invoice.subtotal_minor < 0
        || invoice.tax_minor < 0
        || invoice.total_minor < 0
        || invoice.subtotal_minor + invoice.tax_minor != invoice.total_minor
        || invoice.invoice_id.is_empty()
        || invoice.company_id.is_empty()
        || invoice.journal_id.is_empty()
    {
        return Err(invalid(
            "POS_FINANCE_INVOICE_INVALID",
            "The POS invoice facts are invalid.",
        ));
    }
    let party = match invoice.party_id.as_deref().filter(|value| !value.is_empty()) {
        Some(party) => party.to_owned(),
        None => {
            let party = format!("pos-walkin-{}", invoice.company_id);
            sqlx::query(
                "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) \
                 VALUES($1,$2,'customer','POS-WALKIN','POS walk-in customer',$3,'active',$4,$4) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&party)
            .bind(&invoice.company_id)
            .bind(&invoice.currency)
            .bind(invoice.now)
            .execute(&mut *c)
            .await?;
            party
        }
    };
    sqlx::query(
        "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) \
         VALUES($1,$2,$3,'receivable',$4,$5,$5,$6,$7,$8,$9,0,$10,$11,$12,'pos',$13,'paid',$14,$15,$15,$16,$16,$16)",
    )
    .bind(&invoice.invoice_id)
    .bind(&invoice.company_id)
    .bind(&party)
    .bind(&invoice.number)
    .bind(&invoice.invoice_date)
    .bind(&invoice.currency)
    .bind(invoice.subtotal_minor)
    .bind(invoice.tax_minor)
    .bind(invoice.total_minor)
    .bind(&invoice.receivable_account_id)
    .bind(&invoice.revenue_account_id)
    .bind(&invoice.tax_account_id)
    .bind(&invoice.source_id)
    .bind(&invoice.journal_id)
    .bind(&invoice.actor_id)
    .bind(invoice.now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

/// Write the settled refund payment row on the caller's open Finance
/// transaction. The refund journal must already be posted.
pub async fn insert_pos_refund_payment(
    c: &mut PgConnection,
    receipt: &PosReceiptRecord,
) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "POS_FINANCE_TRANSACTION_REQUIRED",
            )
            .into(),
        );
    }
    if receipt.amount_minor <= 0
        || receipt.company_id.is_empty()
        || receipt.payment_number.is_empty()
        || receipt.journal_id.is_empty()
    {
        return Err(invalid(
            "POS_FINANCE_REFUND_INVALID",
            "The POS refund payment facts are invalid.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) \
         VALUES($1,$2,$3,'refund',$4,$5,$6,$7,0,$8,'settled',$9,$10,$10,$11,$11)",
    )
    .bind(&receipt.finance_payment_id)
    .bind(&receipt.company_id)
    .bind(&receipt.party_id)
    .bind(&receipt.payment_number)
    .bind(&receipt.payment_date)
    .bind(&receipt.currency)
    .bind(receipt.amount_minor)
    .bind(&receipt.account_id)
    .bind(&receipt.journal_id)
    .bind(&receipt.actor_id)
    .bind(receipt.now)
    .execute(&mut *c)
    .await?;
    Ok(())
}
