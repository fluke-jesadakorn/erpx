//! Application ports and handlers for the three native L06 post sub-resources.
//!
//! The owner statements are executed only through the ports below; composition
//! supplies the AI provider and work-authority decisions. This layer validates
//! identities, applies the retained projection rules and assembles the exact
//! `{data}` envelope. No SQL, pool, axum or provider call lives here.

use crate::domain::native_r3_posts::{
    AiDraftView, PostTaskResult, SourceItem, TaskSourceView, ai_unavailable, envelope, not_found,
    normalize_comment_ids, source_changed, source_unsupported, sources_within_limit, validate_id,
    validate_key,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::{Value, json};
use std::{future::Future, pin::Pin, sync::Arc};

pub type PostsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn limit() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "AI_LIMIT",
        "AI quota or request limit reached, or the source changed. Continue manually or try again later.",
    )
}

// --- task-source read ----------------------------------------------------------

#[derive(Clone, Debug)]
pub struct CommentFacts {
    pub id: String,
    pub body: String,
    pub author: String,
    pub version: i64,
}

pub trait TaskSourcePort: Send + Sync {
    fn published_post<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        post: &'a str,
        now: i64,
    ) -> PostsFuture<'a, Option<Value>>;

    fn comments<'a>(&'a self, post: &'a str, ids: &'a [String]) -> PostsFuture<'a, Vec<CommentFacts>>;

    fn hash<'a>(&'a self, serialized: String) -> PostsFuture<'a, String>;
}

#[derive(Clone)]
pub struct TaskSourceHandler {
    owner: Arc<dyn TaskSourcePort>,
}

impl TaskSourceHandler {
    pub fn new(owner: Arc<dyn TaskSourcePort>) -> Self {
        Self { owner }
    }

    pub fn read<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        post: &'a str,
        comment_ids: &'a [String],
        now: i64,
    ) -> PostsFuture<'a, Value> {
        let company = company.to_owned();
        let user = user.to_owned();
        let post = post.to_owned();
        let branch = branch.map(str::to_owned);
        let department = department.map(str::to_owned);
        let ids = comment_ids.to_vec();
        Box::pin(async move {
            validate_id(&company)?;
            validate_id(&user)?;
            validate_id(&post)?;
            let ids = normalize_comment_ids(&ids)?;
            let row = self
                .owner
                .published_post(&company, &user, branch.as_deref(), department.as_deref(), &post, now)
                .await?
                .ok_or_else(not_found)?;
            if row.get("linked_module").and_then(Value::as_str).is_some() {
                return Err(source_unsupported());
            }
            let version = row
                .get("version")
                .and_then(Value::as_i64)
                .ok_or_else(not_found)?;
            let facts = self.owner.comments(&post, &ids).await?;
            if facts.len() != ids.len() {
                return Err(source_changed());
            }
            let mut sources = vec![SourceItem {
                id: row.get("id").and_then(Value::as_str).unwrap_or(&post).to_owned(),
                body: row.get("body").and_then(Value::as_str).unwrap_or_default().to_owned(),
                author: row.get("author_name").and_then(Value::as_str).unwrap_or_default().to_owned(),
                version,
            }];
            sources.extend(facts.into_iter().map(|c| SourceItem {
                id: c.id,
                body: c.body,
                author: c.author,
                version: c.version,
            }));
            if !sources_within_limit(&sources) {
                return Err(crate::domain::native_r3_posts::invalid_source_limit());
            }
            let serialized = serde_json::to_string(&sources).unwrap_or_default();
            let hash = self.owner.hash(serialized).await?;
            Ok(envelope(&TaskSourceView {
                post: row,
                sources,
                hash,
                comment_ids: ids,
            }))
        })
    }
}

// --- task command -------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskFacts {
    pub id: String,
    pub state: String,
}

pub trait TaskCommandPort: Send + Sync {
    fn post_task<'a>(&'a self, post: &'a str) -> PostsFuture<'a, Option<String>>;
    fn task<'a>(&'a self, id: &'a str) -> PostsFuture<'a, Option<TaskFacts>>;
    /// Native Staff Work command: composition owns authority, purchase and the
    /// originating-source binding; the owner slice supplies the post facts.
    fn create_task<'a>(
        &'a self,
        input: Value,
        source: Value,
        key: &'a str,
    ) -> PostsFuture<'a, TaskFacts>;
}

