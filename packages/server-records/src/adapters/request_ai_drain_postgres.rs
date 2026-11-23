//! Records-owned PostgreSQL statements for the native `request.ai.prepare`
//! drain. Every statement runs on the caller's `&mut PgConnection`; this
//! adapter never opens a pool, commits, retries or calls an AI provider.
//!
//! Statement shapes are the exact retained `records_stmt_*` literals from
//! `crate::adapters::native_command_sql` (requests 1-11), expressed here in
//! Postgres placeholders with typed row mapping so the drain can orchestrate
//! claim, authority rechecks, result/usage writes and sweeps in one unit.
use crate::domain::request_ai_drain::{self, DrainedActor};
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};

fn retryable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

fn map_error(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        retryable(
            "REQUEST_AI_DRAIN_RETRY_REQUIRED",
            "The request AI drain must be retried.",
        )
    } else {
        unavailable(
            "REQUEST_AI_DRAIN_WRITE_FAILED",
            "The request AI drain could not be written.",
        )
    }
}

fn require_transaction(connection: &PgConnection) -> Result<(), BusinessError> {
    if !connection.is_in_transaction() {
        return Err(request_ai_drain::transaction_required());
    }
    Ok(())
}

/// One claimed `request_ai_proposals` row with the retained columns needed
/// by the drain (claim, stale checks, provider call, result write).
#[derive(Clone, Debug)]
pub struct ClaimedProposal {
    pub id: String,
    pub company_id: String,
    pub request_id: String,
    pub source_version: i64,
    pub provider_json: String,
    pub actor_json: String,
    pub attempts: i64,
    pub lease_token: String,
}

/// Minimal retained `employee_requests` facts for the Records-scope
/// authority recheck (existence, company scope, version, requester/assignee).
#[derive(Clone, Debug)]
pub struct RequestSnapshot {
    pub id: String,
    pub company_id: String,
    pub version: i64,
    pub state: String,
    pub requester_user_id: String,
    pub assignee_user_id: Option<String>,
}

const DUE_ROWS: &str = "SELECT id FROM request_ai_proposals \
 WHERE (state='queued' OR (state='running' AND claim_until<$1)) \
 AND attempts<3 AND ($2::text IS NULL OR request_id=$2) \
 ORDER BY created_at LIMIT 2";

const CLAIM: &str = "UPDATE request_ai_proposals \
 SET state='running',lease_token=$1,claim_until=$2,attempts=attempts+1,updated_at=$3 \
 WHERE id=$4 AND attempts<3 \
 AND (state='queued' OR (state='running' AND claim_until<$5)) \
 RETURNING id,company_id,request_id,source_version,provider_json,actor_json,\
 attempts,lease_token";

const GRANT: &str = "SELECT role FROM assignments WHERE id=$1 AND user_id=$2 \
 AND company_id=$3 AND role=$4 AND ($5::bigint IS NULL OR version=$5) \
 AND revoked_at IS NULL \
 AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
 AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";

const REQUEST_ROW: &str = "SELECT id,company_id,version,state,\
 requester_user_id,assignee_user_id FROM employee_requests \
 WHERE id=$1 AND company_id=$2";

const WRITE_READY: &str = "UPDATE request_ai_proposals \
 SET state='ready',result_json=$1,error=NULL,lease_token=NULL,claim_until=NULL,updated_at=$2 \
 WHERE id=$3 AND lease_token=$4";

const WRITE_USAGE_SUCCEEDED: &str = "UPDATE request_ai_usage \
 SET state='succeeded',lease_until=0,input_tokens=$1,output_tokens=$2 WHERE id=$3";

const WRITE_CHANGED: &str = "UPDATE request_ai_proposals \
 SET state=$1,error=$2,lease_token=NULL,claim_until=NULL,updated_at=$3 \
 WHERE id=$4 AND state='running' AND lease_token=$5 RETURNING id";

const WRITE_USAGE_FAILED: &str =
    "UPDATE request_ai_usage SET state='failed',lease_until=0 WHERE id=$1";

const SWEEP_RUNNING: &str = "UPDATE request_ai_proposals \
 SET state='failed',error='Preparation was interrupted. Retry or continue manually.',\
 lease_token=NULL,claim_until=NULL \
 WHERE state='running' AND claim_until<$1 AND attempts>=3";

const SWEEP_USAGE: &str = "UPDATE request_ai_usage SET state='failed',lease_until=0 \
 WHERE state='reserved' AND id IN \
 (SELECT id FROM request_ai_proposals WHERE state IN ('failed','stale'))";

