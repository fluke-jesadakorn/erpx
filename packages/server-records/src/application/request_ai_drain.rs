//! Native `request.ai.prepare` drain use case.
//!
//! The caller owns the single serializable Holding transaction and the
//! commit/rollback. This layer claims each due proposal, rechecks Records
//! authority (live assignment grant plus the requester's/assignee's own
//! request row and version), calls the injected provider, writes the
//! result/usage rows and clears the two Workflow-owned assertion keys in the
//! same unit. A per-item stale/failed outcome is recorded on the proposal;
//! only persistence failures abort the drain for a caller retry.
use crate::adapters::request_ai_drain_postgres as postgres;
use crate::domain::request_ai_drain as domain;
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};
use std::{future::Future, pin::Pin};

/// Provider output for one claimed proposal. `result_json` must satisfy the
/// retained `requestAiResult` shape; tokens are bounded non-negative counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderOutput {
    pub result_json: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

impl ProviderOutput {
    pub fn new(result_json: String, input_tokens: i64, output_tokens: i64) -> Self {
        Self {
            result_json,
            input_tokens,
            output_tokens,
        }
    }
}

pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderOutput, BusinessError>> + Send + 'a>>;

/// Owner-injected AI call. Composition supplies the real transport; the
/// focused PG test supplies a canned success/failure. The provider never
/// touches the database.
pub trait RequestAiProvider: Send + Sync {
    fn generate<'a>(
        &'a self,
        proposal: &'a postgres::ClaimedProposal,
        request: &'a postgres::RequestSnapshot,
    ) -> ProviderFuture<'a>;
}

/// Per-drain counts. A replay after a completed drain reports zeros.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrainOutcome {
    pub processed: usize,
    pub succeeded: usize,
    pub stale: usize,
    pub failed: usize,
}

fn failed_message(error: &BusinessError) -> &'static str {
    if domain::is_stale_code(error.code) {
        error.message
    } else {
        "AI could not prepare a valid suggestion. Continue manually or retry."
    }
}

fn failure_state(error: &BusinessError) -> &'static str {
    if domain::is_stale_code(error.code) {
        "stale"
    } else {
        "failed"
    }
}

async fn run_one(
    connection: &mut PgConnection,
    proposal_id: &str,
    now: i64,
    provider: &impl RequestAiProvider,
) -> Result<Option<&'static str>, BusinessError> {
    let lease = domain::lease_token(now, proposal_id);
    let Some(claimed) = postgres::claim(connection, proposal_id, &lease, now).await? else {
        return Ok(None);
    };
    let outcome = run_claimed(connection, &claimed, &lease, now, provider).await?;
    Ok(Some(outcome))
}

async fn run_claimed(
    connection: &mut PgConnection,
    claimed: &postgres::ClaimedProposal,
    lease: &str,
    now: i64,
    provider: &impl RequestAiProvider,
) -> Result<&'static str, BusinessError> {
    let actor = match domain::parse_actor(&claimed.actor_json) {
        Ok(actor) => actor,
        Err(error) => {
            return record_failure(connection, &claimed.id, lease, "failed", error.message, now)
                .await
                .map(|()| "failed");
        }
    };
    if actor.company_id != claimed.company_id
        || !postgres::live_grant(connection, &actor).await?
    {
        return record_stale(
            connection,
            &claimed.id,
            lease,
            "Your access changed. Review the request before retrying.",
            now,
        )
        .await
        .map(|()| "stale");
    }
    let Some(snapshot) = postgres::request_snapshot(
        connection,
        &claimed.request_id,
        &claimed.company_id,
    )
    .await?
    else {
        return record_stale(
            connection,
            &claimed.id,
            lease,
            "The request or provider changed. Prepare a fresh suggestion.",
            now,
        )
        .await
        .map(|()| "stale");
    };
    let allowed = snapshot.requester_user_id == actor.user_id
        || snapshot.assignee_user_id.as_deref() == Some(actor.user_id.as_str());
    if !allowed {
        return record_stale(
            connection,
            &claimed.id,
            lease,
            "Only a person preparing or handling this step can request AI suggestions.",
            now,
        )
        .await
        .map(|()| "stale");
    }
    if snapshot.version != claimed.source_version {
        return record_stale(
            connection,
            &claimed.id,
            lease,
            "The request or provider changed. Prepare a fresh suggestion.",
            now,
        )
        .await
        .map(|()| "stale");
    }
    let output = match provider.generate(claimed, &snapshot).await {
        Ok(output) => output,
        Err(error) => {
            let state = failure_state(&error);
            let message = failed_message(&error);
            return record_failure(connection, &claimed.id, lease, state, message, now)
                .await
                .map(|()| state);
        }
    };
    if output.input_tokens < 0 || output.output_tokens < 0 {
        return record_failure(
            connection,
            &claimed.id,
            lease,
            "failed",
            "AI could not prepare a valid suggestion. Continue manually or retry.",
            now,
        )
        .await
        .map(|()| "failed");
    }
    if domain::validate_result_json(&output.result_json).is_err() {
        return record_failure(
            connection,
            &claimed.id,
            lease,
            "failed",
            "AI could not prepare a valid suggestion. Continue manually or retry.",
            now,
        )
        .await
        .map(|()| "failed");
    }
    postgres::write_ready(connection, &claimed.id, lease, &output.result_json, now).await?;
    postgres::write_usage_succeeded(
        connection,
        &claimed.id,
        output.input_tokens,
        output.output_tokens,
    )
    .await?;
    erpx_server_workflow::public::request_ai_effects::clear_request_ai_assertions(
        connection,
        &domain::run_auth_key(lease),
    )
    .await?;
    erpx_server_workflow::public::request_ai_effects::clear_request_ai_assertions(
        connection,
        &domain::result_command(lease),
    )
    .await?;
    Ok("ready")
}

