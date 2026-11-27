//! Post and comment projection for the Holding Feed and direct conversation.
//! Collaboration supplies the current audience and conversation rows. The
//! composed policy supplies the linked owner and purchase decisions before
//! the text or comments are returned.
use crate::adapters::social::comment_page_row;
use crate::adapters::social_pg::SocialScope;
use crate::domain::social::{PostSource, post_not_found};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, Row, postgres::PgRow, types::Json};
use std::{collections::BTreeMap, future::Future, pin::Pin};

type Error = PgTransactionError<BusinessError>;
type Card = Map<String, Value>;
const INITIAL_CARDS: usize = 8;
const CANDIDATE_BATCH: i64 = 24;

#[derive(Clone, Copy, Debug)]
pub enum SocialHomeSelection<'a> {
    Conversations { tag: Option<&'a str> },
    Milestones { tag: Option<&'a str> },
    Thread { post_id: &'a str },
}

/// Protected source keys are available to composition on the retained
/// connection. Only the authorized source presentation reaches the wire.
#[derive(Debug)]
pub struct SocialHomeCandidate {
    pub id: String,
    pub version: i64,
    pub author_id: String,
    pub purpose: String,
    pub audience_kind: Option<String>,
    pub audience_id: Option<String>,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub linked_module: Option<String>,
    pub linked_record_id: Option<String>,
    pub sources: Vec<PostSource>,
    pub task_origin: Option<String>,
}

pub struct SocialHomeAdditions {
    pub ai_name: String,
    pub project_audience_name: Option<String>,
    pub linked_sources: Vec<Value>,
    pub files: Vec<Value>,
    pub task: Option<Value>,
    pub tracking_state: Option<String>,
    pub purchase: Option<Value>,
    pub handover_receipt: Option<Value>,
}

pub type SocialHomePolicyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<SocialHomeAdditions>, Error>> + Send + 'a>>;

pub trait SocialHomePolicy: Send {
    /// None means this actor cannot read the post under current linked-source,
    /// project-audience or product purchase rules.
    fn present<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        scope: &'a SocialScope,
        candidate: &'a SocialHomeCandidate,
    ) -> SocialHomePolicyFuture<'a>;
}

const CANDIDATES: &str = r#"
SELECT p.id,p.version,p.author_id,p.purpose,p.audience_kind,p.audience_id,p.branch_id,
       p.department_id,p.linked_module,p.linked_record_id,p.created_at,
       COALESCE((SELECT jsonb_agg(jsonb_build_object('kind',s.source_kind,'id',s.source_id)
                                ORDER BY s.ordinal)
                 FROM social_post_sources s WHERE s.post_id=p.id),'[]'::jsonb) AS sources,
       (SELECT task_id FROM social_task_origins o WHERE o.post_id=p.id) AS task_origin
FROM social_posts p
WHERE p.company_id=$1 AND p.state='published'
  AND (p.audience_kind IS NULL OR p.audience_kind IN ('company','branch','department','project')
       OR (p.audience_kind='participants' AND EXISTS(
           SELECT 1 FROM social_post_participants m WHERE m.post_id=p.id AND m.user_id=$2))
       OR (p.audience_kind='team' AND EXISTS(
           SELECT 1 FROM messaging_teams t JOIN messaging_team_members m ON m.team_id=t.id
           WHERE t.id=p.audience_id AND t.company_id=p.company_id
             AND NOT t.organization_managed AND m.user_id=$2)))
  AND EXISTS(SELECT 1 FROM assignments a
    WHERE a.user_id=$2 AND a.company_id=p.company_id AND a.revoked_at IS NULL
      AND a.valid_from<=$4 AND a.valid_until>$4
      AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4)
      AND (a.id=$3 OR EXISTS(SELECT 1 FROM social_post_sources s
                            WHERE s.post_id=p.id AND s.source_kind='pms_record'))
      AND ((p.branch_id IS NULL AND p.department_id IS NULL)
        OR (p.department_id IS NOT NULL AND a.scope_kind='department' AND a.scope_node_id=p.department_id)
        OR (p.department_id IS NULL AND p.branch_id IS NOT NULL
          AND (a.scope_kind='holding'
            OR (a.scope_kind='branch' AND a.scope_node_id=p.branch_id)
            OR (a.scope_kind='department' AND EXISTS(SELECT 1 FROM organization_nodes n
                WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=p.branch_id))))))
  AND (p.id=$6 OR ($6::text IS NULL AND p.linked_module IS NULL
      AND (p.kind='system')=$8
      AND ($5::text IS NULL OR p.branch_id=$5 OR p.branch_id IS NULL)
      AND p.created_at<=$4
      AND ($7::text IS NULL
        OR EXISTS(SELECT 1 FROM social_post_topics t WHERE t.post_id=p.id AND t.tag=$7)
        OR EXISTS(SELECT 1 FROM social_comments c JOIN social_comment_topics t ON t.comment_id=c.id
                  WHERE c.post_id=p.id AND c.state='published' AND t.tag=$7))))
  AND ($9::bigint IS NULL OR (p.created_at,p.id)<($9,$10))
