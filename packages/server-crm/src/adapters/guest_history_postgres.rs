//! PostgreSQL read adapter for the native CRM contact guest-history slice.
//!
//! This is the only layer in the migrated path that holds or executes SQL. It
//! reads the CRM owner's `crm_contacts`, `crm_accounts`, `crm_leads` and
//! `crm_guest_links` rows on the caller's retained serializable Holding unit
//! and joins the closed Hotel property/record projections the retained
//! TypeScript reader used. It never opens a transaction, writes a row or
//! reads the already-migrated `crm_case_events` history.
//!
//! Both statements are the retired TypeScript `contact(id)` and `stays(id)`
//! literals with only mechanical changes: `?` markers became positional `$n`
//! and the composition-owned purchase filter (`crmNativeReadFilter`, `true` in
//! disabled enforcement mode) is inlined as `true`. Purchase and Hotel-history
//! admission are composition facts checked by the application port, never by
//! SQL here.
use crate::application::guest_history::{
    GuestHistoryFuture, GuestHistoryRepository,
};
use crate::domain::guest_history::{
    GuestContactRow, GuestHistoryActor, GuestHistorySource, GuestStayRow, request_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::postgres::PgRow;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Composition-owned purchase filter in disabled enforcement mode, matching
/// `adapters/activity_workspace.rs::NATIVE_READ_FILTER`.
pub const NATIVE_READ_FILTER: &str = "true";

/// Exact retired `crmContactAccountReadSql('c.id,c.name,c.account_id','acc',
/// WHERE c.id=? AND c.company_id=? AND <creator lead scope>)` projection.
pub const CONTACT_SQL: &str = concat!(
    "SELECT c.id,c.name,c.account_id FROM crm_contacts c ",
    "JOIN crm_accounts acc ON acc.id=c.account_id ",
    "WHERE c.id=$1 AND c.company_id=$2 ",
    "AND ($3::text IS NULL OR acc.created_by=$4 ",
    "OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id ",
    "AND l.branch_id=$5 AND ($6::text IS NULL OR l.department_id=$7)))"
);

/// Closed public Hotel projection SQL copied verbatim from the retained
/// `commercial-owner-projection-sources` used by `identities.ts::stays`.
pub const PMS_PROPERTIES_SQL: &str =
    "SELECT id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at FROM pms_properties";
pub const PMS_RECORDS_SQL: &str =
    "SELECT id,property_id,kind,data_json,version,updated_at FROM pms_records";

/// Exact retired `crmGuestLinkReservationSourceSql` projection: granted guest
/// links, confirmed reservation records and their optional stays, newest first,
/// fetching 201 rows so the application can present `hasMore` over 200.
pub const STAYS_SQL: &str = concat!(
    "SELECT g.guest_id,r.id reservation_id,r.data_json::jsonb->>'state' reservation_state,",
    "s.id stay_id,s.data_json::jsonb->>'state' stay_state,s.data_json::jsonb->>'arrival' arrival,",
    "s.data_json::jsonb->>'departure' departure,p.id property_id ",
    "FROM crm_guest_links g JOIN (",
    "SELECT id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at FROM pms_properties",
    ") p ON p.id=g.property_id AND p.company_id=g.company_id ",
    "JOIN (",
    "SELECT id,property_id,kind,data_json,version,updated_at FROM pms_records",
    ") r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id ",
    "LEFT JOIN (",
    "SELECT id,property_id,kind,data_json,version,updated_at FROM pms_records",
    ") s ON s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=r.id ",
    "WHERE g.sharing_state='granted' AND g.contact_id=$1 AND g.company_id=$2 ",
    "AND ($3::text IS NULL OR p.branch_id=$4) ",
    "ORDER BY r.updated_at DESC,s.id LIMIT 201"
);

async fn read_unit(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_GUEST_HISTORY_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_GUEST_HISTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn valid_contact_id(value: &str) -> bool {
    !value.is_empty()
        && value.encode_utf16().count() <= 200
        && !value.chars().any(char::is_control)
}

fn contact_row(row: &PgRow) -> Result<GuestContactRow, Error> {
    Ok(GuestContactRow {
        id: row.try_get("id")?,
        name: row.try_get::<Option<String>, _>("name")?.unwrap_or_default(),
        account_id: row.try_get("account_id")?,
    })
}

fn stay_row(row: &PgRow) -> Result<GuestStayRow, Error> {
    Ok(GuestStayRow {
        guest_id: row.try_get("guest_id")?,
        reservation_id: row.try_get("reservation_id")?,
        reservation_state: row.try_get("reservation_state")?,
        stay_id: row.try_get("stay_id")?,
        stay_state: row.try_get("stay_state")?,
        arrival: row.try_get("arrival")?,
        departure: row.try_get("departure")?,
        property_id: row.try_get("property_id")?,
    })
}

/// CRM-owned guest-history rows on the caller's serializable unit.
pub struct PgGuestHistory<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgGuestHistory<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    /// Execute both real owner SELECTs and map every row to the stable source.
    pub async fn read(
        &mut self,
        actor: &GuestHistoryActor,
        contact_id: &str,
    ) -> Result<GuestHistorySource, Error> {
        read_unit(&mut *self.connection).await?;
        if !valid_contact_id(contact_id) {
            return Err(request_invalid().into());
        }
        let contact = sqlx::query(CONTACT_SQL)
            .bind(contact_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(&actor.user_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
        let stays = sqlx::query(STAYS_SQL)
            .bind(contact_id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_deref())
            .bind(actor.branch_id.as_deref())
            .fetch_all(&mut *self.connection)
            .await?;
        Ok(GuestHistorySource {
            contact: contact.as_ref().map(contact_row).transpose()?,
            stays: stays.iter().map(stay_row).collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl GuestHistoryRepository for PgGuestHistory<'_> {
    type Error = Error;

    fn guest_history<'a>(
        &'a mut self,
        actor: &'a GuestHistoryActor,
        contact_id: &'a str,
    ) -> GuestHistoryFuture<'a, GuestHistorySource, Self::Error> {
        Box::pin(PgGuestHistory::read(self, actor, contact_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_statement_is_a_bounded_owner_read_with_the_retired_projection() {
        let upper = CONTACT_SQL.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        for fragment in [
            "INSERT",
            "UPDATE",
            "DELETE",
            "FOR UPDATE",
            "LOCK TABLE",
            "crm_case_events",
        ] {
            assert!(!upper.contains(&fragment.to_ascii_uppercase()), "{fragment}");
        }
        for column in ["c.id", "c.name", "c.account_id", "acc.created_by", "l.contact_id=c.id"] {
            assert!(CONTACT_SQL.contains(column), "missing {column}");
        }
        assert_eq!(CONTACT_SQL.matches('$').count(), 7);
        assert_eq!(NATIVE_READ_FILTER, "true");
    }

    #[test]
    fn stays_statement_executes_the_exact_granted_guest_link_projection() {
        let upper = STAYS_SQL.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT "));
        for fragment in [" INSERT ", " UPDATE ", " DELETE ", "FOR UPDATE", "LOCK TABLE", "crm_case_events"] {
            assert!(!upper.contains(&fragment.to_ascii_uppercase()), "{fragment}");
        }
        for column in [
            "g.guest_id",
            "r.id reservation_id",
            "reservation_state",
            "s.id stay_id",
            "stay_state",
            "arrival",
            "departure",
            "p.id property_id",
        ] {
            assert!(STAYS_SQL.contains(column), "missing {column}");
        }
        assert!(STAYS_SQL.contains("g.sharing_state='granted'"));
        assert!(STAYS_SQL.contains("r.kind='reservation'"));
        assert!(STAYS_SQL.contains("s.kind='stay'"));
        assert!(STAYS_SQL.contains(PMS_PROPERTIES_SQL));
        assert_eq!(STAYS_SQL.matches(PMS_RECORDS_SQL).count(), 2);
        assert_eq!(STAYS_SQL.matches('$').count(), 4);
        assert!(STAYS_SQL.ends_with("ORDER BY r.updated_at DESC,s.id LIMIT 201"));
    }
}
