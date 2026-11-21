//! PostgreSQL adapter for the native Customer Authority customer-access grant.
//!
//! SQL is Customer Authority-owned (`customer_access_bindings`,
//! `customer_access_events`, `customer_access_reads`,
//! `customer_access_identities`, `customer_portal_invitations` and
//! `customer_portal_invitation_events`) plus read-only projections of the
//! existing Commerce order header, Finance party name and IAM/HR reviewer
//! eligibility already used by the retained reader. No Commerce, Finance, IAM,
//! Support or portal row is mutated. The caller's serializable Holding
//! transaction is required and no commit is performed here.
use crate::application::access::{
    AccessFuture, CustomerAccessCommandRepository, CustomerAccessReadRepository,
};
use crate::domain::access::{
    AccessActor, AccessBindingRow, AccessEventRow, AccessOrder, AccessReadRow, AccessReceipt,
    AccessSourceRow, ChangePlan, ReceiptSnapshot, RequestPlan, binding_changed,
    idempotency_conflict, readback_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const REVIEWER: &str = "m.company_id=b.company_id AND m.user_id<>b.created_by \
AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_role_allows(m.role,m.company_id,'commerce.approve') \
AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=o.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=o.department_id)) \
AND EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
WHERE p.company_id=b.company_id AND p.user_id=m.user_id AND p.status='active' AND p.merged_into_id IS NULL \
AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

const ACTOR_REVIEWER: &str = "m.company_id=b.company_id AND m.user_id=$3 \
AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_role_allows(m.role,m.company_id,'commerce.approve') \
AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=o.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=o.department_id)) \
AND EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
WHERE p.company_id=b.company_id AND p.user_id=m.user_id AND p.status='active' AND p.merged_into_id IS NULL \
AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

fn selected(lock: bool) -> String {
    format!(
        "SELECT b.id,b.company_id,b.order_id,b.customer_id,b.email,b.expires_at,b.status,b.reason,\
         b.created_by,b.approved_by,b.reviewed_at,b.version,b.created_at,b.updated_at,\
         o.number order_number,o.state order_state,o.kind order_kind,o.branch_id,o.department_id,\
         EXISTS(SELECT 1 FROM assignments m WHERE {REVIEWER} AND m.user_id=$1) actor_eligible,\
         (SELECT count(DISTINCT m.user_id) FROM assignments m WHERE {REVIEWER}) reviewers,\
         EXISTS(SELECT 1 FROM customer_access_identities i WHERE i.binding_id=b.id) identity_exists \
         FROM customer_access_bindings b \
         JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
         WHERE b.id=$2 AND b.company_id=$3{}",
        if lock { " FOR UPDATE OF b" } else { "" }
    )
}

fn map_binding(row: sqlx::postgres::PgRow) -> Result<AccessBindingRow, sqlx::Error> {
    Ok(AccessBindingRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        order_id: row.try_get("order_id")?,
        customer_id: row.try_get("customer_id")?,
        email: row.try_get("email")?,
        expires_at: row.try_get("expires_at")?,
        status: row.try_get("status")?,
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        reviewed_at: row.try_get("reviewed_at")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        order_number: row.try_get("order_number")?,
        order_state: row.try_get("order_state")?,
        order_kind: row.try_get("order_kind")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        actor_eligible: row.try_get("actor_eligible")?,
        reviewers: row.try_get("reviewers")?,
        identity_exists: row.try_get("identity_exists")?,
    })
}

fn validate_items(rows: &[AccessBindingRow]) -> Result<(), Error> {
    for row in rows {
        row.validate_source()?;
    }
    Ok(())
}

