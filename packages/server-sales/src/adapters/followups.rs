//! PostgreSQL adapter for native Sales customer follow-up commands.
//!
//! The caller owns the retained serializable Holding transaction. This adapter
//! re-reads the actual sales/quote source and customer witness, enforces owner
//! eligibility and the purchased capability, writes the versioned follow-up,
//! immutable event, staff notification, receipt/audit/outbox unit, flushes
//! deferred effects and then re-checks authority, source witness and readback.
use crate::application::followups::{
    ChangeFollowupPlan, CreateFollowupPlan, FollowupCommandRepository, FollowupFuture,
    FollowupPurpose,
};
use crate::domain::followups::{
    FollowupAction, FollowupActor, FollowupOrder, FollowupReceiptSnapshot, FollowupSource,
    access_changed, not_found, owner_changed, readback_invalid, source_changed, source_not_found,
    version_conflict,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const ORDER_SOURCE: &str = "SELECT actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,\
 actual.party_id,actual.currency,\
 jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency)::text witness \
 FROM commerce_orders actual JOIN finance_parties customer \
 ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' \
 WHERE actual.kind IN ('quote','sales')";

const FOLLOWUP_SOURCE: &str = "SELECT followup.id,followup.company_id,followup.order_id,actual.kind order_kind,\
 actual.branch_id,actual.department_id,followup.owner_id,followup.due_date,followup.status,followup.version,\
 jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),\
 'order',jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency))::text witness \
 FROM customer_followups followup JOIN commerce_orders actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id \
 JOIN finance_parties customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' \
 WHERE actual.kind IN ('quote','sales')";

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments g \
 WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
 AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
 AND iam_role_allows(g.role,g.company_id,'commerce.read') \
 AND iam_role_allows(g.role,g.company_id,'commerce.sales') \
 AND (NOT $6::boolean OR iam_role_allows(g.role,g.company_id,'commerce.approve')) \
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$7 \
   OR g.scope_kind='department' AND g.scope_node_id=$8) \
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r \
   WHERE r.company_id=$4 AND r.user_id=$2 AND r.session_id=$9) \
 AND ($10::bigint IS NULL OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=$1 AND a.version=$10)))";

const OWNER_ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id \
 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
 JOIN commerce_orders o ON o.id=$1 AND o.company_id=$2 \
 WHERE a.company_id=$2 AND a.user_id=$3 \
 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_role_allows(a.role,a.company_id,'commerce.sales') \
 AND iam_role_allows(a.role,a.company_id,'commerce.read') \
 AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=o.branch_id \
   OR a.scope_kind='department' AND a.scope_node_id=o.department_id) \
 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' \
 AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
 AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

pub struct PgFollowups<'c> {
    connection: &'c mut PgConnection,
    mode: PackageEnforcementMode,
}

impl<'c> PgFollowups<'c> {
    pub fn new(connection: &'c mut PgConnection, mode: PackageEnforcementMode) -> Self {
        Self { connection, mode }
    }

