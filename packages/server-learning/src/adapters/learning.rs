//! PostgreSQL adapter for the native Learning owner.
//!
//! SQL is Learning-owned (`learning_courses`, `learning_course_events`,
//! `learning_assignments`, `learning_assignment_events`,
//! `learning_eligibility_handoffs`, `learning_certificates`,
//! `learning_certificate_revocations`) plus read-only projections of the
//! existing IAM assignments/principals and People person/engagement rows used
//! by the retained Learning reader. No IAM, People, Commerce or Files row is
//! mutated. The caller's serializable Holding transaction is required and no
//! commit is performed here.
use crate::application::learning::{
    LearningCommandRepository, LearningFuture, LearningReadRepository,
};
use crate::domain::learning::{
    AssignmentAction, AssignmentChangePlan, AssignmentCreatePlan, AssignmentRow,
    CertificateRevokePlan, CertificateRow, CourseChangePlan, CourseCreatePlan, CourseRow, EventRow,
    HandoffRow, LearningActor, LearningReceipt, PersonRow, PlacementRow, ReceiptSnapshot,
    certificate_revoked, course_changed, idempotency_conflict, readback_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const ELIGIBLE: &str = "EXISTS(SELECT 1 FROM assignments x \
JOIN hr_people eligible_person ON eligible_person.user_id=x.user_id AND eligible_person.company_id=x.company_id \
JOIN hr_engagements eligible_engagement ON eligible_engagement.person_id=eligible_person.id AND eligible_engagement.company_id=eligible_person.company_id \
WHERE x.company_id=a.company_id AND x.user_id=a.learner_id \
AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=a.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=a.department_id)) \
AND eligible_person.status='active' AND eligible_person.merged_into_id IS NULL AND eligible_engagement.status='active')";

const ASSIGNMENT_COLUMNS: &str = "a.id,a.company_id,a.branch_id,a.department_id,a.course_id,a.course_version,\
a.learner_id,a.due_date,a.status,a.attendance,a.score_bps,a.result_evidence,a.completion_reason,\
a.created_by,a.version,a.created_at,a.updated_at,c.title course_title,c.required_for,\
person.id person_id,person.legal_name learner_name,\
CASE WHEN {ELIGIBLE} THEN TRUE ELSE FALSE END learner_eligible";

fn assignment_select(lock: bool) -> String {
    format!(
        "SELECT {} FROM learning_assignments a \
         JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id \
         LEFT JOIN LATERAL (SELECT p.id,p.legal_name FROM hr_people p \
         WHERE p.company_id=a.company_id AND p.user_id=a.learner_id AND p.merged_into_id IS NULL \
         ORDER BY p.id LIMIT 1) person ON TRUE \
         WHERE a.id=$1 AND a.company_id=$2 \
         AND ($3::text IS NULL OR a.branch_id=$3) \
         AND ($4::text IS NULL OR a.department_id=$4){}",
        ASSIGNMENT_COLUMNS.replace("{ELIGIBLE}", ELIGIBLE),
        if lock {
            " FOR UPDATE OF a FOR SHARE OF c"
        } else {
            ""
        }
    )
}

fn course_select(lock: &str) -> String {
    format!(
        "SELECT c.id,c.company_id,c.title,c.description,c.provider,c.duration_minutes,c.required_for,\
         c.status,c.created_by,c.reviewed_by,c.reviewed_at,c.version,c.created_at,c.updated_at \
         FROM learning_courses c WHERE c.id=$1 AND c.company_id=$2{lock}"
    )
}

fn map_course(row: sqlx::postgres::PgRow) -> Result<CourseRow, sqlx::Error> {
    Ok(CourseRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        provider: row.try_get("provider")?,
        duration_minutes: row.try_get("duration_minutes")?,
        required_for: row.try_get("required_for")?,
        status: row.try_get("status")?,
        created_by: row.try_get("created_by")?,
        reviewed_by: row.try_get("reviewed_by")?,
        reviewed_at: row.try_get("reviewed_at")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_assignment(row: sqlx::postgres::PgRow) -> Result<AssignmentRow, sqlx::Error> {
    Ok(AssignmentRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        course_id: row.try_get("course_id")?,
        course_version: row.try_get("course_version")?,
        learner_id: row.try_get("learner_id")?,
        due_date: row.try_get("due_date")?,
        status: row.try_get("status")?,
        attendance: row.try_get("attendance")?,
        score_bps: row.try_get("score_bps")?,
        result_evidence: row.try_get("result_evidence")?,
        completion_reason: row.try_get("completion_reason")?,
        created_by: row.try_get("created_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        course_title: row.try_get("course_title")?,
        required_for: row.try_get("required_for")?,
        person_id: row.try_get("person_id")?,
        learner_name: row.try_get("learner_name")?,
        learner_eligible: row.try_get("learner_eligible")?,
    })
}

fn map_event(row: sqlx::postgres::PgRow) -> Result<EventRow, sqlx::Error> {
    Ok(EventRow {
        id: row.try_get("id")?,
        assignment_id: row.try_get("assignment_id")?,
        version: row.try_get("version")?,
        action: row.try_get("action")?,
        actor_id: row.try_get("actor_id")?,
        reason: row.try_get("reason")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

fn map_handoff(row: sqlx::postgres::PgRow) -> Result<HandoffRow, sqlx::Error> {
    Ok(HandoffRow {
        id: row.try_get("id")?,
        assignment_id: row.try_get("assignment_id")?,
        company_id: row.try_get("company_id")?,
        person_id: row.try_get("person_id")?,
        eligibility_code: row.try_get("eligibility_code")?,
        source_assignment_version: row.try_get("source_assignment_version")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

fn map_certificate(row: sqlx::postgres::PgRow) -> Result<CertificateRow, sqlx::Error> {
    Ok(CertificateRow {
        id: row.try_get("id")?,
        handoff_id: row.try_get("handoff_id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        person_id: row.try_get("person_id")?,
        assignment_id: row.try_get("assignment_id")?,
        source_assignment_version: row.try_get("source_assignment_version")?,
        eligibility_code: row.try_get("eligibility_code")?,
        valid_from: row.try_get("valid_from")?,
        expires_on: row.try_get("expires_on")?,
        completion_kind: row.try_get("completion_kind")?,
        evidence_ref: row.try_get("evidence_ref")?,
        issued_by: row.try_get("issued_by")?,
        created_at: row.try_get("created_at")?,
        revocation_id: row.try_get("revocation_id")?,
    })
}

pub struct PgLearning<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgLearning<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("LEARNING_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("LEARNING_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("learning-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn row_course(
        &mut self,
        actor: &LearningActor,
        id: &str,
        lock: &str,
    ) -> Result<Option<CourseRow>, Error> {
        self.ready().await?;
        let sql = course_select(lock);
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_course).transpose().map_err(Into::into)
    }

    async fn row_assignment(
        &mut self,
        actor: &LearningActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<AssignmentRow>, Error> {
        self.ready().await?;
        let sql = assignment_select(lock);
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_assignment).transpose().map_err(Into::into)
    }

    async fn lock_learner(&mut self, company_id: &str, learner_id: &str) -> Result<(), Error> {
        sqlx::query("SELECT id FROM assignments WHERE company_id=$1 AND user_id=$2 FOR SHARE")
            .bind(company_id)
            .bind(learner_id)
            .execute(&mut *self.connection)
            .await?;
        sqlx::query("SELECT user_id FROM iam_members WHERE company_id=$1 AND user_id=$2 FOR SHARE")
            .bind(company_id)
            .bind(learner_id)
            .execute(&mut *self.connection)
            .await?;
        sqlx::query(
            "SELECT p.id FROM hr_people p \
             JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
             WHERE p.company_id=$1 AND p.user_id=$2 FOR SHARE OF p,e",
        )
        .bind(company_id)
        .bind(learner_id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn row_certificate(
        &mut self,
        actor: &LearningActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<CertificateRow>, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT c.id,c.handoff_id,c.company_id,a.branch_id,a.department_id,c.person_id,c.assignment_id,\
             c.source_assignment_version,c.eligibility_code,c.valid_from,c.expires_on,c.completion_kind,\
             c.evidence_ref,c.issued_by,c.created_at,revocation.id revocation_id \
             FROM learning_certificates c \
             JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id \
             LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=c.id \
             WHERE c.id=$1 AND c.company_id=$2 \
             AND ($3::text IS NULL OR a.branch_id=$3) \
             AND ($4::text IS NULL OR a.department_id=$4){}",
            if lock {
                " FOR UPDATE OF c FOR SHARE OF a"
            } else {
                ""
            }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_certificate).transpose().map_err(Into::into)
    }

    async fn readback_course_create(
        &mut self,
        actor: &LearningActor,
        plan: &CourseCreatePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_course(actor, &plan.id, "").await? else {
            return Err(readback_invalid().into());
        };
        if row.title != plan.title
            || row.description != plan.description
            || row.provider != plan.provider
            || row.duration_minutes != plan.duration_minutes
            || row.required_for != plan.required_for
            || row.status != "draft"
            || row.created_by != plan.created_by
            || row.reviewed_by.is_some()
            || row.reviewed_at.is_some()
            || row.version != 1
            || row.created_at != plan.now
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_course_change(
        &mut self,
        actor: &LearningActor,
        plan: &CourseChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_course(actor, &plan.id, "").await? else {
            return Err(readback_invalid().into());
        };
        if row.status != plan.status
            || row.reviewed_by.as_deref() != Some(plan.reviewed_by.as_str())
            || row.reviewed_at != Some(plan.reviewed_at)
            || row.version != plan.version
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_assignment_create(
        &mut self,
        actor: &LearningActor,
        plan: &AssignmentCreatePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_assignment(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.company_id != plan.company_id
            || row.branch_id != plan.branch_id
            || row.department_id != plan.department_id
            || row.course_id != plan.course_id
            || row.course_version != plan.course_version
            || row.learner_id != plan.learner_id
            || row.due_date != plan.due_date
            || row.status != "assigned"
            || row.attendance.is_some()
            || row.score_bps.is_some()
            || row.result_evidence.is_some()
            || row.completion_reason.is_some()
            || row.created_by != plan.created_by
            || row.version != 1
            || row.created_at != plan.now
            || row.updated_at != plan.now
            || !row.learner_eligible
            || row.person_id.is_none()
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_assignment_change(
        &mut self,
        actor: &LearningActor,
        plan: &AssignmentChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row_assignment(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.attendance != plan.attendance
            || row.score_bps != plan.score_bps
            || row.result_evidence != plan.result_evidence
            || row.completion_reason != plan.completion_reason
            || row.updated_at != plan.now
            || matches!(
                plan.action,
                AssignmentAction::Start | AssignmentAction::Complete
            ) && !row.learner_eligible
        {
            return Err(readback_invalid().into());
        }
        if let Some(certificate) = &plan.certificate {
            let Some(handoff) = sqlx::query(
                "SELECT id,assignment_id,company_id,person_id,eligibility_code,\
                 source_assignment_version,status,created_at \
                 FROM learning_eligibility_handoffs WHERE assignment_id=$1 AND company_id=$2",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .map(map_handoff)
            .transpose()?
            else {
                return Err(readback_invalid().into());
            };
            let Some(issued) = sqlx::query(
                "SELECT c.id,c.handoff_id,c.company_id,a.branch_id,a.department_id,c.person_id,\
                 c.assignment_id,c.source_assignment_version,c.eligibility_code,c.valid_from,\
                 c.expires_on,c.completion_kind,c.evidence_ref,c.issued_by,c.created_at,\
                 revocation.id revocation_id \
                 FROM learning_certificates c \
                 JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id \
                 LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=c.id \
                 WHERE c.assignment_id=$1 AND c.company_id=$2",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .map(map_certificate)
            .transpose()?
            else {
                return Err(readback_invalid().into());
            };
            if handoff.id != certificate.handoff_id
                || handoff.assignment_id != plan.id
                || handoff.company_id != actor.company_id.as_str()
                || handoff.person_id != certificate.person_id
                || handoff.eligibility_code != certificate.eligibility_code
                || handoff.source_assignment_version != certificate.source_assignment_version
                || handoff.created_at != plan.now
                || issued.id != certificate.certificate_id
                || issued.handoff_id != certificate.handoff_id
                || issued.assignment_id != plan.id
                || issued.person_id != certificate.person_id
                || issued.eligibility_code != certificate.eligibility_code
                || issued.source_assignment_version != plan.version
                || issued.valid_from != certificate.valid_from
                || issued.expires_on != certificate.expires_on
                || issued.completion_kind != certificate.completion_kind
                || issued.evidence_ref != certificate.evidence_ref
                || issued.issued_by != actor.user_id.as_str()
                || issued.created_at != plan.now
                || issued.revocation_id.is_some()
            {
                return Err(readback_invalid().into());
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &LearningActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        receipt: &LearningReceipt,
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
        .bind(receipt.to_wire().to_string())
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

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        &mut self,
        actor: &LearningActor,
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
        actor: &LearningActor,
        key: &str,
        operation: &str,
        aggregate_type: &str,
        aggregate_id: &str,
        version: i64,
        details: Value,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": aggregate_id,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": version,
            "details": details,
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
        .bind(aggregate_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl LearningCommandRepository for PgLearning<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> LearningFuture<'a, String, Self::Error> {
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
    ) -> LearningFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn course_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CourseRow>, Self::Error> {
        Box::pin(async move { self.row_course(actor, id, " FOR UPDATE").await })
    }

    fn course_source<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CourseRow>, Self::Error> {
        Box::pin(async move { self.row_course(actor, id, " FOR SHARE").await })
    }

    fn placement<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        learner_id: &'a str,
    ) -> LearningFuture<'a, Option<PlacementRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_learner(actor.company_id.as_str(), learner_id)
                .await?;
            let row = sqlx::query(
                "SELECT p.id person_id,p.company_id,e.branch_id,e.department_id,\
                 EXISTS(SELECT 1 FROM assignments x \
                 WHERE x.company_id=p.company_id AND x.user_id=p.user_id \
                 AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=e.branch_id) \
                 OR (x.scope_kind='department' AND x.scope_node_id=e.department_id))) eligible \
                 FROM hr_people p \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 WHERE p.company_id=$1 AND ($2::text IS NULL OR e.branch_id=$2) \
                 AND ($3::text IS NULL OR e.department_id=$3) \
                 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' \
                 AND p.user_id=$4 ORDER BY p.id,e.id LIMIT 1 FOR SHARE OF p,e",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(learner_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PlacementRow {
                    person_id: row.try_get("person_id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    eligible: row.try_get("eligible")?,
                })
            })
            .transpose()
        })
    }

    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<AssignmentRow>, Self::Error> {
        Box::pin(async move {
            let Some(row) = self.row_assignment(actor, id, true).await? else {
                return Ok(None);
            };
            self.lock_learner(&row.company_id, &row.learner_id).await?;
            self.row_assignment(actor, id, false).await
        })
    }

    fn certificate_for_update<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CertificateRow>, Self::Error> {
        Box::pin(async move { self.row_certificate(actor, id, true).await })
    }

    fn commit_course_create<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CourseCreatePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO learning_courses\
                 (id,company_id,title,description,provider,duration_minutes,required_for,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .bind(&plan.title)
            .bind(&plan.description)
            .bind(&plan.provider)
            .bind(plan.duration_minutes)
            .bind(&plan.required_for)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO learning_course_events\
                 (id,course_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'create',$3,'Course drafted for independent review','draft',$4) \
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
                return Err(idempotency_conflict().into());
            }
            let receipt = LearningReceipt::new(plan.id.clone(), "draft", 1);
            self.insert_receipt(
                actor,
                key,
                crate::domain::learning::COURSE_OPERATION,
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                crate::domain::learning::COURSE_OPERATION,
                "learning_course",
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                crate::domain::learning::COURSE_OPERATION,
                "learning_course",
                &plan.id,
                1,
                json!({"status": "draft"}),
                plan.now,
            )
            .await?;
            self.readback_course_create(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_course_change<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CourseChangePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE learning_courses SET status=$5,reviewed_by=$6,reviewed_at=$7,\
                 version=version+1,updated_at=$8 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4 AND created_by<>$6",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.reviewed_by)
            .bind(plan.reviewed_at)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(course_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO learning_course_events\
                 (id,course_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
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
            let receipt = LearningReceipt::new(plan.id.clone(), &plan.status, plan.version);
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
                "learning_course",
                &plan.id,
                plan.version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                &plan.action.operation(),
                "learning_course",
                &plan.id,
                plan.version,
                json!({"status": plan.status, "reason": plan.reason}),
                plan.now,
            )
            .await?;
            self.readback_course_change(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_assignment_create<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a AssignmentCreatePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO learning_assignments\
                 (id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,\
                 created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10) ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.branch_id)
            .bind(&plan.department_id)
            .bind(&plan.course_id)
            .bind(plan.course_version)
            .bind(&plan.learner_id)
            .bind(&plan.due_date)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO learning_assignment_events\
                 (id,assignment_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'assign',$3,$4,'assigned',$5) ON CONFLICT(id) DO NOTHING",
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
            let receipt = LearningReceipt::new(plan.id.clone(), "assigned", 1);
            self.insert_receipt(
                actor,
                key,
                "learning.assignment.create",
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                "learning.assignment.create",
                "learning_assignment",
                &plan.id,
                1,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                "learning.assignment.create",
                "learning_assignment",
                &plan.id,
                1,
                json!({
                    "status": "assigned",
                    "courseId": plan.course_id,
                    "courseVersion": plan.course_version,
                    "learnerId": plan.learner_id,
                }),
                plan.now,
            )
            .await?;
            self.readback_assignment_create(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_assignment_change<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a AssignmentChangePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE learning_assignments SET status=$5,attendance=$6,score_bps=$7,\
                 result_evidence=$8,completion_reason=$9,version=version+1,updated_at=$10 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(actor.company_id.as_str())
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.attendance)
            .bind(plan.score_bps)
            .bind(&plan.result_evidence)
            .bind(&plan.completion_reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(crate::domain::learning::state_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO learning_assignment_events\
                 (id,assignment_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
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
            if let Some(certificate) = &plan.certificate {
                let handoff = sqlx::query(
                    "INSERT INTO learning_eligibility_handoffs\
                     (id,assignment_id,company_id,person_id,eligibility_code,source_assignment_version,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(assignment_id) DO NOTHING",
                )
                .bind(&certificate.handoff_id)
                .bind(&plan.id)
                .bind(actor.company_id.as_str())
                .bind(&certificate.person_id)
                .bind(&certificate.eligibility_code)
                .bind(certificate.source_assignment_version)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if handoff != 1 {
                    return Err(idempotency_conflict().into());
                }
                let issued = sqlx::query(
                    "INSERT INTO learning_certificates\
                     (id,handoff_id,company_id,person_id,assignment_id,source_assignment_version,\
                     eligibility_code,valid_from,expires_on,completion_kind,evidence_ref,issued_by,created_at) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
                     ON CONFLICT(handoff_id) DO NOTHING",
                )
                .bind(&certificate.certificate_id)
                .bind(&certificate.handoff_id)
                .bind(actor.company_id.as_str())
                .bind(&certificate.person_id)
                .bind(&plan.id)
                .bind(certificate.source_assignment_version)
                .bind(&certificate.eligibility_code)
                .bind(&certificate.valid_from)
                .bind(&certificate.expires_on)
                .bind(&certificate.completion_kind)
                .bind(&certificate.evidence_ref)
                .bind(actor.user_id.as_str())
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if issued != 1 {
                    return Err(idempotency_conflict().into());
                }
            }
            let receipt = LearningReceipt::new(plan.id.clone(), &plan.status, plan.version);
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
                "learning_assignment",
                &plan.id,
                plan.version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                &plan.action.operation(),
                "learning_assignment",
                &plan.id,
                plan.version,
                json!({
                    "status": plan.status,
                    "action": plan.action.as_str(),
                    "certificateId": plan.certificate.as_ref().map(|value| value.certificate_id.as_str()),
                }),
                plan.now,
            )
            .await?;
            self.readback_assignment_change(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_certificate_revoke<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        key: &'a str,
        plan: &'a CertificateRevokePlan,
        request_hash: &'a str,
    ) -> LearningFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO learning_certificate_revocations\
                 (id,certificate_id,company_id,actor_id,reason,command_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(certificate_id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.certificate_id)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(key)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(certificate_revoked().into());
            }
            let receipt = LearningReceipt::new(
                plan.certificate_id.clone(),
                "revoked",
                plan.expected_assignment_version,
            );
            self.insert_receipt(
                actor,
                key,
                "learning.certificate.revoke",
                &plan.certificate_id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(
                actor,
                key,
                "learning.certificate.revoke",
                "learning_certificate",
                &plan.certificate_id,
                plan.expected_assignment_version,
                plan.now,
            )
            .await?;
            self.outbox(
                actor,
                key,
                "learning.certificate.revoke",
                "learning_certificate",
                &plan.certificate_id,
                plan.expected_assignment_version,
                json!({"status": "revoked", "assignmentId": plan.assignment_id}),
                plan.now,
            )
            .await?;
            let Some(row) = self
                .row_certificate(actor, &plan.certificate_id, false)
                .await?
            else {
                return Err(readback_invalid().into());
            };
            if row.revocation_id.as_deref() != Some(plan.id.as_str()) {
                return Err(readback_invalid().into());
            }
            Ok(receipt.to_wire())
        })
    }
}

impl LearningReadRepository for PgLearning<'_> {
    type Error = Error;

    fn courses<'a>(
        &'a mut self,
        actor: &'a LearningActor,
    ) -> LearningFuture<'a, Vec<CourseRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,company_id,title,description,provider,duration_minutes,required_for,\
                 status,created_by,reviewed_by,reviewed_at,version,created_at,updated_at \
                 FROM learning_courses WHERE company_id=$1 ORDER BY updated_at DESC,id LIMIT 501",
            )
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(map_course)
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn assignments<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        personal: bool,
    ) -> LearningFuture<'a, Vec<AssignmentRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT {} FROM learning_assignments a \
                 JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id \
                 LEFT JOIN LATERAL (SELECT p.id,p.legal_name FROM hr_people p \
                 WHERE p.company_id=a.company_id AND p.user_id=a.learner_id AND p.merged_into_id IS NULL \
                 ORDER BY p.id LIMIT 1) person ON TRUE \
                 WHERE a.company_id=$1 AND ($2::text IS NULL OR a.branch_id=$2) \
                 AND ($3::text IS NULL OR a.department_id=$3) \
                 AND (NOT $4::boolean OR a.learner_id=$5) \
                 ORDER BY a.updated_at DESC,a.id LIMIT 1001",
                ASSIGNMENT_COLUMNS.replace("{ELIGIBLE}", ELIGIBLE)
            );
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                .bind(personal)
                .bind(actor.user_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?;
            rows.into_iter()
                .map(map_assignment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn people<'a>(
        &'a mut self,
        actor: &'a LearningActor,
    ) -> LearningFuture<'a, Vec<PersonRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT p.user_id id,p.legal_name name,e.branch_id,e.department_id \
                 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
                 AND e.status='active' AND p.user_id IS NOT NULL \
                 AND ($2::text IS NULL OR e.branch_id=$2) \
                 AND ($3::text IS NULL OR e.department_id=$3) \
                 ORDER BY p.legal_name,p.id,e.id LIMIT 501",
            )
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PersonRow {
                        id: row.try_get("id")?,
                        name: row.try_get("name")?,
                        branch_id: row.try_get("branch_id")?,
                        department_id: row.try_get("department_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn assignment<'a>(
        &'a mut self,
        actor: &'a LearningActor,
        id: &'a str,
    ) -> LearningFuture<'a, Option<AssignmentRow>, Self::Error> {
        Box::pin(async move { self.row_assignment(actor, id, false).await })
    }

    fn events<'a>(&'a mut self, id: &'a str) -> LearningFuture<'a, Vec<EventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,assignment_id,version,action,actor_id,reason,status,created_at \
                 FROM learning_assignment_events WHERE assignment_id=$1 ORDER BY version LIMIT 501",
            )
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(map_event)
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn handoff<'a>(
        &'a mut self,
        id: &'a str,
    ) -> LearningFuture<'a, Option<HandoffRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,assignment_id,company_id,person_id,eligibility_code,\
                 source_assignment_version,status,created_at \
                 FROM learning_eligibility_handoffs WHERE assignment_id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_handoff).transpose().map_err(Into::into)
        })
    }

    fn certificate<'a>(
        &'a mut self,
        id: &'a str,
    ) -> LearningFuture<'a, Option<CertificateRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT c.id,c.handoff_id,c.company_id,a.branch_id,a.department_id,c.person_id,\
                 c.assignment_id,c.source_assignment_version,c.eligibility_code,c.valid_from,\
                 c.expires_on,c.completion_kind,c.evidence_ref,c.issued_by,c.created_at,\
                 revocation.id revocation_id \
                 FROM learning_certificates c \
                 JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id \
                 LEFT JOIN learning_certificate_revocations revocation ON revocation.certificate_id=c.id \
                 WHERE c.assignment_id=$1",
            )
            .bind(id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(map_certificate).transpose().map_err(Into::into)
        })
    }
}