ORDER BY p.created_at DESC,p.id DESC LIMIT $11
"#;

const CARDS: &str = r#"
SELECT p.id,to_jsonb(p) AS card,
       (SELECT t.name FROM messaging_teams t
        WHERE t.id=p.audience_id AND t.company_id=p.company_id) AS team_name,
       (SELECT node.display_name FROM organization_nodes node
        WHERE node.id=COALESCE(p.department_id,p.branch_id) AND node.company_id=p.company_id) AS node_name,
       (SELECT count(*) FROM social_post_participants m
        WHERE m.post_id=p.id AND m.user_id<>p.author_id) AS audience_count,
       (SELECT count(*) FROM social_comments c WHERE c.post_id=p.id AND c.state='published') AS comment_count,
       (SELECT count(*) FROM social_reactions r WHERE r.post_id=p.id) AS appreciation_count,
       EXISTS(SELECT 1 FROM social_reactions r WHERE r.post_id=p.id AND r.principal_id=$2) AS appreciated,
       EXISTS(SELECT 1 FROM social_follows f
              WHERE f.target_type='post' AND f.target_id=p.id AND f.principal_id=$2) AS followed
FROM social_posts p WHERE p.company_id=$1 AND p.state='published' AND p.id=ANY($3)
"#;

const COMMENTS: &str = r#"
WITH RECURSIVE ranked AS (
    SELECT c.*,row_number() OVER(PARTITION BY c.post_id ORDER BY c.created_at DESC,c.id DESC) AS ordinal
    FROM social_comments c WHERE c.post_id=ANY($1) AND c.state='published'
), selected AS (
    SELECT * FROM ranked WHERE ordinal<=$2
), ancestors AS (
    SELECT parent.id,parent.post_id FROM selected child JOIN social_comments parent
      ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
    WHERE parent.state='published'
    UNION
    SELECT parent.id,parent.post_id FROM ancestors ancestor
      JOIN social_comments child ON child.id=ancestor.id AND child.post_id=ancestor.post_id
      JOIN social_comments parent ON parent.id=child.parent_comment_id AND parent.post_id=child.post_id
    WHERE parent.state='published'
), thread AS (
    SELECT id,post_id FROM selected UNION SELECT id,post_id FROM ancestors
)
SELECT c.id,c.post_id,c.body,c.author_id,c.author_name,c.created_at,c.mentions_json,c.rich_content_json,
       c.author_kind,c.parent_comment_id,parent.author_name AS parent_author_name
FROM thread t JOIN social_comments c ON c.id=t.id AND c.post_id=t.post_id
LEFT JOIN social_comments parent ON parent.id=c.parent_comment_id AND parent.post_id=c.post_id
ORDER BY c.created_at,c.id
"#;

struct AcceptedPost {
    candidate: SocialHomeCandidate,
    additions: SocialHomeAdditions,
}

