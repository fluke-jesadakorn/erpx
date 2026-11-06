//! Inventory-owned stock consumption for the native POS sale completion.
//!
//! POS supplies the already-authorized fulfilment ingredients and the value it
//! captured from the availability projection. Inventory re-reads the locked
//! stock rows, applies the exact weighted value, and fails closed if the
//! expected value no longer matches.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosConsumptionPart {
    pub fulfillment_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub expected_value_minor: i64,
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn scaled(amount: i64, numerator: i64, denominator: i64) -> Option<i64> {
    if amount < 0 || numerator <= 0 || denominator <= 0 {
        return None;
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    i64::try_from(value).ok()
}

/// Write the POS sale consumption on the caller's open serializable
/// transaction. Returns the exact cost written; never commits.
pub async fn consume_pos_parts(
    c: &mut PgConnection,
    parts: &[PosConsumptionPart],
) -> Result<i64, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("POS_INVENTORY_TRANSACTION_REQUIRED").into());
    }
    let mut total = 0i64;
    for part in parts {
        if part.quantity_milli <= 0
            || part.expected_value_minor < 0
            || part.fulfillment_id.is_empty()
            || part.warehouse_id.is_empty()
            || part.product_id.is_empty()
        {
            return Err(conflict(
                "POS_STOCK_INVALID",
                "Review the stock consumption.",
            ));
        }
        let stock: Option<(i64, i64)> = sqlx::query_as(
            "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2 FOR UPDATE",
        )
        .bind(&part.warehouse_id)
        .bind(&part.product_id)
        .fetch_optional(&mut *c)
        .await?;
        let Some((quantity, value)) = stock else {
            return Err(conflict(
                "POS_STOCK_UNAVAILABLE",
                "Insufficient product or ingredients for this item.",
            ));
        };
        if quantity < part.quantity_milli || value < 0 {
            return Err(conflict(
                "POS_STOCK_UNAVAILABLE",
                "Insufficient product or ingredients for this item.",
            ));
        }
        let consumed = if quantity == part.quantity_milli {
            value
        } else {
            scaled(value, part.quantity_milli, quantity).ok_or_else(|| {
                conflict("POS_STOCK_CHANGED", "Stock changed. Review it before closing the bill.")
            })?
        };
        if consumed != part.expected_value_minor || consumed > value {
            return Err(conflict(
                "POS_STOCK_CHANGED",
                "Stock changed. Review it before closing the bill.",
            ));
        }
        sqlx::query(
            "UPDATE commerce_stock SET quantity_milli=quantity_milli-$1,value_minor=value_minor-$2,version=version+1 WHERE warehouse_id=$3 AND product_id=$4",
        )
        .bind(part.quantity_milli)
        .bind(consumed)
        .bind(&part.warehouse_id)
        .bind(&part.product_id)
        .execute(&mut *c)
        .await?;
        total = total.checked_add(consumed).ok_or_else(|| {
            conflict("POS_STOCK_CHANGED", "Review the stock value.")
        })?;
    }
    Ok(total)
}
