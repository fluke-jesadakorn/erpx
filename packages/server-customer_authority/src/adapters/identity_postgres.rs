//! PostgreSQL adapter for native Customer Authority customer identity changes.
//!
//! SQL is Customer Authority-owned (`customer_identity_changes`,
//! `customer_identity_change_events`, command receipts/audit/outbox and staff
//! notifications) plus the Finance customer master that the reviewed snapshot
//! applies to in the same transaction. The caller's serializable Holding
//! transaction is required and no commit is performed here.
use crate::application::identity::{IdentityCommandRepository, IdentityFuture, IdentityReadRepository};
use crate::domain::access::{AccessActor, AccessReceipt, ReceiptSnapshot};
use crate::domain::identity::{
    ChangePlan, CustomerAddress, CustomerIdentity, CustomerSnapshot, IdentityChangeRow,
    IdentityCustomer, IdentityEventRow, NotificationConsent, RequestPlan,
    customer_changed, idempotency_conflict, readback_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const ELIGIBLE: &str = "EXISTS(SELECT 1 FROM assignments m \
JOIN hr_people p ON p.company_id=c.company_id AND p.user_id=m.user_id AND p.status='active' AND p.merged_into_id IS NULL \
JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' \
AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
WHERE m.company_id=c.company_id AND m.user_id=$2 AND m.scope_kind='holding' \
AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_principal_active(m.user_id,m.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_role_allows(m.role,m.company_id,'commerce.approve'))";

fn row_sql(lock: bool) -> String {
    format!(
        "SELECT c.id,c.company_id,c.customer_id,c.expected_customer_version,c.previous_json,\
         c.proposed_json,c.status,c.evidence,c.created_by,c.reviewed_by,c.version,c.created_at,\
         c.updated_at,p.name customer_name,p.code customer_code,p.version current_customer_version,\
         {ELIGIBLE} actor_eligible \
         FROM customer_identity_changes c \
         JOIN finance_parties p ON p.id=c.customer_id AND p.company_id=c.company_id \
         WHERE c.company_id=$1{}",
        if lock { " AND c.id=$3 FOR UPDATE OF c" } else { "" }
    )
}

fn map_row(row: sqlx::postgres::PgRow) -> Result<IdentityChangeRow, sqlx::Error> {
    Ok(IdentityChangeRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        customer_id: row.try_get("customer_id")?,
        customer_name: row.try_get("customer_name")?,
        customer_code: row.try_get("customer_code")?,
        current_customer_version: row.try_get("current_customer_version")?,
        expected_customer_version: row.try_get("expected_customer_version")?,
        previous: row.try_get("previous_json")?,
        proposed: row.try_get("proposed_json")?,
        status: row.try_get("status")?,
        evidence: row.try_get("evidence")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        actor_eligible: row.try_get("actor_eligible")?,
    })
}

fn address_from(value: Value) -> Option<CustomerAddress> {
    if value.as_object().is_some_and(|object| !object.is_empty()) {
        serde_json::from_value(value).ok()
    } else {
        None
    }
}

