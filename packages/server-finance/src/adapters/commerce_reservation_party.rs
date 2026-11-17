//! Finance source evidence for the legacy Commerce order/party inner join.
//! This is not actor, Company or purchase authorization. The caller must bind
//! this exact party ID to its Commerce source and recheck its own authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(&mut *c).await?)
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ORDER_NOT_FOUND",
        "Order not found in this workspace.",
    )
    .into()
}

/// A Finance-owned existence observation bound to one retained transaction.
/// It does not assert party status, kind or company: the legacy order read
/// joins by ID only. Adding stricter checks here would change command behavior.
pub struct CommerceReservationParty {
    unit: Unit,
    id: String,
}
impl CommerceReservationParty {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(unavailable());
        }
        let row: Option<String> =
            sqlx::query_scalar("SELECT id FROM finance_parties WHERE id=$1 FOR SHARE")
                .bind(&self.id)
                .fetch_optional(&mut *c)
                .await?;
        if row.as_deref() != Some(self.id.as_str()) {
            return Err(unavailable());
        }
        Ok(())
    }
}

/// Observe the party selected by the real Commerce source. The caller must
/// have acquired the common Finance/stock source fences before any data read.
pub async fn observe_commerce_reservation_party(
    c: &mut PgConnection,
    party_id: &str,
) -> Result<CommerceReservationParty, Error> {
    if party_id.is_empty() {
        return Err(unavailable());
    }
    let unit = unit(c).await?;
    let fenced: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation='finance_parties'::regclass)")
        .fetch_one(&mut *c).await?;
    if !fenced {
        return Err(PersistenceError::new("FINANCE_PARTY_FENCE_REQUIRED").into());
    }
    let row: Option<String> =
        sqlx::query_scalar("SELECT id FROM finance_parties WHERE id=$1 FOR SHARE")
            .bind(party_id)
            .fetch_optional(&mut *c)
            .await?;
    let id = row.ok_or_else(unavailable)?;
    Ok(CommerceReservationParty { unit, id })
}
