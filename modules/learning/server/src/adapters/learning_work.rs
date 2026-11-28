//! Native Learning work read.
//!
//! The work queue is a projection of the owner's scoped assignment rows. Those
//! rows come from the same owner SQL as the workspace/detail ports — `ELIGIBLE`,
//! `ASSIGNMENT_COLUMNS` and `map_assignment` live behind
//! [`learning_workspace`](crate::application::learning::learning_workspace) — so
//! the work read inherits the current Holding/Company/branch/department scope
//! and the same eligibility witness. Results-history columns are redacted here
//! when the caller has not purchased `learn.results` history; no raw result
//! leaves the owner without that admission. No commit is performed.
use crate::application::learning::{LearningReadRepository, learning_workspace};
use crate::domain::learning::LearningActor;
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};

const RESULT_HISTORY_FIELDS: [&str; 4] = [
    "attendance",
    "score_bps",
    "result_evidence",
    "completion_reason",
];

/// Redact result-history columns on owner assignment rows. Independent of the
/// repository so the admission rule is directly unit-testable.
pub fn redact_result_history(rows: Value) -> Value {
    match rows {
        Value::Array(rows) => Value::Array(
            rows.into_iter()
                .map(|mut row| {
                    if let Some(object) = row.as_object_mut() {
                        for field in RESULT_HISTORY_FIELDS {
                            object.insert(field.to_owned(), Value::Null);
                        }
                    }
                    row
                })
                .collect(),
        ),
        other => other,
    }
}

/// Scoped Learning work rows. `include_results` reflects a purchased
/// `learn.results` history; when false, the four result columns are nulled.
pub async fn learning_work<R: LearningReadRepository>(
    repository: &mut R,
    actor: &LearningActor,
    now: i64,
    include_results: bool,
) -> Result<Value, R::Error>
where
    R::Error: From<BusinessError>,
{
    let workspace = learning_workspace(repository, actor, false, now).await?;
    let assignments = workspace
        .get("assignments")
        .cloned()
        .unwrap_or(Value::Null);
    let assignments = if include_results {
        assignments
    } else {
        redact_result_history(assignments)
    };
    Ok(json!({ "assignments": assignments }))
}
