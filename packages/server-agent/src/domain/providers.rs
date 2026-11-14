//! AI provider settings and selection inputs, ported from the retained owner.
//!
//! Credential sealing, provider transport and key storage stay A1-deferred.
//! These values are transport-safe facts only; no credential ever enters a
//! receipt, audit row or log.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DEFAULT_AI_NAME: &str = "AI";
pub const MAX_NAME_UNITS: usize = 100;
pub const MAX_MODEL_UNITS: usize = 200;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn validation(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn integer(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn text<'a>(
    value: Option<&'a Value>,
    code: &'static str,
    message: &'static str,
) -> Result<&'a str, BusinessError> {
    value
        .and_then(Value::as_str)
        .ok_or_else(|| validation(code, message))
}

fn object<'a>(
    value: &'a Value,
    code: &'static str,
    message: &'static str,
) -> Result<&'a Map<String, Value>, BusinessError> {
    value.as_object().ok_or_else(|| validation(code, message))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Bearer,
    None,
}

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bearer => "bearer",
            Self::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderPreset {
    Custom,
    OpencodeZen,
    OpencodeGo,
}

impl ProviderPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::OpencodeZen => "opencode-zen",
            Self::OpencodeGo => "opencode-go",
        }
    }

    pub fn base_url(self) -> Option<&'static str> {
        match self {
            Self::Custom => None,
            Self::OpencodeZen => Some("https://opencode.ai/zen/v1"),
            Self::OpencodeGo => Some("https://opencode.ai/zen/go/v1"),
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "custom" => Some(Self::Custom),
            "opencode-zen" => Some(Self::OpencodeZen),
            "opencode-go" => Some(Self::OpencodeGo),
            _ => None,
        }
    }

    pub fn for_url(url: &str) -> Self {
        let normalized = url.trim_end_matches('/');
        [Self::OpencodeZen, Self::OpencodeGo]
            .into_iter()
            .find(|preset| preset.base_url() == Some(normalized))
            .unwrap_or(Self::Custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiProtocol {
    ChatCompletions,
    Responses,
    Messages,
}

impl AiProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat-completions",
            Self::Responses => "responses",
            Self::Messages => "messages",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    Deployment,
    Custom,
    Disabled,
}

impl SelectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deployment => "deployment",
            Self::Custom => "custom",
            Self::Disabled => "disabled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "deployment" => Some(Self::Deployment),
            "custom" => Some(Self::Custom),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    Default,
    Custom,
    Disabled,
}

impl ImageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Custom => "custom",
            Self::Disabled => "disabled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "custom" => Some(Self::Custom),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

/// Validate and normalise a public HTTPS base URL. The retained owner rejected
/// credentials, queries, fragments, bare hosts, private/local names and
/// provider endpoint suffixes, then removed one trailing slash.
pub fn ai_base_url(value: &str) -> Result<String, BusinessError> {
    let value = value.trim();
    if value.len() > 500 || !value.starts_with("https://") {
        return Err(validation(
            "AI_URL_INVALID",
            "Enter the provider's HTTPS base URL.",
        ));
    }
    let rest = &value["https://".len()..];
    if rest.is_empty()
        || rest.contains('@')
        || rest.contains('?')
        || rest.contains('#')
        || rest.contains(' ')
    {
        return Err(validation(
            "AI_URL_INVALID",
            "Use a public HTTPS hostname without credentials, a query, or a fragment.",
        ));
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.is_empty()
        || authority.contains(':')
        || !authority.contains('.')
        || authority.ends_with('.')
        || authority
            .trim_matches('.')
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err(validation(
            "AI_URL_INVALID",
            "Use a public HTTPS hostname without credentials, a query, or a fragment.",
        ));
    }
    let host = authority.to_ascii_lowercase();
    let forbidden = [
        "localhost",
        "local",
        "internal",
        "test",
        "invalid",
        "example",
    ];
    if host
        .split('.')
        .next_back()
        .is_some_and(|label| forbidden.contains(&label))
    {
        return Err(validation(
            "AI_URL_INVALID",
            "Use a public HTTPS hostname without credentials, a query, or a fragment.",
        ));
    }
    let path = path.trim_end_matches('/');
    for suffix in ["chat/completions", "responses", "messages", "models"] {
        if path == suffix || path.ends_with(&format!("/{suffix}")) {
            return Err(validation(
                "AI_BASE_URL_REQUIRED",
                "Enter the base URL, such as https://provider.com/v1, without /chat/completions or /models.",
            ));
        }
    }
    Ok(value.trim_end_matches('/').to_owned())
}

