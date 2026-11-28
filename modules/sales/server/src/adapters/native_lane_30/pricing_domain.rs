//! Pure domain types for the native Sales price-list workspace list reads.
//!
//! Owner-local successor of the two `SELECT` statements in `adapters/pricing.ts`
//! `repository.workspace()`. Rust owns the statements, the Company scope, the
//! joins to the physical party/product relations, the projection and the exact
//! `{lists,items}` response shape.

use erpx_platform_shared_server::public::{BusinessError, RecordId};
use serde_json::{Map, Value, json};

/// Byte-identical rendered TypeScript statements (git HEAD) before migration.
/// The finance/commerce projection suffixes are expanded exactly as the TS
/// runtime rendered them; these are the SQL result oracles.
pub const ORACLE_PRICE_LISTS_SQL: &str = concat!(
    "SELECT l.*,p.name customer_name FROM sales_price_lists l LEFT JOIN ",
    "(SELECT \"id\",\"company_id\",\"kind\",\"code\",\"name\",\"currency\",\"tax_registration\",\"payment_terms_days\",\"credit_limit_minor\",\"status\",\"version\",\"created_at\",\"updated_at\",\"person_id\",\"notification_email\",\"notification_consent\",\"merged_into_id\",\"merged_at\",\"merged_by\",\"phone\",\"address_json\" FROM finance_parties)",
    " p ON p.id=l.customer_id WHERE l.company_id=? ORDER BY l.created_at DESC"
);
pub const ORACLE_PRICE_ITEMS_SQL: &str = concat!(
    "SELECT i.*,p.sku,p.name product_name FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id JOIN ",
    "(SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products)",
    " p ON p.id=i.product_id WHERE l.company_id=? ORDER BY i.price_list_id,i.ordinal"
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceListQuery {
    pub company: String,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "SALES_PRICE_LIST_WORKSPACE_REQUEST_INVALID",
        "Use one bounded price-list workspace read.",
    )
}

impl PriceListQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if RecordId::new(self.company.as_str()).is_err()
            || self.company.is_empty()
            || self.company.len() > 200
            || !self
                .company
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
        {
            return Err(invalid());
        }
        Ok(())
    }
}

fn nullable_text(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::String)
}
fn nullable_integer(value: Option<i64>) -> Value {
    value.map_or(Value::Null, |number| json!(number))
}

/// Exact `PriceListRow` projection: strings/numbers/null only.
#[derive(Clone, Debug)]
pub struct PriceListRow {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub currency: String,
    pub customer_id: Option<String>,
    pub valid_from: String,
    pub valid_until: String,
    pub max_auto_discount_bps: i64,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub activated_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub customer_name: Option<String>,
}

impl PriceListRow {
    pub fn into_value(self) -> Value {
        let mut object = Map::new();
        for (key, value) in [
            ("id", json!(self.id)),
            ("company_id", json!(self.company_id)),
            ("name", json!(self.name)),
            ("currency", json!(self.currency)),
            ("customer_id", nullable_text(self.customer_id)),
            ("valid_from", json!(self.valid_from)),
            ("valid_until", json!(self.valid_until)),
            ("max_auto_discount_bps", json!(self.max_auto_discount_bps)),
            ("status", json!(self.status)),
            ("version", json!(self.version)),
            ("created_by", json!(self.created_by)),
            ("approved_by", nullable_text(self.approved_by)),
            ("approval_reason", nullable_text(self.approval_reason)),
            ("activated_at", nullable_integer(self.activated_at)),
            ("created_at", json!(self.created_at)),
            ("updated_at", json!(self.updated_at)),
            ("customer_name", nullable_text(self.customer_name)),
        ] {
            object.insert(key.into(), value);
        }
        Value::Object(object)
    }
}

#[derive(Clone, Debug)]
pub struct PriceListItemRow {
    pub id: String,
    pub price_list_id: String,
    pub product_id: String,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub ordinal: i64,
    pub sku: String,
    pub product_name: String,
}

impl PriceListItemRow {
    pub fn into_value(self) -> Value {
        let mut object = Map::new();
        for (key, value) in [
            ("id", json!(self.id)),
            ("price_list_id", json!(self.price_list_id)),
            ("product_id", json!(self.product_id)),
            ("price_minor", json!(self.price_minor)),
            ("tax_bps", json!(self.tax_bps)),
            ("ordinal", json!(self.ordinal)),
            ("sku", json!(self.sku)),
            ("product_name", json!(self.product_name)),
        ] {
            object.insert(key.into(), value);
        }
        Value::Object(object)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PriceListWorkspace {
    pub lists: Vec<PriceListRow>,
    pub items: Vec<PriceListItemRow>,
}

impl PriceListWorkspace {
    pub fn into_value(self) -> Value {
        json!({
            "lists": self.lists.into_iter().map(PriceListRow::into_value).collect::<Vec<_>>(),
            "items": self.items.into_iter().map(PriceListItemRow::into_value).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_validation_is_bounded() {
        let valid = PriceListQuery { company: "company-1".into() };
        assert!(valid.validate().is_ok());
        for bad in ["", "bad id", "x/../y"] {
            assert!(PriceListQuery { company: bad.into() }.validate().is_err());
        }
    }

    #[test]
    fn oracle_sql_is_read_only_company_scoped_and_byte_stable() {
        // Byte counts pin the rendered TypeScript statements from git HEAD.
        for (sql, bytes) in [(ORACLE_PRICE_LISTS_SQL, 444usize), (ORACLE_PRICE_ITEMS_SQL, 328)] {
            assert_eq!(sql.len(), bytes);
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            let words: Vec<&str> = upper.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            assert!(!words.contains(&"INSERT"));
            assert!(!words.contains(&"UPDATE"));
            assert!(!words.contains(&"DELETE"));
            assert!(upper.contains("COMPANY_ID"));
            assert_eq!(sql.matches('?').count(), 1);
        }
    }

    #[test]
    fn response_shape_is_exact() {
        let value = PriceListWorkspace::default().into_value();
        assert_eq!(value, json!({"lists":[],"items":[]}));
    }
}
