//! Pg-backed execution of the native Sales customer-note read.
//!
//! Starts from the byte-identical TypeScript statements (retained as SQL oracles
//! in `domain.rs`) and moves the projection into Rust: only the note/history
//! columns are selected, scope-witness columns from `commerce_orders` stay in
//! the predicate and never enter the response, and every row is projected into
//! a typed record before JSON is built. No caller value is concatenated into
//! SQL; all values are positional binds inside a serializable owner unit.

use super::application::{CustomerNoteRead, CustomerNoteReadFuture};
use super::domain::{NoteQuery, NoteRecord, NotesResponse, HistoryRecord};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_CUSTOMER_NOTES_UNAVAILABLE",
        "The native Sales customer-note read is unavailable.",
    )
}

/// Order-scoped note list. `$1` order id, `$2` company, `$3` branch, `$4`
/// department. The TypeScript statement bound branch/department twice; Postgres
/// reuses one positional parameter, so bind order is preserved and count reduced.
const SQL_ORDER_NOTES: &str = concat!(
    "SELECT n.id,n.company_id,n.customer_id,n.order_id,n.channel,n.occurred_at,n.body,",
    "n.status,n.version,n.created_by,n.created_at,n.updated_at",
    " FROM customer_contact_notes n JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id",
    " WHERE n.order_id=$1 AND n.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3)",
    " AND ($4::text IS NULL OR o.department_id=$4)",
    " ORDER BY n.created_at DESC,n.id DESC"
);

/// Customer-scoped note list. Notes without an order are visible only to an
/// unscoped (holding) actor; order-bound notes obey the branch/department witness.
const SQL_CUSTOMER_NOTES: &str = concat!(
    "SELECT n.id,n.company_id,n.customer_id,n.order_id,n.channel,n.occurred_at,n.body,",
    "n.status,n.version,n.created_by,n.created_at,n.updated_at",
    " FROM customer_contact_notes n LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id",
    " WHERE n.company_id=$1 AND n.customer_id=$2",
    " AND (n.order_id IS NULL AND $3::text IS NULL AND $4::text IS NULL",
    " OR n.order_id IS NOT NULL AND ($3::text IS NULL OR o.branch_id=$3)",
    " AND ($4::text IS NULL OR o.department_id=$4))",
    " ORDER BY n.created_at DESC,n.id DESC"
);

const SQL_ORDER_HISTORY: &str = concat!(
    "SELECT h.id,h.note_id,h.version,h.action,h.body,h.reason,h.actor_id,h.created_at",
    " FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id",
    " JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id",
    " WHERE n.order_id=$1 AND o.company_id=$2 AND ($3::text IS NULL OR o.branch_id=$3)",
    " AND ($4::text IS NULL OR o.department_id=$4)",
    " ORDER BY h.created_at DESC,h.version DESC"
);

const SQL_CUSTOMER_HISTORY: &str = concat!(
    "SELECT h.id,h.note_id,h.version,h.action,h.body,h.reason,h.actor_id,h.created_at",
    " FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id",
    " LEFT JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id",
    " WHERE n.company_id=$1 AND n.customer_id=$2",
    " AND (n.order_id IS NULL AND $3::text IS NULL AND $4::text IS NULL",
    " OR n.order_id IS NOT NULL AND ($3::text IS NULL OR o.branch_id=$3)",
    " AND ($4::text IS NULL OR o.department_id=$4))",
    " ORDER BY h.created_at DESC,h.version DESC"
);

fn note(row: &sqlx::postgres::PgRow) -> Result<NoteRecord, BusinessError> {
    Ok(NoteRecord {
        id: row.try_get("id").map_err(|_| unavailable())?,
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        customer_id: row.try_get("customer_id").map_err(|_| unavailable())?,
        order_id: row.try_get("order_id").map_err(|_| unavailable())?,
        channel: row.try_get("channel").map_err(|_| unavailable())?,
        occurred_at: row.try_get("occurred_at").map_err(|_| unavailable())?,
        body: row.try_get("body").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        created_by: row.try_get("created_by").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
    })
}

fn history(row: &sqlx::postgres::PgRow) -> Result<HistoryRecord, BusinessError> {
    Ok(HistoryRecord {
        id: row.try_get("id").map_err(|_| unavailable())?,
        note_id: row.try_get("note_id").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        action: row.try_get("action").map_err(|_| unavailable())?,
        body: row.try_get("body").map_err(|_| unavailable())?,
        reason: row.try_get("reason").map_err(|_| unavailable())?,
        actor_id: row.try_get("actor_id").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
    })
}

/// Pg-backed owner reader. A single serializable unit runs both statements so
/// the note list and its immutable history observe the same committed snapshot.
pub struct PgCustomerNoteRead {
    pool: DatabasePool,
}

impl PgCustomerNoteRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CustomerNoteRead for PgCustomerNoteRead {
    fn read<'a>(&'a self, query: &'a NoteQuery) -> CustomerNoteReadFuture<'a, NotesResponse> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company.clone();
        let branch = query.branch.clone();
        let department = query.department.clone();
        let source = query.source.clone();
        let id = query.id.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let branch = branch.clone();
                let department = department.clone();
                let source = source.clone();
                let id = id.clone();
                Box::pin(async move {
                    let by_order = source == super::domain::NOTE_SOURCE_ORDER;
                    let notes_sql = if by_order { SQL_ORDER_NOTES } else { SQL_CUSTOMER_NOTES };
                    let history_sql = if by_order { SQL_ORDER_HISTORY } else { SQL_CUSTOMER_HISTORY };
                    let first = if by_order { &id } else { &company };
                    let second = if by_order { &company } else { &id };
                    let branch = branch.as_deref();
                    let department = department.as_deref();

                    let note_rows = sqlx::query(notes_sql)
                        .bind(first.as_str())
                        .bind(second.as_str())
                        .bind(branch)
                        .bind(department)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let history_rows = sqlx::query(history_sql)
                        .bind(first.as_str())
                        .bind(second.as_str())
                        .bind(branch)
                        .bind(department)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;

                    Ok(NotesResponse {
                        notes: note_rows.iter().map(note).collect::<Result<Vec<_>, _>>()?,
                        history: history_rows.iter().map(history).collect::<Result<Vec<_>, _>>()?,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_is_read_only_and_company_scoped() {
        for (sql, scope_binds) in [
            (SQL_ORDER_NOTES, 2usize),
            (SQL_CUSTOMER_NOTES, 3),
            (SQL_ORDER_HISTORY, 2),
            (SQL_CUSTOMER_HISTORY, 3),
        ] {
            let upper = sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            let words: Vec<&str> = upper.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            assert!(!words.contains(&"INSERT"));
            assert!(!words.contains(&"UPDATE"));
            assert!(!words.contains(&"DELETE"));
            assert!(upper.contains("COMPANY_ID"));
            assert_eq!(sql.matches("$1").count(), 1);
            assert_eq!(sql.matches("$2").count(), 1);
            assert_eq!(sql.matches("$3").count(), scope_binds);
            assert_eq!(sql.matches("$4").count(), scope_binds);
        }
    }

    #[test]
    fn projection_never_selects_scope_witness_columns() {
        for sql in [SQL_ORDER_NOTES, SQL_CUSTOMER_NOTES] {
            assert!(sql.starts_with("SELECT n.id,n.company_id"));
            assert!(!sql.contains("o.*"));
        }
        for sql in [SQL_ORDER_HISTORY, SQL_CUSTOMER_HISTORY] {
            assert!(sql.starts_with("SELECT h.id,h.note_id"));
            assert!(!sql.contains("o.*"));
        }
    }
}
