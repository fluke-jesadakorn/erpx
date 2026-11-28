use super::image::{configuration, integer, optional_text, text};
use super::*;
use crate::adapters::postgres_journal_facts::embedded_currency_rules;
use crate::domain::company_money::{
    FinanceCollation, ManualRate, convert_currency_units, resolve_manual_rate,
};
use crate::domain::journal_reversal_money::JournalReversalMoneyMode;
use num_bigint::BigInt;
use serde_json::json;

fn fresh(
    source: &PgJournalReviewSource,
    currency: &str,
    date: &str,
    amount: &str,
) -> Result<(String, Option<String>, bool), Error> {
    let rules =
        embedded_currency_rules().map_err(|e| JournalError::new("CURRENCY_SCALE_REQUIRED", e.0))?;
    let custom = configuration(&source.before, "scales")?
        .iter()
        .find(|s| s["currency"] == currency)
        .map(|s| integer(s, "scale"))
        .transpose()?;
    if rules
        .scale(currency, custom)
        .map_err(|e| JournalError::new("CURRENCY_SCALE_REQUIRED", e.0))?
        != 2
    {
        return Err(JournalError::new("FINANCE_CURRENCY_SCALE_UNSUPPORTED","This Finance ledger currently accepts currencies with two minor-unit decimals. Historical amounts retain their recorded scale; use a supported currency for a new document.").into());
    }
    let rates = configuration(&source.before, "rates")?
        .iter()
        .map(|r| {
            Ok(ManualRate {
                id: text(r, "id")?.into(),
                currency: text(r, "currency")?.into(),
                effective_date: text(r, "effective_date")?.into(),
                currency_per_usd: text(r, "currency_per_usd")?.into(),
                confirmed_at: integer(r, "confirmed_at")?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let collation = FinanceCollation::english()
        .map_err(|e| JournalError::new("MONEY_COLLATION_UNAVAILABLE", e.0))?;
    let rate = resolve_manual_rate(&rates, currency, date, &collation).ok_or_else(|| {
        JournalError::new(
            "MANUAL_RATE_REQUIRED",
            format!("A confirmed manual {currency}/USD rate is required."),
        )
    })?;
    let amount: BigInt = amount.parse().map_err(|_| changed())?;
    let usd = convert_currency_units(&amount, 2, 2, &rate.rate.currency_per_usd, "1")
        .map_err(|e| JournalError::new("MONEY_OVERFLOW", e.0))?;
    Ok((
        usd.to_string(),
        (rate.rate.id != "USD").then_some(rate.rate.id),
        rate.used_latest_fallback,
    ))
}
pub(super) fn prepare(
    source: &PgJournalReviewSource,
    plan: &JournalReviewPlan,
    captured_at: i64,
) -> Result<Value, Error> {
    let (mut source_id, mut date, mut currency) = (
        source.journal.id.clone(),
        source.journal.entry_date.clone(),
        source.journal.currency.clone(),
    );
    let amount: String;
    let (usd, rate, fallback, original, carried) = match plan.action() {
        JournalReviewAction::Post {
            debit_total_units, ..
        } => {
            amount = debit_total_units.clone();
            let (usd, rate, fallback) = fresh(source, &currency, &date, &amount)?;
            (usd, rate, fallback, None, None)
        }
        JournalReviewAction::Reverse {
            reversal_id,
            entry_date,
            money,
            ..
        } => {
            source_id = reversal_id.clone();
            date = entry_date.clone();
            currency = money.currency.clone();
            amount = money.amount_units.clone();
            if money.mode == JournalReversalMoneyMode::CaptureNew {
                let (usd, rate, fallback) = fresh(source, &currency, &date, &amount)?;
                (usd, rate, fallback, None, None)
            } else {
                let selected = money.source_id.clone().ok_or_else(changed)?;
                let row = source
                    .before
                    .money
                    .iter()
                    .find(|m| m["source_id"] == selected && m["amount_kind"] == "debit_total")
                    .ok_or_else(changed)?;
                if matches!(
                    money.mode,
                    JournalReversalMoneyMode::CarryPositive
                        | JournalReversalMoneyMode::ReversePositive
                ) && !row["original_source_type"].is_null()
                {
                    return Err(changed());
                }
                (
                    money.usd_units.clone().ok_or_else(changed)?,
                    money.fx_rate_id.clone(),
                    money.used_latest_fallback.ok_or_else(changed)?,
                    (money.mode == JournalReversalMoneyMode::ReversePositive)
                        .then_some(selected.clone()),
                    (money.mode == JournalReversalMoneyMode::CarryPositive).then_some(selected),
                )
            }
        }
    };
    for value in [&amount, &usd] {
        let digits = value.strip_prefix('-').unwrap_or(value);
        if digits.is_empty() || digits.len() > 38 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(JournalError::new(
                "MONEY_OVERFLOW",
                "Money evidence exceeds the original NUMERIC(38,0) precision.",
            )
            .into());
        }
    }
    Ok(
        json!({"company_id":source.actor.company_id.as_str(),"source_type":"finance_journal","source_id":source_id,"amount_kind":"debit_total","transaction_date":date,"currency":currency,"currency_scale":2,"amount_units":amount,"usd_units":usd,"fx_rate_id":rate,"used_latest_fallback":fallback,"captured_at":captured_at,
        "original_source_type":original.as_ref().map(|_|"finance_journal"),"original_source_id":original,"original_amount_kind":original.as_ref().map(|_|"debit_total"),
        "carried_source_type":carried.as_ref().map(|_|"finance_journal"),"carried_source_id":carried,"carried_amount_kind":carried.as_ref().map(|_|"debit_total"),"carried_allocation_offset_units":null}),
    )
}
pub(super) async fn insert(c: &mut PgConnection, row: &Value) -> Result<(), Error> {
    // Keep both original allocation triggers enabled. The reversal trigger
    // computes the retained USD amount from its cumulative positive origin.
    let usd = if row["original_source_type"].is_null() {
        text(row, "usd_units")?
    } else {
        "0"
    };
    sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,original_source_type,original_source_id,original_amount_kind,carried_source_type,carried_source_id,carried_amount_kind,carried_allocation_offset_units) VALUES($1,'finance_journal',$2,'debit_total',$3::date,$4,$5,$6::numeric,$7::numeric,$8,$9,$10,$11,$12,$13,$14,$15,$16,NULL)")
        .bind(text(row,"company_id")?).bind(text(row,"source_id")?).bind(text(row,"transaction_date")?).bind(text(row,"currency")?).bind(i16::try_from(integer(row,"currency_scale")?).map_err(|_|changed())?).bind(text(row,"amount_units")?).bind(usd).bind(optional_text(row,"fx_rate_id")?).bind(row["used_latest_fallback"].as_bool().ok_or_else(changed)?).bind(integer(row,"captured_at")?).bind(optional_text(row,"original_source_type")?).bind(optional_text(row,"original_source_id")?).bind(optional_text(row,"original_amount_kind")?).bind(optional_text(row,"carried_source_type")?).bind(optional_text(row,"carried_source_id")?).bind(optional_text(row,"carried_amount_kind")?).execute(c).await?;
    Ok(())
}
