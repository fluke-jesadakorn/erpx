//! sqlx execution for the lane 14 Commerce workflow order reads.
//!
//! Every statement starts from the byte-identical TypeScript text of the
//! retired reader; only `?` binds are translated to positional `$n`. The
//! Company scope and the record/cursor are bound by this owner, and the
//! `SELECT *` projection is reduced to a pinned allow-list before it crosses
//! the wire.

use super::application::{CommerceLane14Read, LaneReadFuture};
use super::domain::{
    COLUMNS, Cell, ColumnKind, LaneKind, expected_columns, project_cells, unavailable,
};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::postgres::PgRow;
use sqlx::{Column, Row};
use std::collections::BTreeSet;

/// Retired TypeScript SQL, kept verbatim for the byte/result oracle.
pub const SQL_DOCUMENT_TS: &str = "SELECT * FROM commerce_orders WHERE company_id=? AND id=?";
pub const SQL_DOCUMENT_PAGE_TS: &str =
    "SELECT * FROM commerce_orders WHERE company_id=? AND id>? ORDER BY id LIMIT 100";
pub const SQL_REFERENCE_ROWS_TS: &str =
    "SELECT * FROM commerce_orders WHERE company_id=? ORDER BY updated_at DESC";

/// Executable forms: only the bind markers differ from the retired text.
const SQL_DOCUMENT: &str = "SELECT * FROM commerce_orders WHERE company_id=$1 AND id=$2";
const SQL_DOCUMENT_PAGE: &str =
    "SELECT * FROM commerce_orders WHERE company_id=$1 AND id>$2 ORDER BY id LIMIT 100";
const SQL_REFERENCE_ROWS: &str =
    "SELECT * FROM commerce_orders WHERE company_id=$1 ORDER BY updated_at DESC";

pub const fn statement_for(kind: LaneKind) -> &'static str {
    match kind {
        LaneKind::Document => SQL_DOCUMENT_TS,
        LaneKind::DocumentPage => SQL_DOCUMENT_PAGE_TS,
        LaneKind::ReferenceRows => SQL_REFERENCE_ROWS_TS,
    }
}

const fn executable_statement_for(kind: LaneKind) -> &'static str {
    match kind {
        LaneKind::Document => SQL_DOCUMENT,
        LaneKind::DocumentPage => SQL_DOCUMENT_PAGE,
        LaneKind::ReferenceRows => SQL_REFERENCE_ROWS,
    }
}

/// Translate `?` placeholders to `$1..$n` without changing any other byte.
pub fn render(statement: &str) -> String {
    let mut rendered = String::with_capacity(statement.len());
    let mut ordinal = 0usize;
    for character in statement.chars() {
        if character == '?' {
            ordinal += 1;
            rendered.push('$');
            rendered.push_str(&ordinal.to_string());
        } else {
            rendered.push(character);
        }
    }
    rendered
}

/// Reduce one owner row to the exact `Row` shape consumed by the Workflow
/// owner. A schema drift fails closed instead of leaking an unexpected column.
fn row_value(row: &PgRow) -> Result<Value, BusinessError> {
    let names: BTreeSet<&str> = row.columns().iter().map(|column| column.name()).collect();
    if names != expected_columns() {
        return Err(unavailable());
    }
    let mut cells: Vec<(&str, Option<Cell>)> = Vec::with_capacity(COLUMNS.len());
    for column in COLUMNS {
        let cell = match column.kind {
            ColumnKind::Text => row
                .try_get::<Option<String>, _>(column.name)
                .map_err(|_| unavailable())?
                .map(Cell::Text),
            ColumnKind::Int => row
                .try_get::<Option<i64>, _>(column.name)
                .map_err(|_| unavailable())?
                .map(Cell::Int),
        };
        cells.push((column.name, cell));
    }
    project_cells(&cells)
}

/// Postgres-backed owner reader. `commerce_orders` is Commerce-owned; this
/// read grants no Workflow command, lock or cross-owner authority.
pub struct PgCommerceLane14Read {
    pool: DatabasePool,
}

