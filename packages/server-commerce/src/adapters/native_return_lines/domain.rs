//! Pure domain for the native Commerce order return-line read.
//!
//! No SQL, transport or I/O. The shape is the exact column set the retained
//! TypeScript reader returned, including the computed `returned_milli` value.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde_json::Value;

/// One fulfilled order line with its non-rejected returned quantity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReturnLine {
    pub id: String,
    pub fulfillment_id: String,
    pub order_line_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub product_id: String,
    pub item_type: String,
    pub description: String,
    pub sku: String,
    pub returned_milli: i64,
}

impl ReturnLine {
    /// Exact retired payload: `fl.*`, the four projected columns and the
    /// `returned_milli` aggregate, with no extra or renamed field.
    pub fn into_value(self) -> Value {
        serde_json::json!({
            "id": self.id,
            "fulfillment_id": self.fulfillment_id,
            "order_line_id": self.order_line_id,
            "quantity_milli": self.quantity_milli,
            "value_minor": self.value_minor,
            "subtotal_minor": self.subtotal_minor,
            "tax_minor": self.tax_minor,
            "product_id": self.product_id,
            "item_type": self.item_type,
            "description": self.description,
            "sku": self.sku,
            "returned_milli": self.returned_milli,
        })
    }
}

/// Native envelope. The TypeScript owner consumed the same `data` array and
/// no surrounding context object, so no field is dropped here.
pub fn project(lines: Vec<ReturnLine>) -> Value {
    serde_json::json!({
        "data": lines.into_iter().map(ReturnLine::into_value).collect::<Vec<_>>(),
    })
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_RETURN_LINE_READ_REQUEST_INVALID",
        "Use one bounded Commerce return-line read.",
    )
}

/// Identities are bounded before they reach a bind parameter.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> ReturnLine {
        ReturnLine {
            id: "fl-1".to_owned(),
            fulfillment_id: "f-1".to_owned(),
            order_line_id: "ol-1".to_owned(),
            quantity_milli: 2000,
            value_minor: 500,
            subtotal_minor: 500,
            tax_minor: 35,
            product_id: "p-1".to_owned(),
            item_type: "stock".to_owned(),
            description: "service part".to_owned(),
            sku: "SKU-1".to_owned(),
            returned_milli: 1000,
        }
    }

    #[test]
    fn payload_has_every_retired_column() {
        let value = line().into_value();
        let object = value.as_object().expect("object");
        assert_eq!(object.len(), 12);
        for key in [
            "id",
            "fulfillment_id",
            "order_line_id",
            "quantity_milli",
            "value_minor",
            "subtotal_minor",
            "tax_minor",
            "product_id",
            "item_type",
            "description",
            "sku",
            "returned_milli",
        ] {
            assert!(object.contains_key(key), "missing {key}");
        }
        assert_eq!(value["returned_milli"], 1000);
        assert_eq!(value["quantity_milli"], 2000);
    }

    #[test]
    fn envelope_is_a_single_data_array() {
        let value = project(vec![line()]);
        assert_eq!(value.as_object().expect("object").len(), 1);
        assert_eq!(value["data"][0]["id"], "fl-1");
        assert_eq!(value["data"].as_array().expect("array").len(), 1);
        assert_eq!(project(Vec::new()), serde_json::json!({"data": []}));
    }

    #[test]
    fn identities_are_bounded() {
        assert!(!valid_id(""));
        assert!(!valid_id("bad id!"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_id("order-1"));
        assert!(valid_id("inv:issued.1"));
    }
}