/// Composition-owned native Staff Work command. The owner crate cannot decide
/// work-package authority or the originating-source binding on the wire.
pub trait TaskCreator: Send + Sync {
    fn create<'a>(&'a self, input: Value, source: Value, key: &'a str) -> PostsFuture<'a, TaskFacts>;
}

#[derive(Clone)]
pub struct TaskCommandHandler {
    owner: Arc<dyn TaskCommandPort>,
}

impl TaskCommandHandler {
    pub fn new(owner: Arc<dyn TaskCommandPort>) -> Self {
        Self { owner }
    }

    pub fn create<'a>(
        &'a self,
        post: &'a str,
        key: &'a str,
        input: &'a Value,
        source: &'a Value,
    ) -> PostsFuture<'a, Value> {
        let post = post.to_owned();
        let key = key.to_owned();
        let input = input.clone();
        let source = source.clone();
        Box::pin(async move {
            validate_id(&post)?;
            validate_key(&key)?;
            if let Some(existing) = self.owner.post_task(&post).await? {
                let task = self
                    .owner
                    .task(&existing)
                    .await?
                    .ok_or_else(not_found)?;
                return Ok(envelope(&PostTaskResult {
                    id: task.id,
                    state: task.state,
                    existing: true,
                }));
            }
            let mut bound = source.clone();
            bound["postId"] = json!(post);
            let task = match self.owner.create_task(input, bound, &key).await {
                Ok(task) => task,
                Err(error) => {
                    let other = self
                        .owner
                        .post_task(&post)
                        .await?
                        .ok_or(error)?;
                    let task = self.owner.task(&other).await?.ok_or_else(not_found)?;
                    return Ok(envelope(&PostTaskResult {
                        id: task.id,
                        state: task.state,
                        existing: true,
                    }));
                }
            };
            Ok(envelope(&PostTaskResult {
                id: task.id,
                state: task.state,
                existing: false,
            }))
        })
    }
}

// --- AI drafts ----------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PriorDraft {
    pub actor_json: Option<String>,
    pub post_id: String,
    pub kind: String,
    pub source_hash: String,
    pub result_json: Option<String>,
    pub state: String,
    pub source_version: i64,
    pub comment_ids_json: String,
    pub expires_at: i64,
    pub provider_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AiOutput {
    pub result: Value,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub trait AiDraftPort: Send + Sync {
    fn prior<'a>(&'a self, id: &'a str, user: &'a str) -> PostsFuture<'a, Option<PriorDraft>>;

    fn reserve<'a>(&'a self, values: Value, now: i64) -> PostsFuture<'a, ()>;

    fn capture<'a>(&'a self, column: &'a str, value: &'a str, id: &'a str) -> PostsFuture<'a, ()>;

    fn finish<'a>(&'a self, id: &'a str, result: &'a str, input: i64, output: i64)
        -> PostsFuture<'a, ()>;

    fn fail<'a>(&'a self, id: &'a str) -> PostsFuture<'a, ()>;

    /// Composition-owned provider. Absent provider is a hard R5 blocker.
    fn generate<'a>(&'a self, request: Value) -> PostsFuture<'a, AiOutput>;
}

/// Composition-owned AI provider. Without it the draft write phase has no
/// result to persist; this is the explicit R5-provider parity gap.
pub trait AiProvider: Send + Sync {
    fn generate<'a>(&'a self, request: Value) -> PostsFuture<'a, AiOutput>;
}

#[derive(Clone)]
pub struct AiDraftHandler {
    owner: Arc<dyn AiDraftPort>,
}

impl AiDraftHandler {
    pub fn new(owner: Arc<dyn AiDraftPort>) -> Self {
        Self { owner }
    }