impl PgCommerceLane14Read {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn fetch(
        &self,
        kind: LaneKind,
        company: &str,
        record: Option<&str>,
    ) -> LaneReadFuture<'_, Vec<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let record = record.map(str::to_owned);
        Box::pin(async move {
            unit.run::<_, erpx_server_shared::public::BusinessError, _>(
                TransactionOptions::default(),
                move |connection| {
                    let company = company.clone();
                    let record = record.clone();
                    Box::pin(async move {
                        let mut query =
                            sqlx::query(executable_statement_for(kind)).bind(company.as_str());
                        if let Some(record) = record.as_deref() {
                            query = query.bind(record);
                        }
                        let rows = query
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        rows.iter()
                            .map(row_value)
                            .collect::<Result<Vec<_>, BusinessError>>()
                            .map_err(PgTransactionError::Business)
                    })
                },
            )
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

impl CommerceLane14Read for PgCommerceLane14Read {
    fn document<'a>(&'a self, company: &'a str, id: &'a str) -> LaneReadFuture<'a, Option<Value>> {
        let read = self.fetch(LaneKind::Document, company, Some(id));
        Box::pin(async move { Ok(read.await?.into_iter().next()) })
    }

    fn page<'a>(&'a self, company: &'a str, after: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
        self.fetch(LaneKind::DocumentPage, company, Some(after))
    }