    fn ready(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("FOLLOWUP_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn order_source(&mut self, order_id: &str) -> Result<Option<FollowupOrder>, Error> {
        let sql = format!(
            "SELECT actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,\
             jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,\
             actual.kind,actual.party_id,actual.currency)::text witness FROM ({ORDER_SOURCE}) actual \
             WHERE actual.id=$1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(order_id)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(FollowupOrder {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                kind: row.try_get("kind")?,
                witness: row.try_get("witness")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn followup_source(&mut self, id: &str) -> Result<Option<FollowupSource>, Error> {
        let sql = format!(
            "SELECT id,company_id,order_id,order_kind,branch_id,department_id,owner_id,due_date,\
             status,version,witness FROM ({FOLLOWUP_SOURCE}) followup WHERE followup.id=$1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(FollowupSource {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                order_id: row.try_get("order_id")?,
                order_kind: row.try_get("order_kind")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                owner_id: row.try_get("owner_id")?,
                due_date: row.try_get("due_date")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                witness: row.try_get("witness")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn owner_is_eligible(
        &mut self,
        order_id: &str,
        company_id: &str,
        owner_id: &str,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(OWNER_ELIGIBLE)
            .bind(order_id)
            .bind(company_id)
            .bind(owner_id)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn require_capability(
        &mut self,
        company_id: &str,
        capability: &str,
        purpose: FollowupPurpose,
        now: i64,
    ) -> Result<(), Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(());
        }
        let catalogue = current_package_catalogue()?;
        let company = CompanyId::new(company_id)?;
        let purpose = match purpose {
            FollowupPurpose::NewWork => CapabilityPurpose::NewWork,
            FollowupPurpose::SettleExisting => CapabilityPurpose::SettleExisting,
            FollowupPurpose::History => CapabilityPurpose::History,
        };
        PgCapabilityDecisionReader::new(company, self.connection, &catalogue)
            .require(capability, purpose, self.mode, TimestampMillis::new(now))
            .await?;
        Ok(())
    }

    async fn authority(
        &mut self,
        actor: &FollowupActor,
        manager: bool,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
            .bind(&actor.role)
            .bind(&actor.company_id)
            .bind(now)
            .bind(manager)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.session_id.as_deref())
            .bind(actor.assignment_version)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn ledger(
        &mut self,
        actor: &FollowupActor,
        key: &str,
        operation: &str,
        request_hash: &str,
        record_id: &str,
        result: &str,
        version: i64,
        purpose: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(key)
        .bind(operation)
        .bind(&actor.user_id)
        .bind(request_hash)
        .bind(record_id)
        .bind(result)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,'customer_followup',$6,$7,$8,$9,$10)",
        )
        .bind(format!("audit-followup-{key}"))
        .bind(key)
        .bind(&actor.user_id)
        .bind(&actor.assignment_id)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(purpose)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,'customer_followup',$4,$5,$6,$7)",
        )
        .bind(format!("outbox-followup-{key}"))
        .bind(key)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(result)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn notify(&mut self, id: &str, key: &str) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
             record_version,message,created_at) \
             SELECT $1,company_id,owner_id,'customer_followup',id,version,\
             CASE WHEN status='open' THEN 'Customer follow-up assigned: review next action and due date' \
             ELSE 'Customer follow-up outcome recorded' END,updated_at \
             FROM customer_followups WHERE id=$2 ON CONFLICT DO NOTHING",
        )
        .bind(format!("followup-notice-{key}"))
        .bind(id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), Error> {
        sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
}

