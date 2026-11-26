//! Native Agent assistant-view query.
//!
//! The caller binds the ports to owner SQL and to the cross-owner source
//! readers; this layer owns validation, privacy scope and the exact response
//! shape. It opens no transaction and executes no SQL.
use crate::domain::assistant_view::{
    AssistantViewMode, AssistantViewRequest, canonical_page, conversation_key, page_guide,
};
use crate::domain::governance::agent_source_allowed;
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;

pub type AssistantViewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Identity and policy facts resolved by the caller before the read unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantViewContext {
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub scope_key: String,
    pub cross_context: bool,
}

/// Governance facts needed to scope the conversation and reject stale turns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantGovernanceFacts {
    pub session_id: String,
    pub agent_version_id: Option<String>,
    pub assignment_id: Option<String>,
    pub allowed_sources: Vec<String>,
}

/// One ready conversation turn, rendered by owner SQL as JSON columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantTurnRow {
    pub id: String,
    pub question: String,
    pub paths_json: String,
    pub sources_json: String,
    pub answer_json: String,
    pub english_answer_json: Option<String>,
    pub governance_snapshot_json: Option<String>,
    pub created_at: i64,
}

/// All facts the query needs. Every method is mandatory: no permissive default
/// may let a source silently disappear from the parity check.
pub trait AssistantViewPorts: Send {
    fn context(&self) -> &AssistantViewContext;
    fn now(&self) -> i64;

    fn verify_authority<'a>(&'a mut self, key: &'a str) -> AssistantViewFuture<'a, ()>;
    fn governance<'a>(
        &'a mut self,
        conversation: &'a str,
        scope: &'a str,
        path: &'a str,
    ) -> AssistantViewFuture<'a, AssistantGovernanceFacts>;
    fn validate_governance<'a>(
        &'a mut self,
        path: &'a str,
        governance: &'a AssistantGovernanceFacts,
    ) -> AssistantViewFuture<'a, ()>;
    fn conversation_turns<'a>(
        &'a mut self,
        scope: &'a str,
        conversation: &'a str,
        session_id: &'a str,
    ) -> AssistantViewFuture<'a, Vec<AssistantTurnRow>>;
    /// Full source array the retained application derives for one path.
    fn sources<'a>(&'a mut self, path: &'a str) -> AssistantViewFuture<'a, Value>;
    /// English display answer already prepared for a translated turn, if any.
    fn english_answer<'a>(&'a mut self, turn_id: &'a str) -> AssistantViewFuture<'a, Option<Value>>;
    /// An unavailable reason from the current text provider, or `None`.
    fn provider_unavailable<'a>(&'a mut self) -> AssistantViewFuture<'a, Option<String>>;
}

/// Strip the `data` payload from a derived source, matching `sourceReference`.
fn source_references(sources: &Value) -> Value {
    match sources {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| match item {
                    Value::Object(map) => {
                        let mut copy = map.clone();
                        copy.remove("data");
                        Value::Object(copy)
                    }
                    other => other.clone(),
                })
                .collect(),
        ),
        _ => Value::Array(vec![]),
    }
}

fn answer(value: &Value) -> Result<Value, BusinessError> {
    let object = value.as_object().ok_or_else(|| {
        invalid(
            "AI_ANSWER_INVALID",
            "This earlier AI response cannot be displayed.",
        )
    })?;
    if !object.get("answer").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
        return Err(invalid(
            "AI_ANSWER_INVALID",
            "This earlier AI response cannot be displayed.",
        ));
    }
    Ok(json!({
        "answer": object.get("answer").cloned().unwrap_or(Value::Null),
        "sourceIds": object.get("sourceIds").cloned().unwrap_or(json!([])),
        "followUps": object.get("followUps").cloned().unwrap_or(json!([])),
    }))
}

fn parse_optional(value: &Option<String>) -> Option<Value> {
    value.as_ref().and_then(|raw| serde_json::from_str(raw).ok())
}

const AWAITING_TRANSLATION: &str = "This earlier AI response is awaiting an English display revision. Ask again to receive current English guidance. The original remains preserved in the audit history.";

fn unavailable_view(path: &str, guide: Value, cross: bool, reason: &str) -> Value {
    json!({
        "page": path,
        "guide": guide,
        "crossContext": cross,
        "unavailable": reason,
        "turns": [],
        "hiddenTurns": 0,
    })
}

