//! PostgreSQL adapter for the native Ecommerce channel draft and
//! channel/listing review commands.
//!
//! SQL is Ecommerce-owned (`ecommerce_channels`, `ecommerce_listings`) plus the
//! existing shared `command_receipts`, `audit_events` and `outbox_events`
//! records written by the retained workflow command boundary. No product,
//! customer, warehouse, Sales, Inventory or Finance row is read or written. The
//! caller's serializable Holding transaction is required and no commit is
//! performed here.
use crate::application::{
    AuditEntry, EcommerceCommandRepository, EcommerceFuture, OutboxEntry, ReceiptEntry,
};
use crate::domain::actor::EcommerceActor;
use crate::domain::channel::{ChannelDraft, ChannelReadback};
use crate::domain::command::CommandReceipt;
use crate::domain::review::{ReviewKind, ReviewReadback, ReviewSource, ValidatedReview};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgEcommerceCommand<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgEcommerceCommand<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ECOMMERCE_COMMAND_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("ECOMMERCE_COMMAND_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }
}

impl EcommerceCommandRepository for PgEcommerceCommand<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EcommerceFuture<'a, Option<CommandReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(CommandReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row.try_get("authority_json")?,
                })
            })
            .transpose()
        })
    }

    fn insert_channel<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        channel: &'a ChannelDraft,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "INSERT INTO ecommerce_channels\
                 (id,company_id,name,channel_type,configuration_json,status,created_by,\
                 reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,'draft',$6,NULL,1,$7,$7) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&channel.id)
            .bind(actor.company_id.as_str())
            .bind(&channel.name)
            .bind(&channel.channel_type)
            .bind(&channel.configuration_json)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn channel_readback<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ChannelReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,name,channel_type,configuration_json,status,created_by,\
                 reviewed_by,version,created_at,updated_at \
                 FROM ecommerce_channels WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ChannelReadback {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    channel_type: row.try_get("channel_type")?,
                    configuration_json: row.try_get("configuration_json")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .transpose()
        })
    }

    fn review_source<'a>(
        &'a mut self,
        kind: ReviewKind,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewSource>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row =
                match kind {
                    ReviewKind::Channel => sqlx::query(
                        "SELECT id,company_id,status,version,created_by FROM ecommerce_channels \
                         WHERE id=$1 AND company_id=$2 FOR UPDATE",
                    )
                    .bind(id)
                    .bind(company.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?,
                    ReviewKind::Listing => {
                        sqlx::query(
                            "SELECT l.id,c.company_id,l.status,l.version,l.created_by \
                         FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
                         WHERE l.id=$1 AND c.company_id=$2 FOR UPDATE OF l,c",
                        )
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_optional(&mut *self.connection)
                        .await?
                    }
                };
            row.map(|row| {
                Ok::<_, Error>(ReviewSource {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                })
            })
            .transpose()
        })
    }

    fn commit_review<'a>(
        &'a mut self,
        review: &'a ValidatedReview,
        company: &'a CompanyId,
        actor: &'a EcommerceActor,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = match review.kind {
                ReviewKind::Channel => sqlx::query(
                    "UPDATE ecommerce_channels SET status=$3,reviewed_by=$4,\
                     version=version+1,updated_at=$5 \
                     WHERE id=$1 AND company_id=$2 AND version=$6 AND status=$7",
                )
                .bind(&review.id)
                .bind(company.as_str())
                .bind(review.next_status)
                .bind(actor.user_id.as_str())
                .bind(now.get())
                .bind(review.expected_version)
                .bind(review.expected_status)
                .execute(&mut *self.connection)
                .await?
                .rows_affected(),
                ReviewKind::Listing => sqlx::query(
                    "UPDATE ecommerce_listings l SET status=$3,reviewed_by=$4,\
                     version=l.version+1,updated_at=$5 FROM ecommerce_channels c \
                     WHERE l.channel_id=c.id AND l.id=$1 AND c.company_id=$2 \
                     AND l.version=$6 AND l.status=$7",
                )
                .bind(&review.id)
                .bind(company.as_str())
                .bind(review.next_status)
                .bind(actor.user_id.as_str())
                .bind(now.get())
                .bind(review.expected_version)
                .bind(review.expected_status)
                .execute(&mut *self.connection)
                .await?
                .rows_affected(),
            };
            Ok(changed == 1)
        })
    }

    fn review_readback<'a>(
        &'a mut self,
        kind: ReviewKind,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ReviewReadback>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row =
                match kind {
                    ReviewKind::Channel => sqlx::query(
                        "SELECT id,company_id,status,version,reviewed_by FROM ecommerce_channels \
                         WHERE id=$1 AND company_id=$2",
                    )
                    .bind(id)
                    .bind(company.as_str())
                    .fetch_optional(&mut *self.connection)
                    .await?,
                    ReviewKind::Listing => {
                        sqlx::query(
                            "SELECT l.id,c.company_id,l.status,l.version,l.reviewed_by \
                         FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
                         WHERE l.id=$1 AND c.company_id=$2",
                        )
                        .bind(id)
                        .bind(company.as_str())
                        .fetch_optional(&mut *self.connection)
                        .await?
                    }
                };
            row.map(|row| {
                Ok::<_, Error>(ReviewReadback {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                })
            })
            .transpose()
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let authority = entry.actor.authority_json();
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(entry.actor.user_id.as_str())
            .bind(entry.request_hash)
            .bind(entry.record_id)
            .bind(entry.result_json)
            .bind(now.get())
            .bind(authority)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                 record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
            )
            .bind(entry.key)
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(now.get())
            .bind(entry.authority_json)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let authority: Value =
                serde_json::from_str(entry.authority_json).unwrap_or(Value::Null);
            let company = authority.get("companyId").cloned().unwrap_or(Value::Null);
            let branch = authority.get("branchId").cloned().unwrap_or(Value::Null);
            let department = authority
                .get("departmentId")
                .cloned()
                .unwrap_or(Value::Null);
            let payload = json!({
                "authority": authority,
                "id": entry.record_id,
                "state": entry.state,
                "companyId": company,
                "branchId": branch,
                "departmentId": department,
                "version": entry.version,
            })
            .to_string();
            sqlx::query(
                "INSERT INTO outbox_events\
                 (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                 payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(payload)
            .bind(now.get())
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }
}
