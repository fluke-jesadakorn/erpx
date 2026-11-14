//! Durable PMS day-close job adapter.
//!
//! The retained `drainPms` worker reads up to four pending/running `pms_jobs`,
//! runs the worker-only `night.step` in its own serializable unit with the job's
//! stored authority evidence, and records a blocked job with its error code.
//! Composition owns the Account re-resolution, the clock, the loop and the
//! shutdown signal; this adapter owns only the bounded job turn.
use crate::adapters::night_audit_postgres::PgPmsNightAuditRepository;
use crate::application::night_audit::pms_night_audit_step;
use crate::domain::night_audit::{
    PmsNightAuditActor, PmsNightAuditCommand, PmsNightAuditOperation, PmsNightAuditResult,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, CommandKey};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Retained `readPmsJobs` bound: one drain turn reads at most four jobs.
pub const MAX_NIGHT_JOBS: i64 = 4;

/// One pending/running day-close job. `authority_json` is retained evidence of
/// the selected authority identity, never a permission or a session proof:
/// composition re-resolves the current Account before rebuilding the actor.
#[derive(Clone, Debug)]
pub struct PmsNightAuditJob {
    pub id: String,
    pub property_id: String,
    pub authority_json: String,
}

/// Read the bounded day-close jobs for one optional property or job id.
pub async fn read_night_audit_jobs(
    c: &mut PgConnection,
    property_id: Option<&str>,
    job_id: Option<&str>,
) -> Result<Vec<PmsNightAuditJob>, Error> {
    let rows = sqlx::query(
        "SELECT id,property_id,actor_json FROM pms_jobs \
         WHERE state IN ('pending','running') \
         AND ($1::text IS NULL OR property_id=$1) \
         AND ($2::text IS NULL OR id=$2) \
         ORDER BY updated_at LIMIT $3",
    )
    .bind(property_id)
    .bind(job_id)
    .bind(MAX_NIGHT_JOBS)
    .fetch_all(&mut *c)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(PmsNightAuditJob {
                id: row.try_get("id")?,
                property_id: row.try_get("property_id")?,
                authority_json: row.try_get("actor_json")?,
            })
        })
        .collect()
}

/// Retained `updatePmsJobs2`/`updatePmsRecords` failure recording: the job and
/// its night record are marked blocked with the same error code. Returns
/// whether the job row was still eligible (not already completed).
pub async fn mark_night_audit_job_blocked(
    c: &mut PgConnection,
    job_id: &str,
    code: &str,
    now: i64,
) -> Result<bool, Error> {
    let job = sqlx::query(
        "UPDATE pms_jobs SET state='blocked',error_code=$1,updated_at=$2 \
         WHERE id=$3 AND state<>'completed'",
    )
    .bind(code)
    .bind(now)
    .bind(job_id)
    .execute(&mut *c)
    .await?
    .rows_affected();
    sqlx::query(
        "UPDATE pms_records SET \
         data_json=jsonb_set(jsonb_set(data_json::jsonb,'{state}','\"blocked\"'),\
         '{message}',to_jsonb($1::text))::text,\
         version=version+1,updated_at=$2 \
         WHERE id=$3 AND data_json::jsonb->>'state'<>'completed'",
    )
    .bind(code)
    .bind(now)
    .bind(job_id)
    .execute(&mut *c)
    .await?;
    Ok(job == 1)
}

/// Outcome of one bounded day-close job turn.
#[derive(Clone, Debug)]
pub enum PmsNightAuditSettlement {
    Completed(PmsNightAuditResult),
    /// The job and its night record were marked blocked with this error code.
    Blocked {
        code: String,
    },
    /// The retained `PMS_CONFLICT` sweep, or retryable contention: leave the
    /// job pending for the next drain turn.
    Retry,
}

fn step_command(job: &PmsNightAuditJob) -> PmsNightAuditCommand {
    PmsNightAuditCommand {
        operation: PmsNightAuditOperation::Step,
        property_id: job.property_id.clone(),
        expected_version: None,
        night_id: Some(job.id.clone()),
        worker: true,
    }
}

async fn record_blocked(
    unit: &PgUnitOfWork,
    job_id: &str,
    code: String,
    now: i64,
) -> Result<(), Error> {
    let job_id = job_id.to_owned();
    unit.run(TransactionOptions::default(), move |c| {
        let job_id = job_id.clone();
        let code = code.clone();
        Box::pin(async move {
            mark_night_audit_job_blocked(c, &job_id, &code, now).await?;
            Ok(())
        })
    })
    .await
}

/// Run one retained day-close job turn in its own serializable unit.
pub async fn run_night_audit_job(
    unit: &PgUnitOfWork,
    job: &PmsNightAuditJob,
    actor: &PmsNightAuditActor,
    key: &CommandKey,
    now: i64,
) -> Result<PmsNightAuditSettlement, Error> {
    let actor = actor.clone();
    let key = key.clone();
    let command = step_command(job);
    let result = unit
        .run(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let key = key.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgPmsNightAuditRepository::new(c);
                pms_night_audit_step(&actor, &key, &command, &mut repository, now).await
            })
        })
        .await;
    match result {
        Ok(result) => Ok(PmsNightAuditSettlement::Completed(result)),
        Err(PgTransactionError::Business(error)) if error.code == "PMS_CONFLICT" => {
            Ok(PmsNightAuditSettlement::Retry)
        }
        Err(PgTransactionError::Business(error)) => {
            record_blocked(unit, &job.id, error.code.to_owned(), now).await?;
            Ok(PmsNightAuditSettlement::Blocked {
                code: error.code.to_owned(),
            })
        }
        Err(PgTransactionError::Persistence(error)) if error.retryable() => {
            Ok(PmsNightAuditSettlement::Retry)
        }
        Err(PgTransactionError::Persistence(_)) => {
            record_blocked(unit, &job.id, "PMS_JOB_FAILED".to_owned(), now).await?;
            Ok(PmsNightAuditSettlement::Blocked {
                code: "PMS_JOB_FAILED".to_owned(),
            })
        }
    }
}