/// The retained model/protocol decision surface. The full model catalogue is a
/// separate bounded follow-up; this contract accepts any non-empty model for
/// Custom API and applies the preset's protocol family.
pub fn supported_model_protocol(base_url: &str, model: &str) -> Option<AiProtocol> {
    let model = model.trim();
    if model.is_empty() || model.encode_utf16().count() > MAX_MODEL_UNITS {
        return None;
    }
    match ProviderPreset::for_url(base_url) {
        ProviderPreset::Custom => Some(AiProtocol::ChatCompletions),
        ProviderPreset::OpencodeGo => Some(AiProtocol::Messages),
        ProviderPreset::OpencodeZen => Some(AiProtocol::ChatCompletions),
    }
}

pub fn unsupported_model() -> BusinessError {
    validation(
        "AI_MODEL_UNSUPPORTED",
        "This model is not supported yet. Choose another model.",
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionInput {
    pub expected_version: i64,
    pub name: String,
    pub base_url: String,
    pub auth_mode: AuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub remove_key: bool,
    pub provider_preset: ProviderPreset,
    pub text_model: String,
    pub image_model: String,
}

impl AiConnectionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "AI_REQUEST_INVALID", "Provide an AI connection.")?;
        let expected_version = match root.get("expectedVersion") {
            None | Some(Value::Null) => 0,
            Some(value) => integer(Some(value)).ok_or_else(|| {
                validation("AI_REQUEST_INVALID", "Use the current connection version.")
            })?,
        };
        if expected_version < 0 {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Use the current connection version.",
            ));
        }
        let name = text(
            root.get("name"),
            "AI_REQUEST_INVALID",
            "Provide an AI connection name.",
        )?
        .trim();
        let name_units = name.encode_utf16().count();
        if !(2..=MAX_NAME_UNITS).contains(&name_units) {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Use an AI connection name of 2 to 100 characters.",
            ));
        }
        let base_url = text(
            root.get("baseUrl"),
            "AI_REQUEST_INVALID",
            "Enter the provider's HTTPS base URL.",
        )?;
        let base_url = ai_base_url(base_url)?;
        let auth_mode = match root.get("authMode") {
            None | Some(Value::Null) => AuthMode::Bearer,
            Some(Value::String(value)) if value == "bearer" => AuthMode::Bearer,
            Some(Value::String(value)) if value == "none" => AuthMode::None,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose an AI authentication mode.",
                ));
            }
        };
        let api_key = match root.get("apiKey") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.len() <= 8000 => Some(value.clone()),
            _ => {
                return Err(validation("AI_REQUEST_INVALID", "Enter a bounded API key."));
            }
        };
        let remove_key = match root.get("removeKey") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose whether to remove the stored key.",
                ));
            }
        };
        let provider_preset = match root.get("providerPreset") {
            None | Some(Value::Null) => ProviderPreset::Custom,
            Some(Value::String(value)) => ProviderPreset::parse(value).ok_or_else(|| {
                validation(
                    "AI_REQUEST_INVALID",
                    "Choose a supported AI provider preset.",
                )
            })?,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose a supported AI provider preset.",
                ));
            }
        };
        if let Some(expected) = provider_preset.base_url()
            && expected != base_url
        {
            return Err(validation(
                "AI_PRESET_URL",
                "Use the preset URL or choose Custom API.",
            ));
        }
        let text_model = text(
            root.get("textModel"),
            "AI_REQUEST_INVALID",
            "Choose a text model.",
        )?
        .trim();
        let text_units = text_model.encode_utf16().count();
        if text_units == 0 || text_units > MAX_MODEL_UNITS {
            return Err(validation("AI_REQUEST_INVALID", "Choose a text model."));
        }
        let image_model = match root.get("imageModel") {
            None | Some(Value::Null) => "",
            Some(Value::String(value)) => value.trim(),
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose a supported image model or none.",
                ));
            }
        };
        if image_model.encode_utf16().count() > MAX_MODEL_UNITS {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Choose a supported image model or none.",
            ));
        }
        if supported_model_protocol(&base_url, text_model).is_none()
            || (!image_model.is_empty()
                && supported_model_protocol(&base_url, image_model).is_none())
        {
            return Err(unsupported_model());
        }
        Ok(Self {
            expected_version,
            name: name.to_owned(),
            base_url,
            auth_mode,
            api_key: api_key
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            remove_key,
            provider_preset,
            text_model: text_model.to_owned(),
            image_model: image_model.to_owned(),
        })
    }

    pub fn text_protocol(&self) -> AiProtocol {
        supported_model_protocol(&self.base_url, &self.text_model)
            .unwrap_or(AiProtocol::ChatCompletions)
    }

    pub fn image_protocol(&self) -> AiProtocol {
        if self.image_model.is_empty() {
            AiProtocol::ChatCompletions
        } else {
            supported_model_protocol(&self.base_url, &self.image_model)
                .unwrap_or(AiProtocol::ChatCompletions)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiVersionInput {
    pub expected_version: i64,
}

impl AiVersionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(
            value,
            "AI_REQUEST_INVALID",
            "Use the current connection version.",
        )?;
        let expected_version = integer(root.get("expectedVersion")).ok_or_else(|| {
            validation("AI_REQUEST_INVALID", "Use the current connection version.")
        })?;
        if expected_version < 1 {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Use the current connection version.",
            ));
        }
        Ok(Self { expected_version })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiDisableInput {
    pub expected_version: i64,
    pub disable_uses: bool,
    pub remove: bool,
}

