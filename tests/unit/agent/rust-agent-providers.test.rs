//! AI provider settings contracts: retained URL/preset/model rules, fail-closed
//! credential sealing and transport, and bounded selection decisions.
use erpx_server_agent::public::providers::*;
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn command_actor(company: &str, user: &str, assignment: &str, role: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: role.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: CommandField::Omitted,
        department_id: CommandField::Omitted,
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 1,
            assignment_version: 1,
        }),
        iam: CommandField::Value(
            CommandJson::parse(
                r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
            )
            .unwrap(),
        ),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}

fn actor(roles: &[&str]) -> AiActor {
    AiActor::new(
        "h001",
        command_actor("company", "owner", "assign-1", "holding_owner"),
        roles.iter().map(|value| (*value).to_owned()).collect(),
    )
}

#[derive(Clone)]
struct FailClosed {
    actor: AiActor,
    now: i64,
    commits: Vec<String>,
    receipts: HashMap<String, Value>,
    connection: Option<AiConnection>,
    revision: Option<AiRevision>,
    selection: Option<AiSelection>,
    connections: Vec<AiConnection>,
    usage: Vec<AiUsage>,
}

impl Default for FailClosed {
    fn default() -> Self {
        Self {
            actor: actor(&["holding_owner", "it_admin"]),
            now: 1_000,
            commits: Vec::new(),
            receipts: HashMap::new(),
            connection: None,
            revision: None,
            selection: None,
            connections: Vec::new(),
            usage: Vec::new(),
        }
    }
}

impl AiProviderPorts for FailClosed {
    type Error = BusinessError;

    fn actor(&self) -> &AiActor {
        &self.actor
    }
    fn now(&self) -> i64 {
        self.now
    }
    async fn authorize(&mut self, owner_only: bool) -> Result<(), Self::Error> {
        if owner_only {
            self.actor.require_owner()
        } else {
            self.actor.require_admin()
        }
    }
    async fn replay(
        &mut self,
        key: &str,
        _operation: &str,
        _record_id: &str,
        _input: &CommandJson,
    ) -> Result<Option<Value>, Self::Error> {
        Ok(self.receipts.get(key).cloned())
    }
    async fn commit(
        &mut self,
        mutation: &AiProviderMutation,
        result: &Value,
    ) -> Result<(), Self::Error> {
        self.commits.push(mutation.common.operation.to_owned());
        self.receipts
            .insert(mutation.common.key.clone(), result.clone());
        Ok(())
    }
    async fn connection(&mut self, _id: &str) -> Result<Option<AiConnection>, Self::Error> {
        Ok(self.connection.clone())
    }
    async fn connections(&mut self) -> Result<Vec<AiConnection>, Self::Error> {
        Ok(self.connections.clone())
    }
    async fn revision(
        &mut self,
        _connection: &AiConnection,
        _active: bool,
    ) -> Result<Option<AiRevision>, Self::Error> {
        Ok(self.revision.clone())
    }
    async fn selection(&mut self) -> Result<Option<AiSelection>, Self::Error> {
        Ok(self.selection.clone())
    }
    async fn usage(&mut self, _since: i64) -> Result<Vec<AiUsage>, Self::Error> {
        Ok(self.usage.clone())
    }
    async fn seal_key(&mut self, _connection_id: &str, _key: &str) -> Result<String, Self::Error> {
        Err(credential_unavailable())
    }
    async fn open_key(&mut self, _connection_id: &str) -> Result<Option<String>, Self::Error> {
        Err(credential_unavailable())
    }
    async fn generate(
        &mut self,
        _connection: &AiConnection,
        _revision: &AiRevision,
        _kind: GenerateKind,
    ) -> Result<AiGeneration, Self::Error> {
        Err(transport_unavailable())
    }
    async fn request_models(
        &mut self,
        _base_url: &str,
        _api_key: Option<&str>,
    ) -> Result<Vec<String>, Self::Error> {
        Err(transport_unavailable())
    }
}

fn connection() -> AiConnection {
    AiConnection {
        id: "ai-1".into(),
        company_id: "company".into(),
        name: "Provider".into(),
        version: 2,
        draft_revision: 3,
        active_revision: Some(2),
        disabled: false,
    }
}

