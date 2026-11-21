//! Finance-owned goods-journal handoff for a native Commerce stock movement.
//!
//! Commerce retains the purchase order/sales order, fulfillment, Inventory
//! stock issue/receipt and command receipt. This adapter owns the functional-FX
//! valuation of the movement and persists one draft `goods_receipt` or
//! `delivery` journal inside the caller's serializable transaction. Posting and
//! GL review remain the existing Finance journal workflow.
//!
//! A purchase receipt debits Inventory and credits the goods clearing account in
//! the purchase document currency; its functional value is the Company rate at
//! the movement date. A sales delivery debits cost of sales and credits
//! Inventory in the Company base currency. Both reconcile the exact functional
//! stock value frozen by the caller, so the goods journal can never disagree
//! with Inventory valuation.
use crate::application::journal::{
    FinanceActor, FunctionalFxEvidence, JournalAuthority, JournalDraft, JournalStatus,
    prepare_journal,
};
use crate::domain::company_money::{FinanceCollation, ManualRate, functional_fx_evidence};
use crate::domain::journal::{JournalError, JournalLineInput, convert_at_fx};
use crate::domain::money::units_to_decimal;
use crate::public::{PgJournalPreparation, PgJournalStore, embedded_currency_rules};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE: i64 = 9_007_199_254_740_991;

/// Which goods book a Commerce stock movement retains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommerceGoodsKind {
    /// Purchase goods receipt: Inventory debit, goods clearing credit.
    Receipt { clearing_account_id: String },
    /// Sales delivery: cost-of-sales debit, Inventory credit.
    Delivery { cogs_account_id: String },
}

/// Exact Commerce-owned goods posting facts. The caller reads them from its
/// retained order/fulfillment/settings evidence; this type is not request or
/// client authority, and a `FinanceActor` alone grants no access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceGoodsJournalPosting {
    pub journal_id: String,
    pub actor: FinanceActor,
    pub chart_id: String,
    pub business_date: String,
    pub fulfillment_id: String,
    pub memo: String,
    pub inventory_account_id: String,
    pub kind: CommerceGoodsKind,
    /// Purchase document currency for a receipt; base currency for a delivery.
    pub document_currency: String,
    /// Net goods amount in `document_currency` minor units.
    pub document_subtotal_minor: i64,
    /// Functional (Company base-currency) stock value in minor units.
    pub stock_value_minor: i64,
    /// Optional caller proof of the functional rate it valued with. The adapter
    /// re-resolves the rate on this transaction and rejects a mismatch.
    pub fx_rate_micros: Option<i64>,
}

/// Finance-owned functional-FX request for one Commerce goods movement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceGoodsValuationRequest {
    pub actor: FinanceActor,
    pub chart_id: String,
    pub business_date: String,
    pub document_currency: String,
}

/// Current Company chart and functional quote for a goods movement. Construct
/// only through [`resolve_commerce_goods_valuation`] on the caller's unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceGoodsValuation {
    stock_currency: String,
    fx: FunctionalFxEvidence,
}

