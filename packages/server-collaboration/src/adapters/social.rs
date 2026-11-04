//! Social owner rows. No IAM, purchase, People or Records query is made here;
//! composition supplies the current authority result and readable mention labels.
use crate::domain::social::{
    Audience, CommentSnapshot, CommentState, FeedCursor, Mention, MentionKind, OwnerPlan,
    PostSnapshot, PostState, Receipt, SocialActor, plan_receipt,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SOCIAL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
    )
    .fetch_one(c)
    .await?)
}

/// Acquire before the first selected IAM/People/Records snapshot.
pub async fn lock_social_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("SOCIAL_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE social_posts,social_comments,social_reactions,social_follows,social_topics,social_post_topics,social_comment_topics,social_mentions,staff_notifications IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOCIAL_SOURCE_CHANGED",
        "The conversation changed. Refresh and retry.",
    )
    .into()
}

fn conflict() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Refresh the conversation before saving or removing this comment.",
    )
    .into()
}

fn audience(raw: Option<&str>, branch: Option<&str>) -> Result<Audience, Error> {
    match raw {
        Some("company") | None if branch.is_none() => Ok(Audience::Holding),
        Some("branch") | Some("department") | None => Ok(Audience::Branch),
        _ => Err(changed()),
    }
}

fn mention_rows(raw: &str) -> Result<Vec<Mention>, Error> {
    let rows: Vec<Value> = serde_json::from_str(raw).map_err(|_| changed())?;
    rows.into_iter()
        .map(|row| {
            let kind = MentionKind::parse(
                row.get("kind")
                    .and_then(Value::as_str)
                    .ok_or_else(changed)?,
            )?;
            Ok(Mention {
                kind,
                target_id: row
                    .get("targetId")
                    .and_then(Value::as_str)
                    .ok_or_else(changed)?
                    .into(),
                label: row
                    .get("label")
                    .and_then(Value::as_str)
                    .ok_or_else(changed)?
                    .into(),
            })
        })
        .collect()
}

pub async fn read_post(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<PostSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,author_id,author_name,company_id,branch_id,department_id,audience_kind,body,version,state,mentions_json,linked_module,linked_record_id,linked_record_label,created_at FROM social_posts WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let topics = sqlx::query_scalar::<_, String>(
        "SELECT tag FROM social_post_topics WHERE post_id=$1 ORDER BY tag",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    let linked_module: Option<String> = row.try_get(11)?;
    let linked_id: Option<String> = row.try_get(12)?;
    let linked_label: Option<String> = row.try_get(13)?;
    let linked_record = linked_module
        .zip(linked_id)
        .zip(linked_label)
        .map(|((module, id), label)| crate::domain::social::LinkedRecord {
            module,
            id,
            label,
        });
    Ok(Some(PostSnapshot {
        id: row.try_get(0)?,
        author_id: row.try_get(1)?,
        author_name: row.try_get(2)?,
        company_id: row.try_get(3)?,
        branch_id: row.try_get(4)?,
        department_id: row.try_get(5)?,
        audience: audience(row.try_get::<Option<String>, _>(6)?.as_deref(), None)?,
        body: row.try_get(7)?,
        version: row.try_get(8)?,
        state: PostState::parse(&row.try_get::<String, _>(9)?)?,
        topics,
        mentions: mention_rows(&row.try_get::<String, _>(10)?)?,
        linked_record,
        created_at: row.try_get(14)?,
    }))
}

pub async fn read_comment(
    c: &mut PgConnection,
    id: &str,
    post_id: &str,
) -> Result<Option<CommentSnapshot>, Error> {
    let row = sqlx::query(
        "SELECT id,post_id,author_id,state,version,body,created_at FROM social_comments WHERE id=$1 AND post_id=$2",
    )
    .bind(id)
    .bind(post_id)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok(CommentSnapshot {
            id: row.try_get(0)?,
            post_id: row.try_get(1)?,
            author_id: row.try_get(2)?,
            state: CommentState::parse(&row.try_get::<String, _>(3)?)?,
            version: row.try_get(4)?,
            body: row.try_get(5)?,
            created_at: row.try_get(6)?,
        })
    })
    .transpose()
}

