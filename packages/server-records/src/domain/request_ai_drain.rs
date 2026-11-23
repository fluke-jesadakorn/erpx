//! Pure Records-owned bounds for the native `request.ai.prepare` drain.
//!
//! The drain reproduces the retained
//! `packages/server-records/src/application/request-ai.ts`
//! `drainRequestAi` / `runRequestAi` DB behavior on the caller-owned
//! serializable Holding transaction:
//!
//! - due-row selection (`queued` or expired `running`, `attempts<3`, limit 2,
//!   optional `request_id` filter),
//! - lease claim (`records_stmt_5` shape, `running` + fresh `lease_token`),
//! - Records-scope authority recheck (live assignment grant plus the
//!   requester's/assignee's own request row and version),
//! - provider call through an injected port (no pool/commit/retry inside),
//! - result/usage writes (`records_stmt_6`/`records_stmt_7` on success,
//!   `records_stmt_8`/`records_stmt_9` on stale/failed),
//! - the two Workflow-owned assertion clears (run authorization key and
//!   result command key) via the sibling
//!   `erpx_server_workflow::public::request_ai_effects` port,
//! - the unconditional sweeps (`records_stmt_10`/`records_stmt_11`).
//!
//! No SQL and no IO live here. Production module gates, service scope,
//! quota/capacity guards and IAM mutation guards stay on the retained
//! TypeScript path until the full cutover; this slice covers only the
//! Records-owned claim/provider/result/usage/clear unit.
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Rows claimed per drain pass, matching the retained `LIMIT 2`.
pub const DRAIN_LIMIT: i64 = 2;
/// Lease window in milliseconds, matching the retained `now + 90000`.
pub const LEASE_WINDOW_MS: i64 = 90_000;
/// Maximum accepted `request_id` filter length (UTF-16 code units).
pub const MAX_REQUEST_ID: usize = 180;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn request_filter_invalid() -> BusinessError {
    invalid(
        "REQUEST_AI_DRAIN_REQUEST_INVALID",
        "Provide a valid request filter.",
    )
}

pub fn now_invalid() -> BusinessError {
    invalid(
        "REQUEST_AI_DRAIN_NOW_INVALID",
        "Provide a valid drain clock reading.",
    )
}

pub fn drain_unavailable() -> BusinessError {
    unavailable(
        "REQUEST_AI_DRAIN_UNAVAILABLE",
        "The request AI drain is unavailable.",
    )
}

pub fn transaction_required() -> BusinessError {
    unavailable(
        "REQUEST_AI_DRAIN_TRANSACTION_REQUIRED",
        "The request AI drain requires the caller transaction.",
    )
}

/// Validate the optional `request_id` drain filter. `None` drains every due
/// proposal; `Some` drains only that request. The bound mirrors the
/// Records `validate_id` precedent (non-empty, no NUL, at most 180
/// UTF-16 code units).
pub fn validate_request_filter(request_id: Option<&str>) -> Result<Option<String>, BusinessError> {
    match request_id {
        None => Ok(None),
        Some(value) => {
            if value.is_empty()
                || value.encode_utf16().count() > MAX_REQUEST_ID
                || value.contains('\0')
            {
                return Err(request_filter_invalid());
            }
            Ok(Some(value.to_owned()))
        }
    }
}

pub fn validate_now(now: i64) -> Result<i64, BusinessError> {
    if now < 0 {
        return Err(now_invalid());
    }
    Ok(now)
}

/// Deterministic lease token for one claimed proposal. The retained path
/// mints a random UUID; the native slice derives the token from the caller
/// clock and proposal id so the two assertion-clear keys stay derivable in
/// the same unit and the focused PG test can seed them. The shape stays
/// bounded (`drain-<now>-<proposal_id>`, proposal ids are bounded by the
/// retained `request-ai-<key>` construction).
pub fn lease_token(now: i64, proposal_id: &str) -> String {
    format!("drain-{now}-{proposal_id}")
}

/// Run authorization key cleared after a successful run. This is the native
/// form of the retained `RequestAiWorkflowFacts::stageRunRequestAiEffect`
/// key: a fresh per-run key whose staged assertions are removed in the same
/// batch.
pub fn run_auth_key(lease: &str) -> String {
    format!("request-ai-run:{lease}")
}