async fn history<P: AssistantViewPorts>(
    ports: &mut P,
    context: &AssistantViewContext,
    path: &str,
    mode: AssistantViewMode,
    governance: &AssistantGovernanceFacts,
) -> Result<Value, BusinessError> {
    let scope = &context.scope_key;
    let conversation = conversation_key(path, mode, context.cross_context)?;
    // SQL returns newest first; the retained application reverses it.
    let mut rows = ports
        .conversation_turns(scope, &conversation, &governance.session_id)
        .await?;
    rows.reverse();
    let mut turns = Vec::new();
    let mut hidden = 0usize;
    let mut cache: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for row in rows {
        let keep = async {
            let paths: Vec<String> = serde_json::from_str(&row.paths_json).map_err(|_| ())?;
            if paths.is_empty()
                || paths
                    .iter()
                    .any(|p| !agent_source_allowed(p, &governance.allowed_sources))
            {
                return Err(());
            }
            let policy = parse_optional(&row.governance_snapshot_json).unwrap_or(Value::Null);
            let version = policy.get("agentVersionId").cloned().unwrap_or(Value::Null);
            let assignment = policy.get("assignmentId").cloned().unwrap_or(Value::Null);
            if version != governance.agent_version_id.as_ref().map_or(Value::Null, |v| Value::String(v.clone()))
                || assignment != governance.assignment_id.as_ref().map_or(Value::Null, |v| Value::String(v.clone()))
            {
                return Err(());
            }
            if mode == AssistantViewMode::Page && (paths.len() != 1 || paths[0] != path) {
                return Err(());
            }
            let mut derived = Vec::new();
            for source_path in &paths {
                if !cache.contains_key(source_path) {
                    let sources = ports.sources(source_path).await.map_err(|_| ())?;
                    cache.insert(source_path.clone(), sources);
                }
                if let Some(sources) = cache.get(source_path) {
                    derived.extend(sources.as_array().cloned().unwrap_or_default());
                }
            }
            let references = source_references(&Value::Array(derived));
            let saved: Value = serde_json::from_str(&row.sources_json).map_err(|_| ())?;
            if references != saved {
                return Err(());
            }
            let raw = if row.question.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
                match ports.english_answer(&row.id).await {
                    Ok(Some(translated)) => translated,
                    _ => json!({"answer":AWAITING_TRANSLATION,"sourceIds":[],"followUps":[]}),
                }
            } else {
                parse_optional(&row.english_answer_json)
                    .or_else(|| parse_optional(&Some(row.answer_json.clone())))
                    .ok_or(())?
            };
            Ok(json!({
                "id": row.id,
                "question": row.question,
                "answer": answer(&raw).map_err(|_| ())?,
                "sources": references,
                "createdAt": row.created_at,
            }))
        }
        .await;
        match keep {
            Ok(turn) => turns.push(turn),
            Err(()) => hidden += 1,
        }
    }
    let _ = ports.now();
    Ok(json!({"turns":turns,"hiddenTurns":hidden}))
}

