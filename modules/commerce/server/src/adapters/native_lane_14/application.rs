//! Owner query port and handler for the lane 14 workflow order reads.
//!
//! This layer owns validation, invariants and the response envelope. It never
//! builds SQL; `postgres.rs` owns execution against the owner tables.

use super::domain::{LaneKind, expected_columns, invalid, validate};
use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_shared_server::public::BusinessError;
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
    use erpx_platform_runtime_server::public::http_support::serde_json::json;
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

// ---------------------------------------------------------------------------
// Wave 2: owner-local Sales pricing/order summary reads.
// ---------------------------------------------------------------------------

pub type SalesReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader for the five live Sales reads. The Company/branch/department
/// scope and bounded record identities are the only inputs.
pub trait CommerceLane14SalesRead: Send + Sync {
    fn pricing_products<'a>(&'a self, company: &'a str) -> SalesReadFuture<'a, Vec<Value>>;
    fn pricing_product<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> SalesReadFuture<'a, Option<Value>>;
    fn order_counts<'a>(
        &'a self,
        scope: &'a super::domain::SalesScope,
        ids: &'a [String],
    ) -> SalesReadFuture<'a, Vec<Value>>;
    fn open_orders<'a>(
        &'a self,
        scope: &'a super::domain::SalesScope,
        party: &'a str,
    ) -> SalesReadFuture<'a, Vec<Value>>;
    fn support_source<'a>(
        &'a self,
        scope: &'a super::domain::SalesScope,
        party: &'a str,
    ) -> SalesReadFuture<'a, Option<Value>>;
}

use super::domain::{
    OPEN_ORDER_KEYS, ORDER_COUNT_KEYS, PRODUCT_KEYS, PRODUCT_POINT_KEYS, SUPPORT_ORDER_KEYS,
    SalesScope, sales_invalid, sales_unavailable, validate_sales_scope,
};

fn is_named_row(value: &Value, keys: &[&str]) -> bool {
    value.is_object()
        && value.as_object().is_some_and(|object| {
            object.len() == keys.len()
                && keys.iter().all(|key| {
                    object.get(*key).is_some_and(|cell| {
                        cell.is_string() || cell.is_i64() || cell.is_u64() || cell.is_null()
                    })
                })
        })
}

fn check_named_rows(rows: &[Value], keys: &[&str], max: usize) -> Result<(), BusinessError> {
    if rows.len() > max || !rows.iter().all(|row| is_named_row(row, keys)) {
        return Err(sales_unavailable());
    }
    Ok(())
}

pub async fn read_pricing_products<R: CommerceLane14SalesRead + ?Sized>(
    reader: &R,
    company: &str,
) -> Result<Vec<Value>, BusinessError> {
    validate_sales_scope(
        &SalesScope {
            company: company.to_owned(),
            branch: None,
            department: None,
        },
        None,
    )?;
    let rows = reader.pricing_products(company).await?;
    check_named_rows(&rows, PRODUCT_KEYS, 10_000)?;
    Ok(rows)
}

pub async fn read_pricing_product<R: CommerceLane14SalesRead + ?Sized>(
    reader: &R,
    company: &str,
    id: &str,
) -> Result<Option<Value>, BusinessError> {
    validate_sales_scope(
        &SalesScope {
            company: company.to_owned(),
            branch: None,
            department: None,
        },
        Some(id),
    )?;
    let row = reader.pricing_product(company, id).await?;
    if row
        .as_ref()
        .is_some_and(|row| !is_named_row(row, PRODUCT_POINT_KEYS))
    {
        return Err(sales_unavailable());
    }
    Ok(row)
}

pub async fn read_order_counts<R: CommerceLane14SalesRead + ?Sized>(
    reader: &R,
    scope: &SalesScope,
    ids: &[String],
) -> Result<Vec<Value>, BusinessError> {
    validate_sales_scope(scope, None)?;
    if ids.is_empty() || ids.len() > 64 || !ids.iter().all(|id| super::domain::valid_id(id)) {
        return Err(sales_invalid());
    }
    let rows = reader.order_counts(scope, ids).await?;
    check_named_rows(&rows, ORDER_COUNT_KEYS, 64)?;
    Ok(rows)
}

pub async fn read_open_orders<R: CommerceLane14SalesRead + ?Sized>(
    reader: &R,
    scope: &SalesScope,
    party: &str,
) -> Result<Vec<Value>, BusinessError> {
    validate_sales_scope(scope, Some(party))?;
    let rows = reader.open_orders(scope, party).await?;
    check_named_rows(&rows, OPEN_ORDER_KEYS, 30)?;
    Ok(rows)
}

