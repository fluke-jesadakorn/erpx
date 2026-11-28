//! PostgreSQL adapter for native member-profile commands.
//!
//! The caller owns one serializable transaction and fresh Account/IAM
//! authority. This adapter writes only Organization-owned member_profiles rows
//! and the standard command receipt/audit/outbox ledger. Identity membership,
//! File media and Work facts are supplied through composition ports; their SQL
//! is never reached here.
use crate::application::member_profile::{
    MemberProfileCommandFuture, MemberProfileCommandRepository, MemberProfileLedgerEntry,
    MemberProfileWrite, StoredMemberProfileReceipt,
};
use crate::domain::member_profile::{idempotency_conflict, request_invalid};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn write_error(error: sqlx::Error) -> Error {
    if error
        .as_database_error()
        .and_then(|value| value.code())
        .as_deref()
        == Some("23505")
    {
        conflict(
            "PROFILE_VERSION_CHANGED",
            "The profile changed. Reload and review before saving.",
        )
        .into()
    } else {
        error.into()
    }
}

pub struct PgMemberProfileCommand<'c> {
    connection: &'c mut PgConnection,
    user_id: String,
    assignment_id: String,
    authority_json: String,
}

impl<'c> PgMemberProfileCommand<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        actor: &crate::domain::member_profile::MemberProfileActor,
    ) -> Self {
        Self {
            connection,
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            authority_json: actor.authority_json.clone(),
        }
    }

    async fn read_ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ORGANIZATION_MEMBER_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn ready(&mut self) -> Result<(), Error> {
        self.read_ready().await?;
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("ORGANIZATION_MEMBER_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn insert_ledger(&mut self, entry: &MemberProfileLedgerEntry<'_>) -> Result<(), Error> {
        let result = serde_json::to_string(entry.result).map_err(|_| request_invalid())?;
        let changed = sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(entry.key)
        .bind(entry.operation)
        .bind(&self.user_id)
        .bind(entry.request_hash)
        .bind(entry.record_id)
        .bind(result)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,$10) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("organization-member-audit-{}", entry.key))
        .bind(entry.key)
        .bind(&self.user_id)
        .bind(&self.assignment_id)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(entry.now)
        .bind(&self.authority_json)
        .execute(&mut *self.connection)
        .await?;
        let authority: Value = serde_json::from_str(&self.authority_json).unwrap_or(Value::Null);
        let payload = serde_json::json!({
            "authority": authority,
            "id": entry.record_id,
            "state": entry.state,
            "version": entry.version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(format!("organization-member-outbox-{}", entry.key))
        .bind(entry.key)
        .bind(entry.operation)
        .bind(entry.record_type)
        .bind(entry.record_id)
        .bind(entry.version)
        .bind(payload)
        .bind(entry.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl MemberProfileCommandRepository for PgMemberProfileCommand<'_> {
    type Error = Error;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn lock_command<'a>(
        &'a mut self,
        key: &'a str,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("organization-member-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> MemberProfileCommandFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(identity)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> MemberProfileCommandFuture<'a, Option<StoredMemberProfileReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let request_hash = row.try_get::<String, _>("request_hash")?;
            let result = match row.try_get::<Option<String>, _>("result_json")? {
                None => None,
                Some(value) => Some(serde_json::from_str(&value).map_err(|_| {
                    Error::Business(BusinessError::new(
                        FailureKind::Unavailable,
                        "ORGANIZATION_MEMBER_RECEIPT_INVALID",
                        "The member profile command receipt is invalid.",
                    ))
                })?),
            };
            Ok(Some(StoredMemberProfileReceipt {
                request_hash,
                result,
            }))
        })
    }

    fn ledger<'a>(
        &'a mut self,
        entry: &'a MemberProfileLedgerEntry<'a>,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.insert_ledger(entry).await
        })
    }

    fn current_version<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        user_id: &'a str,
    ) -> MemberProfileCommandFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            self.read_ready().await?;
            Ok(
                sqlx::query_scalar::<_, i64>(
                    "SELECT version FROM member_profiles WHERE user_id=$1",
                )
                .bind(user_id)
                .fetch_optional(&mut *self.connection)
                .await?,
            )
        })
    }

    fn save_profile<'a>(
        &'a mut self,
        command: &'a MemberProfileWrite<'a>,
    ) -> MemberProfileCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if command.insert {
                sqlx::query(
                    "INSERT INTO member_profiles(user_id,bio,links_json,version,updated_at) \
                     VALUES($1,$2,CAST($3 AS text)::jsonb,$4,$5)",
                )
                .bind(command.user_id)
                .bind(command.bio)
                .bind(command.links_json)
                .bind(command.version)
                .bind(command.now)
                .execute(&mut *self.connection)
                .await
                .map_err(write_error)?;
                return Ok(());
            }
            let changed = sqlx::query(
                "UPDATE member_profiles SET bio=$2,links_json=CAST($3 AS text)::jsonb,\
                 version=$4,updated_at=$5 WHERE user_id=$1 AND version=$6",
            )
            .bind(command.user_id)
            .bind(command.bio)
            .bind(command.links_json)
            .bind(command.version)
            .bind(command.now)
            .bind(command.expected_version)
            .execute(&mut *self.connection)
            .await
            .map_err(write_error)?
            .rows_affected();
            if changed != 1 {
                return Err(conflict(
                    "PROFILE_VERSION_CHANGED",
                    "Your profile changed. Reload and review before saving.",
                )
                .into());
            }
            Ok(())
        })
    }
}