pub(super) async fn read_home_posts(
    connection: &mut PgConnection,
    scope: &SocialScope,
    selection: SocialHomeSelection<'_>,
    policy: &mut dyn SocialHomePolicy,
) -> Result<Vec<Value>, Error> {
    let (post_id, tag, milestones) = match selection {
        SocialHomeSelection::Conversations { tag } => (None, tag, false),
        SocialHomeSelection::Milestones { tag } => (None, tag, true),
        SocialHomeSelection::Thread { post_id } => (Some(post_id), None, false),
    };
    let limit = if post_id.is_some() { 1 } else { INITIAL_CARDS };
    let mut before: Option<(i64, String)> = None;
    let mut accepted = Vec::with_capacity(limit);
    while accepted.len() < limit {
        let candidates = sqlx::query(CANDIDATES)
            .bind(scope.company_id.as_str())
            .bind(scope.user_id.as_str())
            .bind(scope.assignment_id.as_str())
            .bind(scope.now)
            .bind(scope.branch_id.as_deref())
            .bind(post_id)
            .bind(tag)
            .bind(milestones)
            .bind(before.as_ref().map(|(at, _)| *at))
            .bind(before.as_ref().map(|(_, id)| id.as_str()))
            .bind(CANDIDATE_BATCH)
            .fetch_all(&mut *connection)
            .await?;
        let more = candidates.len() as i64 == CANDIDATE_BATCH && post_id.is_none();
        for row in candidates {
            let candidate = SocialHomeCandidate {
                id: row.try_get("id")?,
                version: row.try_get("version")?,
                author_id: row.try_get("author_id")?,
                purpose: row.try_get("purpose")?,
                audience_kind: row.try_get("audience_kind")?,
                audience_id: row.try_get("audience_id")?,
                department_id: row.try_get("department_id")?,
                branch_id: row.try_get("branch_id")?,
                linked_module: row.try_get("linked_module")?,
                linked_record_id: row.try_get("linked_record_id")?,
                sources: row.try_get::<Json<Vec<_>>, _>("sources")?.0,
                task_origin: row.try_get("task_origin")?,
            };
            before = Some((row.try_get("created_at")?, candidate.id.clone()));
            let Some(additions) = policy.present(connection, scope, &candidate).await? else {
                continue;
            };
            accepted.push(AcceptedPost {
                candidate,
                additions,
            });
            if accepted.len() == limit {
                break;
            }
        }
        if !more {
            break;
        }
    }
    if accepted.is_empty() {
        return if post_id.is_some() {
            Err(post_not_found().into())
        } else {
            Ok(Vec::new())
        };
    }
    let ids: Vec<_> = accepted
        .iter()
        .map(|entry| entry.candidate.id.as_str())
        .collect();
    let mut cards = BTreeMap::new();
    for row in sqlx::query(CARDS)
        .bind(scope.company_id.as_str())
        .bind(scope.user_id.as_str())
        .bind(&ids)
        .fetch_all(&mut *connection)
        .await?
    {
        cards.insert(row.try_get::<String, _>("id")?, row);
    }
    let mut comments =
        read_comments(connection, &ids, if post_id.is_some() { 20 } else { 2 }).await?;
    let mut replies = read_replies(connection, &ids).await?;
    accepted
        .into_iter()
        .map(|entry| {
            let id = &entry.candidate.id;
            let card = cards.remove(id).ok_or_else(post_not_found)?;
            let comments = comments.remove(id).unwrap_or_default();
            let replies = replies.remove(id).unwrap_or_default();
            project_card(scope, entry, card, comments, replies)
        })
        .collect()
}

fn audience(
    candidate: &SocialHomeCandidate,
    team: Option<String>,
    node: Option<String>,
    project: Option<&str>,
    count: i64,
) -> (Option<String>, String) {
    let within = if candidate.department_id.is_some() {
        " · in department"
    } else if candidate.branch_id.is_some() {
        " · in branch"
    } else {
        ""
    };
    let kind = candidate.audience_kind.as_deref();
    let name = match kind {
        Some("team") => team,
        Some("project") => project.map(str::to_owned),
        _ => node,
    };
    let label = match kind {
        Some("participants") => format!("Selected people ({count}){within}"),
        Some("team") => format!("Team · {}{within}", name.as_deref().unwrap_or("Team")),
        Some("project") => format!("Project · {}{within}", name.as_deref().unwrap_or("Project")),
        _ if candidate.department_id.is_some() => {
            format!(
                "Department · {}",
                name.as_deref().unwrap_or("Your department")
            )
        }
        _ if candidate.branch_id.is_some() => {
            format!("Branch · {}", name.as_deref().unwrap_or("Branch"))
        }
        _ => "Everyone in the company".into(),
    };
    (name, label)
}

