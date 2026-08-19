//! Durable control-plane coordination for the global Marketing delivery pass.
//!
//! The scheduler owns only worker leases, cadence, retry state, and a
//! dead-letter record. Delivery claims, consent checks, provider results, and
//! holding business state remain in the physically separate holding write
//! databases handled by `SqlxMarketingDeliveryStore`.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

pub const MARKETING_DELIVERY_SCHEDULER_JOB_KEY: &str = "marketing_delivery.global";
pub const MARKETING_DELIVERY_SCHEDULER_DEFAULT_CADENCE_SECONDS: u64 = 60;
pub const MARKETING_DELIVERY_SCHEDULER_MAX_CADENCE_SECONDS: u64 = 86_400;
pub const MARKETING_DELIVERY_SCHEDULER_MAX_ATTEMPTS: u16 = 25;
pub const MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_BASE_SECONDS: u64 = 5;
pub const MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_MAX_SECONDS: u64 = 300;
pub const MARKETING_DELIVERY_SCHEDULER_MIN_LEASE_SECONDS: u64 = 1;
pub const MARKETING_DELIVERY_SCHEDULER_MAX_LEASE_SECONDS: u64 = 3_600;
pub const MARKETING_DELIVERY_SCHEDULER_MAX_JOB_KEY_BYTES: usize = 200;
pub const MARKETING_DELIVERY_SCHEDULER_MAX_ERROR_BYTES: usize = 1_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingDeliverySchedulerError {
    #[error("invalid marketing delivery scheduler request: {0}")]
    InvalidRequest(String),
    #[error("marketing delivery scheduler database error: {0}")]
    Database(String),
    #[error("marketing delivery scheduler claim was lost")]
    ClaimLost,
    #[error("marketing delivery scheduler record was not found")]
    NotFound,
    #[error("marketing delivery scheduler request conflicts with an existing job")]
    RequestConflict,
}

impl From<sqlx::Error> for MarketingDeliverySchedulerError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingDeliverySchedulerPolicy {
    pub cadence_seconds: u64,
    pub max_attempts: u16,
    pub backoff_base_seconds: u64,
    pub backoff_max_seconds: u64,
}

impl MarketingDeliverySchedulerPolicy {
    pub const fn default_policy() -> Self {
        Self {
            cadence_seconds: MARKETING_DELIVERY_SCHEDULER_DEFAULT_CADENCE_SECONDS,
            max_attempts: MARKETING_DELIVERY_SCHEDULER_MAX_ATTEMPTS,
            backoff_base_seconds: MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_BASE_SECONDS,
            backoff_max_seconds: MARKETING_DELIVERY_SCHEDULER_DEFAULT_BACKOFF_MAX_SECONDS,
        }
    }

    pub fn validate(self) -> Result<Self, MarketingDeliverySchedulerError> {
        if !(1..=MARKETING_DELIVERY_SCHEDULER_MAX_CADENCE_SECONDS).contains(&self.cadence_seconds)
            || !(1..=MARKETING_DELIVERY_SCHEDULER_MAX_ATTEMPTS).contains(&self.max_attempts)
            || !(1..=MARKETING_DELIVERY_SCHEDULER_MAX_CADENCE_SECONDS)
                .contains(&self.backoff_base_seconds)
            || !(1..=MARKETING_DELIVERY_SCHEDULER_MAX_CADENCE_SECONDS)
                .contains(&self.backoff_max_seconds)
            || self.backoff_base_seconds > self.backoff_max_seconds
        {
            return Err(MarketingDeliverySchedulerError::InvalidRequest(
                "marketing delivery scheduler policy is outside its bound".into(),
            ));
        }
        Ok(self)
    }

