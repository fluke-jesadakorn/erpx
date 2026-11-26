//! PostgreSQL adapter for the native Account profile read.
//!
//! This is the only layer that executes SQL. The statement is the exact
//! Control read used by the retired TypeScript `readAccountProfile`: one
//! `"user"` row left-joined to its `account_preferences` row. It runs inside
//! one caller-owned `PgUnitOfWork`; persistence faults fail closed as 503.

use crate::application::account_profile::{AccountProfileFuture, AccountProfileReader};
use crate::domain::account_profile::ProfileRecord;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, Row};

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_PROFILE_UNAVAILABLE",
        "The native Account profile service is unavailable.",
    )
}

/// Pg-backed owner reader. Composition supplies the Control pool only; the
/// SQL, transaction boundary and row mapping stay inside this owner crate.
pub struct PgAccountProfileReader {
    control: DatabasePool,
}

impl PgAccountProfileReader {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

impl AccountProfileReader for PgAccountProfileReader {
    fn read<'a>(&'a self, user_id: &'a str) -> AccountProfileFuture<'a, Option<ProfileRecord>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let user_id = user_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let user_id = user_id.clone();
                Box::pin(async move {
                    if !connection.is_in_transaction() {
                        return Err(PgTransactionError::Business(unavailable()));
                    }
                    let row = sqlx::query(
                        "SELECT u.name,u.email,p.language,p.appearance,p.version \
                         FROM \"user\" u \
                         LEFT JOIN account_preferences p ON p.user_id=u.id \
                         WHERE u.id=$1",
                    )
                    .bind(&user_id)
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| unavailable())?;
                    let Some(row) = row else {
                        return Ok(None);
                    };
                    Ok(Some(ProfileRecord {
                        name: row.get::<String, _>("name"),
                        email: row.get::<String, _>("email"),
                        language: row.get::<Option<String>, _>("language"),
                        appearance: row.get::<Option<String>, _>("appearance"),
                        version: row.get::<Option<i32>, _>("version"),
                    }))
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
mod postgres_tests {
    use super::*;
    use crate::people_fixture as fixture;
    use erpx_server_runtime::public::postgres::{MigrationTrack, load_migrations, migrate_database};
    use std::path::Path;

    /// Real SQL execution proof: the adapter reads the Control `"user"` row and
    /// its optional `account_preferences` row from a leased isolated database.
    #[tokio::test]
    async fn reads_profile_and_missing_preference_defaults() {
        fixture::with_database(4, |pool| async move {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
            let files = load_migrations(root, MigrationTrack::Control)
                .await
                .unwrap();
            migrate_database(&pool, MigrationTrack::Control, &files)
                .await
                .unwrap();
            sqlx::raw_sql(
                "INSERT INTO \"user\"(id,name,email,\"createdAt\",\"updatedAt\") \
                 VALUES('user-1','Ada','ada@example.test',1,1);",
            )
            .execute(&pool)
            .await
            .unwrap();

            let reader = PgAccountProfileReader::new(pool.clone());
            let row = AccountProfileReader::read(&reader, "user-1")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(row.name, "Ada");
            assert_eq!(row.email, "ada@example.test");
            assert_eq!(row.version, None);

            sqlx::raw_sql(
                "INSERT INTO account_preferences(user_id,language,appearance,version,updated_at) \
                 VALUES('user-1','th','dark',7,1);",
            )
            .execute(&pool)
            .await
            .unwrap();
            let row = AccountProfileReader::read(&reader, "user-1")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(row.version, Some(7));
            assert_eq!(row.language.as_deref(), Some("th"));
            assert_eq!(row.appearance.as_deref(), Some("dark"));

            assert!(AccountProfileReader::read(&reader, "absent")
                .await
                .unwrap()
                .is_none());
        })
        .await;
    }
}