/// Result command key cleared after a successful run. This is the native
/// form of the retained `stageRunRequestAiEffect4` key
/// (`request-ai-result-<token>` in the retained `runRequestAi`).
pub fn result_command(lease: &str) -> String {
    format!("request-ai-result-{lease}")
}

/// Minimal Records-scope actor facts parsed from the retained
/// `actor_json` (`WorkContext` serialized with emptied
/// `workspaces`/`assignments`/`email`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrainedActor {
    pub assignment_id: String,
    pub user_id: String,
    pub company_id: String,
    pub role: String,
    pub assignment_version: Option<i64>,
}

fn actor_invalid() -> BusinessError {
    unavailable(
        "REQUEST_AI_DRAIN_ACTOR_INVALID",
        "The request AI proposal actor is unavailable.",
    )
}

fn bounded_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text)
            if !text.is_empty() && text.encode_utf16().count() <= MAX_REQUEST_ID =>
        {
            Some(text.clone())
        }
        _ => None,
    }
}

/// Parse the retained actor JSON. A corrupt or out-of-bound actor fails
/// closed as a per-item `failed` decision (matching the retained catch-all
/// `AI could not prepare a valid suggestion` path), never as a drain abort.
pub fn parse_actor(actor_json: &str) -> Result<DrainedActor, BusinessError> {
    let value: serde_json::Value =
        serde_json::from_str(actor_json).map_err(|_| actor_invalid())?;
    let assignment_id = value
        .get("assignmentId")
        .and_then(bounded_text)
        .ok_or_else(actor_invalid)?;
    let user_id = value
        .get("userId")
        .and_then(bounded_text)
        .ok_or_else(actor_invalid)?;
    let company_id = value
        .get("companyId")
        .and_then(bounded_text)
        .ok_or_else(actor_invalid)?;
    let role = value
        .get("role")
        .and_then(bounded_text)
        .ok_or_else(actor_invalid)?;
    let assignment_version = value
        .get("authority")
        .and_then(|authority| authority.get("assignmentVersion"))
        .and_then(|version| match version {
            serde_json::Value::Null => Some(None),
            serde_json::Value::Number(number) => number
                .as_i64()
                .filter(|version| *version >= 0)
                .map(Some),
            _ => None,
        })
        .unwrap_or(None);
    Ok(DrainedActor {
        assignment_id,
        user_id,
        company_id,
        role,
        assignment_version,
    })
}

/// Retained stale codes that mark a proposal `stale` instead of `failed`.
/// Mirrors the retained `runRequestAi` catch classification
/// (`AI_STALE`, `REQUEST_NOT_FOUND`, `REQUEST_SCOPE_FORBIDDEN`,
/// `AI_STAGE_FORBIDDEN`, `FEATURE_COMING_SOON`).
pub fn is_stale_code(code: &str) -> bool {
    matches!(
        code,
        "AI_STALE"
            | "REQUEST_NOT_FOUND"
            | "REQUEST_SCOPE_FORBIDDEN"
            | "AI_STAGE_FORBIDDEN"
            | "FEATURE_COMING_SOON"
    )
}

