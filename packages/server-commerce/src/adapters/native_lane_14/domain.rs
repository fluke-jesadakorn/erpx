//! Pure domain for the native Commerce workflow order-read family.
//!
//! The three retired reads in `adapters/workflow-read-sources.ts` all expose
//! Commerce-owned `commerce_orders` rows. This module owns the response shape,
//! the bounded identities and the projection invariant; it never sees SQL or a
//! database connection. Every projected cell is a JSON string, number or null,
//! which is the exact contract the Workflow owner's `Row` type expects.

use erpx_server_runtime::public::http_support::serde_json::{self, Value};
use erpx_server_shared::public::{BusinessError, RecordId};
use std::collections::BTreeSet;

/// The three owner-local reads retired from the lane bucket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneKind {
    Document,
    DocumentPage,
    ReferenceRows,
}

impl LaneKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::DocumentPage => "document-page",
            Self::ReferenceRows => "reference-rows",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "document" => Some(Self::Document),
            "document-page" => Some(Self::DocumentPage),
            "reference-rows" => Some(Self::ReferenceRows),
            _ => None,
        }
    }
}

/// A single typed cell of a projected Commerce order row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cell {
    Text(String),
    Int(i64),
}

impl Cell {
    fn into_value(self) -> Value {
        match self {
            Self::Text(value) => Value::String(value),
            Self::Int(value) => Value::from(value),
        }
    }
}

/// Column kind of the owner table. `SELECT *` is retained in the SQL; the
/// projection is pinned to this allow-list so schema drift fails closed rather
/// than leaking an unexpected value to a cross-owner consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnKind {
    Text,
    Int,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Column {
    pub name: &'static str,
    pub kind: ColumnKind,
}

/// All columns of `commerce_orders` after the active migration tracks, in the
/// physical order of the baseline plus the two later additive migrations.
pub const COLUMNS: &[Column] = &[
    Column {
        name: "id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "company_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "branch_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "department_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "kind",
        kind: ColumnKind::Text,
    },
    Column {
        name: "number",
        kind: ColumnKind::Text,
    },
    Column {
        name: "party_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "warehouse_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "currency",
        kind: ColumnKind::Text,
    },
    Column {
        name: "order_date",
        kind: ColumnKind::Text,
    },
    Column {
        name: "due_date",
        kind: ColumnKind::Text,
    },
    Column {
        name: "note",
        kind: ColumnKind::Text,
    },
    Column {
        name: "state",
        kind: ColumnKind::Text,
    },
    Column {
        name: "subtotal_minor",
        kind: ColumnKind::Int,
    },
    Column {
        name: "tax_minor",
        kind: ColumnKind::Int,
    },
    Column {
        name: "source_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "version",
        kind: ColumnKind::Int,
    },
    Column {
        name: "created_by",
        kind: ColumnKind::Text,
    },
    Column {
        name: "approved_by",
        kind: ColumnKind::Text,
    },
    Column {
        name: "created_at",
        kind: ColumnKind::Int,
    },
    Column {
        name: "updated_at",
        kind: ColumnKind::Int,
    },
    Column {
        name: "price_list_id",
        kind: ColumnKind::Text,
    },
    Column {
        name: "price_list_version",
        kind: ColumnKind::Int,
    },
    Column {
        name: "discount_reason",
        kind: ColumnKind::Text,
    },
    Column {
        name: "discount_approval_required",
        kind: ColumnKind::Int,
    },
    Column {
        name: "discount_approved_by",
        kind: ColumnKind::Text,
    },
    Column {
        name: "discount_approved_at",
        kind: ColumnKind::Int,
    },
];

/// Names expected from `SELECT *`; any mismatch is a persistence failure.
pub fn expected_columns() -> BTreeSet<&'static str> {
    COLUMNS.iter().map(|column| column.name).collect()
}

/// Pure projection used by the Postgres adapter after it has read typed cells.
/// One object key per allow-listed column, in physical order.
pub fn project_cells(cells: &[(&str, Option<Cell>)]) -> Result<Value, BusinessError> {
    let mut object = serde_json::Map::new();
    for (name, cell) in cells {
        if !expected_columns().contains(name) || object.contains_key(*name) {
            return Err(invalid());
        }
        object.insert(
            (*name).to_owned(),
            cell.clone().map(Cell::into_value).unwrap_or(Value::Null),
        );
    }
    if object.len() != COLUMNS.len() {
        return Err(invalid());
    }
    Ok(Value::Object(object))
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_READ_REQUEST_INVALID",
        "Use one bounded Commerce workflow order read.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMERCE_LANE14_READ_UNAVAILABLE",
        "The native Commerce workflow order read service is unavailable.",
    )
}

