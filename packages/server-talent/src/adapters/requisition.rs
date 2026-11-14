//! PostgreSQL adapter for the native Talent requisition lifecycle.
//!
//! SQL is Talent-owned (`talent_requisitions`,
//! `talent_requisition_events`) plus the shared command receipt, audit and
//! outbox tables. Current IAM authority is read through the existing
//! assignments/IAM functions used by the retained owner. No candidate,
//! application, offer, People, Learning, Records or Finance row is mutated.
//! The caller's serializable Holding transaction is required and no commit is
//! performed here.
use crate::application::requisition::{
    RequisitionFuture, TalentRequisitionCommandRepository, TalentRequisitionReadRepository,
};
use crate::domain::requisition::{
    ChangePlan, CreatePlan, ReceiptSnapshot, RequisitionEventRow, RequisitionReceipt,
    RequisitionRow, TalentActor, idempotency_conflict, readback_invalid, requisition_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'records.read') \
 AND iam_role_allows(a.role,a.company_id,'records.review')";

const SELECT: &str = "SELECT id,company_id,branch_id,department_id,job_title,openings,\
 target_start_date,description,status,created_by,reviewed_by,version,created_at,updated_at \
 FROM talent_requisitions";

fn map_row(row: sqlx::postgres::PgRow) -> Result<RequisitionRow, sqlx::Error> {
    Ok(RequisitionRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        job_title: row.try_get("job_title")?,
        openings: row.try_get("openings")?,
        target_start_date: row.try_get("target_start_date")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn scope(actor: &TalentActor) -> (Option<&str>, Option<&str>) {
    (
        actor.branch_id.as_ref().map(|value| value.as_str()),
        actor.department_id.as_ref().map(|value| value.as_str()),
    )
}

pub struct PgTalentRequisitions<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTalentRequisitions<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TALENT_REQUISITION_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("TALENT_REQUISITION_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("talent-requisition-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(&mut self, actor: &TalentActor, write: bool, now: i64) -> Result<bool, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE {AUTHORITY} \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$6::text) \
             OR (a.scope_kind='department' AND a.scope_node_id=$7::text)) \
             AND (NOT $8::boolean OR iam_role_allows(a.role,a.company_id,'records.write')))"
        );
        Ok(sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(actor.role.as_deref())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(write)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn row(
        &mut self,
        actor: &TalentActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<RequisitionRow>, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        let sql = format!(
            "{SELECT} WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) \
             AND ($4::text IS NULL OR department_id=$4){}",
            if lock { " FOR UPDATE" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_row).transpose().map_err(Into::into)
    }

    async fn readback_create(
        &mut self,
        actor: &TalentActor,
        plan: &CreatePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.id != plan.id
            || row.company_id != plan.company_id
            || row.branch_id != plan.branch_id
            || row.department_id != plan.department_id
            || row.job_title != plan.job_title
            || row.openings != plan.openings
            || row.target_start_date != plan.target_start_date
            || row.description != plan.description
            || row.status != "draft"
            || row.created_by != plan.created_by
            || row.reviewed_by.is_some()
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
        actor: &TalentActor,
        plan: &ChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.reviewed_by != plan.reviewed_by
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result: &RequisitionReceipt,
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
        actor: &TalentActor,
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
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'talent_requisition',$5,$6,$4,$7,$8)",
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
        actor: &TalentActor,
        key: &str,
        operation: &str,
        plan_id: &str,
        status: &str,
        version: i64,
        branch_id: &str,
        department_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": plan_id,
            "state": status,
            "companyId": actor.company_id.as_str(),
            "branchId": branch_id,
            "departmentId": department_id,
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'talent_requisition',$3,$4,$5,$6)",
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

impl TalentRequisitionCommandRepository for PgTalentRequisitions<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> RequisitionFuture<'a, String, Self::Error> {
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
    ) -> RequisitionFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn requisition_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error> {
        Box::pin(async move { self.row(actor, id, true).await })
    }

    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreatePlan,
        request_hash: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO talent_requisitions\
                 (id,company_id,branch_id,department_id,job_title,openings,target_start_date,\
                 description,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,'draft',$9,NULL,1,$10,$10) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.branch_id)
            .bind(&plan.department_id)
            .bind(&plan.job_title)
            .bind(plan.openings)
            .bind(&plan.target_start_date)
            .bind(&plan.description)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_requisition_events\
                 (id,requisition_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'create',$3,$4,'draft',$5) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind("Hiring need drafted")
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = RequisitionReceipt::new(plan.id.clone(), "draft", 1);
            self.insert_receipt(
                actor,
                key,
                crate::domain::requisition::CREATE_OPERATION,
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                crate::domain::requisition::CREATE_OPERATION,
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                crate::domain::requisition::CREATE_OPERATION,
                &plan.id,
                "draft",
                1,
                &plan.branch_id,
                plan.department_id.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_create(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ChangePlan,
        request_hash: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE talent_requisitions SET status=$5,reviewed_by=$6,version=version+1,\
                 updated_at=$7 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.reviewed_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(requisition_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_requisition_events\
                 (id,requisition_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(plan.action.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(&plan.status)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let Some(row) = self.row(actor, &plan.id, false).await? else {
                return Err(readback_invalid().into());
            };
            let receipt = RequisitionReceipt::new(plan.id.clone(), &plan.status, plan.version);
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
                &row.branch_id,
                row.department_id.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_change(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl TalentRequisitionReadRepository for PgTalentRequisitions<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn requisitions<'a>(
        &'a mut self,
        actor: &'a TalentActor,
    ) -> RequisitionFuture<'a, Vec<RequisitionRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let (branch, department) = scope(actor);
            let sql = format!(
                "{SELECT} WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) \
                 AND ($3::text IS NULL OR department_id=$3) \
                 ORDER BY updated_at DESC,id LIMIT 200"
            );
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(branch)
                .bind(department)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter()
                .map(map_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn requisition<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error> {
        Box::pin(async move { self.row(actor, id, false).await })
    }

    fn events<'a>(
        &'a mut self,
        id: &'a str,
    ) -> RequisitionFuture<'a, Vec<RequisitionEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT version,action,actor_id,reason,status,created_at \
                 FROM talent_requisition_events WHERE requisition_id=$1 \
                 ORDER BY version LIMIT 500",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(RequisitionEventRow {
                        version: row.try_get("version")?,
                        action: row.try_get("action")?,
                        actor_id: row.try_get("actor_id")?,
                        reason: row.try_get("reason")?,
                        status: row.try_get("status")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
