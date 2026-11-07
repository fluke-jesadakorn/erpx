//! Handbook owner rows. No IAM, purchase, Organization or People query is made
//! here; composition supplies the current authority result and retains it.
use crate::domain::handbook::{
    ArticleSnapshot, AuthorizedHandbookActor, Category, DraftContent, ListCursor, OwnerPlan,
    Receipt, plan_receipt, publication_ready, require_edit, require_manage_editors, require_read,
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
        return Err(PersistenceError::new("HANDBOOK_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
    )
    .fetch_one(c)
    .await?)
}

/// Acquire before the first selected IAM/People/purchase snapshot.
pub async fn lock_handbook_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("HANDBOOK_TRANSACTION_REQUIRED").into());
    }
    sqlx::query(
        "LOCK TABLE handbook_articles,handbook_revisions,handbook_settings,handbook_editors IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "HANDBOOK_SOURCE_CHANGED",
        "The original handbook source or current editor authority changed.",
    )
    .into()
}
fn conflict() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "Review the current article before saving or changing availability.",
    )
    .into()
}
fn not_found() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ARTICLE_NOT_FOUND",
        "Article not found.",
    )
    .into()
}
fn content_required() -> Error {
    BusinessError::invalid(
        "ARTICLE_CONTENT_REQUIRED",
        "Add a title and article content before publishing.",
    )
    .into()
}

async fn article_image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<String>, Error> {
    let row = sqlx::query(
        "WITH s AS MATERIALIZED (SELECT a.*,r.title draft_title,r.category draft_category,r.body draft_body FROM handbook_articles a LEFT JOIN handbook_revisions r ON r.article_id=a.id AND r.company_id=a.company_id AND r.revision=a.draft_revision WHERE a.id=$1 AND a.company_id=$2),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| row.try_get::<Option<String>, _>(0))
        .transpose()?
        .flatten()
        .map(|image| {
            if image.is_empty() {
                Err(changed())
            } else {
                Ok(image)
            }
        })
        .transpose()
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.get(key).and_then(Value::as_str).ok_or_else(changed)
}
fn optional_i64(v: &Value, key: &str) -> Result<Option<i64>, Error> {
    match v.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_i64()
            .filter(|value| *value > 0)
            .map(Some)
            .ok_or_else(changed),
        _ => Err(changed()),
    }
}
fn i64_value(v: &Value, key: &str) -> Result<i64, Error> {
    v.get(key)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(changed)
}
fn archived_value(v: &Value, key: &str) -> Result<bool, Error> {
    match v.get(key).and_then(Value::as_i64) {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(changed()),
    }
}