    fn retry_at(self, attempt: u16, now: DateTime<Utc>) -> DateTime<Utc> {
        let shift = u32::from(attempt.saturating_sub(1)).min(63);
        let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
        let seconds = self
            .backoff_base_seconds
            .saturating_mul(multiplier)
            .min(self.backoff_max_seconds);
        now + Duration::seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingDeliveryScheduleRequest {
    pub job_key: String,
    pub available_at: DateTime<Utc>,
    pub policy: MarketingDeliverySchedulerPolicy,
}

impl MarketingDeliveryScheduleRequest {
    pub fn default_global(now: DateTime<Utc>) -> Self {
        Self {
            job_key: MARKETING_DELIVERY_SCHEDULER_JOB_KEY.into(),
            available_at: now,
            policy: MarketingDeliverySchedulerPolicy::default_policy(),
        }
    }

    pub fn validate(&self) -> Result<(), MarketingDeliverySchedulerError> {
        if self.job_key.trim().is_empty()
            || self.job_key.len() > MARKETING_DELIVERY_SCHEDULER_MAX_JOB_KEY_BYTES
            || self
                .job_key
                .chars()
                .any(|character| !(character.is_ascii_alphanumeric() || ".:_-".contains(character)))
        {
            return Err(MarketingDeliverySchedulerError::InvalidRequest(
                "marketing delivery scheduler job key is invalid".into(),
            ));
        }
        self.policy.validate()?;
        Ok(())
    }

    fn request_hash(&self) -> Result<String, MarketingDeliverySchedulerError> {
        let bytes = serde_json::to_vec(&(&self.job_key, self.available_at, &self.policy)).map_err(
            |_| {
                MarketingDeliverySchedulerError::InvalidRequest(
                    "marketing delivery scheduler request cannot be serialized".into(),
                )
            },
        )?;
        let mut digest = Sha256::new();
        digest.update(bytes);
        Ok(hex::encode(digest.finalize()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingDeliverySchedulerLease {
    pub claim_id: Uuid,
    pub worker_id: Uuid,
    pub claimed_at: DateTime<Utc>,
    pub lease_until: DateTime<Utc>,
}

impl MarketingDeliverySchedulerLease {
    pub fn new(
        claim_id: Uuid,
        worker_id: Uuid,
        claimed_at: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Self, MarketingDeliverySchedulerError> {
        if claim_id.is_nil()
            || worker_id.is_nil()
            || lease < Duration::seconds(MARKETING_DELIVERY_SCHEDULER_MIN_LEASE_SECONDS as i64)
            || lease > Duration::seconds(MARKETING_DELIVERY_SCHEDULER_MAX_LEASE_SECONDS as i64)
        {
            return Err(MarketingDeliverySchedulerError::InvalidRequest(
                "marketing delivery scheduler lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            claim_id,
            worker_id,
            claimed_at,
            lease_until: claimed_at + lease,
        })
    }

    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        now < self.lease_until
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingDeliverySchedulerJob {
    pub id: Uuid,
    pub job_key: String,
    pub state: MarketingDeliverySchedulerState,
    pub available_at: DateTime<Utc>,
    pub policy: MarketingDeliverySchedulerPolicy,
    pub attempts: u16,
    pub current_claim_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub lease_until: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub last_succeeded_at: Option<DateTime<Utc>>,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingDeliverySchedulerState {
    Pending,
    Claimed,
    Retryable,
    DeadLetter,
}

impl MarketingDeliverySchedulerState {
    fn parse(value: &str) -> Result<Self, MarketingDeliverySchedulerError> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "retryable" => Ok(Self::Retryable),
            "dead_letter" => Ok(Self::DeadLetter),
            _ => Err(MarketingDeliverySchedulerError::InvalidRequest(
                "persisted marketing delivery scheduler state is invalid".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingDeliverySchedulerClaim {
    pub job: MarketingDeliverySchedulerJob,
    pub lease: MarketingDeliverySchedulerLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketingDeliverySchedulerFailureOutcome {
    Retry { available_at: DateTime<Utc> },
    DeadLetter { dead_lettered_at: DateTime<Utc> },
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingDeliverySchedulerRepository {
    pool: PgPool,
}

impl SqlxMarketingDeliverySchedulerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_scheduled(
        &self,
        request: MarketingDeliveryScheduleRequest,
        now: DateTime<Utc>,
    ) -> Result<MarketingDeliverySchedulerJob, MarketingDeliverySchedulerError> {
        request.validate()?;
        let request_hash = request.request_hash()?;
        let id = Uuid::new_v4();
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.marketing_delivery_scheduler_jobs
                (id, job_key, state, available_at, cadence_seconds, max_attempts,
                 backoff_base_seconds, backoff_max_seconds, request_hash,
                 version, created_at, updated_at)
             VALUES ($1,$2,'pending',$3,$4,$5,$6,$7,$8,1,$9,$9)
             ON CONFLICT (job_key) DO NOTHING
             RETURNING id",
        )
        .bind(id)
        .bind(&request.job_key)
        .bind(request.available_at)
        .bind(i64::try_from(request.policy.cadence_seconds).map_err(|_| {
            MarketingDeliverySchedulerError::InvalidRequest("cadence is too large".into())
        })?)
        .bind(i32::from(request.policy.max_attempts))
        .bind(
            i64::try_from(request.policy.backoff_base_seconds).map_err(|_| {
                MarketingDeliverySchedulerError::InvalidRequest("backoff is too large".into())
            })?,
        )
        .bind(
            i64::try_from(request.policy.backoff_max_seconds).map_err(|_| {
                MarketingDeliverySchedulerError::InvalidRequest("backoff is too large".into())
            })?,
        )
        .bind(&request_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        let job_id = match inserted {
            Some(row) => row.try_get("id")?,
            None => {
                let row = sqlx::query(
                    "SELECT id
                     FROM erpx_control.marketing_delivery_scheduler_jobs
                     WHERE job_key=$1",
                )
                .bind(&request.job_key)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(MarketingDeliverySchedulerError::NotFound)?;
                let existing = self.get(row.try_get("id")?).await?;
                // `available_at` is the initial due time, not recurring-job
                // identity. Once the global job exists its durable state owns
                // subsequent cadence; process restarts may safely ensure the
                // same key and policy without rewriting that state.
                if existing.policy != request.policy {
                    return Err(MarketingDeliverySchedulerError::RequestConflict);
                }
                return Ok(existing);
            }
        };
        self.get(job_id).await
    }

    pub async fn get(
        &self,
        job_id: Uuid,
    ) -> Result<MarketingDeliverySchedulerJob, MarketingDeliverySchedulerError> {
        if job_id.is_nil() {
            return Err(MarketingDeliverySchedulerError::InvalidRequest(
                "scheduler job id is required".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT id, job_key, state, available_at, cadence_seconds,
                    max_attempts, backoff_base_seconds, backoff_max_seconds,
                    attempts, current_claim_id, worker_id, lease_until,
                    last_error, dead_lettered_at, last_succeeded_at, version
             FROM erpx_control.marketing_delivery_scheduler_jobs
             WHERE id=$1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(MarketingDeliverySchedulerError::NotFound)?;
        job_from_row(&row)
    }

    pub async fn claim_due(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<Option<MarketingDeliverySchedulerClaim>, MarketingDeliverySchedulerError> {
        if worker_id.is_nil() {
            return Err(MarketingDeliverySchedulerError::InvalidRequest(
                "scheduler worker id is required".into(),
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT id, job_key, state, available_at, cadence_seconds,
                    max_attempts, backoff_base_seconds, backoff_max_seconds,
                    attempts, current_claim_id, worker_id, lease_until,
                    last_error, dead_lettered_at, last_succeeded_at, version
             FROM erpx_control.marketing_delivery_scheduler_jobs
             WHERE ((state IN ('pending','retryable') AND available_at <= $1)
                 OR (state='claimed' AND lease_until IS NOT NULL AND lease_until <= $1))
             ORDER BY available_at, id
             LIMIT 1
             FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let mut job = job_from_row(&row)?;
        if job.state == MarketingDeliverySchedulerState::Claimed
            && let Some(previous_claim_id) = job.current_claim_id
        {
            sqlx::query(
                "UPDATE erpx_control.marketing_delivery_scheduler_claims
                 SET state='stale', completed_at=$2, outcome='lease_expired'
                 WHERE claim_id=$1 AND state='active'",
            )
            .bind(previous_claim_id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        if job.attempts >= job.policy.max_attempts {
            sqlx::query(
                "UPDATE erpx_control.marketing_delivery_scheduler_jobs
                 SET state='dead_letter', current_claim_id=NULL, worker_id=NULL,
                     lease_until=NULL, dead_lettered_at=COALESCE(dead_lettered_at,$2),
                     updated_at=$2, version=version+1
                 WHERE id=$1",
            )
            .bind(job.id)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
            return Ok(None);
        }
        let claim_id = Uuid::new_v4();
        let lease = MarketingDeliverySchedulerLease::new(claim_id, worker_id, now, lease)?;
        sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_jobs
             SET state='claimed', current_claim_id=$2, worker_id=$3,
                 lease_until=$4, attempts=attempts+1, updated_at=$5, version=version+1
             WHERE id=$1",
        )
        .bind(job.id)
        .bind(claim_id)
        .bind(worker_id)
        .bind(lease.lease_until)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.marketing_delivery_scheduler_claims
                (claim_id, job_id, worker_id, claimed_at, lease_until, state)
             VALUES ($1,$2,$3,$4,$5,'active')",
        )
        .bind(claim_id)
        .bind(job.id)
        .bind(worker_id)
        .bind(now)
        .bind(lease.lease_until)
        .execute(&mut *transaction)
        .await?;
        job.state = MarketingDeliverySchedulerState::Claimed;
        job.current_claim_id = Some(claim_id);
        job.worker_id = Some(worker_id);
        job.lease_until = Some(lease.lease_until);
        job.attempts = job.attempts.saturating_add(1);
        job.version = job.version.saturating_add(1);
        transaction.commit().await?;
        Ok(Some(MarketingDeliverySchedulerClaim { job, lease }))
    }

    pub async fn renew(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        lease: Duration,
    ) -> Result<MarketingDeliverySchedulerLease, MarketingDeliverySchedulerError> {
        let renewed = MarketingDeliverySchedulerLease::new(
            claim.lease.claim_id,
            claim.lease.worker_id,
            now,
            lease,
        )?;
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_jobs
             SET lease_until=$5, updated_at=$4, version=version+1
             WHERE id=$1 AND state='claimed' AND current_claim_id=$2
               AND worker_id=$3 AND lease_until > $4",
        )
        .bind(claim.job.id)
        .bind(claim.lease.claim_id)
        .bind(claim.lease.worker_id)
        .bind(now)
        .bind(renewed.lease_until)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_claims
             SET lease_until=$4
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim.lease.claim_id)
        .bind(claim.job.id)
        .bind(claim.lease.worker_id)
        .bind(renewed.lease_until)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        transaction.commit().await?;
        Ok(renewed)
    }

    pub async fn complete(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
    ) -> Result<(), MarketingDeliverySchedulerError> {
        let next =
            now + Duration::seconds(i64::try_from(claim.job.policy.cadence_seconds).map_err(
                |_| MarketingDeliverySchedulerError::InvalidRequest("cadence is too large".into()),
            )?);
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_jobs
             SET state='pending', available_at=$5, current_claim_id=NULL,
                 worker_id=NULL, lease_until=NULL, attempts=0, last_error=NULL,
                 dead_lettered_at=NULL, last_succeeded_at=$4, updated_at=$4,
                 version=version+1
             WHERE id=$1 AND state='claimed' AND current_claim_id=$2
               AND worker_id=$3 AND lease_until > $4",
        )
        .bind(claim.job.id)
        .bind(claim.lease.claim_id)
        .bind(claim.lease.worker_id)
        .bind(now)
        .bind(next)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_claims
             SET state='completed', completed_at=$4, outcome='succeeded'
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim.lease.claim_id)
        .bind(claim.job.id)
        .bind(claim.lease.worker_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn fail(
        &self,
        claim: &MarketingDeliverySchedulerClaim,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<MarketingDeliverySchedulerFailureOutcome, MarketingDeliverySchedulerError> {
        let error = bounded_error(error);
        let terminal = claim.job.attempts >= claim.job.policy.max_attempts;
        let available_at = (!terminal).then(|| claim.job.policy.retry_at(claim.job.attempts, now));
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_jobs
             SET state=CASE WHEN $5 THEN 'dead_letter' ELSE 'retryable' END,
                 available_at=COALESCE($6, available_at), current_claim_id=NULL,
                 worker_id=NULL, lease_until=NULL, last_error=$4,
                 dead_lettered_at=CASE WHEN $5 THEN $3 ELSE NULL END,
                 updated_at=$3, version=version+1
             WHERE id=$1 AND state='claimed' AND current_claim_id=$2
               AND worker_id=$7 AND lease_until > $3",
        )
        .bind(claim.job.id)
        .bind(claim.lease.claim_id)
        .bind(now)
        .bind(error)
        .bind(terminal)
        .bind(available_at)
        .bind(claim.lease.worker_id)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_claims
             SET state=$4, completed_at=$5, outcome=$4
             WHERE claim_id=$1 AND job_id=$2 AND worker_id=$3 AND state='active'",
        )
        .bind(claim.lease.claim_id)
        .bind(claim.job.id)
        .bind(claim.lease.worker_id)
        .bind(if terminal { "dead_letter" } else { "failed" })
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::ClaimLost);
        }
        transaction.commit().await?;
        if let Some(available_at) = available_at {
            Ok(MarketingDeliverySchedulerFailureOutcome::Retry { available_at })
        } else {
            Ok(MarketingDeliverySchedulerFailureOutcome::DeadLetter {
                dead_lettered_at: now,
            })
        }
    }

    pub async fn replay_dead_letter(
        &self,
        job_key: &str,
        now: DateTime<Utc>,
    ) -> Result<(), MarketingDeliverySchedulerError> {
        validate_job_key(job_key)?;
        let changed = sqlx::query(
            "UPDATE erpx_control.marketing_delivery_scheduler_jobs
             SET state='pending', available_at=$2, attempts=0,
                 current_claim_id=NULL, worker_id=NULL, lease_until=NULL,
                 last_error=NULL, dead_lettered_at=NULL, updated_at=$2,
                 version=version+1
             WHERE job_key=$1 AND state='dead_letter'",
        )
        .bind(job_key)
        .bind(now)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(MarketingDeliverySchedulerError::NotFound);
        }
        Ok(())
    }
}

fn validate_job_key(job_key: &str) -> Result<(), MarketingDeliverySchedulerError> {
    if job_key.trim().is_empty()
        || job_key.len() > MARKETING_DELIVERY_SCHEDULER_MAX_JOB_KEY_BYTES
        || job_key
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || ".:_-".contains(character)))
    {
        return Err(MarketingDeliverySchedulerError::InvalidRequest(
            "marketing delivery scheduler job key is invalid".into(),
        ));
    }
    Ok(())
}

fn bounded_error(error: &str) -> String {
    if error.is_empty()
        || error.len() > MARKETING_DELIVERY_SCHEDULER_MAX_ERROR_BYTES
        || error.chars().any(char::is_control)
    {
        "marketing_delivery_scheduler_failure".into()
    } else {
        error.to_owned()
    }
}

fn job_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<MarketingDeliverySchedulerJob, MarketingDeliverySchedulerError> {
    let attempts = positive_u16_or_zero(row.try_get("attempts")?)?;
    let policy = MarketingDeliverySchedulerPolicy {
        cadence_seconds: positive_u64(row.try_get("cadence_seconds")?, "cadence")?,
        max_attempts: positive_u16(row.try_get("max_attempts")?, "max attempts")?,
        backoff_base_seconds: positive_u64(row.try_get("backoff_base_seconds")?, "backoff base")?,
        backoff_max_seconds: positive_u64(row.try_get("backoff_max_seconds")?, "backoff max")?,
    };
    policy.validate()?;
    Ok(MarketingDeliverySchedulerJob {
        id: row.try_get("id")?,
        job_key: row.try_get("job_key")?,
        state: MarketingDeliverySchedulerState::parse(&row.try_get::<String, _>("state")?)?,
        available_at: row.try_get("available_at")?,
        policy,
        attempts,
        current_claim_id: row.try_get("current_claim_id")?,
        worker_id: row.try_get("worker_id")?,
        lease_until: row.try_get("lease_until")?,
        last_error: row.try_get("last_error")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        last_succeeded_at: row.try_get("last_succeeded_at")?,
        version: positive_u64(row.try_get("version")?, "version")?,
    })
}

fn positive_u16(value: i32, field: &str) -> Result<u16, MarketingDeliverySchedulerError> {
    u16::try_from(value)
        .map_err(|_| MarketingDeliverySchedulerError::InvalidRequest(format!("{field} is invalid")))
}

fn positive_u16_or_zero(value: i32) -> Result<u16, MarketingDeliverySchedulerError> {
    u16::try_from(value).map_err(|_| {
        MarketingDeliverySchedulerError::InvalidRequest("attempt count is invalid".into())
    })
}

fn positive_u64(value: i64, field: &str) -> Result<u64, MarketingDeliverySchedulerError> {
    u64::try_from(value)
        .map_err(|_| MarketingDeliverySchedulerError::InvalidRequest(format!("{field} is invalid")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_and_request_bounds_are_fail_closed() {
        let now = Utc::now();
        assert!(
            MarketingDeliveryScheduleRequest::default_global(now)
                .validate()
                .is_ok()
        );
        let mut request = MarketingDeliveryScheduleRequest::default_global(now);
        request.job_key = "bad key".into();
        assert!(request.validate().is_err());
        let mut policy = MarketingDeliverySchedulerPolicy::default_policy();
        policy.max_attempts = 0;
        assert!(policy.validate().is_err());
    }

    #[test]
    fn lease_expiry_is_half_open_and_errors_are_redacted_to_a_bound() {
        let now = Utc::now();
        let lease = MarketingDeliverySchedulerLease::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            Duration::seconds(5),
        )
        .expect("lease");
        assert!(lease.is_active_at(now));
        assert!(!lease.is_active_at(lease.lease_until));
        assert_eq!(
            bounded_error("x\nsecret"),
            "marketing_delivery_scheduler_failure"
        );
    }

    #[test]
    fn schedule_idempotency_binds_available_time() {
        let now = Utc::now();
        let first = MarketingDeliveryScheduleRequest::default_global(now);
        let mut shifted = first.clone();
        shifted.available_at += Duration::seconds(1);
        assert_ne!(
            first.request_hash().expect("first request hash"),
            shifted.request_hash().expect("shifted request hash")
        );
    }
}
