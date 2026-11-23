//! Commerce-owned source witness for the native Ecommerce listing command.
//!
//! An active product and the Company's configured functional currency are
//! Commerce facts: product activity lives in `commerce_products`, the Company's
//! configured currency in `commerce_settings` joined to the Finance chart base
//! currency. The caller's serializable Holding transaction supplies the read
//! unit; these functions never commit and never write another owner's tables.
//!
//! The SQL mirrors the retained Commerce oracle exactly so the native port is a
//! faithful replacement rather than a new business rule.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

/// The Company's configured listing currency and its Finance chart base.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EcommerceListingCurrency {
    pub currency: String,
    pub base_currency: String,
}

fn source_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_LISTING_SOURCE_UNAVAILABLE",
        "The Commerce listing source could not be read.",
    )
}

/// Fail closed on malformed currency codes instead of projecting an invalid
/// configuration into the Ecommerce listing command.
fn configured(row: Option<(String, String)>) -> Option<EcommerceListingCurrency> {
    let (currency, base_currency) = row?;
    let valid = |value: &str| value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase());
    if valid(&currency) && valid(&base_currency) {
        Some(EcommerceListingCurrency {
            currency,
            base_currency,
        })
    } else {
        None
    }
}

/// Whether `product_id` is a current active product in `company`. The caller
/// holds the serializable Holding transaction and the Commerce catalogue lock.
pub async fn active_product(
    connection: &mut PgConnection,
    company: &CompanyId,
    product_id: &str,
) -> Result<bool, BusinessError> {
    if !connection.is_in_transaction() {
        return Err(source_unavailable());
    }
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM commerce_products \
         WHERE id=$1 AND company_id=$2 AND status='active')",
    )
    .bind(product_id)
    .bind(company.as_str())
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| source_unavailable())?;
    Ok(active)
}

/// The Company's configured functional currency and Finance chart base, or
/// `None` when no valid configuration exists. The caller holds the serializable
/// Holding transaction and the Commerce settings lock.
pub async fn functional_currency(
    connection: &mut PgConnection,
    company: &CompanyId,
) -> Result<Option<EcommerceListingCurrency>, BusinessError> {
    if !connection.is_in_transaction() {
        return Err(source_unavailable());
    }
    let row = sqlx::query(
        "SELECT s.currency,c.base_currency FROM commerce_settings s \
         JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id \
         WHERE s.company_id=$1",
    )
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| source_unavailable())?;
    Ok(row.and_then(|row| {
        configured(Some((row.get("currency"), row.get("base_currency"))))
    }))
}

#[cfg(test)]
mod tests {
    use super::configured;

    #[test]
    fn configured_rejects_missing_and_malformed_codes() {
        assert_eq!(configured(None), None);
        assert_eq!(
            configured(Some(("THB".into(), "THB".into()))),
            Some(super::EcommerceListingCurrency {
                currency: "THB".into(),
                base_currency: "THB".into(),
            })
        );
        assert_eq!(configured(Some(("thb".into(), "THB".into()))), None);
        assert_eq!(configured(Some(("THB".into(), "TH".into()))), None);
        assert_eq!(configured(Some(("".into(), "THB".into()))), None);
    }
}