async fn topic(c: &mut PgConnection, tag: &str, user: &str, now: i64) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO social_topics(tag,created_by,created_at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(tag)
    .bind(user)
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn post_topics(
    c: &mut PgConnection,
    post_id: &str,
    tags: &[String],
    user: &str,
    now: i64,
) -> Result<(), Error> {
    for tag in tags {
        topic(c, tag, user, now).await?;
        sqlx::query("INSERT INTO social_post_topics(post_id,tag) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(post_id)
            .bind(tag)
            .execute(&mut *c)
            .await?;
    }
    Ok(())
}

async fn comment_topics(
    c: &mut PgConnection,
    comment_id: &str,
    tags: &[String],
    user: &str,
    now: i64,
) -> Result<(), Error> {
    for tag in tags {
        topic(c, tag, user, now).await?;
        sqlx::query(
            "INSERT INTO social_comment_topics(comment_id,tag) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(comment_id)
        .bind(tag)
        .execute(&mut *c)
        .await?;
    }
    Ok(())
}

async fn mentions(
    c: &mut PgConnection,
    owner_type: &str,
    owner_id: &str,
    mentions: &[Mention],
    user: &str,
    actor_company: &str,
    now: i64,
) -> Result<(), Error> {
    for mention in mentions {
        sqlx::query(
            "INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        )
        .bind(owner_type)
        .bind(owner_id)
        .bind(mention.kind.as_str())
        .bind(&mention.target_id)
        .bind(&mention.label)
        .bind(user)
        .bind(now)
        .execute(&mut *c)
        .await?;
        if mention.kind == MentionKind::Person {
            let (record_type, record_id, version) = if owner_type == "comment" {
                ("social_comment", owner_id, 1)
            } else {
                ("post", owner_id, 1)
            };
            sqlx::query(
                "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'You were mentioned in a conversation',$7) ON CONFLICT DO NOTHING",
            )
            .bind(format!("mention-{owner_type}-{owner_id}-{}", mention.target_id))
            .bind(actor_company)
            .bind(&mention.target_id)
            .bind(record_type)
            .bind(record_id)
            .bind(version)
            .bind(now)
            .execute(&mut *c)
            .await?;
        }
    }
    Ok(())
}

async fn clear_mentions(c: &mut PgConnection, owner_type: &str, owner_id: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM social_mentions WHERE owner_type=$1 AND owner_id=$2")
        .bind(owner_type)
        .bind(owner_id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedComment {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRecord {
    pub module: String,
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedPost {
    pub id: String,
    pub kind: String,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
    pub audience: &'static str,
    pub branch_id: Option<String>,
    pub linked_record: Option<FeedRecord>,
    pub version: i64,
    pub created_at: i64,
    pub appreciation_count: i64,
    pub comment_count: i64,
    pub appreciated: bool,
    pub followed: bool,
    pub topics: Vec<String>,
    pub comments: Vec<FeedComment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedFilter {
    All,
    Following,
    Team,
    Records,
}
impl FeedFilter {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "all" => Ok(Self::All),
            "following" => Ok(Self::Following),
            "team" => Ok(Self::Team),
            "records" => Ok(Self::Records),
            _ => Err(BusinessError::invalid(
                "INVALID_FILTER",
                "Choose a valid feed filter.",
            )),
        }
    }
}

fn comment_row(row: sqlx::postgres::PgRow) -> Result<FeedComment, sqlx::Error> {
    Ok(FeedComment {
        id: row.try_get(0)?,
        post_id: row.try_get(1)?,
        author_id: row.try_get(2)?,
        author_name: row.try_get(3)?,
        body: row.try_get(4)?,
        created_at: row.try_get(5)?,
    })
}

async fn comments(
    c: &mut PgConnection,
    post_id: &str,
) -> Result<Vec<FeedComment>, Error> {
    sqlx::query(
        "SELECT id,post_id,author_id,author_name,body,created_at FROM social_comments WHERE post_id=$1 AND state='published' ORDER BY created_at,id LIMIT 50",
    )
    .bind(post_id)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(comment_row)
    .collect::<Result<Vec<_>, sqlx::Error>>()
    .map_err(Into::into)
}

/// Bounded Company feed. The caller must have authorized the actor first; this
/// function selects only Social-owned rows and never widens a branch audience.
pub async fn read_feed(
    c: &mut PgConnection,
    actor: &SocialActor,
    filter: FeedFilter,
    cursor: Option<&FeedCursor>,
    limit: usize,
) -> Result<Vec<FeedPost>, Error> {
    let limit = limit.clamp(1, 100);
    let following = filter == FeedFilter::Following;
    let records = filter == FeedFilter::Records;
    let rows = sqlx::query(
        "SELECT p.id,p.kind,p.author_id,p.author_name,p.body,p.branch_id,p.linked_module,p.linked_record_id,p.linked_record_label,p.version,p.created_at,
           (SELECT count(*) FROM social_reactions r WHERE r.post_id=p.id AND r.kind='appreciate') appreciation_count,
           (SELECT count(*) FROM social_comments c WHERE c.post_id=p.id AND c.state='published') comment_count,
           EXISTS(SELECT 1 FROM social_reactions r WHERE r.post_id=p.id AND r.principal_id=$2 AND r.kind='appreciate') appreciated,
           EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=$2 AND f.target_type='post' AND f.target_id=p.id) followed,
           COALESCE((SELECT string_agg(tag,',' ORDER BY tag) FROM social_post_topics pt WHERE pt.post_id=p.id),'') topic_list
         FROM social_posts p
         WHERE p.company_id=$1 AND p.state='published'
           AND (p.audience_kind='company' OR ($3::text IS NOT NULL AND p.branch_id=$3) OR ($4::text IS NOT NULL AND p.department_id=$4) OR (p.branch_id IS NULL AND p.department_id IS NULL AND p.author_id=$2))
           AND (NOT $5 OR EXISTS(SELECT 1 FROM social_follows f WHERE f.principal_id=$2 AND f.target_type='post' AND f.target_id=p.id))
           AND (NOT $6 OR p.linked_record_id IS NOT NULL)
           AND ($7::bigint IS NULL OR (p.created_at,p.id)<($7,$8))
         ORDER BY p.created_at DESC,p.id DESC LIMIT $9",
    )
    .bind(&actor.company_id)
    .bind(&actor.user_id)
    .bind(actor.branch_id.as_deref())
    .bind(actor.department_id.as_deref())
    .bind(following)
    .bind(records)
    .bind(cursor.map(|value| value.at))
    .bind(cursor.map(|value| value.id.as_str()))
    .bind((limit + 1) as i64)
    .fetch_all(&mut *c)
    .await?;
    let mut posts = Vec::with_capacity(rows.len().min(limit));
    for row in rows.into_iter().take(limit) {
        let id: String = row.try_get(0)?;
        let linked_module: Option<String> = row.try_get(6)?;
        let linked_id: Option<String> = row.try_get(7)?;
        let linked_label: Option<String> = row.try_get(8)?;
        let topic_list: String = row.try_get(15)?;
        let comments = comments(c, &id).await?;
        posts.push(FeedPost {
            id: id.clone(),
            kind: row.try_get(1)?,
            author_id: row.try_get(2)?,
            author_name: row.try_get(3)?,
            body: row.try_get(4)?,
            audience: if row.try_get::<Option<String>, _>(5)?.is_some() {
                "Branch"
            } else {
                "Holding"
            },
            branch_id: row.try_get(5)?,
            linked_record: linked_module
                .zip(linked_id)
                .zip(linked_label)
                .map(|((module, id), label)| FeedRecord { module, id, label }),
            version: row.try_get(9)?,
            created_at: row.try_get(10)?,
            appreciation_count: row.try_get(11)?,
            comment_count: row.try_get(12)?,
            appreciated: row.try_get(13)?,
            followed: row.try_get(14)?,
            topics: topic_list
                .split(',')
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect(),
            comments,
        });
    }
    Ok(posts)
}

pub async fn read_thread(
    c: &mut PgConnection,
    actor: &SocialActor,
    post_id: &str,
) -> Result<Vec<FeedPost>, Error> {
    let posts = read_feed(c, actor, FeedFilter::All, None, 100).await?;
    Ok(posts.into_iter().filter(|post| post.id == post_id).collect())
}

pub async fn read_comments(
    c: &mut PgConnection,
    post_id: &str,
    limit: usize,
) -> Result<Vec<FeedComment>, Error> {
    let limit = limit.clamp(1, 100);
    sqlx::query(
        "SELECT id,post_id,author_id,author_name,body,created_at FROM social_comments WHERE post_id=$1 AND state='published' ORDER BY created_at,id LIMIT $2",
    )
    .bind(post_id)
    .bind(limit as i64)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(comment_row)
    .collect::<Result<Vec<_>, sqlx::Error>>()
    .map_err(Into::into)
}

/// Stage only Social-owned rows in the caller's retained transaction.
pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &SocialActor,
    key: &CommandKey,
    plan: &OwnerPlan,
    now: i64,
) -> Result<Receipt, Error> {
    unit(c).await?;
    let expected = plan_receipt(key, plan)?;
    let result: Result<Receipt, Error> = match plan {
        OwnerPlan::CreatePost {
            id,
            author_name,
            company_id,
            branch_id,
            department_id,
            audience,
            body,
            tags,
            mentions: mention_rows,
            linked_record,
        } => {
            sqlx::query(
                "INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,linked_module,linked_record_id,linked_record_label,state,version,last_command_id,created_at,updated_at,audience_kind,purpose,mentions_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'published',1,$12,$13,$13,$14,'update',$15)",
            )
            .bind(id)
            .bind(if linked_record.is_some() { "record" } else { "team" })
            .bind(&actor.user_id)
            .bind(author_name)
            .bind(company_id)
            .bind(branch_id)
            .bind(department_id)
            .bind(body)
            .bind(linked_record.as_ref().map(|record| record.module.as_str()))
            .bind(linked_record.as_ref().map(|record| record.id.as_str()))
            .bind(linked_record.as_ref().map(|record| record.label.as_str()))
            .bind(key.as_str())
            .bind(now)
            .bind(audience.stored())
            .bind(serde_json::to_string(mention_rows).map_err(|_| changed())?)
            .execute(&mut *c)
            .await?;
            post_topics(c, id, tags, &actor.user_id, now).await?;
            mentions(c, "post", id, mention_rows, &actor.user_id, &actor.company_id, now).await?;
            Ok(expected.clone())
        }
        OwnerPlan::CreateComment {
            id,
            post_id,
            company_id: _,
            author_name,
            body,
            tags,
            mentions: mention_rows,
            parent_comment_id,
        } => {
            sqlx::query(
                "INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,last_command_id,created_at,updated_at,version,mentions_json,parent_comment_id) VALUES($1,$2,$3,$4,$5,'published',$6,$7,$7,1,$8,$9)",
            )
            .bind(id)
            .bind(post_id)
            .bind(&actor.user_id)
            .bind(author_name)
            .bind(body)
            .bind(key.as_str())
            .bind(now)
            .bind(serde_json::to_string(mention_rows).map_err(|_| changed())?)
            .bind(parent_comment_id)
            .execute(&mut *c)
            .await?;
            comment_topics(c, id, tags, &actor.user_id, now).await?;
            mentions(c, "comment", id, mention_rows, &actor.user_id, &actor.company_id, now).await?;
            Ok(expected.clone())
        }
        OwnerPlan::UpdateComment {
            id,
            post_id,
            expected_version,
            body,
            tags,
            mentions: mention_rows,
        } => {
            let changed_rows = sqlx::query(
                "UPDATE social_comments SET body=$1,mentions_json=$2,version=version+1,updated_at=$3,last_command_id=$4 WHERE id=$5 AND post_id=$6 AND author_id=$7 AND state='published' AND version=$8",
            )
            .bind(body)
            .bind(serde_json::to_string(mention_rows).map_err(|_| changed())?)
            .bind(now)
            .bind(key.as_str())
            .bind(id)
            .bind(post_id)
            .bind(&actor.user_id)
            .bind(expected_version)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if changed_rows != 1 {
                return Err(conflict());
            }
            clear_mentions(c, "comment", id).await?;
            sqlx::query("DELETE FROM social_comment_topics WHERE comment_id=$1")
                .bind(id)
                .execute(&mut *c)
                .await?;
            comment_topics(c, id, tags, &actor.user_id, now).await?;
            mentions(c, "comment", id, mention_rows, &actor.user_id, &actor.company_id, now).await?;
            Ok(expected.clone())
        }
        OwnerPlan::TombstoneComment {
            id,
            post_id,
            expected_version,
        } => {
            let changed_rows = sqlx::query(
                "UPDATE social_comments SET state='tombstoned',version=version+1,updated_at=$1,last_command_id=$2 WHERE id=$3 AND post_id=$4 AND state='published' AND version=$5",
            )
            .bind(now)
            .bind(key.as_str())
            .bind(id)
            .bind(post_id)
            .bind(expected_version)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if changed_rows != 1 {
                return Err(conflict());
            }
            Ok(expected.clone())
        }
        OwnerPlan::SetReaction {
            post_id,
            version: _,
            active,
        } => {
            if *active {
                sqlx::query(
                    "INSERT INTO social_reactions(post_id,principal_id,kind,created_at) VALUES($1,$2,'appreciate',$3) ON CONFLICT DO NOTHING",
                )
                .bind(post_id)
                .bind(&actor.user_id)
                .bind(now)
                .execute(&mut *c)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM social_reactions WHERE post_id=$1 AND principal_id=$2 AND kind='appreciate'",
                )
                .bind(post_id)
                .bind(&actor.user_id)
                .execute(&mut *c)
                .await?;
            }
            Ok(expected.clone())
        }
        OwnerPlan::SetFollow {
            target,
            target_id,
            version: _,
            active,
        } => {
            if *active {
                sqlx::query(
                    "INSERT INTO social_follows(principal_id,target_type,target_id,created_at) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
                )
                .bind(&actor.user_id)
                .bind(target.as_str())
                .bind(target_id)
                .bind(now)
                .execute(&mut *c)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM social_follows WHERE principal_id=$1 AND target_type=$2 AND target_id=$3",
                )
                .bind(&actor.user_id)
                .bind(target.as_str())
                .bind(target_id)
                .execute(&mut *c)
                .await?;
            }
            Ok(expected.clone())
        }
    };
    let receipt = result?;
    if receipt != expected {
        return Err(changed());
    }
    Ok(receipt)
}

pub struct CurrentSocialEffects {
    unit: Unit,
    images: Vec<(String, String)>,
}
impl CurrentSocialEffects {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for (key, expected) in &self.images {
            if image(c, key).await?.as_deref() != Some(expected.as_str()) {
                return Err(changed());
            }
        }
        Ok(())
    }
}

async fn image(c: &mut PgConnection, key: &str) -> Result<Option<String>, Error> {
    let (table, id) = key.split_once(':').ok_or_else(changed)?;
    let sql = match table {
        "post" => "SELECT to_jsonb(p)::text FROM social_posts p WHERE p.id=$1",
        "comment" => "SELECT to_jsonb(p)::text FROM social_comments p WHERE p.id=$1",
        "notice" => "SELECT to_jsonb(p)::text FROM staff_notifications p WHERE p.id=$1",
        _ => return Err(changed()),
    };
    Ok(sqlx::query_scalar(sql).bind(id).fetch_optional(c).await?)
}

async fn capture(
    c: &mut PgConnection,
    images: &mut Vec<(String, String)>,
    key: String,
) -> Result<(), Error> {
    let value = image(c, &key).await?.ok_or_else(changed)?;
    images.push((key, value));
    Ok(())
}

/// Retain the exact intended post/comment and person-mention notices. This does
/// not validate Workflow, IAM, purchase or another owner's source evidence.
pub async fn retain_effects(
    c: &mut PgConnection,
    plan: &OwnerPlan,
    receipt: &Receipt,
) -> Result<CurrentSocialEffects, Error> {
    let current_unit = unit(c).await?;
    let mut images = Vec::new();
    match plan {
        OwnerPlan::CreatePost { id, mentions, .. } => {
            if receipt.id != *id {
                return Err(changed());
            }
            capture(c, &mut images, format!("post:{id}")).await?;
            for mention in mentions
                .iter()
                .filter(|mention| mention.kind == MentionKind::Person)
            {
                capture(
                    c,
                    &mut images,
                    format!("notice:mention-post-{id}-{}", mention.target_id),
                )
                .await?;
            }
        }
        OwnerPlan::CreateComment {
            id,
            post_id,
            company_id,
            mentions,
            ..
        } => {
            if receipt.id != *id {
                return Err(changed());
            }
            capture(c, &mut images, format!("comment:{id}")).await?;
            for mention in mentions
                .iter()
                .filter(|mention| mention.kind == MentionKind::Person)
            {
                capture(
                    c,
                    &mut images,
                    format!("notice:mention-comment-{id}-{}", mention.target_id),
                )
                .await?;
            }
            if read_post(c, &CompanyId::new(company_id.clone())?, post_id)
                .await?
                .is_none()
            {
                return Err(changed());
            }
        }
        OwnerPlan::UpdateComment { id, .. } | OwnerPlan::TombstoneComment { id, .. } => {
            if receipt.id != *id {
                return Err(changed());
            }
            capture(c, &mut images, format!("comment:{id}")).await?;
        }
        OwnerPlan::SetReaction { post_id, .. } | OwnerPlan::SetFollow { target_id: post_id, .. } => {
            if receipt.id != *post_id {
                return Err(changed());
            }
        }
    }
    Ok(CurrentSocialEffects {
        unit: current_unit,
        images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_and_mentions_round_trip_from_stored_json() {
        assert_eq!(audience(Some("company"), None).unwrap(), Audience::Holding);
        assert_eq!(audience(Some("branch"), Some("b")).unwrap(), Audience::Branch);
        assert_eq!(audience(Some("department"), Some("b")).unwrap(), Audience::Branch);
        let rows = mention_rows(r#"[{"kind":"person","targetId":"user","label":"User"}]"#)
            .unwrap();
        assert_eq!(rows[0].kind, MentionKind::Person);
        assert_eq!(rows[0].label, "User");
    }

    #[test]
    fn post_and_comment_bounds_match_the_original_schema() {
        assert_eq!(FeedFilter::parse("following").unwrap(), FeedFilter::Following);
        assert_eq!(FeedFilter::parse("bogus").unwrap_err().code, "INVALID_FILTER");
    }
}
