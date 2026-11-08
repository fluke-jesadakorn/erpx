//! Finance-owned Money evidence for Commerce order draft revisions.
//!
//! Commerce owns the order header/lines; this port owns the immutable
//! `money_transaction_snapshots` rows for the order total and for each line's
//! unit price, subtotal and tax. The caller supplies the one serializable
//! Holding transaction and never commits here.
use super::postgres_journal_facts::embedded_currency_rules;
use crate::domain::company_money::{FinanceCollation, convert_currency_units};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use futures_util::TryStreamExt;
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection, Row};
use std::cmp::Ordering;

type Error = PgTransactionError<BusinessError>;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

type StoredMoneyRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    i64,
);
type MoneyKey = (String, String, String);
type MoneyValue = (String, String, Option<String>, bool, i64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceOrderMoneyLine {
    pub id: String,
    pub unit_price: i64,
    pub subtotal: i64,
    pub tax: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceOrderMoneyInput {
    pub order_id: String,
    pub version: i64,
    pub transaction_date: String,
    pub currency: String,
    pub total: i64,
    pub lines: Vec<CommerceOrderMoneyLine>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceOrderMoneyEvidence {
    pub source_type: &'static str,
    pub source_id: String,
    pub amount_kind: &'static str,
    pub transaction_date: String,
    pub currency: String,
    pub currency_scale: i32,
    pub amount_units: String,
    pub usd_units: String,
    pub rate_id: Option<String>,
    pub used_latest_fallback: bool,
    pub captured_at: i64,
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_ORDER_MONEY_CHANGED",
        "The Commerce order Money source changed.",
    )
    .into()
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '?' | '#'))
}

fn valid_date(value: &str) -> bool {
    time::Date::parse(
        value,
        &time::macros::format_description!("[year]-[month]-[day]"),
    )
    .is_ok()
}

/// Verify the serializable unit and the exact Finance utility locks. The
/// coordinator acquires these before its owner write so every snapshot has a
/// stable Finance source image.
async fn unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    let (held, visible): (bool, bool) = sqlx::query_as(
        "SELECT (SELECT count(*)=3 AND bool_and(c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND CASE WHEN c.oid='public.money_transaction_snapshots'::regclass THEN l.mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') ELSE l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') END)) FROM pg_class c WHERE c.oid IN ('public.company_currency_scales'::regclass,'public.company_fx_rates'::regclass,'public.money_transaction_snapshots'::regclass)), NOT (row_security_active('public.company_currency_scales'::regclass) OR row_security_active('public.company_fx_rates'::regclass) OR row_security_active('public.money_transaction_snapshots'::regclass))",
    )
    .fetch_one(&mut *c)
    .await?;
    if !held {
        return Err(PersistenceError::new("COMMERCE_ORDER_MONEY_LOCKS_REQUIRED").into());
    }
    if !visible {
        return Err(PersistenceError::new("COMMERCE_ORDER_MONEY_RLS_UNSUPPORTED").into());
    }
    Ok(())
}

