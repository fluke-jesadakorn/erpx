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