pub async fn due_rows(
    connection: &mut PgConnection,
    now: i64,
    request_id: Option<&str>,
) -> Result<Vec<String>, BusinessError> {
    require_transaction(connection)?;
    let rows = sqlx::query(DUE_ROWS)
        .bind(now)
        .bind(request_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(map_error)?;
    let mut ids = Vec::with_capacity(rows.len());
    for row in rows {
        ids.push(row.try_get::<String, _>("id").map_err(|_| {
            request_ai_drain::drain_unavailable()
        })?);
    }
    Ok(ids)
}

#[allow(clippy::too_many_arguments)]
pub async fn claim(
    connection: &mut PgConnection,
    proposal_id: &str,
    lease: &str,
    now: i64,
) -> Result<Option<ClaimedProposal>, BusinessError> {
    require_transaction(connection)?;
    let row = sqlx::query(CLAIM)
        .bind(lease)
        .bind(now + crate::domain::request_ai_drain::LEASE_WINDOW_MS)
        .bind(now)
        .bind(proposal_id)
        .bind(now)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    let lease_token: Option<String> = row.try_get("lease_token").map_err(|_| {
        request_ai_drain::drain_unavailable()
    })?;
    Ok(Some(ClaimedProposal {
        id: row.try_get("id").map_err(|_| request_ai_drain::drain_unavailable())?,
        company_id: row
            .try_get("company_id")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        request_id: row
            .try_get("request_id")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        source_version: row
            .try_get("source_version")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        provider_json: row
            .try_get("provider_json")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        actor_json: row
            .try_get("actor_json")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        attempts: row
            .try_get("attempts")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        lease_token: lease_token.unwrap_or_default(),
    }))
}

pub async fn live_grant(
    connection: &mut PgConnection,
    actor: &DrainedActor,
) -> Result<bool, BusinessError> {
    require_transaction(connection)?;
    let role: Option<String> = sqlx::query_scalar(GRANT)
        .bind(&actor.assignment_id)
        .bind(&actor.user_id)
        .bind(&actor.company_id)
        .bind(&actor.role)
        .bind(actor.assignment_version)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(role.is_some())
}

pub async fn request_snapshot(
    connection: &mut PgConnection,
    request_id: &str,
    company_id: &str,
) -> Result<Option<RequestSnapshot>, BusinessError> {
    require_transaction(connection)?;
    let row = sqlx::query(REQUEST_ROW)
        .bind(request_id)
        .bind(company_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_error)?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(RequestSnapshot {
        id: row.try_get("id").map_err(|_| request_ai_drain::drain_unavailable())?,
        company_id: row
            .try_get("company_id")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        version: row
            .try_get("version")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        state: row.try_get("state").map_err(|_| {
            request_ai_drain::drain_unavailable()
        })?,
        requester_user_id: row
            .try_get("requester_user_id")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
        assignee_user_id: row
            .try_get("assignee_user_id")
            .map_err(|_| request_ai_drain::drain_unavailable())?,
    }))
}

pub async fn write_ready(
    connection: &mut PgConnection,
    proposal_id: &str,
    lease: &str,
    result_json: &str,
    now: i64,
) -> Result<(), BusinessError> {
    require_transaction(connection)?;
    sqlx::query(WRITE_READY)
        .bind(result_json)
        .bind(now)
        .bind(proposal_id)
        .bind(lease)
        .execute(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(())
}

pub async fn write_usage_succeeded(
    connection: &mut PgConnection,
    proposal_id: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> Result<(), BusinessError> {
    require_transaction(connection)?;
    sqlx::query(WRITE_USAGE_SUCCEEDED)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(proposal_id)
        .execute(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(())
}

/// Record a `failed` or `stale` proposal. Returns `true` when the lease was
/// still held (the retained `RETURNING id` presence check); a `false`
/// return means the lease was lost and the caller must skip the paired
/// usage write, matching the retained `changed` gate.
pub async fn write_changed(
    connection: &mut PgConnection,
    proposal_id: &str,
    lease: &str,
    state: &str,
    message: &str,
    now: i64,
) -> Result<bool, BusinessError> {
    require_transaction(connection)?;
    let row = sqlx::query(WRITE_CHANGED)
        .bind(state)
        .bind(message)
        .bind(now)
        .bind(proposal_id)
        .bind(lease)
        .fetch_optional(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(row.is_some())
}

pub async fn write_usage_failed(
    connection: &mut PgConnection,
    proposal_id: &str,
) -> Result<(), BusinessError> {
    require_transaction(connection)?;
    sqlx::query(WRITE_USAGE_FAILED)
        .bind(proposal_id)
        .execute(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(())
}

pub async fn sweep_expired(connection: &mut PgConnection, now: i64) -> Result<(), BusinessError> {
    require_transaction(connection)?;
    sqlx::query(SWEEP_RUNNING)
        .bind(now)
        .execute(&mut *connection)
        .await
        .map_err(map_error)?;
    sqlx::query(SWEEP_USAGE)
        .execute(&mut *connection)
        .await
        .map_err(map_error)?;
    Ok(())
}