fn revision() -> AiRevision {
    AiRevision {
        connection_id: "ai-1".into(),
        revision: 3,
        base_url: "https://api.example.com/v1".into(),
        auth_mode: AuthMode::Bearer,
        provider_preset: ProviderPreset::Custom,
        text_protocol: AiProtocol::ChatCompletions,
        image_protocol: AiProtocol::ChatCompletions,
        text_model: "model-1".into(),
        image_model: None,
        text_verified: false,
        image_verified: false,
        tested_at: None,
        test_error: None,
        has_credential: true,
    }
}

#[test]
fn base_urls_and_presets_keep_the_retained_rejections() {
    assert_eq!(
        ai_base_url("https://api.provider.com/v1/").unwrap(),
        "https://api.provider.com/v1"
    );
    assert_eq!(
        ai_base_url("https://opencode.ai/zen/v1").unwrap(),
        "https://opencode.ai/zen/v1"
    );
    for invalid in [
        "http://api.provider.com/v1",
        "https://api.provider.com/v1?x=1",
        "https://api.provider.com/v1/chat/completions",
        "https://localhost/v1",
        "https://127.0.0.1/v1",
        "https://api.internal/v1",
        "not a url",
    ] {
        assert_eq!(
            ai_base_url(invalid).unwrap_err().code,
            if invalid.contains("chat/completions") {
                "AI_BASE_URL_REQUIRED"
            } else {
                "AI_URL_INVALID"
            },
            "{invalid}"
        );
    }
    assert_eq!(
        ProviderPreset::for_url("https://opencode.ai/zen/go/v1"),
        ProviderPreset::OpencodeGo
    );
    assert_eq!(
        ProviderPreset::for_url("https://api.provider.com/v1"),
        ProviderPreset::Custom
    );
}

#[test]
fn connection_input_applies_defaults_and_rejects_preset_mismatch() {
    let input = AiConnectionInput::parse(&json!({
        "name": "  Primary provider  ",
        "baseUrl": "https://api.provider.com/v1",
        "textModel": "model-1",
    }))
    .unwrap();
    assert_eq!(input.expected_version, 0);
    assert_eq!(input.auth_mode, AuthMode::Bearer);
    assert_eq!(input.provider_preset, ProviderPreset::Custom);
    assert_eq!(input.image_model, "");
    assert_eq!(input.text_protocol(), AiProtocol::ChatCompletions);
    assert_eq!(
        AiConnectionInput::parse(&json!({
            "name": "Primary",
            "baseUrl": "https://api.provider.com/v1",
            "providerPreset": "opencode-zen",
            "textModel": "model-1",
        }))
        .unwrap_err()
        .code,
        "AI_PRESET_URL"
    );
    assert_eq!(
        AiConnectionInput::parse(&json!({
            "name": "Primary",
            "baseUrl": "https://api.provider.com/v1",
            "textModel": "",
        }))
        .unwrap_err()
        .code,
        "AI_REQUEST_INVALID"
    );
}

#[tokio::test]
async fn new_bearer_connection_fails_closed_without_the_key_service() {
    let mut ports = FailClosed::default();
    let error = save_ai_connection(
        &mut ports,
        None,
        &json!({
            "name": "Primary provider",
            "baseUrl": "https://api.provider.com/v1",
            "textModel": "model-1",
        }),
        "ai-key",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_CREDENTIAL_STORAGE_UNAVAILABLE");
    assert!(ports.commits.is_empty());
}

#[tokio::test]
async fn bearer_key_save_fails_closed_before_commit() {
    let mut ports = FailClosed::default();
    let error = save_ai_connection(
        &mut ports,
        None,
        &json!({
            "name": "Primary provider",
            "baseUrl": "https://api.provider.com/v1",
            "textModel": "model-1",
            "apiKey": "secret",
        }),
        "ai-key",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_CREDENTIAL_STORAGE_UNAVAILABLE");
    assert!(ports.commits.is_empty());
}

#[tokio::test]
async fn connection_test_and_discovery_fail_closed_without_transport() {
    let mut ports = FailClosed {
        connection: Some(connection()),
        revision: Some(revision()),
        ..FailClosed::default()
    };
    let error = test_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 2}),
        "test-key",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_PROVIDER_TRANSPORT_UNAVAILABLE");
    let error = discover_ai_models(
        &mut ports,
        &json!({
            "baseUrl": "https://api.example.com/v1",
            "connectionId": "ai-1",
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_CREDENTIAL_STORAGE_UNAVAILABLE");
    assert!(ports.commits.is_empty());
}

#[tokio::test]
async fn activation_requires_a_verified_revision() {
    let mut ports = FailClosed {
        connection: Some(connection()),
        revision: Some(revision()),
        ..FailClosed::default()
    };
    let error = activate_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 2}),
        "activate-key",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_TEST_REQUIRED");
    ports.revision = Some(AiRevision {
        text_verified: true,
        ..revision()
    });
    let result = activate_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 2}),
        "activate-key-2",
    )
    .await
    .unwrap();
    assert_eq!(result, json!({"id": "ai-1", "version": 3}));
    assert_eq!(ports.commits, vec!["ai.connection.activate"]);
}

