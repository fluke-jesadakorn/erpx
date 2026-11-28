//! Pure domain contract for the native L06 post sub-resource routes.
//!
//! Owner statements retired from the TypeScript handlers are recorded here
//! byte-for-byte (`?` placeholders); the Postgres adapter only renumber them
//! to `$n`. This layer owns bounded identities, response envelopes and the
//! source/command projections. No SQL execution, axum or I/O lives here.

use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// --- task-source owner statements --------------------------------------------

/// Native Collaboration published-post source policy, narrowed to the full
/// `p.*` row the retained `sourceSnapshot` returns. Binds: post, company,
/// user, branch, department, now, now.
pub const POST_SOURCE_SQL: &str = concat!(
    "SELECT to_jsonb(p)::text AS row FROM social_posts p WHERE p.id=$1 ",
    "AND p.company_id=$2 AND p.state='published' ",
    "AND (p.audience_kind IS NULL OR p.audience_kind IN ('company','branch','department') ",
    "OR (p.audience_kind='participants' AND EXISTS(SELECT 1 FROM social_post_participants sp ",
    "WHERE sp.post_id=p.id AND sp.user_id=$3)) ",
    "OR (p.audience_kind='team' AND EXISTS(SELECT 1 FROM messaging_teams mt ",
    "JOIN messaging_team_members mm ON mm.team_id=mt.id WHERE mt.id=p.audience_id ",
    "AND mt.company_id=p.company_id AND NOT mt.organization_managed AND mm.user_id=$3))) ",
    "AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$3 AND a.company_id=$2 ",
    "AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$6) ",
    "AND iam_principal_active(a.user_id,a.company_id,$6) ",
    "AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=$4 ",
    "OR a.scope_kind='department' AND a.scope_node_id=$5)) ",
    "AND (p.branch_id IS NULL OR $4 IS NULL OR p.branch_id=$4) ",
    "AND (p.department_id IS NULL OR $5 IS NULL OR p.department_id=$5)"
);

/// Render the ordered selected-comment statement. Only the count of `?`
/// placeholders is derived from the request; no value reaches the SQL text.
pub fn render_comments_sql(comment_count: usize) -> String {
    let mut sql = String::from(
        "SELECT id,body,author_name,version::bigint AS version FROM social_comments \
         WHERE post_id=? AND state='published' AND id IN (",
    );
    for index in 0..comment_count {
        if index > 0 {
            sql.push(',');
        }
        sql.push('?');
    }
    sql.push_str(") ORDER BY id");
    sql
}

/// Postgres-side SHA-256 keeps the native slice free of an added hash crate
/// while producing the same lowercase hex digest as the retained oracle.
pub const SHA256_SQL: &str = "SELECT encode(sha256(convert_to($1,'UTF8')),'hex')";

// --- task owner statements ----------------------------------------------------

/// Byte-identical to the retired `socialTaskOriginBy('task_id','post_id')`.
pub const POST_TASK_SQL: &str = "SELECT task_id FROM social_task_origins WHERE post_id=$1";

// --- AI draft owner statements ------------------------------------------------

/// Byte-identical head of the retired `socialAiDraftById` statement.
pub const AI_DRAFT_HEAD: &str = concat!(
    "SELECT actor_json,post_id,kind,source_hash,result_json,state,",
    "source_version::bigint AS source_version,comment_ids_json,expires_at,provider_json ",
    "FROM social_ai_drafts WHERE id=$1 AND actor_id=$2"
);
pub const AI_DRAFT_PREPARE_SQL: &str = concat!(
    "INSERT INTO social_ai_drafts(id,post_id,actor_id,kind,source_version,source_hash,",
    "comment_ids_json,sources_json,state,model,created_at,expires_at) ",
    "VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9,$10,$11)"
);
pub const AI_USAGE_RESERVE_SQL: &str = concat!(
    "INSERT INTO social_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) ",
    "VALUES($1,$2,$3,to_char(to_timestamp($5/1000.0),'YYYY-MM'),'reserved',$4,$5)"
);
pub const AI_DRAFT_CAPTURE_PROVIDER_SQL: &str =
    "UPDATE social_ai_drafts SET provider_json=$1 WHERE id=$2";
pub const AI_DRAFT_CAPTURE_ACTOR_SQL: &str =
    "UPDATE social_ai_drafts SET actor_json=$1 WHERE id=$2";
pub const AI_DRAFT_FINISH_SQL: &str =
    "UPDATE social_ai_drafts SET result_json=$1,state='ready' WHERE id=$2";
pub const AI_USAGE_FINISH_SQL: &str = concat!(
    "UPDATE social_ai_usage SET state='succeeded',input_tokens=$1,output_tokens=$2,",
    "lease_until=0 WHERE id=$3"
);
pub const AI_DRAFT_FAIL_SQL: &str =
    "UPDATE social_ai_drafts SET state='failed',result_json=NULL WHERE id=$1";
