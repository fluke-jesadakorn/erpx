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
