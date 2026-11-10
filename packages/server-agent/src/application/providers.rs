//! AI provider and selection use cases. Credential sealing, provider transport
//! and live discovery are ports; this layer makes the retained decisions and
//! never stores or logs a key.
use crate::domain::providers::{
    AiConnection, AiConnectionInput, AiDiscoveryInput, AiGeneration, AiProtocol, AiRevision,
    AiSelection, AiSelectionInput, AiVersionInput, AuthMode, DEFAULT_AI_NAME, ImageMode,
    SelectionMode, ai_base_url, connection_not_found, credential_unavailable, revision_unavailable,
    transport_unavailable,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandActor, CommandJson};
use serde_json::{Value, json};
use std::future::Future;

#[derive(Clone, Debug)]
pub struct AiActor {
    pub holding_id: String,
    pub command: CommandActor,
    pub roles: Vec<String>,
}

impl AiActor {
    pub fn new(holding_id: impl Into<String>, command: CommandActor, roles: Vec<String>) -> Self {
        Self {
            holding_id: holding_id.into(),
            command,
            roles,
        }
    }

    pub fn company_id(&self) -> &str {
        self.command.company_id.as_str()
    }

    pub fn user_id(&self) -> &str {
        self.command.user_id.as_str()
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|value| value == role)
    }

    pub fn require_admin(&self) -> Result<(), BusinessError> {
        if self.has_role("holding_owner") || self.has_role("it_admin") {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "AI_ADMIN_REQUIRED",
                "A company owner or IT administrator is required.",
            ))
        }
    }

    pub fn require_owner(&self) -> Result<(), BusinessError> {
        if self.has_role("holding_owner") {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "AI_OWNER_REQUIRED",
                "Only the company owner can rename the Home AI.",
            ))
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiProviderMutationCommon {
    pub key: String,
    pub operation: &'static str,
    pub record_id: String,
    pub version: i64,
    pub input: CommandJson,
    pub now: i64,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one owned mutation per command, not a hot collection
pub enum AiProviderMutationAction {
    Save {
        input: AiConnectionInput,
        previous: Option<AiRevision>,
        existing: Option<AiConnection>,
        sealed_credential: SealedCredential,
    },
    Test {
        connection_id: String,
        revision: AiRevision,
        expected_version: i64,
        text_verified: bool,
        image_verified: bool,
        error: Option<String>,
    },
    Activate {
        connection_id: String,
        expected_version: i64,
    },
    Disable {
        connection_id: String,
        expected_version: i64,
    },
    Select {
        input: AiSelectionInput,
        current_name: String,
    },
}

#[derive(Clone, Debug)]
pub struct AiProviderMutation {
    pub common: AiProviderMutationCommon,
    pub action: AiProviderMutationAction,
}

/// The exact credential effect of one save. `Keep` preserves the retained
/// sealed value; `Clear` removes it; `Replace` stores a newly sealed value. The
/// sealed value itself is never placed in the command input or result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SealedCredential {
    #[default]
    Keep,
    Clear,
    Replace(String),
}

