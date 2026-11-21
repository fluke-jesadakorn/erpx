//! Collaboration-owned published-post source facts for native Staff Work.
//!
//! This port rechecks the current published post and the selected actor's IAM
//! assignment inside the caller's serializable transaction. Audiences that
//! need another owner's policy (project, PMS record, handover source) fail
//! closed until that owner's port is composed.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize)]
pub struct SourceItem<'a> {
    pub id: &'a str,
    pub body: &'a str,
    pub author: &'a str,
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct PublishedPost {
    pub id: String,
    pub version: i64,
    pub body: String,
    pub author_name: String,
    pub audience_kind: Option<String>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SOCIAL_SOURCE_POLICY_UNAVAILABLE",
        "The Collaboration social source policy is unavailable.",
    )
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SOCIAL_SOURCE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("SOCIAL_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

/// Hash the reviewed conversation exactly as the retained oracle does: the
/// JSON array of the post followed by the selected comments, in sorted id
/// order.
pub fn source_hash(post: &SourceItem<'_>, comments: &[SourceItem<'_>]) -> String {
    let mut items = Vec::with_capacity(comments.len() + 1);
    items.push(post);
    let mut sorted = comments.to_vec();
    sorted.sort_by(|left, right| left.id.cmp(right.id));
    items.extend(sorted.iter());
    let bytes = serde_json::to_vec(&items).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    format!("{digest:x}")
}

/// Read the current published post with the actor's assignment/Company scope.
pub async fn read_published_post(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    post_id: &str,
    now: i64,
) -> Result<PublishedPost, Error> {
    ready(connection).await?;
    let row = sqlx::query(
        "SELECT p.id,p.version::bigint,p.body,p.author_name,p.audience_kind \
         FROM social_posts p WHERE p.id=$1 AND p.company_id=$2 AND p.state='published' \
         AND (p.audience_kind IS NULL OR p.audience_kind IN ('company','branch','department') \
           OR (p.audience_kind='participants' AND EXISTS(SELECT 1 FROM social_post_participants sp \
             WHERE sp.post_id=p.id AND sp.user_id=$3)) \
           OR (p.audience_kind='team' AND EXISTS(SELECT 1 FROM messaging_teams mt \
             JOIN messaging_team_members mm ON mm.team_id=mt.id WHERE mt.id=p.audience_id \
             AND mt.company_id=p.company_id AND NOT mt.organization_managed AND mm.user_id=$3))) \
         AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$3 AND a.company_id=$2 \
           AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$7) \
           AND iam_principal_active(a.user_id,a.company_id,$7) \
           AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=$5 \
             OR a.scope_kind='department' AND a.scope_node_id=$6)) \
         AND (p.branch_id IS NULL OR $5 IS NULL OR p.branch_id=$5) \
         AND (p.department_id IS NULL OR $6 IS NULL OR p.department_id=$6)",
    )
    .bind(post_id)
    .bind(company_id)
    .bind(user_id)
    .bind(now)
    .bind(branch_id)
    .bind(department_id)
    .bind(now)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| invalid("POST_NOT_FOUND", "This post is unavailable."))?;
    let audience: Option<String> = row.get("audience_kind");
    if matches!(
        audience.as_deref(),
        Some("project") | Some("pms_record") | Some("handover_source")
    ) {
        return Err(unavailable().into());
    }
    Ok(PublishedPost {
        id: row.get("id"),
        version: row.get("version"),
        body: row.get("body"),
        author_name: row.get("author_name"),
        audience_kind: audience,
    })
}

/// Retain a current-source guard for one participant inside the caller's
/// command, matching the retained assertion step.
pub async fn source_guard(
    connection: &mut PgConnection,
    key: &str,
    step: &str,
    post_id: &str,
    company_id: &str,
    version: i64,
    user_id: &str,
    now: i64,
) -> Result<(), Error> {
    ready(connection).await?;
    sqlx::query(
        "INSERT INTO command_assertions(command_id,step,ok) \
         SELECT $1,$2,CASE WHEN EXISTS(SELECT 1 FROM social_posts p WHERE p.id=$3 \
           AND p.company_id=$4 AND p.version=$5 AND p.state='published' \
           AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$6 AND a.company_id=$4 \
           AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$7) \
           AND iam_principal_active(a.user_id,a.company_id,$7))) THEN 1 ELSE 0 END",
    )
    .bind(key)
    .bind(step)
    .bind(post_id)
    .bind(company_id)
    .bind(version)
    .bind(user_id)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub fn into_business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(_) => unavailable(),
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceItem, source_hash};

    #[test]
    fn snapshot_hash_orders_comments_by_id() {
        let post = SourceItem {
            id: "p",
            body: "body",
            author: "author",
            version: 1,
        };
        let a = SourceItem {
            id: "a",
            body: "a",
            author: "x",
            version: 1,
        };
        let b = SourceItem {
            id: "b",
            body: "b",
            author: "y",
            version: 1,
        };
        assert_eq!(
            source_hash(&post, &[b.clone(), a.clone()]),
            source_hash(&post, &[a, b])
        );
        assert_ne!(
            source_hash(&post, &[]),
            source_hash(&post, &[]).replace('0', "1")
        );
    }
}