/// Validate one provider result document. Mirrors the retained
/// `requestAiResult` zod shape (`title`/`description`/`category` nullable
/// with bounds, `summary` bounded, `sources` exactly `["request"]`).
/// The category enum is validated as a bounded non-empty string here; the
/// exact service-category membership stays with the retained TS oracle until
/// the full cutover.
pub fn validate_result_json(text: &str) -> Result<(), BusinessError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| {
        invalid(
            "REQUEST_AI_RESULT_INVALID",
            "The AI suggestion is unavailable. Continue manually or retry.",
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        invalid(
            "REQUEST_AI_RESULT_INVALID",
            "The AI suggestion is unavailable. Continue manually or retry.",
        )
    })?;
    if object.len() != 5 {
        return Err(invalid(
            "REQUEST_AI_RESULT_INVALID",
            "The AI suggestion is unavailable. Continue manually or retry.",
        ));
    }
    let check_text = |key: &str, max: usize| -> Result<(), BusinessError> {
        match object.get(key) {
            Some(serde_json::Value::Null) => Ok(()),
            Some(serde_json::Value::String(text)) => {
                let units = text.encode_utf16().count();
                if units > max {
                    return Err(invalid(
                        "REQUEST_AI_RESULT_INVALID",
                        "The AI suggestion is unavailable. Continue manually or retry.",
                    ));
                }
                Ok(())
            }
            _ => Err(invalid(
                "REQUEST_AI_RESULT_INVALID",
                "The AI suggestion is unavailable. Continue manually or retry.",
            )),
        }
    };
    check_text("title", 200)?;
    check_text("description", 10_000)?;
    check_text("category", 120)?;
    match object.get("summary") {
        Some(serde_json::Value::String(summary)) => {
            let units = summary.encode_utf16().count();
            if units == 0 || units > 2000 {
                return Err(invalid(
                    "REQUEST_AI_RESULT_INVALID",
                    "The AI suggestion is unavailable. Continue manually or retry.",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "REQUEST_AI_RESULT_INVALID",
                "The AI suggestion is unavailable. Continue manually or retry.",
            ));
        }
    }
    match object.get("sources") {
        Some(serde_json::Value::Array(sources))
            if sources.len() == 1
                && sources[0] == serde_json::Value::String("request".into()) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "REQUEST_AI_RESULT_INVALID",
            "The AI suggestion is unavailable. Continue manually or retry.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_filter_bounds_match_the_record_precedent() {
        assert!(validate_request_filter(None).unwrap().is_none());
        assert!(validate_request_filter(Some("request-1")).unwrap().is_some());
        assert!(validate_request_filter(Some("")).is_err());
        assert!(validate_request_filter(Some("bad\0id")).is_err());
        assert!(validate_request_filter(Some(&"x".repeat(180))).is_ok());
        assert!(validate_request_filter(Some(&"x".repeat(181))).is_err());
        assert!(validate_now(0).is_ok());
        assert!(validate_now(-1).is_err());
    }

    #[test]
    fn lease_and_clear_keys_stay_bounded_and_derivable() {
        let lease = lease_token(500, "proposal-1");
        assert_eq!(lease, "drain-500-proposal-1");
        assert_eq!(run_auth_key(&lease), "request-ai-run:drain-500-proposal-1");
        assert_eq!(
            result_command(&lease),
            "request-ai-result-drain-500-proposal-1"
        );
    }

    #[test]
    fn actor_parsing_keeps_the_retained_work_context_shape() {
        let actor = parse_actor(
            r#"{"assignmentId":"assignment-1","userId":"actor","companyId":"company","role":"operations_manager","authority":{"assignmentVersion":2}}"#,
        )
        .unwrap();
        assert_eq!(actor.assignment_id, "assignment-1");
        assert_eq!(actor.assignment_version, Some(2));
        assert!(parse_actor("not json").is_err());
        assert!(parse_actor(r#"{"assignmentId":""}"#).is_err());
    }

    #[test]
    fn stale_classification_matches_the_retained_catch() {
        for code in [
            "AI_STALE",
            "REQUEST_NOT_FOUND",
            "REQUEST_SCOPE_FORBIDDEN",
            "AI_STAGE_FORBIDDEN",
            "FEATURE_COMING_SOON",
        ] {
            assert!(is_stale_code(code), "{code}");
        }
        assert!(!is_stale_code("AI_QUOTA_EXCEEDED"));
    }

    #[test]
    fn result_validation_matches_the_retained_zod_shape() {
        let valid = r#"{"title":"T","description":null,"category":null,"summary":"Do this.","sources":["request"]}"#;
        assert!(validate_result_json(valid).is_ok());
        assert!(validate_result_json(r#"{"title":"T"}"#).is_err());
        assert!(validate_result_json(&format!(
            r#"{{"title":null,"description":null,"category":null,"summary":"{}","sources":["request"]}}"#,
            "x".repeat(2001)
        ))
        .is_err());
        assert!(validate_result_json(
            r#"{"title":null,"description":null,"category":null,"summary":"Do this.","sources":[]}"#
        )
        .is_err());
    }
}