pub async fn read_support_source<R: CommerceLane14SalesRead + ?Sized>(
    reader: &R,
    scope: &SalesScope,
    party: &str,
) -> Result<Option<Value>, BusinessError> {
    validate_sales_scope(scope, Some(party))?;
    let row = reader.support_source(scope, party).await?;
    if row
        .as_ref()
        .is_some_and(|row| !is_named_row(row, SUPPORT_ORDER_KEYS))
    {
        return Err(sales_unavailable());
    }
    Ok(row)
}

#[cfg(test)]
mod sales_tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct Probe {
        row: Option<Value>,
        support: Option<Value>,
        broken: bool,
        seen: Arc<Mutex<Vec<String>>>,
    }

    fn scope() -> SalesScope {
        SalesScope {
            company: "co-1".to_owned(),
            branch: None,
            department: Some("dep-1".to_owned()),
        }
    }

    fn named(keys: &[&str]) -> Value {
        let mut object = serde_json::Map::new();
        for (index, key) in keys.iter().enumerate() {
            object.insert(
                (*key).to_owned(),
                if super::super::domain::int_key(key) {
                    Value::from(index as i64)
                } else {
                    Value::String(format!("{key}-{index}"))
                },
            );
        }
        Value::Object(object)
    }

    impl CommerceLane14SalesRead for Probe {
        fn pricing_products<'a>(&'a self, _company: &'a str) -> SalesReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let broken = self.broken;
            Box::pin(async move {
                seen.lock().expect("lock").push("products".to_owned());
                if broken {
                    Ok(vec![json!({"id": "only-id"})])
                } else {
                    Ok(vec![named(PRODUCT_KEYS)])
                }
            })
        }
        fn pricing_product<'a>(
            &'a self,
            _company: &'a str,
            id: &'a str,
        ) -> SalesReadFuture<'a, Option<Value>> {
            let seen = self.seen.clone();
            let row = self.row.clone();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push(id);
                Ok(row)
            })
        }
        fn order_counts<'a>(
            &'a self,
            _scope: &'a SalesScope,
            ids: &'a [String],
        ) -> SalesReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let ids = ids.to_vec();
            Box::pin(async move {
                seen.lock().expect("lock").push(ids.join(","));
                Ok(vec![named(ORDER_COUNT_KEYS)])
            })
        }
        fn open_orders<'a>(
            &'a self,
            _scope: &'a SalesScope,
            party: &'a str,
        ) -> SalesReadFuture<'a, Vec<Value>> {
            let seen = self.seen.clone();
            let party = party.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push(party);
                Ok(vec![named(OPEN_ORDER_KEYS)])
            })
        }
        fn support_source<'a>(
            &'a self,
            _scope: &'a SalesScope,
            party: &'a str,
        ) -> SalesReadFuture<'a, Option<Value>> {
            let seen = self.seen.clone();
            let row = self.support.clone();
            let party = party.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push(party);
                Ok(row)
            })
        }
    }

    fn probe() -> (Probe, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        (
            Probe {
                row: Some(named(PRODUCT_POINT_KEYS)),
                support: Some(named(SUPPORT_ORDER_KEYS)),
                broken: false,
                seen: seen.clone(),
            },
            seen,
        )
    }

    #[tokio::test]
    async fn sales_handlers_preserve_shapes_and_forward_scope() {
        let (reader, seen) = probe();
        let products = read_pricing_products(&reader, "co-1").await.expect("read");
        assert_eq!(products, vec![named(PRODUCT_KEYS)]);
        let product = read_pricing_product(&reader, "co-1", "product-1")
            .await
            .expect("read")
            .expect("row");
        assert_eq!(product, json!({"id": "id-0", "version": 1}));
        let counts = read_order_counts(&reader, &scope(), &["party-1".to_owned()])
            .await
            .expect("read");
        assert_eq!(counts, vec![named(ORDER_COUNT_KEYS)]);
        let orders = read_open_orders(&reader, &scope(), "party-1")
            .await
            .expect("read");
        assert_eq!(orders, vec![named(OPEN_ORDER_KEYS)]);
        let source = read_support_source(&reader, &scope(), "party-1")
            .await
            .expect("read")
            .expect("row");
        assert_eq!(source, json!({"id": "id-0"}));
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![
                "products".to_owned(),
                "product-1".to_owned(),
                "party-1".to_owned(),
                "party-1".to_owned(),
                "party-1".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn sales_handlers_reject_bad_scope_and_malformed_rows() {
        let (reader, _) = probe();
        let bad_scope = SalesScope {
            company: "bad company!".to_owned(),
            branch: None,
            department: None,
        };
        assert!(
            read_order_counts(&reader, &bad_scope, &["party-1".to_owned()])
                .await
                .is_err()
        );
        assert!(read_order_counts(&reader, &scope(), &[]).await.is_err());
        let (mut broken, _) = probe();
        broken.broken = true;
        assert!(read_pricing_products(&broken, "co-1").await.is_err());
    }
}
