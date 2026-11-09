//! PostgreSQL adapter for Control quick-access preferences. The caller owns
//! the transaction; this adapter never commits and never supplies authority.
use crate::application::quick_access_preferences::{
    QuickAccessReceipt, QuickAccessRepository, result_json,
};
use crate::domain::quick_access_preferences::{
    QuickAccessPreference, idempotency_conflict, version_conflict,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

pub struct PgQuickAccessRepository<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgQuickAccessRepository<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("QUICK_ACCESS_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
}

impl QuickAccessRepository for PgQuickAccessRepository<'_> {
    type Error = Error;

    fn read<'a>(
        &'a mut self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> impl Future<Output = Result<Option<QuickAccessPreference>, Self::Error>> + Send + 'a {
        async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT pinned_ids_json,version FROM quick_access_preferences \
                 WHERE user_id=$1 AND surface=$2 AND context_key=$3",
            )
            .bind(user_id)
            .bind(surface)
            .bind(context_key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                let pinned: Option<String> = row.try_get("pinned_ids_json")?;
                let pinned_ids = pinned
                    .map(|value| {
                        serde_json::from_str::<Vec<String>>(&value).map_err(|_| {
                            BusinessError::invalid(
                                "INVALID_SHORTCUT_RESULT",
                                "Review the saved shortcuts.",
                            )
                        })
                    })
                    .transpose()
                    .map_err(Error::from)?;
                Ok(QuickAccessPreference {
                    pinned_ids,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        }
    }

    fn receipt<'a>(
        &'a mut self,
        request_key: &'a str,
    ) -> impl Future<Output = Result<Option<QuickAccessReceipt>, Self::Error>> + Send + 'a {
        async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT user_id,surface,context_key,request_hash,result_json \
                 FROM quick_access_preference_receipts WHERE id=$1",
            )
            .bind(request_key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok(QuickAccessReceipt {
                    user_id: row.try_get("user_id")?,
                    surface: row.try_get("surface")?,
                    context_key: row.try_get("context_key")?,
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                })
            })
            .transpose()
        }
    }

    fn save_preference<'a>(
        &'a mut self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
        result: &'a QuickAccessPreference,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        async move {
            self.ready()?;
            let pinned = result
                .pinned_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).expect("serializable shortcut pins"));
            let updated = sqlx::query(
                "INSERT INTO quick_access_preferences\
                 (user_id,surface,context_key,pinned_ids_json,version,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6) \
                 ON CONFLICT(user_id,surface,context_key) DO UPDATE SET \
                 pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,\
                 updated_at=excluded.updated_at \
                 WHERE quick_access_preferences.version=$7",
            )
            .bind(user_id)
            .bind(surface)
            .bind(context_key)
            .bind(pinned)
            .bind(result.version)
            .bind(now)
            .bind(result.version - 1)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(version_conflict().into());
            }
            Ok(())
        }
    }

    fn save_receipt<'a>(
        &'a mut self,
        request_key: &'a str,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
        request_hash: &'a str,
        result: &'a QuickAccessPreference,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        async move {
            self.ready()?;
            let inserted = sqlx::query(
                "INSERT INTO quick_access_preference_receipts\
                 (id,user_id,surface,context_key,request_hash,result_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING",
            )
            .bind(request_key)
            .bind(user_id)
            .bind(surface)
            .bind(context_key)
            .bind(request_hash)
            .bind(result_json(result))
            .bind(now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(idempotency_conflict().into());
            }
            Ok(())
        }
    }
}
