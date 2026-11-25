//! Owner query port and handler for the lane 14 workflow order reads.
//!
//! This layer owns validation, invariants and the response envelope. It never
//! builds SQL; `postgres.rs` owns execution against the owner tables.

use super::domain::{LaneKind, expected_columns, invalid, validate};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LaneReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Injected owner reader. The Company scope and one bounded record/cursor are
/// the only inputs; no predicate, projection or SQL text crosses this port.
pub trait CommerceLane14Read: Send + Sync {
    fn document<'a>(&'a self, company: &'a str, id: &'a str) -> LaneReadFuture<'a, Option<Value>>;
    fn page<'a>(&'a self, company: &'a str, after: &'a str) -> LaneReadFuture<'a, Vec<Value>>;
    fn references<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<Value>>;
}

fn is_row(value: &Value) -> bool {
    value.is_object()
        && value.as_object().is_some_and(|object| {
            object.len() == expected_columns().len()
                && object
                    .keys()
                    .all(|key| expected_columns().contains(key.as_str()))
                && object.values().all(|cell| {
                    cell.is_string() || cell.is_i64() || cell.is_u64() || cell.is_null()
                })
        })
}

fn check_rows(rows: &[Value], max: usize) -> Result<(), BusinessError> {
    if rows.len() > max || !rows.iter().all(is_row) {
        return Err(super::domain::unavailable());
    }
    Ok(())
}

/// Validate then delegate. The returned `None` is a real missing order, not a
/// transport failure; a malformed row fails closed as unavailable.
pub async fn read_document<R: CommerceLane14Read + ?Sized>(
    reader: &R,
    company: &str,
    id: &str,
) -> Result<Option<Value>, BusinessError> {
    validate(LaneKind::Document, company, id)?;
    let row = reader.document(company, id).await?;
    if row.as_ref().is_some_and(|row| !is_row(row)) {
        return Err(super::domain::unavailable());
    }
    Ok(row)
}

/// The page never exceeds the retained `LIMIT 100` contract.
pub async fn read_page<R: CommerceLane14Read + ?Sized>(
    reader: &R,
    company: &str,
    after: &str,
) -> Result<Vec<Value>, BusinessError> {
    validate(LaneKind::DocumentPage, company, after)?;
    let rows = reader.page(company, after).await?;
    check_rows(&rows, 100)?;
    Ok(rows)
}

/// The unbounded historical reference read is bounded only at the transport
/// edge; a structurally invalid reply still fails closed.
pub async fn read_references<R: CommerceLane14Read + ?Sized>(
    reader: &R,
    company: &str,
) -> Result<Vec<Value>, BusinessError> {
    if !super::domain::valid_id(company) {
        return Err(invalid());
    }
    let rows = reader.references(company).await?;
    check_rows(&rows, 10_000)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Probe {
        row: Option<Value>,
        rows: Vec<Value>,
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    fn good_row(id: &str) -> Value {
        let mut object = serde_json::Map::new();
        for (index, name) in expected_columns().iter().enumerate() {
            let value = if index % 2 == 0 {
                Value::String(format!("{name}-{id}"))
            } else {
                Value::from(index as i64)
            };
            object.insert((*name).to_owned(), value);
        }
        Value::Object(object)
    }

    impl CommerceLane14Read for Probe {
        fn document<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
        ) -> LaneReadFuture<'a, Option<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            let row = self.row.clone();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, id));
                Ok(row)
            })
        }
        fn page<'a>(&'a self, company: &'a str, after: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let after = after.to_owned();
            let rows = self.rows.clone();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, after));
                Ok(rows)
            })
        }
        fn references<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let rows = self.rows.clone();
            Box::pin(async move {
                seen.lock()
                    .expect("lock")
                    .push((company, "references".to_owned()));
                Ok(rows)
            })
        }
    }

    fn probe(row: Option<Value>, rows: Vec<Value>) -> (Probe, Arc<Mutex<Vec<(String, String)>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        (
            Probe {
                row,
                rows,
                seen: seen.clone(),
            },
            seen,
        )
    }

    #[tokio::test]
    async fn handler_forwards_company_scope_and_preserves_row_shape() {
        let (reader, seen) = probe(Some(good_row("1")), vec![good_row("2")]);
        let row = read_document(&reader, "co-1", "order-1")
            .await
            .expect("read")
            .expect("row");
        assert_eq!(row, good_row("1"));
        let rows = read_page(&reader, "co-1", "cursor-1").await.expect("read");
        assert_eq!(rows, vec![good_row("2")]);
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![
                ("co-1".to_owned(), "order-1".to_owned()),
                ("co-1".to_owned(), "cursor-1".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn handler_rejects_unbounded_scope_and_malformed_rows() {
        let (reader, _) = probe(Some(good_row("1")), vec![json!({"id": "x"})]);
        assert!(
            read_document(&reader, "bad company!", "order-1")
                .await
                .is_err()
        );
        assert!(read_page(&reader, "co-1", "").await.is_err());
        assert!(read_page(&reader, "co-1", "cursor-1").await.is_err());
        assert!(read_references(&reader, "co-1").await.is_err());
        let (empty, _) = probe(None, Vec::new());
        assert_eq!(
            read_document(&empty, "co-1", "order-1")
                .await
                .expect("read"),
            None
        );
    }
}