impl AiDisableInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(
            value,
            "AI_REQUEST_INVALID",
            "Use the current connection version.",
        )?;
        let expected_version = integer(root.get("expectedVersion")).ok_or_else(|| {
            validation("AI_REQUEST_INVALID", "Use the current connection version.")
        })?;
        if expected_version < 1 {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Use the current connection version.",
            ));
        }
        let disable_uses = match root.get("disableUses") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose whether to disable AI uses of this connection.",
                ));
            }
        };
        let remove = match root.get("remove") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose whether to delete this connection.",
                ));
            }
        };
        Ok(Self {
            expected_version,
            disable_uses,
            remove,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSelectionInput {
    pub expected_version: i64,
    pub mode: SelectionMode,
    pub text_connection_id: Option<String>,
    pub image_mode: ImageMode,
    pub image_connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_ai_name: Option<String>,
}

impl AiSelectionInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(
            value,
            "AI_REQUEST_INVALID",
            "Choose the Company AI settings.",
        )?;
        let expected_version = integer(root.get("expectedVersion")).ok_or_else(|| {
            validation("AI_REQUEST_INVALID", "Use the current AI settings version.")
        })?;
        if expected_version < 0 {
            return Err(validation(
                "AI_REQUEST_INVALID",
                "Use the current AI settings version.",
            ));
        }
        let mode = SelectionMode::parse(text(
            root.get("mode"),
            "AI_REQUEST_INVALID",
            "Choose an AI selection mode.",
        )?)
        .ok_or_else(|| validation("AI_REQUEST_INVALID", "Choose an AI selection mode."))?;
        let optional = |key: &str| -> Result<Option<String>, BusinessError> {
            match root.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
                _ => Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose an active AI connection.",
                )),
            }
        };
        let text_connection_id = optional("textConnectionId")?;
        let image_mode = match root.get("imageMode") {
            None | Some(Value::Null) => ImageMode::Default,
            Some(Value::String(value)) => ImageMode::parse(value)
                .ok_or_else(|| validation("AI_REQUEST_INVALID", "Choose an image AI mode."))?,
            _ => {
                return Err(validation("AI_REQUEST_INVALID", "Choose an image AI mode."));
            }
        };
        let image_connection_id = optional("imageConnectionId")?;
        let home_ai_name = match root.get("homeAiName") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => {
                let value = value.trim();
                let units = value.encode_utf16().count();
                if units == 0
                    || units > 40
                    || value.contains('@')
                    || value.contains('\r')
                    || value.contains('\n')
                {
                    return Err(validation(
                        "AI_REQUEST_INVALID",
                        "Use a Home AI name of 1 to 40 characters without @ or line breaks.",
                    ));
                }
                Some(value.to_owned())
            }
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Use a Home AI name of 1 to 40 characters without @ or line breaks.",
                ));
            }
        };
        Ok(Self {
            expected_version,
            mode,
            text_connection_id,
            image_mode,
            image_connection_id,
            home_ai_name,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiDiscoveryInput {
    pub base_url: String,
    pub auth_mode: AuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    pub remove_key: bool,
}

impl AiDiscoveryInput {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = object(value, "AI_REQUEST_INVALID", "Provide a provider URL.")?;
        let base_url = ai_base_url(text(
            root.get("baseUrl"),
            "AI_REQUEST_INVALID",
            "Enter the provider's HTTPS base URL.",
        )?)?;
        let auth_mode = match root.get("authMode") {
            None | Some(Value::Null) => AuthMode::Bearer,
            Some(Value::String(value)) if value == "bearer" => AuthMode::Bearer,
            Some(Value::String(value)) if value == "none" => AuthMode::None,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose an AI authentication mode.",
                ));
            }
        };
        let api_key = match root.get("apiKey") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.len() <= 8000 => {
                Some(value.trim().to_owned()).filter(|value| !value.is_empty())
            }
            _ => {
                return Err(validation("AI_REQUEST_INVALID", "Enter a bounded API key."));
            }
        };
        let connection_id = match root.get("connectionId") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.len() <= 180 && !value.is_empty() => {
                Some(value.clone())
            }
            _ => {
                return Err(validation("AI_REQUEST_INVALID", "Choose an AI connection."));
            }
        };
        let remove_key = match root.get("removeKey") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(value)) => *value,
            _ => {
                return Err(validation(
                    "AI_REQUEST_INVALID",
                    "Choose whether to remove the stored key.",
                ));
            }
        };
        Ok(Self {
            base_url,
            auth_mode,
            api_key,
            connection_id,
            remove_key,
        })
    }
}

