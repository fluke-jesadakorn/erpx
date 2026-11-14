//! PostgreSQL adapter for the native Customer Authority duplicate-review
//! request, decision/reopen/merge commands and bounded workspace/detail read.
//!
//! SQL is Customer Authority-owned (`customer_duplicate_reviews`,
//! `customer_duplicate_events`, `customer_duplicate_merge_reconciliations`,
//! command receipts/audit/outbox and staff notifications) plus the existing
//! Finance party merge targets that the retained TypeScript merge already
//! repoints in the same transaction. The caller's serializable Holding
//! transaction is required and no commit is performed here.
use crate::application::duplicates::{
    DuplicateAccessFuture, DuplicateReviewCommandRepository, DuplicateReviewReadRepository,
};
use crate::domain::access::{AccessActor, ReceiptSnapshot};
use crate::domain::duplicates::{
    ChangePlan, DuplicateReceipt, DuplicateReviewCustomer, DuplicateReviewEventRow,
    DuplicateReviewRow, MergeBlockers, MergePartyFacts, MergePlan, RequestPlan,
    idempotency_conflict, readback_invalid, review_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const ELIGIBLE: &str = "EXISTS(SELECT 1 FROM assignments m \
JOIN hr_people p ON p.company_id=r.company_id AND p.user_id=m.user_id AND p.status='active' AND p.merged_into_id IS NULL \
JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' \
AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
WHERE m.company_id=r.company_id AND m.user_id=$2 AND m.scope_kind='holding' \
AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_principal_active(m.user_id,m.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_role_allows(m.role,m.company_id,'commerce.approve'))";

fn row_sql(lock: bool) -> String {
    format!(
        "SELECT r.id,r.company_id,r.left_customer_id,r.right_customer_id,r.status,r.canonical_customer_id,\
         r.evidence,r.created_by,r.reviewed_by,r.version,r.created_at,r.updated_at,\
         l.name left_name,l.code left_code,l.tax_registration left_tax_registration,l.status left_status,\
         l.merged_into_id left_merged_into_id,\
         q.name right_name,q.code right_code,q.tax_registration right_tax_registration,q.status right_status,\
         q.merged_into_id right_merged_into_id,{ELIGIBLE} actor_eligible \
         FROM customer_duplicate_reviews r \
         JOIN finance_parties l ON l.id=r.left_customer_id AND l.company_id=r.company_id \
         JOIN finance_parties q ON q.id=r.right_customer_id AND q.company_id=r.company_id \
         WHERE r.company_id=$1{}",
        if lock { " AND r.id=$3 FOR UPDATE OF r" } else { "" }
    )
}

fn map_row(row: sqlx::postgres::PgRow) -> Result<DuplicateReviewRow, sqlx::Error> {
    Ok(DuplicateReviewRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        left_customer_id: row.try_get("left_customer_id")?,
        right_customer_id: row.try_get("right_customer_id")?,
        left_name: row.try_get("left_name")?,
        right_name: row.try_get("right_name")?,
        left_code: row.try_get("left_code")?,
        right_code: row.try_get("right_code")?,
        left_tax_registration: row.try_get("left_tax_registration")?,
        right_tax_registration: row.try_get("right_tax_registration")?,
        left_status: row.try_get("left_status")?,
        right_status: row.try_get("right_status")?,
        left_merged_into_id: row.try_get("left_merged_into_id")?,
        right_merged_into_id: row.try_get("right_merged_into_id")?,
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

    async fn readback_change(&mut self, plan: &ChangePlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT status,canonical_customer_id,reviewed_by,version,updated_at \
             FROM customer_duplicate_reviews WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(readback_invalid().into());
        };
        let status: String = row.try_get("status").map_err(Error::from)?;
        let canonical: Option<String> =
            row.try_get("canonical_customer_id").map_err(Error::from)?;
        let reviewed_by: Option<String> = row.try_get("reviewed_by").map_err(Error::from)?;
        let version: i64 = row.try_get("version").map_err(Error::from)?;
        let updated_at: i64 = row.try_get("updated_at").map_err(Error::from)?;
        if status != plan.status
            || canonical != plan.canonical_customer_id
            || reviewed_by != plan.reviewed_by
            || version != plan.version
            || updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_merge(&mut self, plan: &MergePlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT status,canonical_customer_id,reviewed_by,version,updated_at \
             FROM customer_duplicate_reviews WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(readback_invalid().into());
        };
        let status: String = row.try_get("status").map_err(Error::from)?;
        let canonical: Option<String> =
            row.try_get("canonical_customer_id").map_err(Error::from)?;
        let reviewed_by: Option<String> = row.try_get("reviewed_by").map_err(Error::from)?;
        let version: i64 = row.try_get("version").map_err(Error::from)?;
        let updated_at: i64 = row.try_get("updated_at").map_err(Error::from)?;
        if status != "merged"
            || canonical.as_deref() != Some(plan.canonical_customer_id.as_str())
            || reviewed_by.as_deref() != Some(plan.reviewed_by.as_str())
            || version != plan.version
            || updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn insert_receipt(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        request_hash: &str,
        result: &DuplicateReceipt,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
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
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    async fn audit(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'customer_duplicate_review',$5,$6,$4,$7,$8)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn outbox(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        plan_id: &str,
        status: &str,
        version: i64,
        canonical: Option<&str>,
        merged: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": plan_id,
            "state": status,
            "companyId": actor.company_id.as_str(),
            "version": version,
            "canonicalCustomerId": canonical,
            "mergedCustomerId": merged,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'customer_duplicate_review',$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(operation)
        .bind(plan_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn notify_request(&mut self, key: &str, id: &str) -> Result<(), Error> {
        sqlx::query(AssertSqlSafe(
            "INSERT INTO staff_notifications\
             (id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
             SELECT DISTINCT $1||a.user_id,r.company_id,a.user_id,'customer_duplicate',r.id,r.version,\
             'Review possible duplicate customer records',r.updated_at \
             FROM customer_duplicate_reviews r \
             JOIN assignments a ON a.company_id=r.company_id AND a.user_id<>r.created_by \
             AND a.scope_kind='holding' \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(a.role,a.company_id,'commerce.approve') \
             WHERE r.id=$2 ON CONFLICT DO NOTHING",
        ))
        .bind(format!("customer-duplicate-notice-{key}-"))
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn notify_change(
        &mut self,
        key: &str,
        id: &str,
        message: &'static str,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO staff_notifications\
             (id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
             SELECT $1,company_id,created_by,'customer_duplicate',id,version,$2,updated_at \
             FROM customer_duplicate_reviews WHERE id=$3 ON CONFLICT DO NOTHING",
        )
        .bind(format!("customer-duplicate-notice-{key}"))
        .bind(message)
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl DuplicateReviewCommandRepository for PgCustomerDuplicateReview<'_> {
    type Error = Error;

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> DuplicateAccessFuture<'a, String, Self::Error> {
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
    ) -> DuplicateAccessFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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
    ) -> DuplicateAccessFuture<'a, i64, Self::Error> {
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

    fn customer_active<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_id: &'a str,
    ) -> DuplicateAccessFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT COUNT(*) FROM finance_parties WHERE id=$1 AND company_id=$2 AND kind='customer' AND status='active'",
            )
            .bind(customer_id)
            .bind(company_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn review_for_update<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> DuplicateAccessFuture<'a, Option<DuplicateReviewRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(AssertSqlSafe(row_sql(true)))
                .bind(actor.company_id.as_str())
                .bind(actor.user_id.as_str())
                .bind(id)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(map_row).transpose().map_err(Into::into)
        })
    }

    fn reviewer_eligible<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> DuplicateAccessFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = "SELECT EXISTS(SELECT 1 FROM customer_duplicate_reviews r \
                 JOIN assignments m ON m.company_id=r.company_id \
                 JOIN hr_people p ON p.company_id=r.company_id AND p.user_id=m.user_id \
                 AND p.status='active' AND p.merged_into_id IS NULL \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 AND e.status='active' \
                 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
                 WHERE r.id=$1 AND r.company_id=$2 AND m.user_id=$3 AND m.scope_kind='holding' \
                 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND iam_principal_active(m.user_id,m.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND iam_role_allows(m.role,m.company_id,'commerce.approve'))";
            Ok(sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(id)
                .bind(actor.company_id.as_str())
                .bind(actor.user_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn merge_blockers<'a>(
        &'a mut self,
        merged_customer_id: &'a str,
    ) -> DuplicateAccessFuture<'a, MergeBlockers, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT \
                 EXISTS(SELECT 1 FROM commerce_customer_notifications WHERE party_id=$1) notifications,\
                 EXISTS(SELECT 1 FROM customer_access_bindings WHERE customer_id=$1) grants,\
                 EXISTS(SELECT 1 FROM field_service_agreements WHERE customer_id=$1) agreements,\
                 EXISTS(SELECT 1 FROM field_service_billing_releases WHERE customer_id=$1) billing,\
                 EXISTS(SELECT 1 FROM commerce_orders o WHERE o.party_id=$1 AND (\
                   EXISTS(SELECT 1 FROM crm_opportunities v WHERE v.quote_id=o.id OR v.sales_id=o.id)\
                   OR EXISTS(SELECT 1 FROM customer_access_bindings b WHERE b.order_id=o.id)\
                   OR EXISTS(SELECT 1 FROM customer_contact_notes n WHERE n.order_id=o.id)\
                   OR EXISTS(SELECT 1 FROM support_tickets t WHERE t.order_id=o.id))) order_evidence",
            )
            .bind(merged_customer_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(MergeBlockers {
                notifications: row.try_get("notifications")?,
                grants: row.try_get("grants")?,
                agreements: row.try_get("agreements")?,
                billing: row.try_get("billing")?,
                order_evidence: row.try_get("order_evidence")?,
            })
        })
    }

    fn merge_parties<'a>(
        &'a mut self,
        company_id: &'a str,
        canonical_customer_id: &'a str,
        merged_customer_id: &'a str,
    ) -> DuplicateAccessFuture<'a, Option<MergePartyFacts>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT l.status canonical_status,l.merged_into_id canonical_merged_into_id,\
                 l.currency canonical_currency,r.status merged_status,\
                 r.merged_into_id merged_merged_into_id,r.currency merged_currency \
                 FROM finance_parties l JOIN finance_parties r ON r.id=$2 \
                 WHERE l.id=$1 AND l.company_id=$3 AND l.kind='customer' \
                 AND r.company_id=l.company_id AND r.kind='customer'",
            )
            .bind(canonical_customer_id)
            .bind(merged_customer_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MergePartyFacts {
                    canonical_status: row.try_get("canonical_status")?,
                    merged_status: row.try_get("merged_status")?,
                    canonical_merged_into_id: row.try_get("canonical_merged_into_id")?,
                    merged_merged_into_id: row.try_get("merged_merged_into_id")?,
                    canonical_currency: row.try_get("canonical_currency")?,
                    merged_currency: row.try_get("merged_currency")?,
                })
            })
            .transpose()
        })
    }

    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> DuplicateAccessFuture<'a, Value, Self::Error> {
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
                return Err(idempotency_conflict().into());
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
                return Err(idempotency_conflict().into());
            }
            let receipt = DuplicateReceipt::new(plan.id.clone(), "pending", 1);
            self.insert_receipt(
                actor,
                key,
                crate::domain::duplicates::REQUEST_OPERATION,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                crate::domain::duplicates::REQUEST_OPERATION,
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                crate::domain::duplicates::REQUEST_OPERATION,
                &plan.id,
                "pending",
                1,
                None,
                None,
                plan.now,
            )
            .await?;
            self.notify_request(key, &plan.id).await?;
            self.readback_request(plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_change<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> DuplicateAccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE customer_duplicate_reviews SET status=$5,canonical_customer_id=$6,\
                 reviewed_by=$7,version=version+1,updated_at=$8 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.canonical_customer_id)
            .bind(&plan.reviewed_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(review_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO customer_duplicate_events\
                 (id,review_id,version,action,actor_id,reason,canonical_customer_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(plan.action.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(&plan.canonical_customer_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = DuplicateReceipt::new(plan.id.clone(), &plan.status, plan.version);
            self.insert_receipt(
                actor,
                key,
                &plan.action.operation(),
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                &plan.action.operation(),
                &plan.id,
                plan.version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                &plan.action.operation(),
                &plan.id,
                &plan.status,
                plan.version,
                plan.canonical_customer_id.as_deref(),
                None,
                plan.now,
            )
            .await?;
            self.notify_change(key, &plan.id, "Customer duplicate review updated")
                .await?;
            self.readback_change(plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_merge<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a MergePlan,
        request_hash: &'a str,
    ) -> DuplicateAccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE customer_duplicate_reviews SET status='merged',reviewed_by=$5,\
                 version=version+1,updated_at=$6 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='duplicate' \
                 AND canonical_customer_id=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.canonical_customer_id)
            .bind(&plan.reviewed_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(review_changed().into());
            }
            sqlx::query(
                "INSERT INTO customer_duplicate_merge_reconciliations(\
                 review_id,company_id,canonical_customer_id,merged_customer_id,currency,\
                 invoice_count,invoice_total_minor,invoice_outstanding_minor,\
                 payment_count,payment_total_minor,payment_outstanding_minor,\
                 refund_count,refund_total_minor,refund_outstanding_minor,created_by,created_at) \
                 SELECT $1,$2,$3,$4,p.currency,\
                 (SELECT COUNT(*) FROM finance_invoices WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(total_minor),0) FROM finance_invoices WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(outstanding_minor),0) FROM finance_invoices WHERE party_id IN ($3,$4)),\
                 (SELECT COUNT(*) FROM finance_payments WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(amount_minor),0) FROM finance_payments WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(outstanding_minor),0) FROM finance_payments WHERE party_id IN ($3,$4)),\
                 (SELECT COUNT(*) FROM finance_refund_obligations WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(amount_minor),0) FROM finance_refund_obligations WHERE party_id IN ($3,$4)),\
                 (SELECT COALESCE(SUM(outstanding_minor),0) FROM finance_refund_obligations WHERE party_id IN ($3,$4)),\
                 $5,$6 FROM finance_parties p WHERE p.id=$3",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.canonical_customer_id)
            .bind(&plan.merged_customer_id)
            .bind(actor.user_id.as_str())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            for sql in [
                "UPDATE commerce_orders SET party_id=$1,version=version+1,updated_at=$3 WHERE party_id=$2",
                "UPDATE finance_invoices SET party_id=$1 WHERE party_id=$2",
                "UPDATE finance_payments SET party_id=$1 WHERE party_id=$2",
                "UPDATE finance_revenue_contracts SET customer_id=$1 WHERE customer_id=$2",
                "UPDATE finance_refund_obligations SET party_id=$1 WHERE party_id=$2",
                "UPDATE ecommerce_carts SET party_id=$1 WHERE party_id=$2",
                "UPDATE crm_opportunities SET customer_id=$1 WHERE customer_id=$2",
                "UPDATE finance_expenses SET payee_party_id=$1 WHERE payee_party_id=$2",
                "UPDATE pos_sales SET party_id=$1 WHERE party_id=$2",
            ] {
                let mut query = sqlx::query(sql)
                    .bind(&plan.canonical_customer_id)
                    .bind(&plan.merged_customer_id);
                if sql.starts_with("UPDATE commerce_orders") {
                    query = query.bind(plan.now);
                }
                query.execute(&mut *self.connection).await?;
            }
            let blocked = sqlx::query(
                "UPDATE finance_parties SET status='blocked',merged_into_id=$1,merged_at=$2,\
                 merged_by=$3,version=version+1,updated_at=$2 WHERE id=$4 AND status='active'",
            )
            .bind(&plan.canonical_customer_id)
            .bind(plan.now)
            .bind(actor.user_id.as_str())
            .bind(&plan.merged_customer_id)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if blocked != 1 {
                return Err(review_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO customer_duplicate_events\
                 (id,review_id,version,action,actor_id,reason,canonical_customer_id,created_at) \
                 VALUES($1,$2,$3,'merge',$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(&plan.canonical_customer_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = DuplicateReceipt::merged(
                plan.id.clone(),
                plan.version,
                plan.canonical_customer_id.clone(),
                plan.merged_customer_id.clone(),
            );
            let operation = crate::domain::duplicates::DuplicateAction::Merge.operation();
            self.insert_receipt(actor, key, &operation, request_hash, &receipt, plan.now)
                .await?;
            self.audit(actor, key, &operation, &plan.id, plan.version, plan.now)
                .await?;
            self.outbox(
                actor,
                key,
                &operation,
                &plan.id,
                "merged",
                plan.version,
                Some(&plan.canonical_customer_id),
                Some(&plan.merged_customer_id),
                plan.now,
            )
            .await?;
            self.notify_change(
                key,
                &plan.id,
                "Customer records merged; review the canonical customer history",
            )
            .await?;
            self.readback_merge(plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl DuplicateReviewReadRepository for PgCustomerDuplicateReview<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!("{} ORDER BY r.updated_at DESC,r.id LIMIT 200", row_sql(false));
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(actor.user_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter()
                .map(map_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn customers<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewCustomer>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,name,code,tax_registration,version FROM finance_parties \
                 WHERE company_id=$1 AND kind='customer' AND status='active' \
                 AND merged_into_id IS NULL ORDER BY name,id LIMIT 500",
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
                        tax_registration: row.try_get("tax_registration")?,
                        version: row.try_get("version")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn history<'a>(
        &'a mut self,
        id: &'a str,
    ) -> DuplicateAccessFuture<'a, Vec<DuplicateReviewEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,version,action,actor_id,reason,canonical_customer_id,created_at \
                 FROM customer_duplicate_events WHERE review_id=$1 ORDER BY version LIMIT 500",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(DuplicateReviewEventRow {
                        id: row.try_get("id")?,
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