pub struct PgCustomerIdentity<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCustomerIdentity<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("CUSTOMER_IDENTITY_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("CUSTOMER_IDENTITY_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("customer-identity-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn readback_request(&mut self, plan: &RequestPlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT id,company_id,customer_id,expected_customer_version,previous_json,proposed_json,\
             status,evidence,created_by,version,created_at,updated_at \
             FROM customer_identity_changes WHERE id=$1 AND company_id=$2",
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
        let customer_id: String = row.try_get("customer_id").map_err(Error::from)?;
        let expected: i64 = row
            .try_get("expected_customer_version")
            .map_err(Error::from)?;
        let previous: Value = row.try_get("previous_json").map_err(Error::from)?;
        let proposed: Value = row.try_get("proposed_json").map_err(Error::from)?;
        let status: String = row.try_get("status").map_err(Error::from)?;
        let evidence: String = row.try_get("evidence").map_err(Error::from)?;
        let created_by: String = row.try_get("created_by").map_err(Error::from)?;
        let version: i64 = row.try_get("version").map_err(Error::from)?;
        let created_at: i64 = row.try_get("created_at").map_err(Error::from)?;
        let updated_at: i64 = row.try_get("updated_at").map_err(Error::from)?;
        if id != plan.id
            || company_id != plan.company_id
            || customer_id != plan.customer_id
            || expected != plan.expected_customer_version
            || previous != plan.previous.to_json()
            || proposed != plan.proposed.to_json()
            || status != "pending"
            || evidence != plan.evidence
            || created_by != plan.created_by
            || version != 1
            || created_at != plan.now
            || updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_change(&mut self, plan: &ChangePlan) -> Result<(), Error> {
        let row = sqlx::query(
            "SELECT status,expected_customer_version,proposed_json,reviewed_by,version,updated_at \
             FROM customer_identity_changes WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.id)
        .bind(&plan.company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Err(readback_invalid().into());
        };
        let status: String = row.try_get("status").map_err(Error::from)?;
        let expected: i64 = row
            .try_get("expected_customer_version")
            .map_err(Error::from)?;
        let proposed: Value = row.try_get("proposed_json").map_err(Error::from)?;
        let reviewed_by: Option<String> = row.try_get("reviewed_by").map_err(Error::from)?;
        let version: i64 = row.try_get("version").map_err(Error::from)?;
        let updated_at: i64 = row.try_get("updated_at").map_err(Error::from)?;
        if status != plan.status
            || expected != plan.expected_customer_version
            || proposed != plan.proposed.to_json()
            || reviewed_by != plan.reviewed_by
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
        result: &AccessReceipt,
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
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'customer_identity_change',$5,$6,$4,$7,$8)",
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
        customer_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = serde_json::json!({
            "authority": authority,
            "id": plan_id,
            "state": status,
            "customerId": customer_id,
            "companyId": actor.company_id.as_str(),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'customer_identity_change',$3,$4,$5,$6)",
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
             SELECT DISTINCT $1||a.user_id,c.company_id,a.user_id,'customer_identity',c.id,c.version,\
             'Review a proposed customer identity change',c.updated_at \
             FROM customer_identity_changes c \
             JOIN assignments a ON a.company_id=c.company_id AND a.user_id<>c.created_by \
             AND a.scope_kind='holding' \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(a.role,a.company_id,'commerce.approve') \
             WHERE c.id=$2 ON CONFLICT DO NOTHING",
        ))
        .bind(format!("customer-identity-notice-{key}-"))
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn notify_change(&mut self, key: &str, id: &str) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO staff_notifications\
             (id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
             SELECT $1,company_id,created_by,'customer_identity',id,version,\
             'Customer identity review updated',updated_at \
             FROM customer_identity_changes WHERE id=$2 ON CONFLICT DO NOTHING",
        )
        .bind(format!("customer-identity-notice-{key}"))
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl IdentityCommandRepository for PgCustomerIdentity<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> IdentityFuture<'a, String, Self::Error> {
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
    ) -> IdentityFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn source_customer<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_id: &'a str,
    ) -> IdentityFuture<'a, Option<CustomerSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT name,tax_registration,phone,address_json,notification_email,\
                 notification_consent,version,code FROM finance_parties \
                 WHERE id=$1 AND company_id=$2 AND kind='customer' AND status='active'",
            )
            .bind(customer_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                let address: Value = row.try_get("address_json")?;
                let consent: String = row.try_get("notification_consent")?;
                Ok::<_, Error>(CustomerSnapshot {
                    identity: CustomerIdentity {
                        name: row.try_get("name")?,
                        tax_registration: row.try_get("tax_registration")?,
                        phone: row.try_get("phone")?,
                        address: address_from(address),
                        notification_email: row.try_get("notification_email")?,
                        notification_consent: NotificationConsent::from_database(Some(consent)),
                    },
                    version: row.try_get("version")?,
                    code: row.try_get("code")?,
                })
            })
            .transpose()
        })
    }

    fn customer_version<'a>(
        &'a mut self,
        company_id: &'a str,
        customer_id: &'a str,
    ) -> IdentityFuture<'a, Option<i64>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT version FROM finance_parties WHERE id=$1 AND company_id=$2 \
                 AND kind='customer' AND status='active'",
            )
            .bind(customer_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn change_for_update<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> IdentityFuture<'a, Option<IdentityChangeRow>, Self::Error> {
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
    ) -> IdentityFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = "SELECT EXISTS(SELECT 1 FROM customer_identity_changes c \
                 JOIN assignments m ON m.company_id=c.company_id \
                 JOIN hr_people p ON p.company_id=c.company_id AND p.user_id=m.user_id \
                 AND p.status='active' AND p.merged_into_id IS NULL \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 AND e.status='active' \
                 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
                 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) \
                 WHERE c.id=$1 AND c.company_id=$2 AND m.user_id=$3 AND m.scope_kind='holding' \
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

    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a RequestPlan,
        request_hash: &'a str,
    ) -> IdentityFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let previous = plan.previous.to_json().to_string();
            let proposed = plan.proposed.to_json().to_string();
            let changed = sqlx::query(
                "INSERT INTO customer_identity_changes\
                 (id,company_id,customer_id,expected_customer_version,previous_json,proposed_json,\
                 evidence,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5::jsonb,$6::jsonb,$7,$8,$9,$9) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.customer_id)
            .bind(plan.expected_customer_version)
            .bind(&previous)
            .bind(&proposed)
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
                "INSERT INTO customer_identity_change_events\
                 (id,change_id,version,action,actor_id,reason,customer_version,previous_json,\
                 proposed_json,created_at) \
                 VALUES($1,$2,1,'request',$3,$4,$5,$6::jsonb,$7::jsonb,$8) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind(&plan.evidence)
            .bind(plan.expected_customer_version)
            .bind(&previous)
            .bind(&proposed)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = AccessReceipt::new(plan.id.clone(), "pending", 1);
            self.insert_receipt(
                actor,
                key,
                crate::domain::identity::REQUEST_OPERATION,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                crate::domain::identity::REQUEST_OPERATION,
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                crate::domain::identity::REQUEST_OPERATION,
                &plan.id,
                "pending",
                1,
                &plan.customer_id,
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
    ) -> IdentityFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            if plan.action == crate::domain::identity::IdentityChangeAction::Apply {
                let proposed = &plan.proposed;
                let address = proposed.address.as_ref().map_or_else(
                    || "{}".to_owned(),
                    |address| serde_json::to_string(address).unwrap_or_else(|_| "{}".to_owned()),
                );
                let changed = sqlx::query(
                    "UPDATE finance_parties SET name=$1,tax_registration=$2,phone=$3,\
                     address_json=$4::jsonb,notification_email=$5,notification_consent=$6,\
                     version=version+1,updated_at=$7 \
                     WHERE id=$8 AND company_id=$9 AND kind='customer' AND status='active' \
                     AND version=$10",
                )
                .bind(&proposed.name)
                .bind(&proposed.tax_registration)
                .bind(&proposed.phone)
                .bind(address)
                .bind(&proposed.notification_email)
                .bind(proposed.notification_consent.as_str())
                .bind(plan.now)
                .bind(&plan.customer_id)
                .bind(&plan.company_id)
                .bind(plan.expected_customer_version)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(customer_changed().into());
                }
            }
            let proposed = plan.proposed.to_json().to_string();
            let changed = sqlx::query(
                "UPDATE customer_identity_changes SET status=$5,expected_customer_version=$6,\
                 proposed_json=$7::jsonb,reviewed_by=$8,version=version+1,updated_at=$9 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(plan.expected_customer_version)
            .bind(&proposed)
            .bind(&plan.reviewed_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(crate::domain::identity::change_changed().into());
            }
            let previous = plan.previous.to_string();
            let event = sqlx::query(
                "INSERT INTO customer_identity_change_events\
                 (id,change_id,version,action,actor_id,reason,customer_version,previous_json,\
                 proposed_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb,$9::jsonb,$10) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(plan.action.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(plan.expected_customer_version)
            .bind(previous)
            .bind(proposed)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = AccessReceipt::new(plan.id.clone(), &plan.status, plan.version);
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
                &plan.customer_id,
                plan.now,
            )
            .await?;
            self.notify_change(key, &plan.id).await?;
            self.readback_change(plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl IdentityReadRepository for PgCustomerIdentity<'_> {
    type Error = Error;

    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> IdentityFuture<'a, Vec<IdentityChangeRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!("{} ORDER BY c.updated_at DESC,c.id LIMIT 200", row_sql(false));
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
    ) -> IdentityFuture<'a, Vec<IdentityCustomer>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,name,code,tax_registration,phone,address_json,version \
                 FROM finance_parties WHERE company_id=$1 AND kind='customer' \
                 AND status='active' ORDER BY name,id LIMIT 500",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(IdentityCustomer {
                        id: row.try_get("id")?,
                        name: row.try_get("name")?,
                        code: row.try_get("code")?,
                        tax_registration: row.try_get("tax_registration")?,
                        phone: row.try_get("phone")?,
                        address: row.try_get("address_json")?,
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
    ) -> IdentityFuture<'a, Vec<IdentityEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,change_id,version,action,actor_id,reason,customer_version,\
                 previous_json,proposed_json,created_at \
                 FROM customer_identity_change_events WHERE change_id=$1 ORDER BY version LIMIT 500",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(IdentityEventRow {
                        id: row.try_get("id")?,
                        change_id: row.try_get("change_id")?,
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        customer_version: row.try_get("customer_version")?,
                        previous: row.try_get("previous_json")?,
                        proposed: row.try_get("proposed_json")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
