//! Native Agent assistant-view application behavior: canonical page scope,
//! conversation privacy, stale/source-drift turn hiding, unavailable fallback,
//! and the exact response field set. Wire as a `[[test]]` target named
//! `agent_assistant_view` in modules/agent/server/Cargo.toml (lead-owned).
use erpx_agent_server::public::assistant_view::{
    AssistantGovernanceFacts, AssistantTurnRow, AssistantViewContext, AssistantViewMode,
    AssistantViewPorts, AssistantViewRequest, assistant_view,
};
use erpx_platform_runtime_server::public::http_support::serde_json::{Value, json};
use erpx_platform_shared_server::public::BusinessError;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

struct Ports {
    context: AssistantViewContext,
    governance: AssistantGovernanceFacts,
    rows: Vec<AssistantTurnRow>,
    sources: BTreeMap<String, Value>,
    unavailable: Option<String>,
    authority_calls: usize,
    validate_calls: usize,
    seen_scope: String,
    seen_conversation: String,
}

impl AssistantViewPorts for Ports {
    fn context(&self) -> &AssistantViewContext {
        &self.context
    }
    fn now(&self) -> i64 {
        1_800_000_000_000
    }
    fn verify_authority<'a>(&'a mut self, key: &'a str) -> Fut<'a, ()> {
        assert_eq!(key, "key");
        self.authority_calls += 1;
        Box::pin(async { Ok(()) })
    }
    fn governance<'a>(
        &'a mut self,
        conversation: &'a str,
        scope: &'a str,
        _: &'a str,
    ) -> Fut<'a, AssistantGovernanceFacts> {
        self.seen_conversation = conversation.to_owned();
        self.seen_scope = scope.to_owned();
        let value = self.governance.clone();
        Box::pin(async move { Ok(value) })
    }
    fn validate_governance<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a AssistantGovernanceFacts,
    ) -> Fut<'a, ()> {
        self.validate_calls += 1;
        Box::pin(async { Ok(()) })
    }
    fn conversation_turns<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        session: &'a str,
    ) -> Fut<'a, Vec<AssistantTurnRow>> {
        assert_eq!(session, "session");
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn sources<'a>(&'a mut self, path: &'a str) -> Fut<'a, Value> {
        let value = self
            .sources
            .get(path)
            .cloned()
            .unwrap_or_else(|| json!([{"id":"guide","title":"Guide","path":path,"version":"1","data":{"hidden":true}}]));
        Box::pin(async move { Ok(value) })
    }
    fn english_answer<'a>(&'a mut self, _: &'a str) -> Fut<'a, Option<Value>> {
        Box::pin(async { Ok(None) })
    }
    fn provider_unavailable<'a>(&'a mut self) -> Fut<'a, Option<String>> {
        let value = self.unavailable.clone();
        Box::pin(async move { Ok(value) })
    }
}

fn ports() -> Ports {
    Ports {
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
        authority_calls: 0,
        validate_calls: 0,
        seen_scope: String::new(),
        seen_conversation: String::new(),
    }
}

fn request(path: &str, mode: AssistantViewMode) -> AssistantViewRequest {
    AssistantViewRequest {
        path: path.into(),
        mode,
    }
}

fn row(id: &str, answer: &str, snapshot_agent: &str, created_at: i64, refs: &str) -> AssistantTurnRow {
    AssistantTurnRow {
        id: id.into(),
        question: "What next?".into(),
        paths_json: json!(["/work"]).to_string(),
        sources_json: refs.to_owned(),
        answer_json: json!({"answer":answer,"sourceIds":["guide"],"followUps":[]}).to_string(),
        english_answer_json: None,
        governance_snapshot_json: Some(
            json!({"agentVersionId":snapshot_agent,"assignmentId":"assignment"}).to_string(),
        ),
        created_at,
    }
}

#[tokio::test]
async fn full_view_shape_and_authority_recheck() {
    let mut ports = ports();
    let value = assistant_view(
        &request("/work?cursor=1&view=actions", AssistantViewMode::Page),
        "key",
        &mut ports,
    )
    .await
    .unwrap();
    assert_eq!(value.as_object().map(|map| map.len()), Some(6));
    assert_eq!(value["page"], json!("/work?view=actions"));
    assert_eq!(value["guide"]["title"], json!("My work"));
    assert_eq!(value["crossContext"], json!(false));
    assert_eq!(value["unavailable"], Value::Null);
    assert_eq!(value["turns"], json!([]));
    assert_eq!(value["hiddenTurns"], json!(0));
    assert_eq!(ports.authority_calls, 2, "authority checked before and after history");
    assert_eq!(ports.validate_calls, 1);
    assert_eq!(ports.seen_scope, "scope");
    assert_eq!(ports.seen_conversation, "/work?view=actions");
}

#[tokio::test]
async fn hides_stale_policy_and_source_drift_turns() {
    let source = json!([{"id":"guide","title":"Guide","path":"/work","version":"1","data":{"hidden":true}}]);
    // Exact reference projection the application derives (without `data`).
    let refs = json!([{"id":"guide","title":"Guide","path":"/work","version":"1"}]).to_string();
    let mut ports = ports();
    ports.sources.insert("/work".into(), source);
    ports.rows.push(row("kept", "Do the work", "version", 1, &refs));
    ports.rows.push(row("stale", "Old guidance", "older-version", 2, &refs));
    ports.rows.push(row("drifted", "Changed context", "version", 3, &json!([]).to_string()));
    let value = assistant_view(&request("/work", AssistantViewMode::Page), "key", &mut ports)
        .await
        .unwrap();
    assert_eq!(value["turns"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["turns"][0]["id"], json!("kept"));
    assert_eq!(value["turns"][0]["answer"]["answer"], json!("Do the work"));
    assert_eq!(value["turns"][0]["sources"][0].get("data"), None);
    assert_eq!(value["hiddenTurns"], json!(2));
}

#[tokio::test]
async fn forbidden_source_returns_guide_only() {
    let mut ports = ports();
    ports.governance.allowed_sources = vec!["/home".into()];
    let value = assistant_view(&request("/work", AssistantViewMode::Page), "key", &mut ports)
        .await
        .unwrap();
    assert!(value["unavailable"]
        .as_str()
        .unwrap()
        .contains("approved source policy"));
    assert_eq!(value["turns"], json!([]));
    assert_eq!(value["guide"]["title"], json!("My work"));
    assert_eq!(ports.validate_calls, 0);
}

#[tokio::test]
async fn provider_failure_is_reported_without_losing_history() {
    let mut ports = ports();
    ports.unavailable = Some("AI is temporarily unavailable. The workflow guide remains available.".into());
    let value = assistant_view(&request("/work", AssistantViewMode::Page), "key", &mut ports)
        .await
        .unwrap();
    assert_eq!(
        value["unavailable"],
        json!("AI is temporarily unavailable. The workflow guide remains available.")
    );
    assert_eq!(ports.validate_calls, 1);
}

#[tokio::test]
async fn workspace_mode_requires_cross_context_permission() {
    let mut ports = ports();
    assert!(assistant_view(&request("/work", AssistantViewMode::Workspace), "key", &mut ports)
        .await
        .is_err());
    ports.context.cross_context = true;
    let value = assistant_view(&request("/work", AssistantViewMode::Workspace), "key", &mut ports)
        .await
        .unwrap();
    assert_eq!(ports.seen_conversation, "workspace");
    assert_eq!(value["crossContext"], json!(true));
}