/// Record identities are bounded before they reach a bind parameter. The
/// Company scope must be a real `WorkContext` identity, not arbitrary SQL.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

/// Validate the caller's Company and the record/cursor bound for one read.
pub fn validate(kind: LaneKind, company: &str, record: &str) -> Result<(), BusinessError> {
    if !valid_id(company) || !valid_id(record) {
        return Err(invalid());
    }
    if kind == LaneKind::DocumentPage && record.len() > 200 {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_registry_is_closed() {
        for kind in ["document", "document-page", "reference-rows"] {
            let parsed = LaneKind::parse(kind).expect("registered kind");
            assert_eq!(parsed.as_str(), kind);
        }
        assert!(LaneKind::parse("SELECT * FROM commerce_orders").is_none());
        assert!(LaneKind::parse("").is_none());
    }

    #[test]
    fn identities_are_bounded() {
        assert!(!valid_id(""));
        assert!(!valid_id("bad id!"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_id("order-1"));
        assert!(validate(LaneKind::Document, "co-1", "order-1").is_ok());
        assert!(validate(LaneKind::Document, "bad company!", "order-1").is_err());
        assert!(validate(LaneKind::DocumentPage, "co-1", "").is_err());
    }

    #[test]
    fn projection_is_exactly_the_order_column_allow_list() {
        let cells: Vec<(&str, Option<Cell>)> = COLUMNS
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let cell = match column.kind {
                    ColumnKind::Text => Cell::Text(format!("v{index}")),
                    ColumnKind::Int => Cell::Int(index as i64),
                };
                (column.name, Some(cell))
            })
            .collect();
        let value = project_cells(&cells).expect("projection");
        let object = value.as_object().expect("object");
        assert_eq!(object.len(), COLUMNS.len());
        assert_eq!(object.get("id"), Some(&Value::String("v0".to_owned())));
        assert_eq!(object.get("subtotal_minor"), Some(&Value::from(13)));
        let mut sparse = cells.clone();
        sparse.pop();
        assert!(project_cells(&sparse).is_err());
        let mut unknown = cells;
        unknown[0].0 = "dropped_column";
        assert!(project_cells(&unknown).is_err());
    }

    #[test]
    fn null_cells_are_preserved() {
        let mut cells: Vec<(&str, Option<Cell>)> =
            COLUMNS.iter().map(|column| (column.name, None)).collect();
        cells[0].1 = Some(Cell::Text("order-1".to_owned()));
        let value = project_cells(&cells).expect("projection");
        let object = value.as_object().expect("object");
        assert_eq!(object.get("id"), Some(&Value::String("order-1".to_owned())));
        assert_eq!(object.get("company_id"), Some(&Value::Null));
    }
}

// ---------------------------------------------------------------------------
// Wave 2: owner-local Sales pricing/order summary reads.
// ---------------------------------------------------------------------------

/// Caller scope already resolved from `WorkContext`. `None` branch/department
/// binds SQL NULL so the retained `?::text IS NULL` predicate is unchanged.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesScope {
    pub company: String,
    pub branch: Option<String>,
    pub department: Option<String>,
}

pub fn sales_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_LANE14_SALES_REQUEST_INVALID",
        "Use one bounded Commerce Sales read.",
    )
}

pub fn sales_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "COMMERCE_LANE14_SALES_READ_UNAVAILABLE",
        "The native Commerce Sales read service is unavailable.",
    )
}

pub fn validate_sales_scope(scope: &SalesScope, record: Option<&str>) -> Result<(), BusinessError> {
    if !valid_id(&scope.company)
        || scope
            .branch
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || scope
            .department
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || record.is_some_and(|value| !valid_id(value))
    {
        return Err(sales_invalid());
    }
    Ok(())
}

/// Pinned key lists for the five retired call-site shapes.
pub const PRODUCT_KEYS: &[&str] = &["id", "sku", "name", "tax_bps"];
pub const PRODUCT_POINT_KEYS: &[&str] = &["id", "version"];
pub const ORDER_COUNT_KEYS: &[&str] = &["party_id", "count"];
pub const OPEN_ORDER_KEYS: &[&str] = &[
    "id",
    "number",
    "kind",
    "state",
    "due_date",
    "currency",
    "total_minor",
    "version",
];
pub const SUPPORT_ORDER_KEYS: &[&str] = &["id"];

