//! Pure domain kind registry for native Commerce owner writes.
//!
//! Closed owner-owned kinds for the retired Commerce TS write literals. No IO,
//! no SQL and no fallback: the application executor and owner Postgres adapter
//! own the actual write.
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

pub const OWNER_WRITE_OPERATION: &str = "commerce.owner-write.v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerWrite {
    pub kind: &'static str,
    pub values: Vec<Value>,
    pub record_type: &'static str,
    pub record_id: String,
}

/// Closed owner kind -> (SQL discriminator, arity, record type).
pub fn kind_spec(kind: &str) -> Option<(&'static str, usize, &'static str)> {
    Some(match kind {
        "settings.initialize" => ("settings_initialize", 12, "commerce_settings"),
        "billing-test.document" => ("billing_test_document", 8, "billing_test_finance_documents"),
        "billing-test.entry" => ("billing_test_entry", 6, "billing_test_finance_entries"),
        "billing-test.line" => ("billing_test_line", 8, "billing_test_finance_lines"),
        "supplier.fulfillment-version" => ("fulfillment_version", 1, "commerce_fulfillments"),
        "supplier.order-version" => ("order_version", 2, "commerce_orders"),
        "goods-carrying" => ("goods_carrying", 20, "commerce_invoice_goods_carrying"),
        "product.create" => ("product_create", 10, "commerce_products"),
        "product.update" => ("product_update", 9, "commerce_products"),
        "order.version-bump" => ("order_version_bump", 2, "commerce_orders"),
        "order.posted-revision" => ("order_posted_revision", 3, "commerce_orders"),
        "return.create" => ("return_create", 16, "commerce_orders"),
        "return.line" => ("return_line", 13, "commerce_orders"),
        "return.lot-allocation" => ("return_lot_allocation", 9, "commerce_orders"),
        "fulfillment.version-guarded" => ("fulfillment_version_guarded", 2, "commerce_orders"),
        "return.reject" => ("return_reject", 4, "commerce_orders"),
        "return.stock-move" => ("return_stock_move", 8, "commerce_orders"),
        "return.approve" => ("return_approve", 7, "commerce_orders"),
        "return.post" => ("return_post", 8, "commerce_orders"),
        "order.update-parties" => ("order_update_parties", 14, "commerce_orders"),
        "order.lines-delete" => ("order_lines_delete", 1, "commerce_orders"),
        "order.create" => ("order_create", 24, "commerce_orders"),
        "order.lines-create" => ("order_lines_create", 13, "commerce_orders"),
        "order.transition" => ("order_transition", 10, "commerce_orders"),
        "order.convert" => ("order_convert", 6, "commerce_orders"),
        "order.lines-convert" => ("order_lines_convert", 3, "commerce_orders"),
        "fulfillment.invoice-link" => ("fulfillment_invoice_link", 2, "commerce_orders"),
        "mfg-existing-posting-confirmation" => {
            ("mfg_existing_posting_confirmation", 10, "commerce_settings")
        }
        "supplier.fulfillment-links" => ("supplier_fulfillment_links", 2, "commerce_fulfillments"),
        _ => return None,
    })
}

pub fn parse_owner_write(raw: &Value) -> Result<OwnerWrite, BusinessError> {
    let kind = raw
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let (key, arity, record_type) = kind_spec(kind).ok_or_else(invalid)?;
    let values = match raw.get("values") {
        Some(Value::Array(items)) => items.clone(),
        _ => return Err(invalid()),
    };
    if values.len() != arity
        || values
            .iter()
            .any(|v| matches!(v, Value::Object(_) | Value::Array(_)))
    {
        return Err(invalid());
    }
    let record_id = match raw.get("recordId") {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => value.clone(),
        _ => return Err(invalid()),
    };
    let _ = key;
    Ok(OwnerWrite {
        kind: key,
        values,
        record_type,
        record_id,
    })
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_OWNER_WRITE_INVALID",
        "Review the Commerce owner write.",
    )
}
pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMERCE_OWNER_WRITE_UNAVAILABLE",
        "The native Commerce service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closed_registry_and_arity() {
        assert!(kind_spec("settings.initialize").is_some());
        assert!(kind_spec("bogus").is_none());
        let good = json!({"kind":"supplier.order-version","recordId":"o","values":[1,"o"]});
        assert!(parse_owner_write(&good).is_ok());
        assert!(
            parse_owner_write(
                &json!({"kind":"supplier.order-version","recordId":"o","values":[1]})
            )
            .is_err()
        );
    }
}
