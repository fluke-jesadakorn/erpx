//! PostgreSQL read adapter for the native CRM lead case-event history.
//!
//! This is the only layer in the migrated path that holds or executes SQL. It
//! reads the CRM owner's `crm_case_events` table on the caller's retained
//! serializable Holding unit and never opens a transaction, writes a row or
//! reads a cross-owner table. The statement is the retired TypeScript
//! `crmCaseEventReadSql('*', 'lead_id=?', ' ORDER BY created_at DESC,id DESC')`
//! projection with its exact positional payload.
use crate::application::crm_case_event_history::{
    CrmCaseEventHistoryFuture, CrmCaseEventHistoryRepository,
};
use crate::domain::crm_case_event_history::{CrmCaseEventRow, request_invalid};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Exact owner SELECT. `SELECT *` in the retired TypeScript row became an
/// explicit column list so the Rust row maps by name; the column set, order and
/// single positional bind are unchanged.
const HISTORY_SQL: &str = "SELECT e.id,e.lead_id,e.opportunity_id,e.version,e.action,e.actor_id,e.reason,e.snapshot_json,e.created_at FROM crm_case_events e WHERE e.lead_id=$1 ORDER BY e.created_at DESC,e.id DESC";

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_HISTORY_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_HISTORY_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCrmCaseEventHistory<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmCaseEventHistory<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    /// Execute the real owner read and map every row to the stable payload.
    pub async fn lead_events(&mut self, lead_id: &str) -> Result<Vec<CrmCaseEventRow>, Error> {
        serializable(&mut *self.connection).await?;
        if lead_id.is_empty() || lead_id.encode_utf16().count() > 200 {
            return Err(request_invalid().into());
        }
        let rows = sqlx::query(HISTORY_SQL)
            .bind(lead_id)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(CrmCaseEventRow {
                    id: row.try_get::<String, _>("id")?,
                    lead_id: row.try_get::<String, _>("lead_id")?,
                    opportunity_id: row.try_get::<Option<String>, _>("opportunity_id")?,
                    version: row.try_get::<i64, _>("version")?,
                    action: row.try_get::<String, _>("action")?,
                    actor_id: row.try_get::<String, _>("actor_id")?,
                    reason: row.try_get::<String, _>("reason")?,
                    snapshot_json: row.try_get::<serde_json::Value, _>("snapshot_json")?,
                    created_at: row.try_get::<i64, _>("created_at")?,
                })
            })
            .collect()
    }
}

impl CrmCaseEventHistoryRepository for PgCrmCaseEventHistory<'_> {
    type Error = Error;

    fn lead_events<'a>(
        &'a mut self,
        lead_id: &'a str,
    ) -> CrmCaseEventHistoryFuture<'a, Vec<CrmCaseEventRow>, Self::Error> {
        Box::pin(async move { PgCrmCaseEventHistory::lead_events(self, lead_id).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_statement_is_a_bounded_read_with_full_owner_projection() {
        let upper = HISTORY_SQL.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert!(!upper.contains("FOR UPDATE"));
        assert_eq!(HISTORY_SQL.matches("$1").count(), 1);
        assert_eq!(HISTORY_SQL.matches('$').count(), 1);
        for column in [
            "e.id",
            "e.lead_id",
            "e.opportunity_id",
            "e.version",
            "e.action",
            "e.actor_id",
            "e.reason",
            "e.snapshot_json",
            "e.created_at",
        ] {
            assert!(HISTORY_SQL.contains(column), "missing {column}");
        }
        assert!(HISTORY_SQL.ends_with("ORDER BY e.created_at DESC,e.id DESC"));
    }
}