    pub fn create<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        post: &'a str,
        key: &'a str,
        kind: &'a str,
        comment_ids: &'a [String],
        actor_json: &'a str,
        now: i64,
        hash: &'a str,
        version: i64,
        sources: &'a [SourceItem],
    ) -> PostsFuture<'a, Value> {
        let company = company.to_owned();
        let user = user.to_owned();
        let post = post.to_owned();
        let key = key.to_owned();
        let kind = kind.to_owned();
        let ids = comment_ids.to_vec();
        let actor = actor_json.to_owned();
        let source_hash = hash.to_owned();
        let source_version = version;
        let source_refs: Vec<_> = sources
            .iter()
            .map(|s| json!({"id": s.id, "version": s.version}))
            .collect();
        Box::pin(async move {
            validate_id(&company)?;
            validate_id(&user)?;
            validate_id(&post)?;
            validate_key(&key)?;
            if kind != "summary" && kind != "task" {
                return Err(crate::domain::native_r3_posts::invalid_ai_kind());
            }
            let ids = normalize_comment_ids(&ids)?;
            // Idempotency id is derived by composition from company, user and key.
            let id = key.clone();
            if let Some(prior) = self.owner.prior(&id, &user).await? {
                if prior.actor_json.as_deref() != Some(actor.as_str()) {
                    return Err(conflict("IDEMPOTENCY_CONFLICT", "This AI request belongs to another work context. Review the current role before starting a new request."));
                }
                if prior.post_id != post
                    || prior.kind != kind
                    || prior.source_hash != source_hash
                    || prior.comment_ids_json != serde_json::to_string(&ids).unwrap_or_default()
                    || prior.expires_at <= now
                {
                    return Err(conflict("AI_DRAFT_CHANGED", "Review the current conversation and start a new draft."));
                }
                if prior.state != "ready" {
                    return Err(conflict("AI_REQUEST_EXISTS", "This request is still running or failed. Start a new request to retry."));
                }
                let result: Value = serde_json::from_str(&prior.result_json.unwrap_or_default())
                    .unwrap_or(Value::Null);
                return Ok(envelope(&AiDraftView {
                    id,
                    kind,
                    result,
                    source_version,
                    source_hash,
                    sources: serde_json::from_value(json!(source_refs)).unwrap_or_default(),
                    comment_ids: ids,
                }));
            }
            let reserve_values = json!({
                "id": id, "postId": post, "userId": user, "kind": kind,
                "version": source_version, "hash": source_hash,
                "commentIds": ids, "sources": source_refs, "companyId": company,
                "now": now, "expiresAt": now + 86_400_000,
            });
            self.owner.reserve(reserve_values, now).await.map_err(|_| limit())?;
            self.owner.capture("actor_json", &actor, &id).await?;
            let output = match self
                .owner
                .generate(json!({"kind": kind, "sources": source_refs}))
                .await
            {
                Ok(output) => output,
                Err(_) => {
                    self.owner.fail(&id).await?;
                    return Err(ai_unavailable("AI could not prepare a valid draft. Your text is safe; create the task manually."));
                }
            };
            let result = serde_json::to_string(&output.result).unwrap_or_default();
            self.owner
                .finish(&id, &result, output.input_tokens, output.output_tokens)
                .await?;
            Ok(envelope(&AiDraftView {
                id,
                kind,
                result: output.result,
                source_version,
                source_hash,
                sources: serde_json::from_value(json!(source_refs)).unwrap_or_default(),
                comment_ids: ids,
            }))
        })
    }
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Conflict,
        code,
        message,
    )
}

