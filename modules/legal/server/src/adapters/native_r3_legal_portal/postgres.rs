//! Real sqlx execution for the native Legal portal list read.
//!
//! The statement is fixed: the purchase predicate is the owner-verified `TRUE`
//! fragment, the two binds are the verified actor's lowercased email and user
//! id, and the expiry clock is the same PostgreSQL expression the retired read
//! used. Enforced commercial predicates are not executed here.

use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::{PortalActor, PortalFuture, PortalSource};
use super::domain::{PURCHASE_PREDICATE_OPEN, PortalShareWire};

pub const SHARES_SQL: &str = concat!(
    "SELECT s.id,s.document_id,s.expires_at,s.user_id,d.name,d.version,d.checksum ",
    "FROM legal_shares s JOIN legal_items i ON i.id=s.item_id ",
    "JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id ",
    "WHERE TRUE AND lower(s.email)=$1 AND (s.user_id IS NULL OR s.user_id=$2) ",
    "AND s.revoked_at IS NULL ",
    "AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ",
    "ORDER BY s.created_at DESC LIMIT 100"
);

/// Pg-backed share reader. Composition owns the pool and mounts the router.
pub struct PgPortalSource {
    pool: DatabasePool,
}

impl PgPortalSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_PORTAL_READ_UNAVAILABLE",
        "The shared Legal collection could not be read.",
    )
}

fn corrupt() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_PORTAL_CORRUPT_RECORD",
        "The shared Legal record is corrupt.",
    )
}

impl PortalSource for PgPortalSource {
    fn shares<'a>(
        &'a self,
        actor: &'a PortalActor,
        predicate: &'a str,
    ) -> PortalFuture<'a, Vec<PortalShareWire>> {
        let pool = self.pool.clone();
        let email = actor.email.clone();
        let user_id = actor.user_id.clone();
        let predicate_ok = predicate == PURCHASE_PREDICATE_OPEN;
        Box::pin(async move {
            if !predicate_ok {
                return Err(unavailable());
            }
            let rows = sqlx::query(SHARES_SQL)
                .bind(email)
                .bind(user_id)
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    Ok(PortalShareWire {
                        id: r.try_get("id").map_err(|_| corrupt())?,
                        document_id: r.try_get("document_id").map_err(|_| corrupt())?,
                        expires_at: r.try_get("expires_at").map_err(|_| corrupt())?,
                        user_id: r.try_get("user_id").map_err(|_| corrupt())?,
                        name: r.try_get("name").map_err(|_| corrupt())?,
                        version: i64::from(
                            r.try_get::<i32, _>("version").map_err(|_| corrupt())?,
                        ),
                        checksum: r.try_get("checksum").map_err(|_| corrupt())?,
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_list_statement_is_fixed_read_only_and_two_bind() {
        assert_eq!(
            SHARES_SQL,
            concat!(
                "SELECT s.id,s.document_id,s.expires_at,s.user_id,d.name,d.version,d.checksum ",
                "FROM legal_shares s JOIN legal_items i ON i.id=s.item_id ",
                "JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id ",
                "WHERE TRUE AND lower(s.email)=$1 AND (s.user_id IS NULL OR s.user_id=$2) ",
                "AND s.revoked_at IS NULL ",
                "AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ",
                "ORDER BY s.created_at DESC LIMIT 100"
            )
        );
        assert_eq!(SHARES_SQL.matches('$').count(), 2);
        assert!(SHARES_SQL.contains("lower(s.email)=$1"));
        assert!(SHARES_SQL.contains("s.user_id=$2"));
        assert!(SHARES_SQL.contains("s.revoked_at IS NULL"));
        assert!(SHARES_SQL.contains("s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint"));
        assert!(SHARES_SQL.ends_with("ORDER BY s.created_at DESC LIMIT 100"));
        for write in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            "FOR UPDATE",
            "FOR SHARE",
            "LOCK TABLE",
            "pg_advisory",
        ] {
            assert!(!SHARES_SQL.contains(write), "{write}");
        }
        assert!(PURCHASE_PREDICATE_OPEN == "TRUE");
    }
}