    fn references<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<Value>> {
        self.fetch(LaneKind::ReferenceRows, company, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn retired_sql_is_byte_identical() {
        assert_eq!(
            sha(SQL_DOCUMENT_TS),
            "9d28de781a98785f44379455b7062e8d7b9438634001c16ba91e049406df6d8b"
        );
        assert_eq!(
            sha(SQL_DOCUMENT_PAGE_TS),
            "39d63f741e627b34d20c246af1fe682b4fe335af24cddf78853e9c9d7a688f7b"
        );
        assert_eq!(
            sha(SQL_REFERENCE_ROWS_TS),
            "24c6e6d4950d17a27762f0760f4501fb823e2b7da062c3d1f20f38d0232b505e"
        );
        for kind in [
            LaneKind::Document,
            LaneKind::DocumentPage,
            LaneKind::ReferenceRows,
        ] {
            assert!(statement_for(kind).starts_with("SELECT "));
        }
    }

    #[test]
    fn bind_translation_preserves_every_other_byte() {
        assert_eq!(
            render(SQL_DOCUMENT_TS),
            "SELECT * FROM commerce_orders WHERE company_id=$1 AND id=$2"
        );
        assert_eq!(
            render(SQL_DOCUMENT_PAGE_TS),
            "SELECT * FROM commerce_orders WHERE company_id=$1 AND id>$2 ORDER BY id LIMIT 100"
        );
        assert_eq!(
            render(SQL_REFERENCE_ROWS_TS),
            "SELECT * FROM commerce_orders WHERE company_id=$1 ORDER BY updated_at DESC"
        );
        assert_eq!(render("SELECT ? , ? , ?"), "SELECT $1 , $2 , $3");
        for kind in [
            LaneKind::Document,
            LaneKind::DocumentPage,
            LaneKind::ReferenceRows,
        ] {
            assert_eq!(render(statement_for(kind)), executable_statement_for(kind));
        }
    }
}

// ---------------------------------------------------------------------------
// Wave 2: sqlx execution for the five live Sales reads.
// ---------------------------------------------------------------------------

use super::application::CommerceLane14SalesRead;
use super::application::SalesReadFuture;
use super::domain::{
    OPEN_ORDER_KEYS, ORDER_COUNT_KEYS, PRODUCT_KEYS, PRODUCT_POINT_KEYS, SUPPORT_ORDER_KEYS,
    SalesScope, int_key, project_named, sales_unavailable, valid_id,
};
use sqlx::QueryBuilder;

/// Retired TypeScript text for the byte/result oracle. `SALES_COUNTS_TS_2`
/// renders the canonical two-party-id request.
#[cfg(test)]
pub const SALES_PRODUCTS_TS: &str = "SELECT id,sku,name,tax_bps FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku";
#[cfg(test)]
pub const SALES_PRODUCT_TS: &str =
    "SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'";
#[cfg(test)]
pub const SALES_COUNTS_TS_2: &str = "SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id IN (?,?) AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id";
#[cfg(test)]
pub const SALES_OPEN_ORDERS_TS: &str = "SELECT o.id,o.number,o.kind,o.state,o.due_date,o.currency,(o.subtotal_minor+o.tax_minor) total_minor,o.version FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id=? AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id LIMIT 30";
#[cfg(test)]
pub const SALES_SUPPORT_TS: &str = "SELECT o.id FROM commerce_orders o WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.party_id=? AND o.kind IN ('quote','sales') ORDER BY o.updated_at DESC LIMIT 1";

/// Executable forms: only bind markers differ from the retired text.
const SALES_PRODUCTS_SQL: &str = "SELECT id,sku,name,tax_bps FROM commerce_products WHERE company_id=$1 AND status='active' ORDER BY sku";
const SALES_PRODUCT_SQL: &str =
    "SELECT id,version FROM commerce_products WHERE id=$1 AND company_id=$2 AND status='active'";
const SALES_OPEN_ORDERS_SQL: &str = "SELECT o.id,o.number,o.kind,o.state,o.due_date,o.currency,(o.subtotal_minor+o.tax_minor) total_minor,o.version FROM commerce_orders o WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$5) AND o.party_id=$6 AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') ORDER BY o.due_date,o.id LIMIT 30";
const SALES_SUPPORT_SQL: &str = "SELECT o.id FROM commerce_orders o WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$5) AND o.party_id=$6 AND o.kind IN ('quote','sales') ORDER BY o.updated_at DESC LIMIT 1";
const SALES_COUNTS_HEAD: &str =
    "SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE o.company_id=";
const SALES_COUNTS_SCOPE: &str = " AND (";
const SALES_COUNTS_SCOPE_MID: &str = "::text IS NULL OR o.branch_id=";
const SALES_COUNTS_TAIL: &str = ") AND (";
const SALES_COUNTS_TAIL_MID: &str = "::text IS NULL OR o.department_id=";
const SALES_COUNTS_IN: &str = ") AND o.party_id IN (";
const SALES_COUNTS_END: &str = ") AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id";

fn sales_row(row: &PgRow, keys: &[&str]) -> Result<Value, BusinessError> {
    let expected: BTreeSet<&str> = keys.iter().copied().collect();
    let names: BTreeSet<&str> = row.columns().iter().map(|column| column.name()).collect();
    if names != expected {
        return Err(sales_unavailable());
    }
    let mut cells: Vec<(&str, Option<Cell>)> = Vec::with_capacity(keys.len());
    for key in keys {
        let cell = if int_key(key) {
            row.try_get::<Option<i64>, _>(*key)
                .map_err(|_| sales_unavailable())?
                .map(Cell::Int)
        } else {
            row.try_get::<Option<String>, _>(*key)
                .map_err(|_| sales_unavailable())?
                .map(Cell::Text)
        };
        cells.push((*key, cell));
    }
    project_named(&cells, keys)
}

fn scope_binds(
    scope: &SalesScope,
    party: Option<&str>,
) -> Result<Vec<Option<String>>, BusinessError> {
    if !valid_id(&scope.company)
        || scope
            .branch
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || scope
            .department
            .as_deref()
            .is_some_and(|value| !valid_id(value))
        || party.is_some_and(|value| !valid_id(value))
    {
        return Err(sales_unavailable());
    }
    let branch = scope.branch.clone();
    let department = scope.department.clone();
    let mut binds = vec![
        Some(scope.company.clone()),
        branch.clone(),
        branch,
        department.clone(),
        department,
    ];
    if let Some(party) = party {
        binds.push(Some(party.to_owned()));
    }
    Ok(binds)
}

/// Postgres-backed Sales reader. Only Commerce-owned product/order tables are
/// touched; no Sales permission, cross-owner projection or lock is inferred.
pub struct PgCommerceLane14SalesRead {
    pool: DatabasePool,
}

impl PgCommerceLane14SalesRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn fetch(
        &self,
        sql: &'static str,
        keys: &'static [&'static str],
        binds: Vec<Option<String>>,
    ) -> SalesReadFuture<'_, Vec<Value>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let binds = binds.clone();
                Box::pin(async move {
                    let mut query = sqlx::query(sql);
                    for bind in &binds {
                        query = query.bind(bind.as_deref());
                    }
                    let rows = query
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| sales_unavailable())?;
                    rows.iter()
                        .map(|row| sales_row(row, keys))
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => sales_unavailable(),
            })
        })
    }
}