pub trait AiProviderPorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &AiActor;
    fn now(&self) -> i64;
    fn authorize(
        &mut self,
        owner_only: bool,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> impl Future<Output = Result<Option<Value>, Self::Error>> + Send;
    fn commit(
        &mut self,
        mutation: &AiProviderMutation,
        result: &Value,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn connection(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<AiConnection>, Self::Error>> + Send;
    fn revision(
        &mut self,
        connection: &AiConnection,
        active: bool,
    ) -> impl Future<Output = Result<Option<AiRevision>, Self::Error>> + Send;
    fn selection(
        &mut self,
    ) -> impl Future<Output = Result<Option<AiSelection>, Self::Error>> + Send;
    fn seal_key(
        &mut self,
        connection_id: &str,
        key: &str,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
    fn open_key(
        &mut self,
        connection_id: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send;
    fn generate(
        &mut self,
        connection: &AiConnection,
        revision: &AiRevision,
        kind: GenerateKind,
    ) -> impl Future<Output = Result<AiGeneration, Self::Error>> + Send;
    fn request_models(
        &mut self,
        base_url: &str,
        api_key: Option<&str>,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerateKind {
    Text,
    Image,
}

fn input_json<T: serde::Serialize>(input: &T) -> Result<CommandJson, BusinessError> {
    CommandJson::from_serializable(input)
}

fn rule(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

fn version_conflict() -> BusinessError {
    rule(
        FailureKind::Conflict,
        "AI_VERSION_CONFLICT",
        "The connection changed. Refresh before testing.",
    )
}

async fn connection<P: AiProviderPorts>(ports: &mut P, id: &str) -> Result<AiConnection, P::Error> {
    ports
        .connection(id)
        .await?
        .ok_or_else(connection_not_found)
        .map_err(Into::into)
}

async fn revision<P: AiProviderPorts>(
    ports: &mut P,
    connection: &AiConnection,
    active: bool,
) -> Result<AiRevision, P::Error> {
    ports
        .revision(connection, active)
        .await?
        .ok_or_else(revision_unavailable)
        .map_err(Into::into)
}

pub async fn save_ai_connection<P: AiProviderPorts>(
    ports: &mut P,
    id: Option<&str>,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiConnectionInput::parse(raw)?;
    let record_id = id.map(str::to_owned).unwrap_or_else(|| format!("ai-{key}"));
    let operation = "ai.connection.save";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let existing = match id {
        Some(id) => Some(connection(ports, id).await?),
        None => None,
    };
    let previous = match &existing {
        Some(row) => Some(revision(ports, row, false).await?),
        None => None,
    };
    if let Some(previous) = &previous
        && previous.base_url != input.base_url
        && previous.has_credential
        && input.auth_mode == AuthMode::Bearer
        && !input.remove_key
        && input.api_key.is_none()
    {
        return Err(rule(
            FailureKind::InvalidInput,
            "AI_KEY_REQUIRED",
            "Enter a new API key when changing the provider URL.",
        )
        .into());
    }
    let credential_required = input.auth_mode == AuthMode::Bearer
        && !input.remove_key
        && input.api_key.is_none()
        && previous
            .as_ref()
            .is_none_or(|revision| !revision.has_credential);
    if credential_required {
        // A new Bearer connection cannot be persisted without a sealed key.
        return Err(credential_unavailable().into());
    }
    let sealed_credential = if input.auth_mode == AuthMode::None || input.remove_key {
        SealedCredential::Clear
    } else if input.api_key.is_some() {
        // The adapter persists only the sealed form. A missing key service
        // fails closed here rather than storing plaintext.
        SealedCredential::Replace(
            ports
                .seal_key(&record_id, input.api_key.as_deref().unwrap_or_default())
                .await?,
        )
    } else {
        SealedCredential::Keep
    };
    let version = existing.as_ref().map_or(1, |row| row.version + 1);
    let result = json!({"id": record_id, "version": version});
    ports
        .commit(
            &AiProviderMutation {
                common: AiProviderMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AiProviderMutationAction::Save {
                    input,
                    previous,
                    existing,
                    sealed_credential,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn test_ai_connection<P: AiProviderPorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiVersionInput::parse(raw)?;
    let operation = "ai.connection.test";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let row = connection(ports, id).await?;
    let current = revision(ports, &row, false).await?;
    if row.version != input.expected_version {
        return Err(version_conflict().into());
    }
    if current.auth_mode == AuthMode::Bearer && !current.has_credential {
        return Err(rule(
            FailureKind::InvalidInput,
            "AI_KEY_REQUIRED",
            "Enter an API key before testing this Bearer connection.",
        )
        .into());
    }
    let text = ports.generate(&row, &current, GenerateKind::Text).await?;
    let text_verified = text.result.get("ok").and_then(Value::as_bool) == Some(true);
    if !text_verified {
        return Err(rule(
            FailureKind::Unavailable,
            "AI_JSON_UNSUPPORTED",
            "The text model did not return the expected JSON test result.",
        )
        .into());
    }
    let (image_verified, error) = if current.image_model.is_some() {
        let image = ports.generate(&row, &current, GenerateKind::Image).await?;
        let color = image
            .result
            .get("color")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_ascii_lowercase);
        if color.as_deref() == Some("red") {
            (true, None)
        } else {
            (
                false,
                Some("The image model did not pass the synthetic image test.".to_owned()),
            )
        }
    } else {
        (false, None)
    };
    let version = input.expected_version + 1;
    let result = json!({
        "id": id,
        "version": version,
        "textVerified": text_verified,
        "imageVerified": image_verified,
        "error": error,
    });
    ports
        .commit(
            &AiProviderMutation {
                common: AiProviderMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AiProviderMutationAction::Test {
                    connection_id: id.to_owned(),
                    revision: current,
                    expected_version: input.expected_version,
                    text_verified,
                    image_verified,
                    error,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn activate_ai_connection<P: AiProviderPorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiVersionInput::parse(raw)?;
    let operation = "ai.connection.activate";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let row = connection(ports, id).await?;
    let current = revision(ports, &row, false).await?;
    if !current.text_verified {
        return Err(rule(
            FailureKind::InvalidInput,
            "AI_TEST_REQUIRED",
            "Test this revision successfully before activation.",
        )
        .into());
    }
    if row.version != input.expected_version {
        return Err(version_conflict().into());
    }
    let version = input.expected_version + 1;
    let result = json!({"id": id, "version": version});
    ports
        .commit(
            &AiProviderMutation {
                common: AiProviderMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AiProviderMutationAction::Activate {
                    connection_id: id.to_owned(),
                    expected_version: input.expected_version,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn disable_ai_connection<P: AiProviderPorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiVersionInput::parse(raw)?;
    let operation = "ai.connection.disable";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let version = input.expected_version + 1;
    let result = json!({"id": id, "version": version});
    ports
        .commit(
            &AiProviderMutation {
                common: AiProviderMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AiProviderMutationAction::Disable {
                    connection_id: id.to_owned(),
                    expected_version: input.expected_version,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn discover_ai_models<P: AiProviderPorts>(
    ports: &mut P,
    raw: &Value,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiDiscoveryInput::parse(raw)?;
    let mut api_key = input.api_key.clone();
    match (
        &input.connection_id,
        input.auth_mode,
        input.remove_key,
        api_key.as_deref(),
    ) {
        (Some(id), AuthMode::Bearer, false, None) => {
            let row = connection(ports, id).await?;
            let current = revision(ports, &row, false).await?;
            if current.base_url != input.base_url {
                return Err(rule(
                    FailureKind::InvalidInput,
                    "AI_KEY_REQUIRED",
                    "Enter a new API key when changing the provider URL.",
                )
                .into());
            }
            api_key = ports.open_key(id).await?;
        }
        (None, AuthMode::Bearer, false, None) => {
            return Err(rule(
                FailureKind::InvalidInput,
                "AI_KEY_REQUIRED",
                "Enter an API key to fetch models.",
            )
            .into());
        }
        _ => {}
    }
    if input.auth_mode == AuthMode::Bearer && api_key.is_none() {
        return Err(rule(
            FailureKind::InvalidInput,
            "AI_KEY_REQUIRED",
            "Enter an API key to fetch models.",
        )
        .into());
    }
    let all = ports
        .request_models(&input.base_url, api_key.as_deref())
        .await?;
    let mut models = all
        .into_iter()
        .map(|model| model.trim().to_owned())
        .filter(|model| {
            crate::domain::providers::supported_model_protocol(&input.base_url, model).is_some()
        })
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    Ok(json!({
        "models": models,
        "totalModels": models.len(),
        "fetchedAt": ports.now(),
        "message": Value::Null,
    }))
}

pub async fn select_ai_providers<P: AiProviderPorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(false).await?;
    let input = AiSelectionInput::parse(raw)?;
    let operation = "ai.settings.save";
    let company = ports.actor().company_id().to_owned();
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &company, &canonical).await? {
        return Ok(prior);
    }
    let current_name = ports
        .selection()
        .await?
        .map(|selection| selection.home_ai_name)
        .unwrap_or_else(|| DEFAULT_AI_NAME.to_owned());
    let home_ai_name = input
        .home_ai_name
        .clone()
        .unwrap_or_else(|| current_name.clone());
    if home_ai_name != current_name {
        ports.actor().require_owner()?;
    }
    if input.mode == SelectionMode::Custom {
        let id = input.text_connection_id.as_deref().ok_or_else(|| {
            rule(
                FailureKind::InvalidInput,
                "AI_CONNECTION_REQUIRED",
                "Choose an active connection.",
            )
        })?;
        let row = connection(ports, id).await?;
        let active = revision(ports, &row, true).await?;
        if row.disabled
            || !active.text_verified
            || (input.image_mode == ImageMode::Custom && !active.image_verified)
        {
            return Err(rule(
                FailureKind::InvalidInput,
                "AI_CAPABILITY_REQUIRED",
                "Choose an active connection with the required tested capability.",
            )
            .into());
        }
    }
    if input.image_mode == ImageMode::Custom {
        let id = input.image_connection_id.as_deref().ok_or_else(|| {
            rule(
                FailureKind::InvalidInput,
                "AI_CONNECTION_REQUIRED",
                "Choose an active connection.",
            )
        })?;
        let row = connection(ports, id).await?;
        let active = revision(ports, &row, true).await?;
        if row.disabled || !active.image_verified {
            return Err(rule(
                FailureKind::InvalidInput,
                "AI_CAPABILITY_REQUIRED",
                "Choose an active connection with the required tested capability.",
            )
            .into());
        }
    }
    let version = input.expected_version + 1;
    let result = json!({"version": version});
    ports
        .commit(
            &AiProviderMutation {
                common: AiProviderMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: company,
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AiProviderMutationAction::Select {
                    input,
                    current_name,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Whether a revision can serve text/image without reading the credential.
pub fn capability_available(revision: &AiRevision, kind: GenerateKind) -> bool {
    revision.text_verified
        && (kind == GenerateKind::Text || revision.image_model.is_some() && revision.image_verified)
}

/// The explicit failure used by adapters that have no A1 credential service.
pub fn fail_closed_credentials() -> BusinessError {
    credential_unavailable()
}

/// The explicit failure used by adapters that have no A1 provider transport.
pub fn fail_closed_transport() -> BusinessError {
    transport_unavailable()
}

/// Protocol facts carried to transport adapters without credential material.
pub fn protocol_for(revision: &AiRevision, kind: GenerateKind) -> AiProtocol {
    match kind {
        GenerateKind::Text => revision.text_protocol,
        GenerateKind::Image => revision.image_protocol,
    }
}

/// Validate one discovery model list using the bounded protocol decision.
pub fn supported_model_ids(base_url: &str, ids: Vec<String>) -> Vec<String> {
    let mut models = ids
        .into_iter()
        .map(|model| model.trim().to_owned())
        .filter(|model| {
            crate::domain::providers::supported_model_protocol(base_url, model).is_some()
        })
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models
}

/// Keep the public URL validator in the application surface for callers.
pub fn normalized_base_url(value: &str) -> Result<String, BusinessError> {
    ai_base_url(value)
}
