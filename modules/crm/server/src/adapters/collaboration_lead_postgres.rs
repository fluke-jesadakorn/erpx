//! PostgreSQL read adapter for the native CRM collaboration-lead slice.
//!
//! This is the only layer in the migrated path that holds or executes SQL. It
//! executes the exact retired owner statement (`?` markers became positional
//! `$n`) on the caller's retained serializable Holding unit so the witness sees
//! uncommitted effects, and maps the single row to the stable owner type. It
//! never opens a transaction, writes a row or carries a ledger.
use crate::application::collaboration_lead::{
    CollaborationLeadFuture, CollaborationLeadRepository,
};
use crate::domain::collaboration_lead::{
    CollaborationLeadActor, CollaborationLeadRow, request_invalid,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::postgres::PgRow;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Exact retired `crm_rd_collab_notification_lead` literal with positional binds.
pub const LEAD_SQL: &str =
    "SELECT lead_id FROM crm_lead_activities WHERE id=$1 AND company_id=$2";

async fn read_unit(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_COLLAB_LEAD_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_COLLAB_LEAD_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn valid_record_id(value: &str) -> bool {
    !value.is_empty()
        && value.encode_utf16().count() <= 200
        && !value.chars().any(char::is_control)
}

fn lead_row(row: &PgRow) -> Result<CollaborationLeadRow, Error> {
    Ok(CollaborationLeadRow {
        lead_id: row.try_get("lead_id")?,
    })
}

/// CRM-owned collaboration lead row on the caller's serializable unit.
pub struct PgCollaborationLead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCollaborationLead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    /// Execute the real owner SELECT and map its optional row to the domain.
    pub async fn read(
        &mut self,
        actor: &CollaborationLeadActor,
        record_id: &str,
    ) -> Result<Option<CollaborationLeadRow>, Error> {
        read_unit(&mut *self.connection).await?;
        if !valid_record_id(record_id) {
            return Err(request_invalid().into());
        }
        let row = sqlx::query(LEAD_SQL)
            .bind(record_id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.as_ref().map(lead_row).transpose()
    }
}

impl CollaborationLeadRepository for PgCollaborationLead<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        actor: &'a CollaborationLeadActor,
        record_id: &'a str,
    ) -> CollaborationLeadFuture<'a, Option<CollaborationLeadRow>, Self::Error> {
        Box::pin(PgCollaborationLead::read(self, actor, record_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_statement_is_the_exact_retired_owner_read() {
        assert_eq!(
            LEAD_SQL,
            "SELECT lead_id FROM crm_lead_activities WHERE id=$1 AND company_id=$2"
        );
        let upper = LEAD_SQL.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "FOR UPDATE", "LOCK TABLE"] {
            assert!(!upper.contains(forbidden), "forbidden {forbidden}");
        }
        assert_eq!(LEAD_SQL.matches('$').count(), 2);
        assert!(LEAD_SQL.contains("FROM crm_lead_activities"));
        assert!(LEAD_SQL.contains("WHERE id=$1 AND company_id=$2"));
    }

    #[test]
    fn record_id_guard_matches_the_domain_identifier_limit() {
        assert!(valid_record_id("activity-1"));
        assert!(!valid_record_id(""));
        assert!(!valid_record_id("line\nbreak"));
    }
}