/// Read the current draft article state. A missing draft revision is a changed
/// source, never an absent article.
pub async fn read_article(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<ArticleSnapshot>, Error> {
    let Some(raw) = article_image(c, company, id).await? else {
        return Ok(None);
    };
    let v: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
    let draft_revision = i64_value(&v, "draft_revision")?;
    let title = text(&v, "draft_title")?;
    let category = Category::parse(text(&v, "draft_category")?)?;
    let body = text(&v, "draft_body")?;
    Ok(Some(ArticleSnapshot {
        id: text(&v, "id")?.into(),
        version: i64_value(&v, "version")?,
        draft_revision,
        published_revision: optional_i64(&v, "published_revision")?,
        archived: archived_value(&v, "archived")?,
        draft: DraftContent {
            title: title.into(),
            category,
            body: body.into(),
        },
    }))
}

pub async fn read_settings_version(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<i64, Error> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE((SELECT version FROM handbook_settings WHERE company_id=$1),0)",
    )
    .bind(company.as_str())
    .fetch_one(c)
    .await?)
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookEditorRow {
    pub user_id: String,
    pub granted_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookEditorOptions {
    pub version: i64,
    pub editors: Vec<HandbookEditorRow>,
}

pub async fn read_editor_options(
    c: &mut PgConnection,
    actor: &AuthorizedHandbookActor,
    company: &CompanyId,
) -> Result<HandbookEditorOptions, Error> {
    if actor.company_id != company.as_str() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into());
    }
    require_manage_editors(actor)?;
    let version = read_settings_version(c, company).await?;
    let editors = sqlx::query(
        "SELECT user_id,granted_at FROM handbook_editors WHERE company_id=$1 ORDER BY user_id",
    )
    .bind(company.as_str())
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(|row| {
        Ok(HandbookEditorRow {
            user_id: row.try_get(0)?,
            granted_at: row.try_get(1)?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(HandbookEditorOptions { version, editors })
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookRevisionRow {
    pub revision: i64,
    pub title: String,
    pub category: String,
    pub body: String,
    pub created_by: String,
    pub created_at: i64,
    pub published_by: Option<String>,
    pub published_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookHistoryRow {
    pub revision: i64,
    pub title: String,
    pub category: String,
    pub created_by: String,
    pub created_at: i64,
    pub published_by: Option<String>,
    pub published_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookArticleSummary {
    pub id: String,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_revision: Option<i64>,
    pub published_revision: Option<i64>,
    pub archived: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookArticleRead {
    pub article: HandbookArticleSummary,
    pub revision: HandbookRevisionRow,
    pub history: Vec<HandbookHistoryRow>,
}

async fn revision(
    c: &mut PgConnection,
    company: &CompanyId,
    article_id: &str,
    revision: i64,
    published_only: bool,
) -> Result<Option<HandbookRevisionRow>, Error> {
    let row = sqlx::query(
        "SELECT revision,title,category,body,created_by,created_at,published_by,published_at FROM handbook_revisions WHERE article_id=$1 AND company_id=$2 AND revision=$3 AND (NOT $4 OR published_at IS NOT NULL)",
    )
    .bind(article_id)
    .bind(company.as_str())
    .bind(revision)
    .bind(published_only)
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok(HandbookRevisionRow {
            revision: row.try_get(0)?,
            title: row.try_get(1)?,
            category: row.try_get(2)?,
            body: row.try_get(3)?,
            created_by: row.try_get(4)?,
            created_at: row.try_get(5)?,
            published_by: row.try_get(6)?,
            published_at: row.try_get(7)?,
        })
    })
    .transpose()
}

pub async fn read_article_detail(
    c: &mut PgConnection,
    actor: &AuthorizedHandbookActor,
    company: &CompanyId,
    id: &str,
    editing: bool,
    requested_revision: Option<i64>,
) -> Result<HandbookArticleRead, Error> {
    if actor.company_id != company.as_str() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into());
    }
    if editing {
        require_edit(actor)?;
    } else {
        require_read(actor)?;
    }
    let Some(article) = read_article(c, company, id).await? else {
        return Err(not_found());
    };
    if !editing && article.archived {
        return Err(not_found());
    }
    let published_revision = article.published_revision.filter(|revision| *revision > 0);
    if !editing && published_revision.is_none() {
        return Err(not_found());
    }
    if requested_revision.is_some_and(|revision| revision < 1) {
        return Err(not_found());
    }
    let selected = requested_revision.unwrap_or(if editing {
        article.draft_revision
    } else {
        published_revision.ok_or_else(not_found)?
    });
    let revision = revision(c, company, id, selected, !editing)
        .await?
        .ok_or_else(not_found)?;
    let history = sqlx::query(
        "SELECT revision,title,category,created_by,created_at,published_by,published_at FROM handbook_revisions WHERE article_id=$1 AND company_id=$2 AND (NOT $3 OR published_at IS NOT NULL) ORDER BY revision DESC LIMIT 100",
    )
    .bind(id)
    .bind(company.as_str())
    .bind(!editing)
    .fetch_all(&mut *c)
    .await?
    .into_iter()
    .map(|row| {
        Ok(HandbookHistoryRow {
            revision: row.try_get(0)?,
            title: row.try_get(1)?,
            category: row.try_get(2)?,
            created_by: row.try_get(3)?,
            created_at: row.try_get(4)?,
            published_by: row.try_get(5)?,
            published_at: row.try_get(6)?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let summary = if editing {
        HandbookArticleSummary {
            id: article.id,
            version: article.version,
            draft_revision: Some(article.draft_revision),
            published_revision: article.published_revision,
            archived: i64::from(article.archived),
        }
    } else {
        HandbookArticleSummary {
            id: article.id,
            version: published_revision.ok_or_else(not_found)?,
            draft_revision: None,
            published_revision,
            archived: 0,
        }
    };
    Ok(HandbookArticleRead {
        article: summary,
        revision,
        history,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookListRow {
    pub id: String,
    pub page_key: String,
    pub version: i64,
    pub archived: i64,
    pub revision: i64,
    pub title: String,
    pub category: String,
    pub excerpt: String,
    pub published_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HandbookListPage {
    pub items: Vec<HandbookListRow>,
    #[serde(rename = "previousCursor")]
    pub previous_cursor: Option<String>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

pub async fn read_list(
    c: &mut PgConnection,
    actor: &AuthorizedHandbookActor,
    company: &CompanyId,
    query: &str,
    editing: bool,
    cursor: Option<&ListCursor>,
    limit: usize,
) -> Result<HandbookListPage, Error> {
    if actor.company_id != company.as_str() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MEMBERSHIP_REQUIRED",
            "Current company membership is required.",
        )
        .into());
    }
    if editing {
        require_edit(actor)?;
    } else {
        require_read(actor)?;
    }
    let limit = limit.clamp(1, 100);
    let search = query
        .trim()
        .to_lowercase()
        .chars()
        .take(100)
        .collect::<String>();
    let rows = sqlx::query(
        "WITH rows AS (
           SELECT a.id,a.version,a.archived,
             CASE WHEN $2 THEN a.draft_revision ELSE a.published_revision END revision,
             r.title,r.category,left(r.body,220) excerpt,r.published_at,
             CASE WHEN $2 THEN a.updated_at ELSE COALESCE(r.published_at,a.updated_at) END sort_at
           FROM handbook_articles a
           JOIN handbook_revisions r ON r.article_id=a.id AND r.company_id=a.company_id
             AND r.revision=CASE WHEN $2 THEN a.draft_revision ELSE a.published_revision END
           WHERE a.company_id=$1 AND ($2 OR (a.archived=0 AND a.published_revision IS NOT NULL))
             AND strpos(lower(r.title||' '||r.body||' '||r.category),$3)>0
         )
         SELECT id,version,archived,revision,title,category,excerpt,published_at,sort_at
         FROM rows WHERE ($4::bigint IS NULL OR (sort_at,id)<($4,$5))
         ORDER BY sort_at DESC,id DESC LIMIT $6",
    )
    .bind(company.as_str())
    .bind(editing)
    .bind(&search)
    .bind(cursor.map(|value| value.at))
    .bind(cursor.map(|value| value.id.as_str()))
    .bind((limit + 1) as i64)
    .fetch_all(&mut *c)
    .await?;
    let has_more = rows.len() > limit;
    let mut items = Vec::with_capacity(rows.len().min(limit));
    let mut last: Option<(i64, String)> = None;
    for row in rows.into_iter().take(limit) {
        let at: i64 = row.try_get("sort_at")?;
        let id: String = row.try_get("id")?;
        let stored_version: i64 = row.try_get("version")?;
        let revision: i64 = row.try_get("revision")?;
        last = Some((at, id.clone()));
        items.push(HandbookListRow {
            id,
            page_key: row.try_get("title")?,
            version: if editing { stored_version } else { revision },
            archived: row.try_get("archived")?,
            revision,
            title: row.try_get("title")?,
            category: row.try_get("category")?,
            excerpt: row.try_get("excerpt")?,
            published_at: row.try_get("published_at")?,
        });
    }
    let next_cursor = if has_more {
        last.map(|(at, id)| ListCursor { at, id }.encode())
    } else {
        None
    };
    Ok(HandbookListPage {
        items,
        previous_cursor: None,
        next_cursor,
        has_more,
    })
}

/// Stage only Handbook-owned rows in the caller's retained transaction.
pub async fn apply_plan(
    c: &mut PgConnection,
    actor: &AuthorizedHandbookActor,
    key: &CommandKey,
    plan: &OwnerPlan,
    now: i64,
) -> Result<Receipt, Error> {
    unit(c).await?;
    let expected = plan_receipt(key, plan)?;
    let result: Result<Receipt, Error> = match plan {
        OwnerPlan::CreateArticle { id, content } => {
            sqlx::query(
                "INSERT INTO handbook_articles(id,company_id,version,draft_revision,published_revision,archived,created_by,created_at,updated_at) VALUES($1,$2,1,1,NULL,0,$3,$4,$4)",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            sqlx::query(
                "INSERT INTO handbook_revisions(company_id,article_id,revision,title,category,body,created_by,created_at) VALUES($1,$2,1,$3,$4,$5,$6,$7)",
            )
            .bind(&actor.company_id)
            .bind(id)
            .bind(&content.title)
            .bind(content.category.as_str())
            .bind(&content.body)
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            Ok(Receipt {
                id: id.clone(),
                version: 1,
                revision: 1,
                status: "draft",
            })
        }
        OwnerPlan::UpdateArticle {
            id,
            expected_version,
            revision,
            content,
        } => {
            let row = sqlx::query(
                "UPDATE handbook_articles SET draft_revision=$4,version=version+1,updated_at=$5 WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING version,draft_revision",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(expected_version)
            .bind(revision)
            .bind(now)
            .fetch_optional(&mut *c)
            .await?;
            let Some(row) = row else {
                return Err(conflict());
            };
            let version: i64 = row.try_get(0)?;
            let stored_revision: i64 = row.try_get(1)?;
            if stored_revision != *revision {
                return Err(changed());
            }
            sqlx::query(
                "INSERT INTO handbook_revisions(company_id,article_id,revision,title,category,body,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&actor.company_id)
            .bind(id)
            .bind(stored_revision)
            .bind(&content.title)
            .bind(content.category.as_str())
            .bind(&content.body)
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?;
            Ok(Receipt {
                id: id.clone(),
                version,
                revision: stored_revision,
                status: "draft",
            })
        }
        OwnerPlan::PublishArticle {
            id,
            expected_version,
            revision,
        } => {
            let draft = sqlx::query(
                "SELECT title,body FROM handbook_revisions WHERE article_id=$1 AND company_id=$2 AND revision=$3",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(revision)
            .fetch_optional(&mut *c)
            .await?
            .ok_or_else(changed)?;
            let title: String = draft.try_get(0)?;
            let body: String = draft.try_get(1)?;
            if !publication_ready(&title, &body) {
                return Err(content_required());
            }
            let row = sqlx::query(
                "UPDATE handbook_articles SET published_revision=draft_revision,archived=0,version=version+1,updated_at=$4 WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING version,draft_revision",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(expected_version)
            .bind(now)
            .fetch_optional(&mut *c)
            .await?;
            let Some(row) = row else {
                return Err(conflict());
            };
            let version: i64 = row.try_get(0)?;
            let published: i64 = row.try_get(1)?;
            if published != *revision {
                return Err(changed());
            }
            let updated = sqlx::query(
                "UPDATE handbook_revisions SET published_by=COALESCE(published_by,$1),published_at=COALESCE(published_at,$2) WHERE article_id=$3 AND company_id=$4 AND revision=$5",
            )
            .bind(&actor.user_id)
            .bind(now)
            .bind(id)
            .bind(&actor.company_id)
            .bind(revision)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(changed());
            }
            Ok(Receipt {
                id: id.clone(),
                version,
                revision: published,
                status: "published",
            })
        }
        OwnerPlan::SetArchived {
            id,
            expected_version,
            archived,
        } => {
            let row = sqlx::query(
                "UPDATE handbook_articles SET archived=$4,version=version+1,updated_at=$5 WHERE id=$1 AND company_id=$2 AND version=$3 RETURNING version",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(expected_version)
            .bind(if *archived { 1_i64 } else { 0_i64 })
            .bind(now)
            .fetch_optional(&mut *c)
            .await?;
            let Some(row) = row else {
                return Err(conflict());
            };
            let version: i64 = row.try_get(0)?;
            Ok(Receipt {
                id: id.clone(),
                version,
                revision: 0,
                status: if *archived { "archived" } else { "restored" },
            })
        }
        OwnerPlan::SetEditor {
            user_id,
            active,
            expected_version,
        } => {
            let current =
                sqlx::query("SELECT version FROM handbook_settings WHERE company_id=$1 FOR UPDATE")
                    .bind(&actor.company_id)
                    .fetch_optional(&mut *c)
                    .await?
                    .map(|row| row.try_get::<i64, _>(0))
                    .transpose()?;
            let version = match (current, *expected_version) {
                (None, 0) => {
                    let inserted = sqlx::query(
                        "INSERT INTO handbook_settings(company_id,version) VALUES($1,1) ON CONFLICT(company_id) DO NOTHING",
                    )
                    .bind(&actor.company_id)
                    .execute(&mut *c)
                    .await?
                    .rows_affected();
                    if inserted != 1 {
                        return Err(conflict());
                    }
                    1
                }
                (Some(version), expected) if version == expected => {
                    sqlx::query(
                        "UPDATE handbook_settings SET version=version+1 WHERE company_id=$1 AND version=$2",
                    )
                    .bind(&actor.company_id)
                    .bind(expected)
                    .execute(&mut *c)
                    .await?;
                    version
                        .checked_add(1)
                        .filter(|next| *next <= crate::domain::handbook::MAX_SAFE_VERSION)
                        .ok_or_else(changed)?
                }
                (None, _) | (Some(_), _) => return Err(conflict()),
            };
            if *active {
                sqlx::query(
                    "INSERT INTO handbook_editors(company_id,user_id,granted_by,granted_at) VALUES($1,$2,$3,$4) ON CONFLICT(company_id,user_id) DO UPDATE SET granted_by=excluded.granted_by,granted_at=excluded.granted_at",
                )
                .bind(&actor.company_id)
                .bind(user_id)
                .bind(&actor.user_id)
                .bind(now)
                .execute(&mut *c)
                .await?;
            } else {
                sqlx::query("DELETE FROM handbook_editors WHERE company_id=$1 AND user_id=$2")
                    .bind(&actor.company_id)
                    .bind(user_id)
                    .execute(&mut *c)
                    .await?;
            }
            Ok(Receipt {
                id: user_id.clone(),
                version,
                revision: 0,
                status: if *active { "active" } else { "inactive" },
            })
        }
    };
    let result = result?;
    if result != expected {
        return Err(changed());
    }
    Ok(result)
}

#[derive(Clone, Debug)]
enum ImageKey {
    Article { id: String },
    Revision { article_id: String, revision: i64 },
    Settings { company_id: String },
    Editor { company_id: String, user_id: String },
}

#[derive(Clone, Debug)]
struct OwnedImage {
    key: ImageKey,
    image: String,
}

async fn owned_image(c: &mut PgConnection, key: &ImageKey) -> Result<Option<String>, Error> {
    let sql = match key {
        ImageKey::Article { .. } => {
            "WITH s AS MATERIALIZED (SELECT * FROM handbook_articles WHERE id=$1),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        ImageKey::Revision { .. } => {
            "WITH s AS MATERIALIZED (SELECT * FROM handbook_revisions WHERE article_id=$1 AND revision=$2),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        ImageKey::Settings { .. } => {
            "WITH s AS MATERIALIZED (SELECT * FROM handbook_settings WHERE company_id=$1),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
        ImageKey::Editor { .. } => {
            "WITH s AS MATERIALIZED (SELECT * FROM handbook_editors WHERE company_id=$1 AND user_id=$2),p AS (SELECT to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(image)<=2097152 THEN image END FROM p"
        }
    };
    let mut query = sqlx::query(sql);
    match key {
        ImageKey::Article { id } => query = query.bind(id),
        ImageKey::Revision {
            article_id,
            revision,
        } => query = query.bind(article_id).bind(revision),
        ImageKey::Settings { company_id } => query = query.bind(company_id),
        ImageKey::Editor {
            company_id,
            user_id,
        } => query = query.bind(company_id).bind(user_id),
    }
    match query.fetch_optional(&mut *c).await? {
        None => Ok(None),
        Some(row) => Ok(Some(
            row.try_get::<Option<String>, _>(0)?.ok_or_else(changed)?,
        )),
    }
}

/// Exact post-effect images retained until the selected IAM and purchase proof
/// have been rechecked after deferred constraints flush.
pub struct CurrentHandbookEffects {
    unit: Unit,
    images: Vec<OwnedImage>,
    absent_editor: Option<(String, String)>,
}
impl CurrentHandbookEffects {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        for owned in &self.images {
            if owned_image(c, &owned.key).await?.as_deref() != Some(owned.image.as_str()) {
                return Err(changed());
            }
        }
        if let Some((company, user)) = &self.absent_editor
            && owned_image(
                c,
                &ImageKey::Editor {
                    company_id: company.clone(),
                    user_id: user.clone(),
                },
            )
            .await?
            .is_some()
        {
            return Err(changed());
        }
        Ok(())
    }
}

fn image_matches(v: &Value, expected: &[(&str, Value)]) -> bool {
    expected
        .iter()
        .all(|(key, value)| v.get(*key) == Some(value))
}

/// Retain the exact intended article/revision/settings/editor rows. This does
/// not validate Workflow, IAM, purchase or another owner's source evidence.
pub async fn retain_effects(
    c: &mut PgConnection,
    actor: &AuthorizedHandbookActor,
    _key: &CommandKey,
    plan: &OwnerPlan,
    receipt: &Receipt,
    now: i64,
) -> Result<CurrentHandbookEffects, Error> {
    let current_unit = unit(c).await?;
    let expected_id = match plan {
        OwnerPlan::CreateArticle { id, .. }
        | OwnerPlan::UpdateArticle { id, .. }
        | OwnerPlan::PublishArticle { id, .. }
        | OwnerPlan::SetArchived { id, .. } => id,
        OwnerPlan::SetEditor { user_id, .. } => user_id,
    };
    if receipt.id != *expected_id {
        return Err(changed());
    }
    let mut images = vec![];
    let mut absent_editor = None;
    match plan {
        OwnerPlan::CreateArticle { id, content } => {
            let article_key = ImageKey::Article { id: id.clone() };
            let raw = owned_image(c, &article_key).await?.ok_or_else(changed)?;
            let article: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &article,
                &[
                    ("id", serde_json::json!(id)),
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("version", serde_json::json!(1)),
                    ("draft_revision", serde_json::json!(1)),
                    ("published_revision", Value::Null),
                    ("archived", serde_json::json!(0)),
                    ("created_by", serde_json::json!(actor.user_id)),
                    ("created_at", serde_json::json!(now)),
                    ("updated_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: article_key,
                image: raw,
            });
            let revision_key = ImageKey::Revision {
                article_id: id.clone(),
                revision: 1,
            };
            let raw = owned_image(c, &revision_key).await?.ok_or_else(changed)?;
            let revision: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &revision,
                &[
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("article_id", serde_json::json!(id)),
                    ("revision", serde_json::json!(1)),
                    ("title", serde_json::json!(content.title)),
                    ("category", serde_json::json!(content.category.as_str())),
                    ("body", serde_json::json!(content.body)),
                    ("created_by", serde_json::json!(actor.user_id)),
                    ("created_at", serde_json::json!(now)),
                    ("published_by", Value::Null),
                    ("published_at", Value::Null),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: revision_key,
                image: raw,
            });
        }
        OwnerPlan::UpdateArticle {
            id,
            expected_version,
            revision,
            content,
        } => {
            let article_key = ImageKey::Article { id: id.clone() };
            let raw = owned_image(c, &article_key).await?.ok_or_else(changed)?;
            let article: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &article,
                &[
                    ("id", serde_json::json!(id)),
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("version", serde_json::json!(expected_version + 1)),
                    ("draft_revision", serde_json::json!(revision)),
                    ("updated_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: article_key,
                image: raw,
            });
            let revision_key = ImageKey::Revision {
                article_id: id.clone(),
                revision: *revision,
            };
            let raw = owned_image(c, &revision_key).await?.ok_or_else(changed)?;
            let stored: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &stored,
                &[
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("article_id", serde_json::json!(id)),
                    ("revision", serde_json::json!(revision)),
                    ("title", serde_json::json!(content.title)),
                    ("category", serde_json::json!(content.category.as_str())),
                    ("body", serde_json::json!(content.body)),
                    ("created_by", serde_json::json!(actor.user_id)),
                    ("created_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: revision_key,
                image: raw,
            });
        }
        OwnerPlan::PublishArticle {
            id,
            expected_version,
            revision,
        } => {
            let article_key = ImageKey::Article { id: id.clone() };
            let raw = owned_image(c, &article_key).await?.ok_or_else(changed)?;
            let article: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &article,
                &[
                    ("id", serde_json::json!(id)),
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("version", serde_json::json!(expected_version + 1)),
                    ("published_revision", serde_json::json!(revision)),
                    ("archived", serde_json::json!(0)),
                    ("updated_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: article_key,
                image: raw,
            });
            let revision_key = ImageKey::Revision {
                article_id: id.clone(),
                revision: *revision,
            };
            let raw = owned_image(c, &revision_key).await?.ok_or_else(changed)?;
            let stored: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &stored,
                &[
                    ("article_id", serde_json::json!(id)),
                    ("revision", serde_json::json!(revision)),
                    ("published_by", serde_json::json!(actor.user_id)),
                    ("published_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: revision_key,
                image: raw,
            });
        }
        OwnerPlan::SetArchived {
            id,
            expected_version,
            archived,
        } => {
            let article_key = ImageKey::Article { id: id.clone() };
            let raw = owned_image(c, &article_key).await?.ok_or_else(changed)?;
            let article: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &article,
                &[
                    ("id", serde_json::json!(id)),
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("version", serde_json::json!(expected_version + 1)),
                    ("archived", serde_json::json!(if *archived { 1 } else { 0 })),
                    ("updated_at", serde_json::json!(now)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: article_key,
                image: raw,
            });
        }
        OwnerPlan::SetEditor {
            user_id,
            active,
            expected_version,
        } => {
            let settings_key = ImageKey::Settings {
                company_id: actor.company_id.clone(),
            };
            let raw = owned_image(c, &settings_key).await?.ok_or_else(changed)?;
            let settings: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
            if !image_matches(
                &settings,
                &[
                    ("company_id", serde_json::json!(actor.company_id)),
                    ("version", serde_json::json!(expected_version + 1)),
                ],
            ) {
                return Err(changed());
            }
            images.push(OwnedImage {
                key: settings_key,
                image: raw,
            });
            if *active {
                let editor_key = ImageKey::Editor {
                    company_id: actor.company_id.clone(),
                    user_id: user_id.clone(),
                };
                let raw = owned_image(c, &editor_key).await?.ok_or_else(changed)?;
                let editor: Value = serde_json::from_str(&raw).map_err(|_| changed())?;
                if !image_matches(
                    &editor,
                    &[
                        ("company_id", serde_json::json!(actor.company_id)),
                        ("user_id", serde_json::json!(user_id)),
                        ("granted_by", serde_json::json!(actor.user_id)),
                        ("granted_at", serde_json::json!(now)),
                    ],
                ) {
                    return Err(changed());
                }
                images.push(OwnedImage {
                    key: editor_key,
                    image: raw,
                });
            } else {
                absent_editor = Some((actor.company_id.clone(), user_id.clone()));
            }
        }
    }
    Ok(CurrentHandbookEffects {
        unit: current_unit,
        images,
        absent_editor,
    })
}