pub fn int_key(key: &str) -> bool {
    matches!(key, "tax_bps" | "version" | "count" | "total_minor")
}

/// Generic projection for the small Sales shapes. The key order is owned here,
/// never by the caller, so the wire shape is deterministic.
pub fn project_named(
    cells: &[(&str, Option<Cell>)],
    keys: &[&str],
) -> Result<Value, BusinessError> {
    if cells.len() != keys.len() {
        return Err(sales_invalid());
    }
    let mut object = serde_json::Map::new();
    for ((name, cell), key) in cells.iter().zip(keys.iter()) {
        if name != key || object.contains_key(*key) {
            return Err(sales_invalid());
        }
        object.insert(
            (*key).to_owned(),
            cell.clone().map(Cell::into_value).unwrap_or(Value::Null),
        );
    }
    Ok(Value::Object(object))
}

#[cfg(test)]
mod sales_tests {
    use super::*;

    fn sha(value: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn sales_scope_is_bounded_and_nullable() {
        let scope = SalesScope {
            company: "co-1".to_owned(),
            branch: None,
            department: Some("dep-1".to_owned()),
        };
        assert!(validate_sales_scope(&scope, Some("party-1")).is_ok());
        let bad = SalesScope {
            branch: Some("bad branch!".to_owned()),
            ..scope
        };
        assert!(validate_sales_scope(&bad, None).is_err());
    }

    #[test]
    fn sales_result_oracles_are_stable() {
        let fixtures: Vec<(&[&str], Vec<(&str, Option<Cell>)>, &str)> = vec![
            (
                PRODUCT_KEYS,
                vec![
                    ("id", Some(Cell::Text("product-1".to_owned()))),
                    ("sku", Some(Cell::Text("SKU-1".to_owned()))),
                    ("name", Some(Cell::Text("Widget".to_owned()))),
                    ("tax_bps", Some(Cell::Int(700))),
                ],
                "091ea29c871fff5e9c70657ec69f0f648ac607a8183108cb10acfd47b4155f71",
            ),
            (
                PRODUCT_POINT_KEYS,
                vec![
                    ("id", Some(Cell::Text("product-1".to_owned()))),
                    ("version", Some(Cell::Int(3))),
                ],
                "62771ef3ad2fd314e49f310d1f4ed5cefc6893c7bd9a5c6061505bcad2e4be1c",
            ),
            (
                ORDER_COUNT_KEYS,
                vec![
                    ("party_id", Some(Cell::Text("party-1".to_owned()))),
                    ("count", Some(Cell::Int(2))),
                ],
                "376f5551224c63e500583160ac14d19bcfbdd6b7be4bdf95c6c62909ec8333c4",
            ),
            (
                SUPPORT_ORDER_KEYS,
                vec![("id", Some(Cell::Text("order-2".to_owned())))],
                "6c08f1ca016a4892425c4e530f700f28a968a35512484ac68c37c8c96dd04f4f",
            ),
        ];
        for (keys, cells, oracle) in fixtures {
            let value = project_named(&cells, keys).expect("projection");
            assert_eq!(sha(&value.to_string()), oracle);
        }
        let open = vec![
            ("id", Some(Cell::Text("order-1".to_owned()))),
            ("number", Some(Cell::Text("SO-1".to_owned()))),
            ("kind", Some(Cell::Text("sales".to_owned()))),
            ("state", Some(Cell::Text("confirmed".to_owned()))),
            ("due_date", Some(Cell::Text("2026-12-01".to_owned()))),
            ("currency", Some(Cell::Text("USD".to_owned()))),
            ("total_minor", Some(Cell::Int(12345))),
            ("version", Some(Cell::Int(2))),
        ];
        assert_eq!(
            sha(&project_named(&open, OPEN_ORDER_KEYS)
                .expect("projection")
                .to_string()),
            "991e393828df803bf5f238a07f556251f185014a57d91a3dfb10aaa771431cb4"
        );
        let mut reordered = open;
        reordered.swap(0, 1);
        assert!(project_named(&reordered, OPEN_ORDER_KEYS).is_err());
    }
}