impl CommerceLane14SalesRead for PgCommerceLane14SalesRead {
    fn pricing_products<'a>(&'a self, company: &'a str) -> SalesReadFuture<'a, Vec<Value>> {
        self.fetch(
            SALES_PRODUCTS_SQL,
            PRODUCT_KEYS,
            vec![Some(company.to_owned())],
        )
    }

    fn pricing_product<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> SalesReadFuture<'a, Option<Value>> {
        let read = self.fetch(
            SALES_PRODUCT_SQL,
            PRODUCT_POINT_KEYS,
            vec![Some(id.to_owned()), Some(company.to_owned())],
        );
        Box::pin(async move { Ok(read.await?.into_iter().next()) })
    }

    fn order_counts<'a>(
        &'a self,
        scope: &'a SalesScope,
        ids: &'a [String],
    ) -> SalesReadFuture<'a, Vec<Value>> {
        let binds = match scope_binds(scope, None) {
            Ok(binds) => binds,
            Err(error) => return Box::pin(async { Err(error) }),
        };
        if ids.is_empty() || ids.len() > 64 || !ids.iter().all(|id| valid_id(id)) {
            return Box::pin(async { Err(sales_unavailable()) });
        }
        let unit = PgUnitOfWork::new(self.pool.clone());
        let ids: Vec<String> = ids.to_vec();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let binds = binds.clone();
                let ids = ids.clone();
                Box::pin(async move {
                    let mut builder = QueryBuilder::new(SALES_COUNTS_HEAD);
                    builder.push_bind(binds[0].as_deref());
                    builder.push(SALES_COUNTS_SCOPE);
                    builder.push_bind(binds[1].as_deref());
                    builder.push(SALES_COUNTS_SCOPE_MID);
                    builder.push_bind(binds[2].as_deref());
                    builder.push(SALES_COUNTS_TAIL);
                    builder.push_bind(binds[3].as_deref());
                    builder.push(SALES_COUNTS_TAIL_MID);
                    builder.push_bind(binds[4].as_deref());
                    builder.push(SALES_COUNTS_IN);
                    for (index, id) in ids.iter().enumerate() {
                        if index > 0 {
                            builder.push(", ");
                        }
                        builder.push_bind(id.as_str());
                    }
                    builder.push(SALES_COUNTS_END);
                    let query = builder.build();
                    let rows = query
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| sales_unavailable())?;
                    rows.iter()
                        .map(|row| sales_row(row, ORDER_COUNT_KEYS))
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => sales_unavailable(),
            })
        })
    }

    fn open_orders<'a>(
        &'a self,
        scope: &'a SalesScope,
        party: &'a str,
    ) -> SalesReadFuture<'a, Vec<Value>> {
        let binds = match scope_binds(scope, Some(party)) {
            Ok(binds) => binds,
            Err(error) => return Box::pin(async { Err(error) }),
        };
        self.fetch(SALES_OPEN_ORDERS_SQL, OPEN_ORDER_KEYS, binds)
    }

    fn support_source<'a>(
        &'a self,
        scope: &'a SalesScope,
        party: &'a str,
    ) -> SalesReadFuture<'a, Option<Value>> {
        let binds = match scope_binds(scope, Some(party)) {
            Ok(binds) => binds,
            Err(error) => return Box::pin(async { Err(error) }),
        };
        let read = self.fetch(SALES_SUPPORT_SQL, SUPPORT_ORDER_KEYS, binds);
        Box::pin(async move { Ok(read.await?.into_iter().next()) })
    }
}

