//! Native Company Money read ports backing the retired Finance TypeScript
//! rate-list and currency-scale bridges. Every SQL statement lives here; the
//! TypeScript clients carry no SQL and offer no fallback.
use crate::domain::company_money::{ManualRate, rate_decimal_units};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Normalize a stored USD quote the same way the retired `companyRates` bridge
/// did, validating its precision with the shared Finance domain rule first.
fn normalize_quote(value: &str) -> Result<String, Error> {
    rate_decimal_units(value, 18).map_err(|_| {
        BusinessError::invalid(
            "FX_RATE_PRECISION",
            "Confirm a manual USD rate with supported precision.",
        )
    })?;
    if value.contains('.') {
        let trimmed = value.trim_end_matches('0').trim_end_matches('.');
        Ok(if trimmed.is_empty() {
            "0".to_owned()
        } else {
            trimmed.to_owned()
        })
    } else {
        Ok(value.to_owned())
    }
}

/// Company currency scale row, or `None` when no scale was configured. The
/// ISO/company scale decision remains in the TS consumer, unchanged.
pub async fn company_currency_scale(
    c: &mut PgConnection,
    company: &str,
    currency: &str,
) -> Result<Option<i64>, Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT scale::bigint FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
    )
    .bind(company)
    .bind(currency)
    .fetch_optional(&mut *c)
    .await
    .map_err(Into::into)
}

/// All confirmed manual FX rates for a Company, in the retained `companyRates`
/// order. The SQL is owned here; the domain `ManualRate` is the wire model.
pub async fn company_fx_rates(
    c: &mut PgConnection,
    company: &str,
) -> Result<Vec<ManualRate>, Error> {
    let rows = sqlx::query(
        "SELECT id,currency,effective_date::text effective_date,currency_per_usd::text quote,confirmed_at::bigint confirmed_at \
         FROM company_fx_rates WHERE company_id=$1 \
         ORDER BY effective_date DESC,confirmed_at DESC,id DESC",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            let quote = row.try_get::<String, _>("quote")?;
            Ok(ManualRate {
                id: row.try_get("id")?,
                currency: row.try_get("currency")?,
                effective_date: row
                    .try_get::<String, _>("effective_date")?
                    .chars()
                    .take(10)
                    .collect(),
                currency_per_usd: normalize_quote(&quote)?,
                confirmed_at: row.try_get("confirmed_at")?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalize_quote;

    #[test]
    fn quote_trims_trailing_zeros_like_the_retired_bridge() {
        assert_eq!(normalize_quote("32.500").unwrap(), "32.5");
        assert_eq!(normalize_quote("0.000").unwrap(), "0");
        assert_eq!(normalize_quote("1").unwrap(), "1");
        assert_eq!(normalize_quote("1.0").unwrap(), "1");
    }

    #[test]
    fn malformed_quote_is_refused() {
        assert!(normalize_quote("not-a-rate").is_err());
    }
}
