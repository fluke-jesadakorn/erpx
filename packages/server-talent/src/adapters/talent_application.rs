//! PostgreSQL adapter for the native Talent application and proposal lifecycle.
//!
//! SQL is Talent-owned (`talent_applications`, `talent_application_events`,
//! `talent_proposals`) plus read-only requisition, candidate and IAM authority
//! facts and the shared command receipt, audit and outbox tables. Requisition
//! and candidate rows are pinned by their committed versions; the caller's
//! serializable Holding transaction is required and no commit is performed
//! here.
use crate::application::talent_application::{
    ApplicationFuture, TalentApplicationCommandRepository,
};
use crate::domain::receipt::ReceiptSnapshot;
use crate::domain::talent_application::{
    ApplicationChangePlan, ApplicationReceipt, ApplicationRow, CREATE_APPLICATION_OPERATION,
    CREATE_PROPOSAL_OPERATION, CandidateFact, CreateApplicationPlan, CreateProposalPlan,
    ProposalChangePlan, ProposalReceipt, ProposalRow, RequisitionFact, TalentActor,
    application_changed, application_idempotency_conflict, application_readback_invalid,
    proposal_changed, proposal_idempotency_conflict, proposal_readback_invalid, source_invalid,
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

const APPLICATION_SELECT: &str = "SELECT a.id,a.company_id,a.requisition_id,a.requisition_version,\
 a.candidate_id,a.status,a.owner_id,a.created_by,a.version,a.created_at,a.updated_at,\
 c.status candidate_status,c.version candidate_version,c.retention_until \
 FROM talent_applications a \
 JOIN talent_requisitions r ON r.id=a.requisition_id \
 JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id";

const PROPOSAL_SELECT: &str = "SELECT p.id,a.company_id,p.application_id,p.application_version,\
 p.recommendation,p.score_bps,p.evidence,p.status,p.submitted_by,p.reviewed_by,p.version,\
 p.created_at,p.updated_at \
 FROM talent_proposals p \
 JOIN talent_applications a ON a.id=p.application_id \
 JOIN talent_requisitions r ON r.id=a.requisition_id";

fn map_application(row: sqlx::postgres::PgRow) -> Result<ApplicationRow, sqlx::Error> {
    Ok(ApplicationRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        requisition_id: row.try_get("requisition_id")?,
        requisition_version: row.try_get("requisition_version")?,
        candidate_id: row.try_get("candidate_id")?,
        status: row.try_get("status")?,
        owner_id: row.try_get("owner_id")?,
        created_by: row.try_get("created_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        candidate_status: row.try_get("candidate_status")?,
        candidate_version: row.try_get("candidate_version")?,
        retention_until: row.try_get("retention_until")?,
    })
}

fn map_proposal(row: sqlx::postgres::PgRow) -> Result<ProposalRow, sqlx::Error> {
    Ok(ProposalRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        application_id: row.try_get("application_id")?,
        application_version: row.try_get("application_version")?,
        recommendation: row.try_get("recommendation")?,
        score_bps: row.try_get("score_bps")?,
        evidence: row.try_get("evidence")?,
        status: row.try_get("status")?,
        submitted_by: row.try_get("submitted_by")?,
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

pub struct PgTalentApplications<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTalentApplications<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TALENT_APPLICATION_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("TALENT_APPLICATION_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("talent-application-command:{key}"))
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

    async fn application_row(
        &mut self,
        actor: &TalentActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<ApplicationRow>, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        let sql = format!(
            "{APPLICATION_SELECT} WHERE a.id=$1 AND a.company_id=$2 \
             AND ($3::text IS NULL OR r.branch_id=$3) \
             AND ($4::text IS NULL OR r.department_id=$4){}",
            if lock { " FOR UPDATE OF a" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_application).transpose().map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result_json: &str,
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
        .bind(result_json)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(application_idempotency_conflict().into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_type: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(record_type)
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
        aggregate_type: &str,
        record_id: &str,
        status: &str,
        version: i64,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value = serde_json::from_str(&actor.authority_json())
            .map_err(|_| application_readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": record_id,
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
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(operation)
        .bind(aggregate_type)
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn requisition_placement(
        &mut self,
        actor: &TalentActor,
        requisition_id: &str,
    ) -> Result<(String, Option<String>), Error> {
        let row = sqlx::query(
            "SELECT branch_id,department_id FROM talent_requisitions \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(requisition_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(source_invalid)?;
        Ok((row.try_get("branch_id")?, row.try_get("department_id")?))
    }

    async fn placement(
        &mut self,
        actor: &TalentActor,
        application_id: &str,
    ) -> Result<(String, Option<String>), Error> {
        let row = sqlx::query(
            "SELECT r.branch_id,r.department_id FROM talent_applications a \
             JOIN talent_requisitions r ON r.id=a.requisition_id \
             WHERE a.id=$1 AND a.company_id=$2",
        )
        .bind(application_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(source_invalid)?;
        Ok((row.try_get("branch_id")?, row.try_get("department_id")?))
    }

    async fn readback_application(
        &mut self,
        actor: &TalentActor,
        plan: &ApplicationChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.application_row(actor, &plan.id, false).await? else {
            return Err(application_readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.owner_id != plan.owner_id
            || row.updated_at != plan.now
        {
            return Err(application_readback_invalid().into());
        }
        Ok(())
    }

    async fn proposal_row(
        &mut self,
        actor: &TalentActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<ProposalRow>, Error> {
        self.ready().await?;
        let (branch, department) = scope(actor);
        let sql = format!(
            "{PROPOSAL_SELECT} WHERE p.id=$1 AND a.company_id=$2 \
             AND ($3::text IS NULL OR r.branch_id=$3) \
             AND ($4::text IS NULL OR r.department_id=$4){}",
            if lock { " FOR UPDATE OF p" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_proposal).transpose().map_err(Into::into)
    }

    async fn readback_proposal(
        &mut self,
        actor: &TalentActor,
        plan: &ProposalChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.proposal_row(actor, &plan.id, false).await? else {
            return Err(proposal_readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.reviewed_by.as_deref() != Some(plan.reviewed_by.as_str())
            || row.updated_at != plan.now
        {
            return Err(proposal_readback_invalid().into());
        }
        Ok(())
    }
}

impl TalentApplicationCommandRepository for PgTalentApplications<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> ApplicationFuture<'a, String, Self::Error> {
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
    ) -> ApplicationFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn requisition<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<RequisitionFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let (branch, department) = scope(actor);
            let row = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,version,status \
                 FROM talent_requisitions WHERE id=$1 AND company_id=$2 \
                 AND ($3::text IS NULL OR branch_id=$3) \
                 AND ($4::text IS NULL OR department_id=$4)",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(RequisitionFact {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    version: row.try_get("version")?,
                    status: row.try_get("status")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn candidate<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<CandidateFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,status,retention_until,version FROM talent_candidates \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(CandidateFact {
                    id: row.try_get("id")?,
                    status: row.try_get("status")?,
                    retention_until: row.try_get("retention_until")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn owner_eligible<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        owner_id: &'a str,
        now: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM assignments x WHERE x.company_id=$1 AND x.user_id=$2 \
                 AND iam_assignment_active(x.id,$3) AND iam_principal_active(x.user_id,x.company_id,$3))",
            )
            .bind(actor.company_id.as_str())
            .bind(owner_id)
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn accepted_proposal<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        application_id: &'a str,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM talent_proposals p \
                 JOIN talent_applications a ON a.id=p.application_id \
                 WHERE p.application_id=$1 AND a.company_id=$2 AND p.status='accepted')",
            )
            .bind(application_id)
            .bind(actor.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn commit_application_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateApplicationPlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO talent_applications\
                 (id,company_id,requisition_id,requisition_version,candidate_id,status,owner_id,\
                 created_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,'applied',$6,$7,1,$8,$8) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.requisition_id)
            .bind(plan.requisition_version)
            .bind(&plan.candidate_id)
            .bind(&plan.owner_id)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(application_idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_application_events\
                 (id,application_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'apply',$3,'Candidate applied to approved requisition','applied',$4) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(application_idempotency_conflict().into());
            }
            let (branch, department) = self
                .requisition_placement(actor, &plan.requisition_id)
                .await?;
            let receipt = ApplicationReceipt::new(plan.id.clone(), "applied", 1);
            self.insert_receipt(
                actor,
                key,
                CREATE_APPLICATION_OPERATION,
                &plan.id,
                request_hash,
                &receipt.to_wire().to_string(),
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                CREATE_APPLICATION_OPERATION,
                "talent_application",
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                CREATE_APPLICATION_OPERATION,
                "talent_application",
                &plan.id,
                "applied",
                1,
                Some(&branch),
                department.as_deref(),
                plan.now,
            )
            .await?;
            let Some(row) = self.application_row(actor, &plan.id, false).await? else {
                return Err(application_readback_invalid().into());
            };
            if row.status != "applied"
                || row.version != 1
                || row.owner_id != plan.owner_id
                || row.requisition_version != plan.requisition_version
                || row.candidate_id != plan.candidate_id
                || row.updated_at != plan.now
            {
                return Err(application_readback_invalid().into());
            }
            Ok(receipt.to_wire())
        })
    }

    fn application_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<ApplicationRow>, Self::Error> {
        Box::pin(async move { self.application_row(actor, id, true).await })
    }

    fn commit_application_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ApplicationChangePlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE talent_applications SET status=$5,owner_id=$6,version=version+1,\
                 updated_at=$7 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.owner_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(application_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_application_events\
                 (id,application_id,version,action,actor_id,reason,status,created_at) \
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
                return Err(application_idempotency_conflict().into());
            }
            let (branch, department) = self.placement(actor, &plan.id).await?;
            let receipt = ApplicationReceipt::new(plan.id.clone(), &plan.status, plan.version);
            let operation = plan.action.operation();
            self.insert_receipt(
                actor,
                key,
                &operation,
                &plan.id,
                request_hash,
                &receipt.to_wire().to_string(),
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                &operation,
                "talent_application",
                &plan.id,
                plan.version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                &operation,
                "talent_application",
                &plan.id,
                &plan.status,
                plan.version,
                Some(&branch),
                department.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_application(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_proposal_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateProposalPlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO talent_proposals\
                 (id,application_id,application_version,recommendation,score_bps,evidence,status,\
                 submitted_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'submitted',$7,NULL,1,$8,$8) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.application_id)
            .bind(plan.application_version)
            .bind(plan.recommendation.as_str())
            .bind(plan.score_bps)
            .bind(&plan.evidence)
            .bind(&plan.submitted_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(proposal_idempotency_conflict().into());
            }
            let (branch, department) = self.placement(actor, &plan.application_id).await?;
            let receipt = ProposalReceipt::new(plan.id.clone(), "submitted", 1);
            self.insert_receipt(
                actor,
                key,
                CREATE_PROPOSAL_OPERATION,
                &plan.id,
                request_hash,
                &receipt.to_wire().to_string(),
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                CREATE_PROPOSAL_OPERATION,
                "talent_proposal",
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                CREATE_PROPOSAL_OPERATION,
                "talent_proposal",
                &plan.id,
                "submitted",
                1,
                Some(&branch),
                department.as_deref(),
                plan.now,
            )
            .await?;
            let Some(row) = self.proposal_row(actor, &plan.id, false).await? else {
                return Err(proposal_readback_invalid().into());
            };
            if row.status != "submitted"
                || row.version != 1
                || row.application_version != plan.application_version
                || row.submitted_by != plan.submitted_by
                || row.updated_at != plan.now
            {
                return Err(proposal_readback_invalid().into());
            }
            Ok(receipt.to_wire())
        })
    }

    fn proposal_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<ProposalRow>, Self::Error> {
        Box::pin(async move { self.proposal_row(actor, id, true).await })
    }

    fn commit_proposal_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a ProposalChangePlan,
        request_hash: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE talent_proposals SET status=$4,reviewed_by=$5,version=version+1,\
                 updated_at=$6 WHERE id=$1 AND version=$2 AND status=$3",
            )
            .bind(&plan.id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.reviewed_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(proposal_changed().into());
            }
            let application_id: String =
                sqlx::query_scalar("SELECT application_id FROM talent_proposals WHERE id=$1")
                    .bind(&plan.id)
                    .fetch_one(&mut *self.connection)
                    .await?;
            let (branch, department) = self.placement(actor, &application_id).await?;
            let receipt = ProposalReceipt::new(plan.id.clone(), &plan.status, plan.version);
            let operation = plan.action.operation();
            self.insert_receipt(
                actor,
                key,
                &operation,
                &plan.id,
                request_hash,
                &receipt.to_wire().to_string(),
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                &operation,
                "talent_proposal",
                &plan.id,
                plan.version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                &operation,
                "talent_proposal",
                &plan.id,
                &plan.status,
                plan.version,
                Some(&branch),
                department.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_proposal(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }
}
