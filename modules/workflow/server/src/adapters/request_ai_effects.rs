//! Native Workflow-owned statements for the `request.ai.prepare` drain family.
//!
//! Exact port of the two Workflow-owned effects in
//! `modules/workflow/server/src/adapters/records-request-ai-facts.ts`:
//! `stageRunRequestAiEffect(authorizationKey, companyId)` and
//! `stageRunRequestAiEffect4(command, companyId)` are both
//! `nativeClearCommandAssertions(db, companyId, key)`, which resolves through
//! the retained `workflow_command_assertion_clear` notification kind to the
//! retained statement `DELETE FROM command_assertions WHERE command_id=?`.
//! The two effects differ only in the cleared key (the run authorization key
//! versus the result command key); the statement below serves both.
//!
//! Every statement runs on the caller's `&mut sqlx::PgConnection`; this
//! adapter never opens a pool, commits, retries or calls an AI provider. The
//! Records-owned remainder of the drain (claim/run/result/usage rows and the
//! provider call) stays with the Records owner crate; composition forwards
//! `ProjectionSideEffects::request_ai` to it. Authority is rechecked in owner
//! scope by the caller before this cleanup runs, matching the retained batch
//! order where the clear stages after the run guards.
use erpx_platform_runtime_server::public::postgres::PersistenceError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

/// Exact retained assertion-clear statement in Postgres placeholders.
pub const REQUEST_AI_ASSERTION_CLEAR_SQL: &str =
    "DELETE FROM command_assertions WHERE command_id=$1";

/// Clear the staged command assertions for one request-AI key on the caller's
/// transaction.
///
/// This is the native form of both retained `RequestAiWorkflowFacts` effects;
/// which key to clear (run authorization key or result command key) is the
/// caller's decision, exactly like the retained `stageRunRequestAiEffect` /
/// `stageRunRequestAiEffect4` split.
pub async fn clear_request_ai_assertions(
    connection: &mut PgConnection,
    command_id: &str,
) -> Result<(), BusinessError> {
    sqlx::query(REQUEST_AI_ASSERTION_CLEAR_SQL)
        .bind(command_id)
        .execute(&mut *connection)
        .await
        .map_err(clear_failed)?;
    Ok(())
}

/// Preserve serialization/deadlock classification for a caller retry while
/// keeping every other persistence failure sanitized.
fn clear_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "REQUEST_AI_ASSERTION_RETRY_REQUIRED",
            "The request AI assertion cleanup must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "REQUEST_AI_ASSERTION_WRITE_FAILED",
            "The request AI assertion cleanup could not be written.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::REQUEST_AI_ASSERTION_CLEAR_SQL;

    /// The native statement must stay byte-identical to the retained D1
    /// contract (`native-command-assertion-sql.json`), the Collaboration
    /// `WORKFLOW_ASSERTION_CLEAR_SQL` and the inline workflow command-pipeline
    /// clears, modulo the D1 `?` / Postgres `$1` placeholder dialect.
    #[test]
    fn assertion_clear_statement_matches_the_retained_contract() {
        for retained in [
            "DELETE FROM command_assertions WHERE command_id=?",
            "DELETE FROM command_assertions WHERE command_id=$1",
        ] {
            assert_eq!(
                retained.replace('?', "$1"),
                REQUEST_AI_ASSERTION_CLEAR_SQL,
                "retained contract drift"
            );
        }
    }
}