pub struct PgCustomerAccess<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgCustomerAccess<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("CUSTOMER_ACCESS_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("CUSTOMER_ACCESS_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("customer-access-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn row_binding(
        &mut self,
        actor: &AccessActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<AccessBindingRow>, Error> {
        self.ready().await?;
        let sql = selected(lock);
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(actor.user_id.as_str())
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_binding).transpose().map_err(Into::into)
    }

    async fn readback_request(
        &mut self,
        actor: &AccessActor,
        plan: &RequestPlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_binding(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.id != plan.id
            || row.company_id != actor.company_id.as_str()
            || row.order_id != plan.order_id
            || row.customer_id != plan.customer_id
            || row.email != plan.email
            || row.expires_at != plan.expires_at
            || row.status != "pending"
            || row.reason != plan.reason
            || row.created_by != plan.created_by
            || row.approved_by.is_some()
            || row.reviewed_at.is_some()
            || row.version != 1
            || row.created_at != plan.now
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_change(
        &mut self,
        actor: &AccessActor,
        plan: &ChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_binding(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.reason != plan.reason
            || row.approved_by != plan.approved_by
            || row.reviewed_at != plan.reviewed_at
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &AccessActor,
        key: &str,
        operation: &str,
        record_id: &str,
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
        .bind(record_id)
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
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'customer_access_binding',$5,$6,$4,$7,$8)",
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
        order_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": plan_id,
            "state": status,
            "orderId": order_id,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'customer_access_binding',$3,$4,$5,$6)",
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
}

impl CustomerAccessCommandRepository for PgCustomerAccess<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> AccessFuture<'a, String, Self::Error> {
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
    ) -> AccessFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn order<'a>(
        &'a mut self,
        company_id: &'a str,
        order_id: &'a str,
    ) -> AccessFuture<'a, Option<AccessOrder>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,party_id customer_id,number,kind,state,version \
                 FROM commerce_orders WHERE id=$1 AND company_id=$2 FOR SHARE",
            )
            .bind(order_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(AccessOrder {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    customer_id: row.try_get("customer_id")?,
                    number: row.try_get("number")?,
                    kind: row.try_get("kind")?,
                    state: row.try_get("state")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn binding_for_update<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error> {
        Box::pin(async move { self.row_binding(actor, id, true).await })
    }

    fn reviewer_eligible<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        binding_id: &'a str,
    ) -> AccessFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT EXISTS(SELECT 1 FROM customer_access_bindings b \
                 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
                 JOIN assignments m ON m.company_id=b.company_id                  WHERE b.id=$1 AND b.company_id=$2 AND {ACTOR_REVIEWER})"
            );
            Ok(sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(binding_id)
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
    ) -> AccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO customer_access_bindings\
                 (id,company_id,order_id,customer_id,email,expires_at,reason,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .bind(&plan.order_id)
            .bind(&plan.customer_id)
            .bind(&plan.email)
            .bind(plan.expires_at)
            .bind(&plan.reason)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) \
                 VALUES($1,$2,1,'request',$3,$4,$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
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
                crate::domain::access::REQUEST_OPERATION,
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                crate::domain::access::REQUEST_OPERATION,
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                crate::domain::access::REQUEST_OPERATION,
                &plan.id,
                "pending",
                1,
                &plan.order_id,
                plan.now,
            )
            .await?;
            self.readback_request(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_change<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> AccessFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = if plan.action.is_review() {
                sqlx::query(
                    "UPDATE customer_access_bindings SET status=$5,approved_by=$6,reviewed_at=$7,\
                     reason=$8,version=version+1,updated_at=$9 \
                     WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
                )
                .bind(&plan.id)
                .bind(actor.company_id.as_str())
                .bind(plan.expected_version)
                .bind(&plan.expected_status)
                .bind(&plan.status)
                .bind(&plan.approved_by)
                .bind(plan.reviewed_at)
                .bind(&plan.reason)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            } else {
                sqlx::query(
                    "UPDATE customer_access_bindings SET status=$4,approved_by=$5,reviewed_at=$6,\
                     reason=$7,version=version+1,updated_at=$8 \
                     WHERE id=$1 AND company_id=$2 AND version=$3 AND status IN ('pending','approved')",
                )
                .bind(&plan.id)
                .bind(actor.company_id.as_str())
                .bind(plan.expected_version)
                .bind(&plan.status)
                .bind(&plan.approved_by)
                .bind(plan.reviewed_at)
                .bind(&plan.reason)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            };
            if changed != 1 {
                return Err(binding_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO customer_access_events(id,binding_id,version,action,actor_id,reason,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(plan.action.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            if plan.action == crate::domain::access::AccessAction::Revoke {
                sqlx::query(
                    "WITH changed AS (\
                     UPDATE customer_portal_invitations SET state='revoked',lease_token=NULL,\
                     lease_until=NULL,last_error='REVOKED' \
                     WHERE binding_id=$1 AND state IN ('queued','running','sent') RETURNING id) \
                     INSERT INTO customer_portal_invitation_events\
                     (id,invitation_id,action,actor_id,reason,created_at) \
                     SELECT 'customer-portal-invite-event-revoke:'||changed.id||':'||$2,\
                     changed.id,'revoked',$3,$4,$5 FROM changed",
                )
                .bind(&plan.id)
                .bind(key)
                .bind(actor.user_id.as_str())
                .bind(&plan.reason)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?;
            }
            let receipt = AccessReceipt::new(plan.id.clone(), &plan.status, plan.version);
            self.insert_receipt(
                actor,
                key,
                &plan.action.operation(),
                &plan.id,
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
                &plan.order_id,
                plan.now,
            )
            .await?;
            self.readback_change(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl CustomerAccessReadRepository for PgCustomerAccess<'_> {
    type Error = Error;

    fn binding<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        id: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error> {
        Box::pin(async move {
            let Some(row) = self.row_binding(actor, id, false).await? else {
                return Ok(None);
            };
            if !actor.in_scope(
                &row.company_id,
                row.branch_id.as_deref(),
                row.department_id.as_deref(),
            ) {
                return Ok(None);
            }
            Ok(Some(row))
        })
    }

    fn workspace<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessBindingRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT b.id,b.company_id,b.order_id,b.customer_id,b.email,b.expires_at,b.status,b.reason,\
                 b.created_by,b.approved_by,b.reviewed_at,b.version,b.created_at,b.updated_at,\
                 o.number order_number,o.state order_state,o.kind order_kind,o.branch_id,o.department_id,\
                 EXISTS(SELECT 1 FROM assignments m WHERE {REVIEWER} AND m.user_id=$4) actor_eligible,\
                 (SELECT count(DISTINCT m.user_id) FROM assignments m WHERE {REVIEWER}) reviewers,\
                 EXISTS(SELECT 1 FROM customer_access_identities i WHERE i.binding_id=b.id) identity_exists \
                 FROM customer_access_bindings b \
                 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
                 WHERE b.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
                 AND ($3::text IS NULL OR o.department_id=$3) \
                 ORDER BY b.updated_at DESC,b.id LIMIT 200"
            );
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                .bind(actor.user_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?;
            let rows = rows
                .into_iter()
                .map(map_binding)
                .collect::<Result<Vec<_>, _>>()?;
            validate_items(&rows)?;
            Ok(rows)
        })
    }

    fn sources<'a>(
        &'a mut self,
        actor: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessSourceRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT o.id,o.number,o.version,p.name \
                 FROM commerce_orders o \
                 JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
                 WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
                 AND ($3::text IS NULL OR o.department_id=$3) \
                 AND o.kind IN ('sales','quote') \
                 AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') \
                 ORDER BY o.updated_at DESC,o.id LIMIT 250",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(AccessSourceRow {
                        id: row.try_get("id")?,
                        number: row.try_get("number")?,
                        name: row.try_get("name")?,
                        version: row.try_get("version")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn events<'a>(&'a mut self, id: &'a str) -> AccessFuture<'a, Vec<AccessEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT version,action,actor_id,reason,created_at \
                 FROM customer_access_events WHERE binding_id=$1 ORDER BY version LIMIT 500",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(AccessEventRow {
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn reads<'a>(&'a mut self, id: &'a str) -> AccessFuture<'a, Vec<AccessReadRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT binding_version,user_id,read_at,projection_version \
                 FROM customer_access_reads WHERE binding_id=$1 ORDER BY read_at DESC LIMIT 100",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(AccessReadRow {
                        binding_version: row.try_get("binding_version")?,
                        user_id: row.try_get("user_id")?,
                        read_at: row.try_get("read_at")?,
                        projection_version: row.try_get("projection_version")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