// --- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_r3_posts::DraftSourceRef;

    #[derive(Clone)]
    struct Source;

    impl TaskSourcePort for Source {
        fn published_post<'a>(
            &'a self,
            _c: &'a str,
            _u: &'a str,
            _b: Option<&'a str>,
            _d: Option<&'a str>,
            _p: &'a str,
            _now: i64,
        ) -> PostsFuture<'a, Option<Value>> {
            Box::pin(async move {
                Ok(Some(json!({"id": "p1", "body": "hello", "author_name": "a", "version": 3, "linked_module": null})))
            })
        }
        fn comments<'a>(&'a self, _p: &'a str, _ids: &'a [String]) -> PostsFuture<'a, Vec<CommentFacts>> {
            Box::pin(async move { Ok(vec![]) })
        }
        fn hash<'a>(&'a self, _serialized: String) -> PostsFuture<'a, String> {
            Box::pin(async move { Ok("abc".into()) })
        }
    }

    #[tokio::test]
    async fn task_source_read_builds_the_retained_envelope() {
        let handler = TaskSourceHandler::new(Arc::new(Source));
        let body = handler
            .read("acme", "user-1", None, None, "p1", &[], 100)
            .await
            .unwrap();
        assert_eq!(body["data"]["hash"], json!("abc"));
        assert_eq!(body["data"]["commentIds"], json!([]));
        assert_eq!(body["data"]["sources"][0]["version"], json!(3));
    }

    #[derive(Clone)]
    struct Busy;

    impl TaskCommandPort for Busy {
        fn post_task<'a>(&'a self, _p: &'a str) -> PostsFuture<'a, Option<String>> {
            Box::pin(async move { Ok(Some("task-1".into())) })
        }
        fn task<'a>(&'a self, _id: &'a str) -> PostsFuture<'a, Option<TaskFacts>> {
            Box::pin(async move {
                Ok(Some(TaskFacts { id: "task-1".into(), state: "ready".into() }))
            })
        }
        fn create_task<'a>(&'a self, _i: Value, _s: Value, _k: &'a str) -> PostsFuture<'a, TaskFacts> {
            Box::pin(async move { Ok(TaskFacts { id: "new".into(), state: "ready".into() }) })
        }
    }

    #[tokio::test]
    async fn task_creation_reuses_an_existing_post_task() {
        let handler = TaskCommandHandler::new(Arc::new(Busy));
        let body = handler
            .create("p1", "key-12345", &json!({}), &json!({}))
            .await
            .unwrap();
        assert_eq!(body, json!({"data": {"id": "task-1", "state": "ready", "existing": true}}));
    }

    #[derive(Clone)]
    struct Offline;

    impl AiDraftPort for Offline {
        fn prior<'a>(&'a self, _id: &'a str, _u: &'a str) -> PostsFuture<'a, Option<PriorDraft>> {
            Box::pin(async move { Ok(None) })
        }
        fn reserve<'a>(&'a self, _v: Value, _now: i64) -> PostsFuture<'a, ()> {
            Box::pin(async move { Ok(()) })
        }
        fn capture<'a>(&'a self, _c: &'a str, _v: &'a str, _id: &'a str) -> PostsFuture<'a, ()> {
            Box::pin(async move { Ok(()) })
        }
        fn finish<'a>(&'a self, _id: &'a str, _r: &'a str, _i: i64, _o: i64) -> PostsFuture<'a, ()> {
            Box::pin(async move { Ok(()) })
        }
        fn fail<'a>(&'a self, _id: &'a str) -> PostsFuture<'a, ()> {
            Box::pin(async move { Ok(()) })
        }
        fn generate<'a>(&'a self, _r: Value) -> PostsFuture<'a, AiOutput> {
            Box::pin(async move {
                Err(BusinessError::new(
                    erpx_platform_shared_server::public::FailureKind::Unavailable,
                    "AI_UNAVAILABLE",
                    "offline",
                ))
            })
        }
    }

    #[tokio::test]
    async fn ai_draft_failure_fails_the_reserved_draft() {
        let item = SourceItem { id: "p1".into(), body: "b".into(), author: "a".into(), version: 1 };
        let handler = AiDraftHandler::new(Arc::new(Offline));
        let error = handler
            .create("acme", "user-1", "p1", "key-12345", "summary", &[], "{}", 100, "hash", 1, &[item])
            .await
            .unwrap_err();
        assert_eq!(error.code, "AI_UNAVAILABLE");
    }

    #[test]
    fn draft_source_refs_use_camel_case() {
        let refs = vec![DraftSourceRef { id: "p".into(), version: 1 }];
        assert_eq!(serde_json::to_value(&refs).unwrap(), json!([{"id": "p", "version": 1}]));
    }
}