async fn record_stale(
    connection: &mut PgConnection,
    proposal_id: &str,
    lease: &str,
    message: &'static str,
    now: i64,
) -> Result<(), BusinessError> {
    record_failure(connection, proposal_id, lease, "stale", message, now).await
}

async fn record_failure(
    connection: &mut PgConnection,
    proposal_id: &str,
    lease: &str,
    state: &str,
    message: &str,
    now: i64,
) -> Result<(), BusinessError> {
    if postgres::write_changed(connection, proposal_id, lease, state, message, now).await? {
        postgres::write_usage_failed(connection, proposal_id).await?;
    }
    Ok(())
}

/// Drain due `request_ai_proposals` on the caller's transaction.
///
/// `request_id` filters one request (`None` drains every due proposal);
/// `now` is the caller clock in milliseconds. The provider is called once
/// per claimed row while the lease is held. Sweeps for expired running
/// proposals and orphaned reserved usage always run in the same unit,
/// matching the retained `drainRequestAi` tail.
pub async fn drain_request_ai(
    connection: &mut PgConnection,
    request_id: Option<&str>,
    now: i64,
    provider: &impl RequestAiProvider,
) -> Result<DrainOutcome, BusinessError> {
    let filter = domain::validate_request_filter(request_id)?;
    let now = domain::validate_now(now)?;
    if !connection.is_in_transaction() {
        return Err(domain::transaction_required());
    }
    let due = postgres::due_rows(connection, now, filter.as_deref()).await?;
    let mut outcome = DrainOutcome::default();
    for proposal_id in due {
        match run_one(connection, &proposal_id, now, provider).await? {
            None => {}
            Some("ready") => {
                outcome.processed += 1;
                outcome.succeeded += 1;
            }
            Some("stale") => {
                outcome.processed += 1;
                outcome.stale += 1;
            }
            Some(_) => {
                outcome.processed += 1;
                outcome.failed += 1;
            }
        }
    }
    postgres::sweep_expired(connection, now).await?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        output: Result<ProviderOutput, BusinessError>,
    }

    impl RequestAiProvider for StaticProvider {
        fn generate<'a>(
            &'a self,
            _proposal: &'a postgres::ClaimedProposal,
            _request: &'a postgres::RequestSnapshot,
        ) -> ProviderFuture<'a> {
            let output = self.output.clone();
            Box::pin(async move { output })
        }
    }

    #[test]
    fn failure_classification_matches_the_retained_stale_list() {
        let stale = BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "AI_STALE",
            "stale",
        );
        assert_eq!(failure_state(&stale), "stale");
        assert_eq!(failed_message(&stale), "stale");
        let failed = BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "AI_UNAVAILABLE",
            "down",
        );
        assert_eq!(failure_state(&failed), "failed");
        assert_eq!(
            failed_message(&failed),
            "AI could not prepare a valid suggestion. Continue manually or retry."
        );
    }
}