/// Lock the Finance Money sources in a stable order before any snapshot read.
pub async fn lock_commerce_order_money_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    for sql in [
        "LOCK TABLE public.company_currency_scales IN SHARE MODE NOWAIT",
        "LOCK TABLE public.company_fx_rates IN SHARE MODE NOWAIT",
        "LOCK TABLE public.money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE NOWAIT",
    ] {
        sqlx::query(sql)
            .execute(&mut *c)
            .await
            .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    }
    unit(c).await
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedRate {
    id: String,
    date: String,
    quote: String,
    confirmed_at: i64,
    fallback: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Quote {
    scale: Option<i64>,
    selected: Option<SelectedRate>,
}

async fn quote(
    c: &mut PgConnection,
    company: &CompanyId,
    currency: &str,
    date: &str,
) -> Result<Quote, Error> {
    let scale: Option<i64> = sqlx::query_scalar(
        "SELECT scale::bigint FROM company_currency_scales \
         WHERE company_id=$1 AND currency=$2 FOR SHARE",
    )
    .bind(company.as_str())
    .bind(currency)
    .fetch_optional(&mut *c)
    .await?;
    let coll = FinanceCollation::english().map_err(|_| changed())?;
    let mut stream = sqlx::query(
        "SELECT id,effective_date::text effective_date,currency_per_usd::text quote,\
         confirmed_at::bigint confirmed_at FROM company_fx_rates \
         WHERE company_id=$1 AND currency=$2 \
         ORDER BY effective_date DESC,confirmed_at DESC,id DESC FOR SHARE",
    )
    .bind(company.as_str())
    .bind(currency)
    .fetch(c);
    let mut dated: Option<SelectedRate> = None;
    let mut fallback: Option<SelectedRate> = None;
    while let Some(row) = stream.try_next().await? {
        let candidate = SelectedRate {
            id: row.try_get("id")?,
            date: row.try_get("effective_date")?,
            quote: row.try_get("quote")?,
            confirmed_at: row.try_get("confirmed_at")?,
            fallback: false,
        };
        let number_order = |a: i64, b: i64| {
            (a as f64)
                .partial_cmp(&(b as f64))
                .unwrap_or(Ordering::Equal)
        };
        let dated_order = |a: &SelectedRate, b: &SelectedRate| {
            coll.compare(&a.date, &b.date)
                .then_with(|| number_order(a.confirmed_at, b.confirmed_at))
                .then_with(|| coll.compare(&a.id, &b.id))
        };
        let fallback_order = |a: &SelectedRate, b: &SelectedRate| {
            number_order(a.confirmed_at, b.confirmed_at)
                .then_with(|| coll.compare(&a.id, &b.id))
                .then_with(|| coll.compare(&a.date, &b.date))
        };
        if candidate.date.as_str() <= date
            && dated
                .as_ref()
                .is_none_or(|old| dated_order(&candidate, old) == Ordering::Greater)
        {
            dated = Some(candidate.clone());
        }
        if fallback
            .as_ref()
            .is_none_or(|old| fallback_order(&candidate, old) == Ordering::Greater)
        {
            fallback = Some(candidate);
        }
    }
    let selected = if currency == "USD" {
        Some(SelectedRate {
            id: "USD".into(),
            date: date.into(),
            quote: "1".into(),
            confirmed_at: 0,
            fallback: false,
        })
    } else if dated.is_some() {
        dated
    } else {
        fallback.map(|mut rate| {
            rate.fallback = true;
            rate
        })
    };
    Ok(Quote { scale, selected })
}

fn valid_amount(amount: i64) -> bool {
    (0..=MAX_SAFE_INTEGER).contains(&amount)
}

/// Write the exact retained Commerce order-revision Money rows. The caller has
/// already locked the Finance sources and owns the serializable unit.
pub async fn snapshot_commerce_order_revision(
    c: &mut PgConnection,
    company: &CompanyId,
    input: &CommerceOrderMoneyInput,
    now: TimestampMillis,
) -> Result<Vec<CommerceOrderMoneyEvidence>, Error> {
    unit(c).await?;
    if !valid_identifier(&input.order_id)
        || input.version < 1
        || !valid_date(&input.transaction_date)
        || input.currency.len() != 3
        || !input.currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || !valid_amount(input.total)
        || input.lines.len() > 200
        || !input.lines.iter().all(|line| {
            valid_identifier(&line.id)
                && valid_amount(line.unit_price)
                && valid_amount(line.subtotal)
                && valid_amount(line.tax)
        })
        || input.lines.iter().any(|line| line.id == input.order_id)
    {
        return Err(invalid(
            "COMMERCE_ORDER_MONEY_INVALID",
            "Provide valid Commerce order Money facts.",
        ));
    }
    let quote = quote(c, company, &input.currency, &input.transaction_date).await?;
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    let configured = quote.scale;
    let scale = rules.scale(&input.currency, configured).map_err(|_| {
        invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    if scale != 2 {
        return Err(invalid(
            "CURRENCY_SCALE_MISMATCH",
            "A Commerce order amount must use the configured two-decimal scale.",
        ));
    }
    let rate = quote.selected.as_ref().ok_or_else(|| {
        invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency/USD rate is required.",
        )
    })?;
    let captured_at = now.get();
    if !(0..=MAX_SAFE_INTEGER).contains(&captured_at) {
        return Err(changed());
    }
    let order_source = format!("{}:{}", input.order_id, input.version);
    let mut rows = Vec::with_capacity(1 + input.lines.len() * 3);
    rows.push((
        "commerce_order_revision",
        order_source.clone(),
        "total",
        input.total,
    ));
    for line in &input.lines {
        let source = format!("{}:{}", line.id, input.version);
        rows.push((
            "commerce_order_line_revision",
            source.clone(),
            "unit_price",
            line.unit_price,
        ));
        rows.push((
            "commerce_order_line_revision",
            source.clone(),
            "subtotal",
            line.subtotal,
        ));
        rows.push(("commerce_order_line_revision", source, "tax", line.tax));
    }
    let mut evidence = Vec::with_capacity(rows.len());
    for (source_type, source_id, amount_kind, amount) in rows {
        let usd = convert_currency_units(&BigInt::from(amount), scale, 2, &rate.quote, "1")
            .map_err(|_| {
                invalid(
                    "MONEY_OVERFLOW",
                    "Money conversion exceeds its exact bounds.",
                )
            })?
            .to_string();
        if usd.len() > 38 {
            return Err(invalid(
                "MONEY_OVERFLOW",
                "Money evidence exceeds its numeric bounds.",
            ));
        }
        sqlx::query(
            "INSERT INTO money_transaction_snapshots\
             (company_id,source_type,source_id,amount_kind,transaction_date,currency,\
             currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
             VALUES($1,$2,$3,$4,$5::date,$6,$7,$8::numeric,$9::numeric,$10,$11,$12)",
        )
        .bind(company.as_str())
        .bind(source_type)
        .bind(&source_id)
        .bind(amount_kind)
        .bind(&input.transaction_date)
        .bind(&input.currency)
        .bind(i32::from(scale))
        .bind(amount.to_string())
        .bind(&usd)
        .bind((rate.id != "USD").then_some(rate.id.as_str()))
        .bind(rate.fallback)
        .bind(captured_at)
        .execute(&mut *c)
        .await?;
        evidence.push(CommerceOrderMoneyEvidence {
            source_type,
            source_id,
            amount_kind,
            transaction_date: input.transaction_date.clone(),
            currency: input.currency.clone(),
            currency_scale: i32::from(scale),
            amount_units: amount.to_string(),
            usd_units: usd,
            rate_id: (rate.id != "USD").then(|| rate.id.clone()),
            used_latest_fallback: rate.fallback,
            captured_at,
        });
    }
    // Re-read the exact persisted rows and the locked transaction identity
    // before the caller continues. Any drift fails the owner unit.
    let source_ids: Vec<String> = evidence.iter().map(|row| row.source_id.clone()).collect();
    let stored: Vec<StoredMoneyRow> = sqlx::query_as(
        "SELECT source_type,source_id,amount_kind,amount_units::text,usd_units::text,\
             fx_rate_id,used_latest_fallback,captured_at \
             FROM public.money_transaction_snapshots WHERE company_id=$1 \
             AND source_id = ANY($2) ORDER BY source_type,source_id,amount_kind",
    )
    .bind(company.as_str())
    .bind(&source_ids)
    .fetch_all(&mut *c)
    .await?;
    let expected: std::collections::BTreeMap<MoneyKey, MoneyValue> = evidence
        .iter()
        .map(|row| {
            (
                (
                    row.source_type.to_owned(),
                    row.source_id.clone(),
                    row.amount_kind.to_owned(),
                ),
                (
                    row.amount_units.clone(),
                    row.usd_units.clone(),
                    row.rate_id.clone(),
                    row.used_latest_fallback,
                    row.captured_at,
                ),
            )
        })
        .collect();
    if stored.len() != evidence.len()
        || stored.iter().any(|row| {
            expected.get(&(row.0.clone(), row.1.clone(), row.2.clone()))
                != Some(&(row.3.clone(), row.4.clone(), row.5.clone(), row.6, row.7))
        })
    {
        return Err(changed());
    }
    unit(c).await?;
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_amounts_are_bounded() {
        assert!(valid_identifier("order-1"));
        assert!(!valid_identifier(""));
        assert!(!valid_identifier("order/1"));
        assert!(!valid_identifier(&"x".repeat(201)));
        assert!(valid_amount(0));
        assert!(!valid_amount(-1));
        assert!(!valid_amount(MAX_SAFE_INTEGER + 1));
        assert!(valid_date("2026-11-09"));
        assert!(!valid_date("2026-02-30"));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommerceFulfillmentMoneyLine {
    pub id: String,
    pub subtotal: i64,
    pub tax: i64,
}

/// Write the exact `commerce_fulfillment_line` Money rows for one service
/// completion. The caller has locked the Finance sources and owns the
/// serializable unit.
pub async fn snapshot_commerce_fulfillment_money(
    c: &mut PgConnection,
    company: &CompanyId,
    transaction_date: &str,
    currency: &str,
    lines: &[CommerceFulfillmentMoneyLine],
    now: TimestampMillis,
) -> Result<Vec<CommerceOrderMoneyEvidence>, Error> {
    unit(c).await?;
    if !valid_date(transaction_date)
        || currency.len() != 3
        || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
        || lines.is_empty()
        || lines.len() > 200
        || lines.iter().any(|line| {
            !valid_identifier(&line.id) || !valid_amount(line.subtotal) || !valid_amount(line.tax)
        })
    {
        return Err(invalid(
            "COMMERCE_ORDER_MONEY_INVALID",
            "Provide valid Commerce fulfillment Money facts.",
        ));
    }
    let quote = quote(c, company, currency, transaction_date).await?;
    let rules = embedded_currency_rules().map_err(|_| {
        invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    let scale = rules.scale(currency, quote.scale).map_err(|_| {
        invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    if scale != 2 {
        return Err(invalid(
            "CURRENCY_SCALE_MISMATCH",
            "A Commerce fulfillment amount must use the configured two-decimal scale.",
        ));
    }
    let rate = quote.selected.as_ref().ok_or_else(|| {
        invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency/USD rate is required.",
        )
    })?;
    let captured_at = now.get();
    if !(0..=MAX_SAFE_INTEGER).contains(&captured_at) {
        return Err(changed());
    }
    let mut evidence = Vec::with_capacity(lines.len() * 3);
    for line in lines {
        for (amount_kind, amount) in [
            ("subtotal", line.subtotal),
            ("tax", line.tax),
            ("total", line.subtotal.saturating_add(line.tax)),
        ] {
            let usd = convert_currency_units(&BigInt::from(amount), scale, 2, &rate.quote, "1")
                .map_err(|_| {
                    invalid(
                        "MONEY_OVERFLOW",
                        "Money conversion exceeds its exact bounds.",
                    )
                })?
                .to_string();
            if usd.len() > 38 {
                return Err(invalid(
                    "MONEY_OVERFLOW",
                    "Money evidence exceeds its numeric bounds.",
                ));
            }
            sqlx::query(
                "INSERT INTO money_transaction_snapshots\
                 (company_id,source_type,source_id,amount_kind,transaction_date,currency,\
                 currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) \
                 VALUES($1,'commerce_fulfillment_line',$2,$3,$4::date,$5,$6,$7::numeric,$8::numeric,$9,$10,$11)",
            )
            .bind(company.as_str())
            .bind(&line.id)
            .bind(amount_kind)
            .bind(transaction_date)
            .bind(currency)
            .bind(i32::from(scale))
            .bind(amount.to_string())
            .bind(&usd)
            .bind((rate.id != "USD").then_some(rate.id.as_str()))
            .bind(rate.fallback)
            .bind(captured_at)
            .execute(&mut *c)
            .await?;
            evidence.push(CommerceOrderMoneyEvidence {
                source_type: "commerce_fulfillment_line",
                source_id: line.id.clone(),
                amount_kind,
                transaction_date: transaction_date.into(),
                currency: currency.into(),
                currency_scale: i32::from(scale),
                amount_units: amount.to_string(),
                usd_units: usd,
                rate_id: (rate.id != "USD").then(|| rate.id.clone()),
                used_latest_fallback: rate.fallback,
                captured_at,
            });
        }
    }
    let source_ids: Vec<String> = evidence.iter().map(|row| row.source_id.clone()).collect();
    let stored: Vec<StoredMoneyRow> = sqlx::query_as(
        "SELECT source_type,source_id,amount_kind,amount_units::text,usd_units::text,\
             fx_rate_id,used_latest_fallback,captured_at \
             FROM public.money_transaction_snapshots WHERE company_id=$1 \
             AND source_type='commerce_fulfillment_line' AND source_id = ANY($2) \
             ORDER BY source_id,amount_kind",
    )
    .bind(company.as_str())
    .bind(&source_ids)
    .fetch_all(&mut *c)
    .await?;
    let expected: std::collections::BTreeMap<MoneyKey, MoneyValue> = evidence
        .iter()
        .map(|row| {
            (
                (
                    row.source_type.to_owned(),
                    row.source_id.clone(),
                    row.amount_kind.to_owned(),
                ),
                (
                    row.amount_units.clone(),
                    row.usd_units.clone(),
                    row.rate_id.clone(),
                    row.used_latest_fallback,
                    row.captured_at,
                ),
            )
        })
        .collect();
    if stored.len() != evidence.len()
        || stored.iter().any(|row| {
            expected.get(&(row.0.clone(), row.1.clone(), row.2.clone()))
                != Some(&(row.3.clone(), row.4.clone(), row.5.clone(), row.6, row.7))
        })
    {
        return Err(changed());
    }
    unit(c).await?;
    Ok(evidence)
}
