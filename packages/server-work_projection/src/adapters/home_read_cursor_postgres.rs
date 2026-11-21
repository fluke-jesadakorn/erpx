//! PostgreSQL adapter for the Work home read-cursor write. The caller owns one
//! Holding transaction; the adapter never opens a transaction and writes only
//! the Work-owned `home_read_cursors` row. All SQL lives here.
use crate::application::home_read_cursor::HomeReadCursorRepository;
use crate::domain::home_read_cursor::MarkHomeReadInput;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub type HomeReadCursorError = PgTransactionError<BusinessError>;

pub struct PgHomeReadCursor<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgHomeReadCursor<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl HomeReadCursorRepository for PgHomeReadCursor<'_> {
    type Error = HomeReadCursorError;

    async fn mark(&mut self, input: &MarkHomeReadInput) -> Result<(), Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("HOME_READ_CURSOR_TRANSACTION_REQUIRED").into());
        }
        sqlx::query(
            "INSERT INTO home_read_cursors(company_id,user_id,workspace_base,read_through) \
             VALUES($1,$2,$3,$4) ON CONFLICT(company_id,user_id,workspace_base) \
             DO UPDATE SET read_through=GREATEST(home_read_cursors.read_through,excluded.read_through)",
        )
        .bind(&input.company_id)
        .bind(&input.user_id)
        .bind(&input.workspace_base)
        .bind(input.read_through)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}