impl FollowupCommandRepository for PgFollowups<'_> {
    type Error = Error;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> FollowupFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("customer-followup:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> FollowupFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(
                "SELECT encode(sha256(convert_to($1,'UTF8')),'hex')",
            )
            .bind(value)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(FollowupReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn order<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupOrder>, Self::Error> {
        Box::pin(async move { self.order_source(order_id).await })
    }

    fn followup<'a>(
        &'a mut self,
        id: &'a str,
    ) -> FollowupFuture<'a, Option<FollowupSource>, Self::Error> {
        Box::pin(async move { self.followup_source(id).await })
    }

    fn owner_eligible<'a>(
        &'a mut self,
        order_id: &'a str,
        owner_id: &'a str,
        _: i64,
    ) -> FollowupFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let company: String =
                sqlx::query_scalar("SELECT company_id FROM commerce_orders WHERE id=$1")
                    .bind(order_id)
                    .fetch_optional(&mut *self.connection)
                    .await?
                    .ok_or_else(source_not_found)?;
            self.owner_is_eligible(order_id, &company, owner_id).await
        })
    }

    fn capability<'a>(
        &'a mut self,
        company_id: &'a str,
        capability: &'a str,
        purpose: FollowupPurpose,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            self.require_capability(company_id, capability, purpose, now)
                .await
        })
    }

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a FollowupActor,
        manager: bool,
        now: i64,
    ) -> FollowupFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.authority(actor, manager, now).await })
    }

    fn commit_create<'a>(
        &'a mut self,
        plan: &'a CreateFollowupPlan<'a>,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let actual = self
                .order_source(&plan.input.order_id)
                .await?
                .ok_or_else(source_not_found)?;
            if actual.witness != plan.order.witness || actual.company_id != plan.actor.company_id {
                return Err(source_changed().into());
            }
            sqlx::query(
                "INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,\
                 status,version,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'open',1,$7,$8,$8)",
            )
            .bind(plan.id)
            .bind(&plan.actor.company_id)
            .bind(&plan.input.order_id)
            .bind(&plan.input.owner_id)
            .bind(&plan.input.due_date)
            .bind(&plan.input.next_action)
            .bind(&plan.actor.user_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,\
                 owner_id,due_date,reason,created_at) VALUES($1,$2,1,'create',$3,$4,$5,$6,$7)",
            )
            .bind(format!("followup-event-{}", plan.key))
            .bind(plan.id)
            .bind(&plan.actor.user_id)
            .bind(&plan.input.owner_id)
            .bind(&plan.input.due_date)
            .bind(&plan.input.next_action)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            self.notify(plan.id, plan.key).await?;
            let result = plan.receipt.wire().to_string();
            self.ledger(
                plan.actor,
                plan.key,
                plan.input.operation(),
                plan.request_hash,
                plan.id,
                &result,
                1,
                &plan.input.next_action,
                now,
            )
            .await?;
            self.flush().await?;
            if !self.authority(plan.actor, false, now).await? {
                return Err(access_changed().into());
            }
            let capability = if plan.order.kind == "quote" {
                "sales.quotes"
            } else {
                "sales.orders"
            };
            self.require_capability(
                &plan.actor.company_id,
                capability,
                FollowupPurpose::NewWork,
                now,
            )
            .await?;
            if !self
                .owner_is_eligible(&plan.input.order_id, &plan.actor.company_id, &plan.input.owner_id)
                .await?
            {
                return Err(owner_changed().into());
            }
            let final_source = self
                .followup_source(plan.id)
                .await?
                .ok_or_else(readback_invalid)?;
            if final_source.order_id != plan.input.order_id
                || final_source.version != Some(1)
                || final_source.status.as_deref() != Some("open")
                || final_source.owner_id.as_deref() != Some(plan.input.owner_id.as_str())
                || final_source.due_date.as_deref() != Some(plan.input.due_date.as_str())
            {
                return Err(source_changed().into());
            }
            let final_order = self
                .order_source(&plan.input.order_id)
                .await?
                .ok_or_else(source_changed)?;
            if final_order.witness != plan.order.witness {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }

    fn commit_change<'a>(
        &'a mut self,
        plan: &'a ChangeFollowupPlan<'a>,
        now: i64,
    ) -> FollowupFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let outcome = if plan.input.action == FollowupAction::Reassign {
                None
            } else {
                Some(plan.input.reason.clone())
            };
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE customer_followups SET owner_id=$1,due_date=$2,status=$3,outcome=$4,\
                 version=version+1,updated_at=$5 WHERE id=$6 AND version=$7 AND status='open' \
                 RETURNING id",
            )
            .bind(plan.owner)
            .bind(plan.due)
            .bind(plan.input.action.status())
            .bind(&outcome)
            .bind(now)
            .bind(plan.id)
            .bind(plan.input.expected_version)
            .fetch_optional(&mut *self.connection)
            .await?;
            if changed.is_none() {
                return Err(version_conflict().into());
            }
            sqlx::query(
                "INSERT INTO customer_followup_events(id,followup_id,version,action,actor_id,\
                 owner_id,due_date,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(format!("followup-event-{}", plan.key))
            .bind(plan.id)
            .bind(plan.input.expected_version + 1)
            .bind(plan.input.action.as_str())
            .bind(&plan.actor.user_id)
            .bind(plan.owner)
            .bind(plan.due)
            .bind(&plan.input.reason)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            self.notify(plan.id, plan.key).await?;
            let result = plan.receipt.wire().to_string();
            self.ledger(
                plan.actor,
                plan.key,
                &plan.input.action.operation(),
                plan.request_hash,
                plan.id,
                &result,
                plan.input.expected_version + 1,
                &plan.input.reason,
                now,
            )
            .await?;
            self.flush().await?;
            if !self.authority(plan.actor, plan.manager, now).await? {
                return Err(access_changed().into());
            }
            self.require_capability(
                &plan.actor.company_id,
                plan.source.capability(),
                FollowupPurpose::SettleExisting,
                now,
            )
            .await?;
            if matches!(plan.input.action, FollowupAction::Reassign | FollowupAction::Complete)
                && !self
                    .owner_is_eligible(&plan.source.order_id, &plan.actor.company_id, plan.owner)
                    .await?
            {
                return Err(owner_changed().into());
            }
            let final_source = self
                .followup_source(plan.id)
                .await?
                .ok_or_else(readback_invalid)?;
            if final_source.witness != plan.source.witness
                || final_source.version != Some(plan.input.expected_version + 1)
                || final_source.status.as_deref() != Some(plan.input.action.status())
                || final_source.owner_id.as_deref() != Some(plan.owner)
                || final_source.due_date.as_deref() != Some(plan.due)
            {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }
}
