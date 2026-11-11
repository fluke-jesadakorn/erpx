//! Read-only Collaboration post facts for native Staff Work composition.
//!
//! The visibility predicate is the Social owner's current row rule: a post in
//! the caller's Company must be published, its audience must admit the user,
//! and the user must hold a current assignment matching the post's branch or
//! department placement. It opens no transaction and never writes a Social row.
//! Commercial purchase and linked business-record gates stay with composition.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::PgConnection;

/// The current published post identity and author.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadablePostFact {
    pub id: String,
    pub author_id: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COLLABORATION_FACT_UNAVAILABLE",
        "The current Collaboration post fact is unavailable.",
    )
}

/// Match the retained `readablePost`/`workSocialReadScope` visibility rule:
/// archived or moderated posts (and ids that only exist as drafts) are not
/// readable, `participants`/`team`/`project` audiences resolve on their owner
/// rows, and `branch`/`department` placement requires a current assignment with
/// matching scope. `iam_same_person`-style alias widening is not used by the
/// original post read and is not invented here.
pub async fn readable_post(
    c: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    post_id: &str,
) -> Result<Option<ReadablePostFact>, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let user = UserId::new(user_id)?;
    let post = RecordId::new(post_id)?;
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT p.id,p.author_id FROM social_posts p \
         WHERE p.id=$3 AND p.company_id=$1 AND p.state='published' \
           AND EXISTS(SELECT 1 FROM assignments a \
             WHERE a.user_id=$2 AND a.company_id=p.company_id AND a.revoked_at IS NULL \
               AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
               AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint \
               AND ((p.branch_id IS NULL AND p.department_id IS NULL) \
                 OR (p.department_id IS NOT NULL AND a.scope_kind='department' \
                   AND a.scope_node_id=p.department_id) \
                 OR (p.department_id IS NULL AND p.branch_id IS NOT NULL AND (\
                   a.scope_kind='holding' \
                   OR (a.scope_kind='branch' AND a.scope_node_id=p.branch_id) \
                   OR (a.scope_kind='department' AND EXISTS(SELECT 1 FROM organization_nodes n \
                     WHERE n.id=a.scope_node_id AND n.company_id=a.company_id \
                       AND n.branch_id=p.branch_id)))))) \
           AND (p.audience_kind IS NULL \
             OR p.audience_kind IN ('company','branch','department') \
             OR (p.audience_kind='participants' AND EXISTS(SELECT 1 FROM social_post_participants sp \
               WHERE sp.post_id=p.id AND sp.user_id=$2)) \
             OR (p.audience_kind='team' AND EXISTS(SELECT 1 FROM messaging_teams mt \
               JOIN messaging_team_members mm ON mm.team_id=mt.id \
               WHERE mt.id=p.audience_id AND mt.company_id=p.company_id \
                 AND NOT mt.organization_managed AND mm.user_id=$2)) \
             OR (p.audience_kind='project' AND EXISTS(SELECT 1 FROM staff_projects pr \
               WHERE pr.id=p.audience_id AND pr.company_id=p.company_id \
                 AND (pr.owner_user_id=$2 OR EXISTS(SELECT 1 FROM staff_project_members pm \
                   WHERE pm.project_id=pr.id AND pm.user_id=$2))))) \
         ORDER BY p.id LIMIT 1",
    )
    .bind(company.as_str())
    .bind(user.as_str())
    .bind(post.as_str())
    .fetch_optional(c)
    .await
    .map_err(|_| unavailable())?;
    Ok(row.map(|(id, author_id)| ReadablePostFact { id, author_id }))
}
