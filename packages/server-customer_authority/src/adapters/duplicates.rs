//! PostgreSQL adapter for the native Customer Authority duplicate-review request
//! and bounded workspace/detail read.
use crate::application::duplicates::{
    DuplicateReviewCommandRepository, DuplicateReviewReadRepository,
};
use crate::domain::access::{AccessReceipt, ReceiptSnapshot};
use crate::domain::duplicates::{
    DuplicateReviewCustomer, DuplicateReviewEventRow, DuplicateReviewRow, RequestPlan,
    readback_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn map_row(row: sqlx::postgres::PgRow) -> Result<DuplicateReviewRow, sqlx::Error> {
    Ok(DuplicateReviewRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        left_customer_id: row.try_get("left_customer_id")?,
        right_customer_id: row.try_get("right_customer_id")?,
        left_name: row.try_get("left_name")?,
        right_name: row.try_get("right_name")?,
        status: row.try_get("status")?,
        canonical_customer_id: row.try_get("canonical_customer_id")?,
        evidence: row.try_get("evidence")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        actor_eligible: row.try_get("actor_eligible")?,
    })
}

pub struct PgCustomerDuplicateReview<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCustomerDuplicateReview<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("CUSTOMER_DUPLICATE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("CUSTOMER_DUPLICATE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("customer-duplicate-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn readback_request(&mut self, plan: &RequestPlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT id,company_id,left_customer_id,right_customer_id,status,version,evidence,created_by,created_at,updated_at \
             FROM customer_duplicate_reviews WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(readback_invalid().into());
        };
        let id: String = row.try_get("id").map_err(Error::from)?;
        let company_id: String = row.try_get("company_id").map_err(Error::from)?;
        let left: String = row.try_get("left_customer_id").map_err(Error::from)?;
        let right: String = row.try_get("right_customer_id").map_err(Error::from)?;
        let status: String = row.try_get("status").map_err(Error::from)?;
        let version: i64 = row.try_get("version").map_err(Error::from)?;
        let evidence: String = row.try_get("evidence").map_err(Error::from)?;
        let created_by: String = row.try_get("created_by").map_err(Error::from)?;
        let created_at: i64 = row.try_get("created_at").map_err(Error::from)?;
        let updated_at: i64 = row.try_get("updated_at").map_err(Error::from)?;
        if id != plan.id
            || company_id != plan.company_id
            || left != plan.left_customer_id
            || right != plan.right_customer_id
            || status != "pending"
            || version != 1
            || evidence != plan.evidence
            || created_by != plan.created_by
            || created_at != plan.now
            || updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn insert_receipt(
        &mut self,
        actor: &crate::domain::access::AccessActor,
        key: &str,
        request_hash: &str,
        result: &AccessReceipt,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(crate::domain::duplicates::REQUEST_OPERATION)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(result.id.as_str())
        .bind(result.to_wire().to_string())
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(crate::domain::duplicates::idempotency_conflict().into());
        }
        Ok(())
    }

    async fn audit(
        &mut self,
        actor: &crate::domain::access::AccessActor,
        key: &str,
        record_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'customer_duplicate_review',$5,1,$4,$6,$7)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(crate::domain::duplicates::REQUEST_OPERATION)
        .bind(record_id)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl DuplicateReviewCommandRepository for PgCustomerDuplicateReview<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> crate::application::duplicates::DuplicateAccessFuture<'a, String, Self::Error> {
        // placeholder: define future type alias in application
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                .bind(value)
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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
                Ok::<_, Error>(ReceiptSnapshot {
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

    fn customers_active<'a>(
        &'a mut self,
        company_id: &'a str,
        left: &'a str,
        right: &'a str,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COUNT(*) FROM finance_parties WHERE company_id=$1 AND kind='customer' AND status='active' AND id IN ($2,$3)",
            )
            .bind(company_id)
            .bind(left)
            .bind(right)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn commit_request<'a>(
        &'a mut self,
        actor: &'a crate::domain::access::AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO customer_duplicate_reviews\
                 (id,company_id,left_customer_id,right_customer_id,evidence,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$7) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.left_customer_id)
            .bind(&plan.right_customer_id)
            .bind(&plan.evidence)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(crate::domain::duplicates::idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO customer_duplicate_events\
                 (id,review_id,version,action,actor_id,reason,canonical_customer_id,created_at) \
                 VALUES($1,$2,1,'request',$3,$4,NULL,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind(&plan.evidence)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(crate::domain::duplicates::idempotency_conflict().into());
            }
            let receipt = AccessReceipt::new(plan.id.clone(), "pending", 1);
            self.insert_receipt(actor, key, request_hash, &receipt, plan.now).await?;
            self.audit(actor, key, &plan.id, plan.now).await?;
            self.readback_request(plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl DuplicateReviewReadRepository for PgCustomerDuplicateReview<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a crate::domain::access::AccessActor,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = "SELECT r.id,r.company_id,r.left_customer_id,r.right_customer_id,r.status,r.canonical_customer_id,r.evidence,r.created_by,r.reviewed_by,r.version,r.created_at,r.updated_at, \
             l.name left_name, q.name right_name, \
             FALSE::boolean actor_eligible \
             FROM customer_duplicate_reviews r \
             JOIN finance_parties l ON l.id=r.left_customer_id AND l.company_id=r.company_id \
             JOIN finance_parties q ON q.id=r.right_customer_id AND q.company_id=r.company_id \
             WHERE r.company_id=$1 ORDER BY r.updated_at DESC,r.id LIMIT 200";
            let rows = sqlx::query(sql)
                .bind(actor.company_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter().map(map_row).collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    fn customers<'a>(
        &'a mut self,
        actor: &'a crate::domain::access::AccessActor,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewCustomer>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,name,code FROM finance_parties WHERE company_id=$1 AND kind='customer' AND status='active' ORDER BY name,id LIMIT 500",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(DuplicateReviewCustomer {
                        id: row.try_get("id")?,
                        name: row.try_get("name")?,
                        code: row.try_get("code")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> crate::application::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT version,action,actor_id,reason,canonical_customer_id,created_at \
                 FROM customer_duplicate_events WHERE review_id=$1 ORDER BY version LIMIT 500",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(DuplicateReviewEventRow {
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        canonical_customer_id: row.try_get("canonical_customer_id")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
