//! Finance-owned goods-journal handoff for a native Commerce sales delivery.
//!
//! Commerce retains the sales order, fulfillment, Inventory stock issue and
//! command receipt. This adapter revalidates the exact chart, open period,
//! accounts, base currency and two balanced lines inside the caller's
//! serializable transaction and persists one draft `delivery` journal. Posting
//! and GL review remain the existing Finance journal workflow.
use crate::application::journal::{
    FinanceActor, JournalAuthority, JournalDraft, JournalStatus, prepare_journal,
};
use crate::domain::journal::{JournalError, JournalLineInput};
use crate::domain::money::units_to_decimal;
use crate::public::{
    FinanceCollation, PgJournalPreparation, PgJournalStore, embedded_currency_rules,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// Exact Commerce-owned goods posting facts. The caller reads them from its
/// retained settings/snapshot; this type is not request or client authority.
#[derive(Clone, Debug)]
pub struct CommerceGoodsJournalPosting {
    pub journal_id: String,
    pub actor: FinanceActor,
    pub chart_id: String,
    pub currency: String,
    pub business_date: String,
    pub fulfillment_id: String,
    pub memo: String,
    pub inventory_account_id: String,
    pub cogs_account_id: String,
    pub value_minor: i64,
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
        let same_lines = draft.lines.len() == self.draft.lines.len()
            && draft
                .lines
                .iter()
                .zip(&self.draft.lines)
                .all(|(actual, expected)| {
                    actual.account_id == expected.account_id
                        && actual.debit == expected.debit
                        && actual.credit == expected.credit
                        && actual.description == expected.description
                        && actual.dimensions == expected.dimensions
                });
        if actor != &self.actor
            || !same_lines
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
                code: "GOODS_FINANCE_BINDING_REQUIRED",
                message: "Use the currently authorized goods posting.".into(),
            }
            .into());
        }
        Ok(())
    }
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn journal_error(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Persistence(error) => PgTransactionError::Persistence(error),
        PgTransactionError::Business(error) => BusinessError::new(
            error.kind,
            error.code,
            "The goods Finance journal is unavailable.",
        )
        .into(),
    }
}

/// Persist one balanced draft delivery journal on the caller's serializable
/// transaction. The debit is cost of sales and the credit is inventory, both
/// in the exact stock/base currency, so the two lines must equal the Inventory
/// stock-issue value. Never commits.
pub async fn post_commerce_goods_journal<C: Clock + Sync>(
    c: &mut PgConnection,
    clock: &C,
    posting: &CommerceGoodsJournalPosting,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("GOODS_FINANCE_TRANSACTION_REQUIRED").into());
    }
    if posting.fulfillment_id.is_empty()
        || posting.fulfillment_id.len() > 200
        || posting.journal_id.is_empty()
        || posting.journal_id.len() > 200
        || posting.value_minor <= 0
        || posting.value_minor > 9_007_199_254_740_991
        || posting.inventory_account_id.is_empty()
        || posting.cogs_account_id.is_empty()
        || posting.inventory_account_id == posting.cogs_account_id
    {
        return Err(invalid(
            "GOODS_FINANCE_POSTING_INVALID",
            "Use a current goods delivery and posting account.",
        ));
    }
    let existing: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT source_type,source_id,status FROM finance_journals WHERE id=$1 AND company_id=$2",
    )
    .bind(&posting.journal_id)
    .bind(posting.actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some((source_type, source_id, status)) = existing {
        if source_type == "delivery"
            && source_id.as_deref() == Some(posting.fulfillment_id.as_str())
            && status == "draft"
        {
            return Ok(posting.journal_id.clone());
        }
        return Err(invalid(
            "GOODS_FINANCE_JOURNAL_CHANGED",
            "The goods journal identity belongs to different evidence.",
        ));
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
            "GOODS_FINANCE_PERIOD_CLOSED",
            "An open accounting period is required for the goods delivery date.",
        ));
    };
    let value = units_to_decimal(&BigInt::from(posting.value_minor), 2).map_err(|_| {
        invalid(
            "GOODS_FINANCE_AMOUNT_INVALID",
            "The goods value is invalid.",
        )
    })?;
    let draft = JournalDraft {
        chart_id: posting.chart_id.clone(),
        period_id,
        entry_date: posting.business_date.clone(),
        currency: posting.currency.clone(),
        fx_rate: None,
        is_opening: false,
        memo: posting.memo.clone(),
        source_type: "delivery".into(),
        source_id: Some(posting.fulfillment_id.clone()),
        lines: vec![
            JournalLineInput {
                account_id: posting.cogs_account_id.clone(),
                description: Some("Cost of sales".into()),
                debit: value.clone(),
                credit: "0".into(),
                dimensions: None,
            },
            JournalLineInput {
                account_id: posting.inventory_account_id.clone(),
                description: Some("Inventory".into()),
                debit: "0".into(),
                credit: value,
                dimensions: None,
            },
        ],
    };
    let seed = posting.journal_id.strip_prefix("journal-").ok_or_else(|| {
        invalid(
            "GOODS_FINANCE_JOURNAL_ID_INVALID",
            "Use a journal-<source> goods journal identity.",
        )
    })?;
    let key = CommandKey::new(seed).map_err(|_| {
        invalid(
            "GOODS_FINANCE_SOURCE_KEY_INVALID",
            "The goods journal source key is invalid.",
        )
    })?;
    let mut authority = BoundAuthority {
        actor: posting.actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "GOODS_FINANCE_CURRENCY_UNAVAILABLE",
            "The Finance currency rules are unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid(
            "GOODS_FINANCE_COLLATION_UNAVAILABLE",
            "The Finance collation is unavailable.",
        )
    })?;
    let plan = {
        let mut ports =
            PgJournalPreparation::new(c, &posting.actor, &mut authority, clock, &rules, &collation)
                .map_err(journal_error)?;
        prepare_journal(
            &mut ports,
            &posting.actor,
            &draft,
            &key,
            JournalStatus::Draft,
        )
        .await
        .map_err(journal_error)?
    };
    if plan.journal_id() != posting.journal_id {
        return Err(invalid(
            "GOODS_FINANCE_JOURNAL_ID_CHANGED",
            "The goods journal identity changed before persistence.",
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
        "SELECT EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND company_id=$2 \
         AND status='draft' AND source_type='delivery' AND source_id=$3)",
    )
    .bind(plan.journal_id())
    .bind(posting.actor.company_id.as_str())
    .bind(&posting.fulfillment_id)
    .fetch_one(&mut *c)
    .await?;
    if !persisted {
        return Err(invalid(
            "GOODS_FINANCE_JOURNAL_UNAVAILABLE",
            "The goods Finance journal was not persisted.",
        ));
    }
    Ok(plan.journal_id().to_owned())
}