/// Execute the bounded assistant-view read. `key` is the fresh authority
/// assertion identifier minted by the caller.
pub async fn assistant_view<P: AssistantViewPorts>(
    request: &AssistantViewRequest,
    key: &str,
    ports: &mut P,
) -> Result<Value, BusinessError> {
    ports.verify_authority(key).await?;
    let (path, root) = canonical_page(&request.path)?;
    let context = ports.context().clone();
    let guide = page_guide(&root, None);
    // The retained application derives sources first so unsupported contexts
    // fail before any governance work.
    ports.sources(&path).await?;
    let conversation = conversation_key(&path, request.mode, context.cross_context)?;
    let governance = match ports
        .governance(&conversation, &context.scope_key, &path)
        .await
    {
        Ok(governance) => governance,
        Err(error) => {
            return Ok(unavailable_view(
                &path,
                guide,
                context.cross_context,
                error.message,
            ));
        }
    };
    if !agent_source_allowed(&path, &governance.allowed_sources) {
        return Ok(unavailable_view(
            &path,
            guide,
            context.cross_context,
            "This page is outside the assigned agent's approved source policy. The manual guide remains available.",
        ));
    }
    let h = history(ports, &context, &path, request.mode, &governance).await?;
    let unavailable = ports.provider_unavailable().await?;
    ports.verify_authority(key).await?;
    ports.validate_governance(&path, &governance).await?;
    Ok(json!({
        "page": path,
        "guide": guide,
        "crossContext": context.cross_context,
        "unavailable": unavailable,
        "turns": h["turns"].clone(),
        "hiddenTurns": h["hiddenTurns"].clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fake {
        context: AssistantViewContext,
        governance: AssistantGovernanceFacts,
        rows: Vec<AssistantTurnRow>,
        sources: BTreeMap<String, Value>,
        unavailable: Option<String>,
    }

    impl AssistantViewPorts for Fake {
        fn context(&self) -> &AssistantViewContext {
            &self.context
        }
        fn now(&self) -> i64 {
            1_800_000_000_000
        }
        fn verify_authority<'a>(&'a mut self, _: &'a str) -> AssistantViewFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn governance<'a>(
            &'a mut self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
        ) -> AssistantViewFuture<'a, AssistantGovernanceFacts> {
            let value = self.governance.clone();
            Box::pin(async move { Ok(value) })
        }
        fn validate_governance<'a>(
            &'a mut self,
            _: &'a str,
            _: &'a AssistantGovernanceFacts,
        ) -> AssistantViewFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
        fn conversation_turns<'a>(
            &'a mut self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
        ) -> AssistantViewFuture<'a, Vec<AssistantTurnRow>> {
            let rows = self.rows.clone();
            Box::pin(async move { Ok(rows) })
        }
        fn sources<'a>(&'a mut self, path: &'a str) -> AssistantViewFuture<'a, Value> {
            let value = self
                .sources
                .get(path)
                .cloned()
                .unwrap_or(json!([{"id":"guide","title":"Guide","path":path,"version":"1","data":{"x":1}}]));
            Box::pin(async move { Ok(value) })
        }
        fn english_answer<'a>(&'a mut self, _: &'a str) -> AssistantViewFuture<'a, Option<Value>> {
            Box::pin(async { Ok(None) })
        }
        fn provider_unavailable<'a>(&'a mut self) -> AssistantViewFuture<'a, Option<String>> {
            let value = self.unavailable.clone();
            Box::pin(async move { Ok(value) })
        }
    }

    fn fake() -> Fake {
        Fake {
            context: AssistantViewContext {
                company_id: "company".into(),
                user_id: "user".into(),
                assignment_id: "assignment".into(),
                scope_key: "scope".into(),
                cross_context: false,
            },
            governance: AssistantGovernanceFacts {
                session_id: "session".into(),
                agent_version_id: Some("version".into()),
                assignment_id: Some("assignment".into()),
                allowed_sources: vec!["/".into()],
            },
            rows: vec![],
            sources: BTreeMap::new(),
            unavailable: None,
        }
    }

    #[tokio::test]
    async fn renders_the_complete_view_shape() {
        let request = AssistantViewRequest {
            path: "/work?view=actions".into(),
            mode: AssistantViewMode::Page,
        };
        let mut ports = fake();
        let value = assistant_view(&request, "key", &mut ports).await.unwrap();
        let map = value.as_object().unwrap();
        assert_eq!(map.len(), 6);
        assert_eq!(value["page"], json!("/work?view=actions"));
        assert_eq!(value["guide"]["title"], json!("My work"));
        assert_eq!(value["crossContext"], json!(false));
        assert_eq!(value["unavailable"], Value::Null);
        assert_eq!(value["turns"], json!([]));
        assert_eq!(value["hiddenTurns"], json!(0));
    }

    #[tokio::test]
    async fn hides_stale_and_foreign_turns() {
        let source = json!([{"id":"guide-1","title":"Guide","path":"/work","version":"1","data":{"x":1}}]);
        let references = source_references(&source).to_string();
        let mut ports = fake();
        ports.sources.insert("/work".into(), source);
        ports.rows.push(AssistantTurnRow {
            id: "kept".into(),
            question: "What next?".into(),
            paths_json: json!(["/work"]).to_string(),
            sources_json: references.clone(),
            answer_json: json!({"answer":"Do the work","sourceIds":["guide-1"],"followUps":[]}).to_string(),
            english_answer_json: None,
            governance_snapshot_json: Some(json!({"agentVersionId":"version","assignmentId":"assignment"}).to_string()),
            created_at: 1,
        });
        ports.rows.push(AssistantTurnRow {
            id: "stale".into(),
            question: "Old?".into(),
            paths_json: json!(["/work"]).to_string(),
            sources_json: references,
            answer_json: json!({"answer":"old","sourceIds":[],"followUps":[]}).to_string(),
            english_answer_json: None,
            governance_snapshot_json: Some(json!({"agentVersionId":"old","assignmentId":"assignment"}).to_string()),
            created_at: 2,
        });
        let request = AssistantViewRequest {
            path: "/work".into(),
            mode: AssistantViewMode::Page,
        };
        let value = assistant_view(&request, "key", &mut ports).await.unwrap();
        assert_eq!(value["turns"].as_array().unwrap().len(), 1);
        assert_eq!(value["turns"][0]["id"], json!("kept"));
        assert_eq!(value["turns"][0]["answer"]["answer"], json!("Do the work"));
        assert_eq!(value["turns"][0]["sources"][0].get("data"), None);
        assert_eq!(value["hiddenTurns"], json!(1));
    }

    #[tokio::test]
    async fn forbidden_source_returns_guide_only() {
        let mut ports = fake();
        ports.governance.allowed_sources = vec!["/home".into()];
        let request = AssistantViewRequest {
            path: "/work".into(),
            mode: AssistantViewMode::Page,
        };
        let value = assistant_view(&request, "key", &mut ports).await.unwrap();
        assert!(value["unavailable"].as_str().unwrap().contains("approved source policy"));
        assert_eq!(value["turns"], json!([]));
    }
}
