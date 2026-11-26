//! PostgreSQL adapter for the native Support ticket/incident event-history reads.
//!
//! Both entry points resolve an already-registered owner statement and execute
//! it with a real bound SQL parameter on the caller's already-open serializable
//! Holding unit. The adapter plans no SQL for TypeScript, never opens or
//! commits a transaction and never shapes an HTTP response.
use crate::adapters::native_support_read_sql::{
    read_statement, RD_INCIDENT_EVENT_HISTORY, RD_TICKET_EVENT_HISTORY,
};
use crate::application::support_event_history::{
    read_incident_history, read_ticket_history, SupportEventHistoryFuture,
    SupportEventHistoryPorts,
};
use crate::domain::support_event_history::{
    IncidentEventRow, SupportEventHistoryScope, TicketEventRow,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_EVENT_HISTORY_UNAVAILABLE",
            "The Support event-history reader is unavailable.",
        ),
    }
}

/// Translate the owner-registered `?` bind markers to PostgreSQL `$n` markers.
/// The registry is authoritative for the bind count; an unexpected shape fails
/// closed instead of executing a diluted immutable-event predicate.
fn postgres_statement(kind: &str, bind_count: usize) -> Result<String, Error> {
    let statement = read_statement(kind).ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_EVENT_HISTORY_UNAVAILABLE",
            "The registered Support event-history read is unavailable.",
        )
    })?;
    let mut index = 0usize;
    let mut translated = String::with_capacity(statement.sql.len());
    for character in statement.sql.chars() {
        if character == '?' {
            index += 1;
            translated.push('$');
            translated.push_str(&index.to_string());
        } else {
            translated.push(character);
        }
    }
    if index != bind_count || statement.bind_count != bind_count {
        return Err(BusinessError::invalid(
            "SUPPORT_EVENT_HISTORY_BINDS_INVALID",
            "The registered Support event-history bind set does not match.",
        )
        .into());
    }
    Ok(translated)
}

fn map_ticket(row: &sqlx::postgres::PgRow) -> Result<TicketEventRow, Error> {
    Ok(TicketEventRow {
        id: row.try_get("id")?,
        ticket_id: row.try_get("ticket_id")?,
        version: row.try_get("version")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        reason: row.try_get("reason")?,
        owner_id: row.try_get::<Option<String>, _>("owner_id")?,
        due_date: row.try_get::<Option<String>, _>("due_date")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        evidence_json: row.try_get("evidence_json")?,
    })
}

fn map_incident(row: &sqlx::postgres::PgRow) -> Result<IncidentEventRow, Error> {
    Ok(IncidentEventRow {
        id: row.try_get("id")?,
        incident_id: row.try_get("incident_id")?,
        version: row.try_get("version")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
        evidence_json: row.try_get("evidence_json")?,
    })
}

/// One bounded history unit bound to the caller's connection and selected scope.
pub struct PgSupportEventHistory<'c> {
    connection: &'c mut PgConnection,
    scope: SupportEventHistoryScope,
}

impl<'c> PgSupportEventHistory<'c> {
    pub fn new(connection: &'c mut PgConnection, scope: SupportEventHistoryScope) -> Self {
        Self { connection, scope }
    }

    pub fn scope(&self) -> &SupportEventHistoryScope {
        &self.scope
    }
}

impl SupportEventHistoryPorts for PgSupportEventHistory<'_> {
    /// Executes the registered ticket-event statement with the single `(id)`
    /// bind. `ORDER BY version DESC` is fixed owner SQL.
    fn ticket_events<'a>(
        &'a mut self,
        id: &'a str,
    ) -> SupportEventHistoryFuture<'a, Vec<TicketEventRow>> {
        Box::pin(async move {
            let result: Result<Vec<TicketEventRow>, Error> = async {
                let sql = postgres_statement(RD_TICKET_EVENT_HISTORY, 1)?;
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(id)
                    .fetch_all(&mut *self.connection)
                    .await?;
                rows.iter().map(map_ticket).collect()
            }
            .await;
            result.map_err(business)
        })
    }

    /// Executes the registered incident-event statement with the single `(id)`
    /// bind. `ORDER BY version DESC` is fixed owner SQL.
    fn incident_events<'a>(
        &'a mut self,
        id: &'a str,
    ) -> SupportEventHistoryFuture<'a, Vec<IncidentEventRow>> {
        Box::pin(async move {
            let result: Result<Vec<IncidentEventRow>, Error> = async {
                let sql = postgres_statement(RD_INCIDENT_EVENT_HISTORY, 1)?;
                let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                    .bind(id)
                    .fetch_all(&mut *self.connection)
                    .await?;
                rows.iter().map(map_incident).collect()
            }
            .await;
            result.map_err(business)
        })
    }
}

/// Composition entry point for the ticket-detail history. Real SQL, complete
/// event row parity, no TypeScript plan/execute fallback.
pub async fn load_ticket_history(
    connection: &mut PgConnection,
    scope: SupportEventHistoryScope,
    id: &str,
) -> Result<Value, BusinessError> {
    let mut ports = PgSupportEventHistory::new(connection, scope);
    let selected = ports.scope().clone();
    let view = read_ticket_history(&mut ports, &selected, id).await?;
    serde_json::to_value(view).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_HISTORY_RESPONSE_INVALID",
            "Invalid Support event-history response.",
        )
    })
}

/// Composition entry point for the incident-detail history.
pub async fn load_incident_history(
    connection: &mut PgConnection,
    scope: SupportEventHistoryScope,
    id: &str,
) -> Result<Value, BusinessError> {
    let mut ports = PgSupportEventHistory::new(connection, scope);
    let selected = ports.scope().clone();
    let view = read_incident_history(&mut ports, &selected, id).await?;
    serde_json::to_value(view).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SUPPORT_HISTORY_RESPONSE_INVALID",
            "Invalid Support event-history response.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_history_statements_bind_one_postgres_parameter() {
        let sql = postgres_statement(RD_TICKET_EVENT_HISTORY, 1).unwrap();
        assert!(sql.contains("support_ticket_events WHERE ticket_id=$1"));
        assert!(sql.contains("ORDER BY version DESC"));
        assert!(!sql.contains('?'));

        let sql = postgres_statement(RD_INCIDENT_EVENT_HISTORY, 1).unwrap();
        assert!(sql.contains("support_incident_events WHERE incident_id=$1"));
        assert!(sql.contains("ORDER BY version DESC"));
        assert!(!sql.contains('?'));

        assert!(postgres_statement(RD_TICKET_EVENT_HISTORY, 2).is_err());
        assert!(postgres_statement("support_rd_unregistered", 1).is_err());
    }
}