impl CommerceGoodsValuation {
    pub fn stock_currency(&self) -> &str {
        &self.stock_currency
    }
    pub fn fx(&self) -> &FunctionalFxEvidence {
        &self.fx
    }
    pub fn fx_rate_micros(&self) -> i64 {
        self.fx.micros
    }
    /// Convert a document-currency goods amount into the frozen functional stock
    /// value with the same half-away-from-zero rule as the journal lines.
    pub fn stock_value_minor(&self, document_minor: i64) -> Result<i64, Error> {
        if !(0..=MAX_SAFE).contains(&document_minor) {
            return Err(invalid(
                "GOODS_FINANCE_AMOUNT_INVALID",
                "The goods value is invalid.",
            ));
        }
        convert_at_fx(document_minor, self.fx.micros).map_err(|error| valuation_error(error))
    }
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

fn invalid_input(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
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

fn valuation_error(error: JournalError) -> Error {
    let code = match error.code {
        "MANUAL_RATE_REQUIRED" => "GOODS_FINANCE_RATE_REQUIRED",
        "FX_RATE_PRECISION" => "GOODS_FINANCE_RATE_INVALID",
        _ => "GOODS_FINANCE_VALUATION_INVALID",
    };
    invalid_input(
        code,
        "Confirm the Company functional-currency quote for this goods movement.",
    )
}

fn valid_currency(currency: &str) -> bool {
    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200
}

/// Resolve the current Company chart base currency and functional quote for a
/// goods movement on the caller's transaction. Row-share locks keep the quote
/// stable until the enclosing owner unit commits or rolls back.
pub async fn resolve_commerce_goods_valuation(
    c: &mut PgConnection,
    request: &CommerceGoodsValuationRequest,
) -> Result<CommerceGoodsValuation, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("GOODS_FINANCE_TRANSACTION_REQUIRED").into());
    }
    if !valid_identity(&request.chart_id)
        || !valid_currency(&request.document_currency)
        || request.business_date.len() != 10
    {
        return Err(invalid_input(
            "GOODS_FINANCE_VALUATION_REQUEST_INVALID",
            "Use a current goods movement, chart and currency.",
        ));
    }
    let stock_currency: Option<String> = sqlx::query_scalar(
        "SELECT base_currency FROM finance_charts WHERE id=$1 AND company_id=$2 \
         AND status='active' FOR SHARE",
    )
    .bind(&request.chart_id)
    .bind(request.actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(stock_currency) = stock_currency else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "GOODS_FINANCE_CHART_UNAVAILABLE",
            "The active Company chart is unavailable.",
        )
        .into());
    };
    let collation = FinanceCollation::english().map_err(|_| {
        invalid_input(
            "GOODS_FINANCE_COLLATION_UNAVAILABLE",
            "The Finance collation is unavailable.",
        )
    })?;
    let fx = if request.document_currency == stock_currency {
        functional_fx_evidence(
            &[],
            &request.document_currency,
            &stock_currency,
            &request.business_date,
            &collation,
        )
        .map_err(valuation_error)?
    } else {
        let rates = read_manual_rates(
            c,
            &request.actor,
            &[request.document_currency.clone(), stock_currency.clone()],
        )
        .await?;
        functional_fx_evidence(
            &rates,
            &request.document_currency,
            &stock_currency,
            &request.business_date,
            &collation,
        )
        .map_err(valuation_error)?
    };
    Ok(CommerceGoodsValuation { stock_currency, fx })
}