pub const AI_USAGE_FAIL_SQL: &str = concat!(
    "UPDATE social_ai_usage SET state='failed',input_tokens=0,output_tokens=0,",
    "lease_until=0 WHERE id=$1"
);

// --- DTOs ---------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: String,
    pub body: String,
    pub author: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSourceView {
    pub post: Value,
    pub sources: Vec<SourceItem>,
    pub hash: String,
    pub comment_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostTaskResult {
    pub id: String,
    pub state: String,
    pub existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSourceRef {
    pub id: String,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiDraftView {
    pub id: String,
    pub kind: String,
    pub result: Value,
    pub source_version: i64,
    pub source_hash: String,
    pub sources: Vec<DraftSourceRef>,
    pub comment_ids: Vec<String>,
}

// --- invariants ---------------------------------------------------------------

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn bounded(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':')
        })
}

pub fn validate_id(value: &str) -> Result<(), BusinessError> {
    if bounded(value) {
        Ok(())
    } else {
        Err(invalid("OPERATIONS_POSTS_INVALID", "Use one bounded post identity."))
    }
}

/// Retained `COMMAND_KEY_REQUIRED` rule: `/^[a-zA-Z0-9:_.-]{8,120}$/`.
pub fn validate_key(key: &str) -> Result<(), BusinessError> {
    let valid = (8..=120).contains(&key.len())
        && key.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(invalid("COMMAND_KEY_REQUIRED", "Provide an Idempotency-Key."))
    }
}

/// Retained `sourceSnapshot` comment rules: unique, lexicographically sorted,
/// at most twenty bounded ids.
pub fn normalize_comment_ids(ids: &[String]) -> Result<Vec<String>, BusinessError> {
    let mut unique = ids.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() > 20
        || unique.iter().any(|id| {
            id.is_empty() || id.len() > 160 || !bounded(id)
        })
    {
        return Err(invalid("SOURCE_LIMIT", "Select up to 20 comments."));
    }
    Ok(unique)
}

/// The retained 18,000-byte UTF-8 bound on the serialized source array.
pub fn sources_within_limit(items: &[SourceItem]) -> bool {
    serde_json::to_vec(items)
        .map(|bytes| bytes.len() <= 18_000)
        .unwrap_or(false)
}

pub fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "POST_NOT_FOUND", "This post is unavailable.")
}

pub fn source_unsupported() -> BusinessError {
    invalid(
        "SOURCE_UNSUPPORTED",
        "Create a standalone team post for this workflow. Linked business records keep their existing permissions.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_CHANGED",
        "A selected comment is unavailable. Review the conversation again.",
    )
}

pub fn ai_unavailable(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "AI_UNAVAILABLE", message)
}

pub fn invalid_source_limit() -> BusinessError {
    invalid("SOURCE_LIMIT", "Select fewer comments.")
}

pub fn invalid_ai_kind() -> BusinessError {
    invalid("AI_INVALID_KIND", "Use a bounded AI draft kind.")
}

pub fn envelope(data: &impl Serialize) -> Value {
    json!({ "data": data })
}

// --- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_sql_keeps_retired_bytes_and_bind_order() {
        assert_eq!(POST_TASK_SQL, "SELECT task_id FROM social_task_origins WHERE post_id=$1");
        assert_eq!(render_comments_sql(2), "SELECT id,body,author_name,version::bigint AS version FROM social_comments WHERE post_id=? AND state='published' AND id IN (?,?) ORDER BY id");
        assert_eq!(AI_DRAFT_HEAD.contains("id=$1 AND actor_id=$2"), true);
        assert!(POST_SOURCE_SQL.contains("iam_assignment_active(a.id,$6)"));
    }

    #[test]
    fn identities_and_keys_are_bounded() {
        assert!(validate_id("post-1").is_ok());
        assert!(validate_id("bad id").is_err());
        assert!(validate_key("abc12345").is_ok());
        assert_eq!(
            validate_key("short").unwrap_err().code,
            "COMMAND_KEY_REQUIRED"
        );
    }

    #[test]
    fn comment_ids_are_unique_sorted_and_bounded() {
        assert_eq!(
            normalize_comment_ids(&["b".into(), "a".into(), "a".into()]).unwrap(),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert_eq!(
            normalize_comment_ids(&vec!["c".repeat(161)]).unwrap_err().code,
            "SOURCE_LIMIT"
        );
    }

    #[test]
    fn envelopes_and_dtos_are_exact() {
        let view = PostTaskResult {
            id: "task-1".into(),
            state: "ready".into(),
            existing: false,
        };
        assert_eq!(
            envelope(&view),
            json!({"data": {"id": "task-1", "state": "ready", "existing": false}})
        );
        let source = TaskSourceView {
            post: json!({"id": "p1"}),
            sources: vec![SourceItem {
                id: "p1".into(),
                body: "b".into(),
                author: "a".into(),
                version: 2,
            }],
            hash: "abc".into(),
            comment_ids: vec![],
        };
        assert_eq!(envelope(&source)["data"]["commentIds"], json!([]));
    }
}