#[tokio::test]
async fn select_custom_requires_tested_active_capability() {
    let mut ports = FailClosed {
        connection: Some(connection()),
        revision: Some(revision()),
        ..FailClosed::default()
    };
    let error = select_ai_providers(
        &mut ports,
        &json!({
            "expectedVersion": 0,
            "mode": "custom",
            "textConnectionId": "ai-1",
        }),
        "select-key",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_CAPABILITY_REQUIRED");
    ports.revision = Some(AiRevision {
        text_verified: true,
        ..revision()
    });
    let result = select_ai_providers(
        &mut ports,
        &json!({
            "expectedVersion": 0,
            "mode": "custom",
            "textConnectionId": "ai-1",
        }),
        "select-key-2",
    )
    .await
    .unwrap();
    assert_eq!(result, json!({"version": 1}));
    assert_eq!(ports.commits, vec!["ai.settings.save"]);
}

#[tokio::test]
async fn overview_returns_the_full_retained_payload() {
    let mut ports = FailClosed {
        connections: vec![connection()],
        revision: Some(revision()),
        selection: Some(AiSelection {
            company_id: "company".into(),
            version: 4,
            mode: SelectionMode::Custom,
            text_connection_id: Some("ai-1".into()),
            image_connection_id: Some("ai-1".into()),
            image_mode: ImageMode::Custom,
            home_ai_name: "Workspace AI".into(),
        }),
        usage: vec![AiUsage {
            connection_id: Some("ai-1".into()),
            model: "model-1".into(),
            requests: 3,
            input_tokens: 30,
            output_tokens: 12,
            failures: 1,
        }],
        ..FailClosed::default()
    };
    let overview = ai_provider_overview(&mut ports, true, true, false)
        .await
        .unwrap();
    assert_eq!(overview["deploymentAvailable"], true);
    assert_eq!(overview["keyStorageReady"], true);
    assert_eq!(overview["selection"]["mode"], "custom");
    assert_eq!(overview["selection"]["home_ai_name"], "Workspace AI");
    assert_eq!(overview["connections"][0]["hasKey"], true);
    assert_eq!(overview["connections"][0]["disabled"], 0);
    assert_eq!(overview["connections"][0]["tested_at"], Value::Null);
    assert_eq!(
        overview["connections"][0]["active"]["text_model"],
        "model-1"
    );
    assert_eq!(overview["usage"][0]["requests"], 3);
    assert_eq!(overview["usage"][0]["failures"], 1);
}

#[tokio::test]
async fn disable_and_delete_keep_the_retained_use_guard() {
    let mut ports = FailClosed {
        connection: Some(connection()),
        revision: Some(revision()),
        selection: Some(AiSelection {
            company_id: "company".into(),
            version: 1,
            mode: SelectionMode::Custom,
            text_connection_id: Some("ai-1".into()),
            image_connection_id: None,
            image_mode: ImageMode::Default,
            home_ai_name: "AI".into(),
        }),
        ..FailClosed::default()
    };
    let error = disable_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 2, "disableUses": false}),
        "disable-used",
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AI_CONNECTION_IN_USE");
    let disabled = disable_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 2, "disableUses": true}),
        "disable-explicit",
    )
    .await
    .unwrap();
    assert_eq!(disabled, json!({"id": "ai-1", "version": 3}));
    let deleted = disable_ai_connection(
        &mut ports,
        "ai-1",
        &json!({"expectedVersion": 3, "remove": true, "disableUses": true}),
        "delete-connection",
    )
    .await
    .unwrap();
    assert_eq!(deleted, json!({"id": "ai-1", "version": 4}));
    assert_eq!(
        ports.commits,
        vec!["ai.connection.disable", "ai.connection.delete"]
    );
}