async fn read_manual_rates(
    c: &mut PgConnection,
    actor: &FinanceActor,
    currencies: &[String],
) -> Result<Vec<ManualRate>, Error> {
    let rows = sqlx::query(
        "SELECT id,currency,effective_date::text AS effective_date,\
         currency_per_usd::text AS currency_per_usd,confirmed_at \
         FROM company_fx_rates WHERE company_id=$1 AND currency=ANY($2) \
         ORDER BY currency,effective_date DESC,confirmed_at DESC,id DESC FOR SHARE",
    )
    .bind(actor.company_id.as_str())
    .bind(currencies.to_vec())
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            let quote: String = row.try_get("currency_per_usd")?;
            let quote = if quote.contains('.') {
                quote.trim_end_matches('0').trim_end_matches('.').to_owned()
            } else {
                quote
            };
            Ok(ManualRate {
                id: row.try_get("id")?,
                currency: row.try_get("currency")?,
                effective_date: row.try_get("effective_date")?,
                currency_per_usd: quote,
                confirmed_at: row.try_get("confirmed_at")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(Into::into)
}

/// Persist one balanced draft goods journal on the caller's serializable
/// transaction. Never commits and never writes Commerce, Inventory, Quality,
/// audit or outbox rows: the owning command keeps those on the same unit.
pub async fn post_commerce_goods_journal<C: Clock + Sync>(
    c: &mut PgConnection,
    clock: &C,
    posting: &CommerceGoodsJournalPosting,
) -> Result<String, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("GOODS_FINANCE_TRANSACTION_REQUIRED").into());
    }
    if !valid_identity(&posting.fulfillment_id)
        || !valid_identity(&posting.journal_id)
        || !valid_currency(&posting.document_currency)
        || !(1..=MAX_SAFE).contains(&posting.document_subtotal_minor)
        || !(1..=MAX_SAFE).contains(&posting.stock_value_minor)
        || posting.inventory_account_id.is_empty()
    {
        return Err(invalid_input(
            "GOODS_FINANCE_POSTING_INVALID",
            "Use a current goods movement and posting account.",
        ));
    }
    let valuation = resolve_commerce_goods_valuation(
        c,
        &CommerceGoodsValuationRequest {
            actor: posting.actor.clone(),
            chart_id: posting.chart_id.clone(),
            business_date: posting.business_date.clone(),
            document_currency: posting.document_currency.clone(),
        },
    )
    .await?;
    let (
        source_type,
        debit_account_id,
        credit_account_id,
        journal_currency,
        amount,
        debit_label,
        credit_label,
    ) = match &posting.kind {
        CommerceGoodsKind::Receipt {
            clearing_account_id,
        } => {
            if clearing_account_id.is_empty() {
                return Err(invalid_input(
                    "GOODS_FINANCE_POSTING_INVALID",
                    "Use a current goods movement and posting account.",
                ));
            }
            (
                "goods_receipt",
                &posting.inventory_account_id,
                clearing_account_id,
                posting.document_currency.as_str(),
                posting.document_subtotal_minor,
                "Inventory",
                "Goods clearing",
            )
        }
        CommerceGoodsKind::Delivery { cogs_account_id } => {
            if cogs_account_id.is_empty()
                || posting.document_currency != valuation.stock_currency()
                || posting.document_subtotal_minor != posting.stock_value_minor
            {
                return Err(invalid_input(
                    "GOODS_FINANCE_POSTING_INVALID",
                    "Value a delivery in the Company base currency.",
                ));
            }
            (
                "delivery",
                cogs_account_id,
                &posting.inventory_account_id,
                valuation.stock_currency(),
                posting.stock_value_minor,
                "Cost of sales",
                "Inventory",
            )
        }
    };
    if debit_account_id == credit_account_id {
        return Err(invalid_input(
            "GOODS_FINANCE_POSTING_INVALID",
            "Use distinct goods posting accounts.",
        ));
    }
    let expected_stock_value = valuation.stock_value_minor(posting.document_subtotal_minor)?;
    if expected_stock_value != posting.stock_value_minor {
        return Err(invalid(
            "GOODS_FINANCE_VALUATION_MISMATCH",
            "The goods journal must use the frozen functional stock value.",
        ));
    }
    if posting
        .fx_rate_micros
        .is_some_and(|rate| rate != valuation.fx_rate_micros())
    {
        return Err(invalid(
            "GOODS_FINANCE_RATE_CHANGED",
            "The Company functional-currency quote changed. Value the movement again.",
        ));
    }
    let rate = valuation.fx_rate_micros();
    let base_value = convert_at_fx(amount, rate).map_err(valuation_error)?;
    if replay_matches(
        c,
        posting,
        source_type,
        journal_currency,
        rate,
        debit_account_id,
        credit_account_id,
        amount,
        base_value,
    )
    .await?
    {
        return Ok(posting.journal_id.clone());
    }
    let value = units_to_decimal(&BigInt::from(amount), 2).map_err(|_| {
        invalid(
            "GOODS_FINANCE_AMOUNT_INVALID",
            "The goods value is invalid.",
        )
    })?;
    let fx_rate = units_to_decimal(&BigInt::from(rate), 6).map_err(|_| {
        invalid(
            "GOODS_FINANCE_RATE_INVALID",
            "The functional-currency rate is invalid.",
        )
    })?;
    let draft = JournalDraft {
        chart_id: posting.chart_id.clone(),
        period_id: String::new(),
        entry_date: posting.business_date.clone(),
        currency: journal_currency.to_owned(),
        fx_rate: Some(fx_rate),
        is_opening: false,
        memo: posting.memo.clone(),
        source_type: source_type.into(),
        source_id: Some(posting.fulfillment_id.clone()),
        lines: vec![
            JournalLineInput {
                account_id: debit_account_id.to_owned(),
                description: Some(debit_label.into()),
                debit: value.clone(),
                credit: "0".into(),
                dimensions: None,
            },
            JournalLineInput {
                account_id: credit_account_id.to_owned(),
                description: Some(credit_label.into()),
                debit: "0".into(),
                credit: value,
                dimensions: None,
            },
        ],
    };
    let period_id = open_period_id(c, posting).await?;
    let mut draft = draft;
    draft.period_id = period_id;
    let seed = posting.journal_id.strip_prefix("journal-").ok_or_else(|| {
        invalid_input(
            "GOODS_FINANCE_JOURNAL_ID_INVALID",
            "Use a journal-<source> goods journal identity.",
        )
    })?;
    let key = CommandKey::new(seed).map_err(|_| {
        invalid_input(
            "GOODS_FINANCE_SOURCE_KEY_INVALID",
            "The goods journal source key is invalid.",
        )
    })?;
    let mut authority = BoundAuthority {
        actor: posting.actor.clone(),
        draft: draft.clone(),
    };
    let rules = embedded_currency_rules().map_err(|_| {
        invalid_input(
            "GOODS_FINANCE_CURRENCY_UNAVAILABLE",
            "The Finance currency rules are unavailable.",
        )
    })?;
    let collation = FinanceCollation::english().map_err(|_| {
        invalid_input(
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
    if plan.fx() != rate {
        return Err(invalid(
            "GOODS_FINANCE_RATE_CHANGED",
            "The Company functional-currency quote changed. Value the movement again.",
        ));
    }
    let base_debit: i64 = plan
        .amounts()
        .lines
        .iter()
        .map(|line| line.base_debit)
        .sum();
    let base_credit: i64 = plan
        .amounts()
        .lines
        .iter()
        .map(|line| line.base_credit)
        .sum();
    if base_debit != posting.stock_value_minor || base_credit != posting.stock_value_minor {
        return Err(invalid(
            "GOODS_FINANCE_VALUATION_RECONCILIATION_REQUIRED",
            "The functional stock value must match the exact goods journal.",
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
         AND status='draft' AND source_type=$3 AND source_id=$4 AND currency=$5 \
         AND fx_rate_micros=$6)",
    )
    .bind(plan.journal_id())
    .bind(posting.actor.company_id.as_str())
    .bind(source_type)
    .bind(&posting.fulfillment_id)
    .bind(journal_currency)
    .bind(rate)
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

#[allow(clippy::too_many_arguments)]
async fn replay_matches(
    c: &mut PgConnection,
    posting: &CommerceGoodsJournalPosting,
    source_type: &str,
    journal_currency: &str,
    rate: i64,
    debit_account_id: &str,
    credit_account_id: &str,
    amount: i64,
    base_value: i64,
) -> Result<bool, Error> {
    let existing: Option<(String, Option<String>, String, String, Option<i64>, String)> =
        sqlx::query_as(
            "SELECT source_type,source_id,status,currency,fx_rate_micros,chart_id \
             FROM finance_journals WHERE id=$1 AND company_id=$2",
        )
        .bind(&posting.journal_id)
        .bind(posting.actor.company_id.as_str())
        .fetch_optional(&mut *c)
        .await?;
    let Some((stored_source_type, source_id, status, currency, fx_rate, chart_id)) = existing
    else {
        return Ok(false);
    };
    if stored_source_type != source_type
        || source_id.as_deref() != Some(posting.fulfillment_id.as_str())
        || status != "draft"
        || currency != journal_currency
        || fx_rate != Some(rate)
        || chart_id != posting.chart_id
    {
        return Err(invalid(
            "GOODS_FINANCE_JOURNAL_CHANGED",
            "The goods journal identity belongs to different evidence.",
        ));
    }
    let lines: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor \
         FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal",
    )
    .bind(&posting.journal_id)
    .fetch_all(&mut *c)
    .await?;
    if lines
        == [
            (debit_account_id.to_owned(), amount, 0, base_value, 0),
            (credit_account_id.to_owned(), 0, amount, 0, base_value),
        ]
    {
        return Ok(true);
    }
    Err(invalid(
        "GOODS_FINANCE_JOURNAL_CHANGED",
        "The goods journal identity belongs to different evidence.",
    ))
}

async fn open_period_id(
    c: &mut PgConnection,
    posting: &CommerceGoodsJournalPosting,
) -> Result<String, Error> {
    let period_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' \
         AND start_date<=$2 AND end_date>=$2 ORDER BY start_date DESC, id LIMIT 1",
    )
    .bind(posting.actor.company_id.as_str())
    .bind(&posting.business_date)
    .fetch_optional(&mut *c)
    .await?;
    period_id.ok_or_else(|| {
        invalid(
            "GOODS_FINANCE_PERIOD_CLOSED",
            "An open accounting period is required for the goods movement date.",
        )
    })
}