async fn read_comments(
    connection: &mut PgConnection,
    posts: &[&str],
    limit: i64,
) -> Result<BTreeMap<String, Vec<Value>>, Error> {
    let mut result: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for row in sqlx::query(COMMENTS)
        .bind(posts)
        .bind(limit)
        .fetch_all(connection)
        .await?
    {
        let row = comment_page_row(row)?;
        let mut card = json!(&row);
        if let Some(fields) = card.as_object_mut() {
            fields.remove("rich_content");
            fields.insert(
                "richContent".into(),
                row.rich_content.unwrap_or(Value::Null),
            );
        }
        result.entry(row.post_id).or_default().push(card);
    }
    Ok(result)
}

async fn read_replies(
    connection: &mut PgConnection,
    posts: &[&str],
) -> Result<BTreeMap<String, Vec<Value>>, Error> {
    let mut result: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let replies = sqlx::query(
        "SELECT s.post_id,to_jsonb(s)-'created_at' AS reply FROM (
           SELECT post_id,id,owner_type,owner_id,state,error,error_code,actor_id,
                  requester_name,attempt,'reply-'||id AS reply_id,created_at
           FROM social_agent_replies WHERE post_id=ANY($1)
         ) s ORDER BY s.created_at,s.id",
    )
    .bind(posts)
    .fetch_all(connection)
    .await?;
    for row in replies {
        result
            .entry(row.try_get("post_id")?)
            .or_default()
            .push(row.try_get("reply")?);
    }
    Ok(result)
}

fn decode(data: &Card, field: &str) -> Result<Value, Error> {
    let Some(value) = data
        .get(field)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
    else {
        return Ok(Value::Null);
    };
    serde_json::from_str(value).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "SOCIAL_CONTENT_UNAVAILABLE",
            "The stored conversation could not be read.",
        )
        .into()
    })
}

fn project_card(
    scope: &SocialScope,
    entry: AcceptedPost,
    row: PgRow,
    comments: Vec<Value>,
    replies: Vec<Value>,
) -> Result<Value, Error> {
    let AcceptedPost {
        candidate,
        additions,
    } = entry;
    let mut data = row.try_get::<Json<Card>, _>("card")?.0;
    let audience_count: i64 = row.try_get("audience_count")?;
    let (audience_name, audience_label) = audience(
        &candidate,
        row.try_get("team_name")?,
        row.try_get("node_name")?,
        additions.project_audience_name.as_deref(),
        audience_count,
    );
    let content = decode(&data, "content_json")?;
    let rich_content = decode(&data, "rich_content_json")?;
    let mentions = decode(&data, "mentions_json")?;
    let mut files = additions.files;
    if let Some(blocks) = content.as_array() {
        files.retain(|file| {
            blocks.iter().any(|block| {
                block.get("type").and_then(Value::as_str) == Some("image")
                    && block
                        .get("attachmentId")
                        .and_then(Value::as_str)
                        .is_some_and(|id| file.get("id").and_then(Value::as_str) == Some(id))
            })
        });
    }
    data.extend([
        ("audience_name".into(), json!(audience_name)),
        ("audienceLabel".into(), json!(audience_label)),
        ("audience_count".into(), json!(audience_count)),
        (
            "appreciationCount".into(),
            json!(row.try_get::<i64, _>("appreciation_count")?),
        ),
        (
            "commentCount".into(),
            json!(row.try_get::<i64, _>("comment_count")?),
        ),
        (
            "appreciated".into(),
            json!(row.try_get::<bool, _>("appreciated")?),
        ),
        (
            "followed".into(),
            json!(row.try_get::<bool, _>("followed")?),
        ),
        ("aiName".into(), json!(additions.ai_name)),
        ("content".into(), content),
        ("richContent".into(), rich_content),
        ("mentions".into(), mentions),
        ("linkedSources".into(), json!(additions.linked_sources)),
        ("comments".into(), json!(comments)),
        ("files".into(), json!(files)),
        ("agentReplies".into(), json!(replies)),
        ("task".into(), json!(additions.task)),
        ("hasTask".into(), json!(candidate.task_origin.is_some())),
        (
            "trackingState".into(),
            json!(
                additions
                    .tracking_state
                    .filter(|_| { candidate.author_id == scope.user_id.as_str() })
            ),
        ),
    ]);
    if let Some(purchase) = additions.purchase {
        data.insert("purchase".into(), purchase);
    }
    if candidate.purpose == "handover" && candidate.audience_kind.as_deref() == Some("participants")
    {
        data.insert(
            "handoverReceipt".into(),
            additions.handover_receipt.unwrap_or(Value::Null),
        );
    }
    Ok(Value::Object(data))
}