/// Safe stored connection revision. No credential material is represented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiRevision {
    pub connection_id: String,
    pub revision: i64,
    pub base_url: String,
    pub auth_mode: AuthMode,
    pub provider_preset: ProviderPreset,
    pub text_protocol: AiProtocol,
    pub image_protocol: AiProtocol,
    pub text_model: String,
    pub image_model: Option<String>,
    pub text_verified: bool,
    pub image_verified: bool,
    pub tested_at: Option<i64>,
    pub test_error: Option<String>,
    pub has_credential: bool,
}

/// Safe connection row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiConnection {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub version: i64,
    pub draft_revision: i64,
    pub active_revision: Option<i64>,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSelection {
    pub company_id: String,
    pub version: i64,
    pub mode: SelectionMode,
    pub text_connection_id: Option<String>,
    pub image_connection_id: Option<String>,
    pub image_mode: ImageMode,
    pub home_ai_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiGeneration {
    pub result: Value,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn credential_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "AI_CREDENTIAL_STORAGE_UNAVAILABLE",
        "Native AI credential storage is unavailable until the A1 key service is enabled.",
    )
}

pub fn transport_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        "AI_PROVIDER_TRANSPORT_UNAVAILABLE",
        "Native AI provider transport is unavailable until the A1 provider runtime is enabled.",
    )
}

pub fn connection_not_found() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::NotFound,
        "AI_CONNECTION_NOT_FOUND",
        "AI connection not found in this company.",
    )
}

pub fn revision_unavailable() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "AI_REVISION_UNAVAILABLE",
        "This connection has no usable configuration revision.",
    )
}