#[cfg(test)]
mod sales_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn sales_sql_oracles_are_byte_identical() {
        assert_eq!(
            sha(SALES_PRODUCTS_TS),
            "3c9de1470c375e7c3787511f34544b637a5440f92248fa8671e52da0ef7d177a"
        );
        assert_eq!(
            sha(SALES_PRODUCT_TS),
            "34dbd55e3ba33962a21d9352dd2ff074807deb4bd4fe2dd5d481a0e794947460"
        );
        assert_eq!(
            sha(SALES_COUNTS_TS_2),
            "409d3c8f3b2b0a3705683ee546fbadc3fb9a63f3336d0c0dcb11cbc2640d19e5"
        );
        assert_eq!(
            sha(SALES_OPEN_ORDERS_TS),
            "64de36fd517ec90e29e15b6a3ea0e7b8d1a60e4ae8dd9f107bb47f11fb2166f5"
        );
        assert_eq!(
            sha(SALES_SUPPORT_TS),
            "909df75fecd0af7faa90b2ced8d44b5096f148236a3fe5cdd354265273a6ea29"
        );
        assert_eq!(
            sha(&[
                SALES_PRODUCTS_TS,
                SALES_PRODUCT_TS,
                SALES_COUNTS_TS_2,
                SALES_OPEN_ORDERS_TS,
                SALES_SUPPORT_TS,
            ]
            .join("\n")),
            "5c97d39db999d5b406cc990b067f435f550560597e6865f65fc9e1c7a44cec97"
        );
        for statement in [
            SALES_PRODUCTS_TS,
            SALES_PRODUCT_TS,
            SALES_COUNTS_TS_2,
            SALES_OPEN_ORDERS_TS,
            SALES_SUPPORT_TS,
        ] {
            assert!(statement.starts_with("SELECT "));
            assert!(statement.contains("company_id=?"));
        }
    }

    #[test]
    fn query_builder_renders_the_canonical_counts_bind_order() {
        let mut builder = QueryBuilder::new(SALES_COUNTS_HEAD);
        builder
            .push_bind(Some("co-1"))
            .push(SALES_COUNTS_SCOPE)
            .push_bind(Some("br-1"))
            .push(SALES_COUNTS_SCOPE_MID)
            .push_bind(Some("br-1"))
            .push(SALES_COUNTS_TAIL)
            .push_bind(None::<&str>)
            .push(SALES_COUNTS_TAIL_MID)
            .push_bind(None::<&str>)
            .push(SALES_COUNTS_IN)
            .push_bind("party-1")
            .push(", ")
            .push_bind("party-2")
            .push(SALES_COUNTS_END);
        assert_eq!(
            builder.sql(),
            "SELECT o.party_id,COUNT(*) count FROM commerce_orders o WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$5) AND o.party_id IN ($6, $7) AND o.kind IN ('quote','sales') AND o.state NOT IN ('fulfilled','converted','cancelled') GROUP BY o.party_id"
        );
        let mut text = builder.sql().as_str().to_owned();
        for ordinal in (1..=7).rev() {
            text = text.replace(&format!("${ordinal}"), "?");
        }
        text = text.replace(", ", ",");
        assert_eq!(text, SALES_COUNTS_TS_2);
    }

    #[test]
    fn sales_scope_binds_match_retired_order() {
        let scope = SalesScope {
            company: "co-1".to_owned(),
            branch: Some("br-1".to_owned()),
            department: None,
        };
        assert_eq!(
            scope_binds(&scope, Some("party-1")).expect("binds"),
            vec![
                Some("co-1".to_owned()),
                Some("br-1".to_owned()),
                Some("br-1".to_owned()),
                None,
                None,
                Some("party-1".to_owned()),
            ]
        );
    }
}
