#![doc = "Neutral OpenAI-compatible AI provider with Custom URL support."]
#![forbid(unsafe_code)]

use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

/// Provider kind. Free tier: OpenAi + Custom. Paid unlocks others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    OpenAi,
    Custom,
    Anthropic,
    Gemini,
    Azure,
    Ollama,
}

impl AiProviderKind {
    pub const fn is_free(self) -> bool {
        matches!(self, Self::OpenAi | Self::Custom)
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Custom => "custom",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Azure => "azure",
            Self::Ollama => "ollama",
        }
    }
}

/// Per-Holding provider config. Free: 1 set/Holding. Paid: per-module override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProviderConfig {
    pub holding_id: Uuid,
    pub kind: AiProviderKind,
    pub endpoint_url: String,
    pub model: String,
    pub api_key_ref: String,
    pub per_module_override: Option<String>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AiProviderError {
    #[error("ai provider endpoint is invalid")]
    InvalidEndpoint,
    #[error("ai provider model is invalid")]
    InvalidModel,
    #[error("ai provider key reference is invalid")]
    InvalidKeyRef,
    #[error("ai provider kind requires paid entitlement")]
    PaidProviderRequiresEntitlement,
    #[error("ai provider custom url requires holding scope")]
    InvalidHolding,
    #[error("ai provider per-module override requires paid entitlement")]
    PerModuleRequiresPaid,
}

const MAX_URL_LEN: usize = 512;
const MAX_MODEL_LEN: usize = 128;
const MAX_KEY_REF_LEN: usize = 256;

impl AiProviderConfig {
    pub fn validate(&self, has_flow_entitlement: bool) -> Result<(), AiProviderError> {
        if self.holding_id.is_nil() {
            return Err(AiProviderError::InvalidHolding);
        }
        validate_endpoint(&self.endpoint_url)?;
        validate_model(&self.model)?;
        validate_key_ref(&self.api_key_ref)?;
        if !self.kind.is_free() && !has_flow_entitlement {
            return Err(AiProviderError::PaidProviderRequiresEntitlement);
        }
        if self.per_module_override.is_some() && !has_flow_entitlement {
            return Err(AiProviderError::PerModuleRequiresPaid);
        }
        if let Some(module) = &self.per_module_override
            && (module.trim().is_empty() || module.len() > 64)
        {
            return Err(AiProviderError::PerModuleRequiresPaid);
        }
        Ok(())
    }

    pub fn sanitized_endpoint(&self) -> String {
        self.endpoint_url.trim_end_matches('/').to_owned()
    }

    pub fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.sanitized_endpoint())
    }
}

fn validate_endpoint(value: &str) -> Result<(), AiProviderError> {
    if value.is_empty() || value.len() > MAX_URL_LEN {
        return Err(AiProviderError::InvalidEndpoint);
    }
    let url = Url::parse(value).map_err(|_| AiProviderError::InvalidEndpoint)?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(url.host_str().unwrap_or("")) => {}
        _ => return Err(AiProviderError::InvalidEndpoint),
    }
    if url.host_str().is_none() {
        return Err(AiProviderError::InvalidEndpoint);
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "localhost" || host == "::1"
}

fn validate_model(value: &str) -> Result<(), AiProviderError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_MODEL_LEN {
        return Err(AiProviderError::InvalidModel);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AiProviderError::InvalidModel);
    }
    Ok(())
}

fn validate_key_ref(value: &str) -> Result<(), AiProviderError> {
    if value.trim().is_empty() || value.len() > MAX_KEY_REF_LEN {
        return Err(AiProviderError::InvalidKeyRef);
    }
    if value.chars().any(char::is_control) {
        return Err(AiProviderError::InvalidKeyRef);
    }
    Ok(())
}

/// Preset catalog for UI dropdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProviderPreset {
    pub kind: AiProviderKind,
    pub label: &'static str,
    pub default_endpoint: &'static str,
    pub default_model: &'static str,
    pub free: bool,
}

pub const AI_PROVIDER_PRESETS: &[AiProviderPreset] = &[
    AiProviderPreset {
        kind: AiProviderKind::OpenAi,
        label: "OpenAI",
        default_endpoint: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
        free: true,
    },
    AiProviderPreset {
        kind: AiProviderKind::Custom,
        label: "Custom URL (OpenAI Compatible)",
        default_endpoint: "https://api.example.com/v1",
        default_model: "gpt-4o-mini",
        free: true,
    },
    AiProviderPreset {
        kind: AiProviderKind::Anthropic,
        label: "Anthropic",
        default_endpoint: "https://api.anthropic.com/v1",
        default_model: "claude-3-5-sonnet-latest",
        free: false,
    },
    AiProviderPreset {
        kind: AiProviderKind::Gemini,
        label: "Gemini",
        default_endpoint: "https://generativelanguage.googleapis.com/v1",
        default_model: "gemini-1.5-flash",
        free: false,
    },
    AiProviderPreset {
        kind: AiProviderKind::Azure,
        label: "Azure OpenAI",
        default_endpoint: "https://your-resource.openai.azure.com/openai/v1",
        default_model: "gpt-4o",
        free: false,
    },
    AiProviderPreset {
        kind: AiProviderKind::Ollama,
        label: "Ollama Local",
        default_endpoint: "http://127.0.0.1:11434/v1",
        default_model: "llama3.1",
        free: false,
    },
];

/// Minimal OpenAI-compatible chat request for autofill/suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Flow analysis request — paid cross-department intelligence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowAnalysisRequest {
    pub holding_id: Uuid,
    pub flow_id: String,
    pub context_refs: Vec<String>,
    pub prompt: String,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowAnalysisResponse {
    pub summary: String,
    pub recommendations: Vec<String>,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyCipher {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
    pub key_reference: String,
}

impl ApiKeyCipher {
    pub fn from_parts(nonce: [u8; 12], ciphertext: Vec<u8>, key_reference: String) -> Self {
        Self {
            nonce,
            ciphertext,
            key_reference,
        }
    }
}

pub fn encrypt_api_key(
    holding_id: Uuid,
    key_reference: &str,
    key_bytes: &[u8; 32],
    plaintext: &str,
) -> Result<ApiKeyCipher, AiProviderError> {
    if plaintext.trim().is_empty() || plaintext.len() > 4096 {
        return Err(AiProviderError::InvalidKeyRef);
    }
    if key_reference.trim().is_empty() || key_reference.len() > 128 {
        return Err(AiProviderError::InvalidKeyRef);
    }
    let cipher =
        Aes256Gcm::new_from_slice(key_bytes).map_err(|_| AiProviderError::InvalidKeyRef)?;
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let aad = holding_id.as_bytes();
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad,
            },
        )
        .map_err(|_| AiProviderError::InvalidKeyRef)?;
    Ok(ApiKeyCipher {
        nonce,
        ciphertext,
        key_reference: key_reference.to_owned(),
    })
}

pub fn decrypt_api_key(
    holding_id: Uuid,
    key_bytes: &[u8; 32],
    cipher: &ApiKeyCipher,
) -> Result<String, AiProviderError> {
    let aes = Aes256Gcm::new_from_slice(key_bytes).map_err(|_| AiProviderError::InvalidKeyRef)?;
    let aad = holding_id.as_bytes();
    let plaintext = aes
        .decrypt(
            Nonce::from_slice(&cipher.nonce),
            aes_gcm::aead::Payload {
                msg: cipher.ciphertext.as_ref(),
                aad,
            },
        )
        .map_err(|_| AiProviderError::InvalidKeyRef)?;
    let mut text = String::from_utf8(plaintext).map_err(|_| AiProviderError::InvalidKeyRef)?;
    let result = text.clone();
    text.zeroize();
    Ok(result)
}

pub fn decode_encryption_key(encoded: &str) -> Result<[u8; 32], AiProviderError> {
    if encoded.len() == 64 && encoded.bytes().all(|b| b.is_ascii_hexdigit()) {
        let mut bytes = [0_u8; 32];
        for (i, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
            let hi = hex_val(pair[0]).ok_or(AiProviderError::InvalidKeyRef)?;
            let lo = hex_val(pair[1]).ok_or(AiProviderError::InvalidKeyRef)?;
            bytes[i] = (hi << 4) | lo;
        }
        return Ok(bytes);
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| AiProviderError::InvalidKeyRef)?;
    bytes.try_into().map_err(|_| AiProviderError::InvalidKeyRef)
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub const FLOW_AI_CAPABILITY: &str = "flow_ai";
pub const AI_PROVIDER_KEY_REFERENCE_ENV: &str = "ERPX_AI_PROVIDER_KEY";
pub const AI_PROVIDER_KEY_REF_ENV: &str = "ERPX_AI_PROVIDER_KEY_REFERENCE";

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(kind: AiProviderKind, url: &str, model: &str) -> AiProviderConfig {
        AiProviderConfig {
            holding_id: Uuid::from_u128(1),
            kind,
            endpoint_url: url.to_owned(),
            model: model.to_owned(),
            api_key_ref: "ref:one".to_owned(),
            per_module_override: None,
        }
    }

    #[test]
    fn free_providers_pass() {
        assert!(
            cfg(
                AiProviderKind::OpenAi,
                "https://api.openai.com/v1",
                "gpt-4o-mini"
            )
            .validate(false)
            .is_ok()
        );
        assert!(
            cfg(
                AiProviderKind::Custom,
                "https://api.example.com/v1",
                "model-x"
            )
            .validate(false)
            .is_ok()
        );
    }

    #[test]
    fn https_required_unless_loopback() {
        assert!(
            cfg(AiProviderKind::Custom, "http://api.example.com/v1", "m")
                .validate(false)
                .is_err()
        );
        assert!(
            cfg(
                AiProviderKind::Ollama,
                "http://127.0.0.1:11434/v1",
                "llama3.1"
            )
            .validate(true)
            .is_ok()
        );
        assert!(
            cfg(AiProviderKind::Custom, "https://api.example.com/v1/", "m")
                .validate(false)
                .is_ok()
        );
        assert_eq!(
            cfg(AiProviderKind::OpenAi, "https://api.openai.com/v1/", "m").chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn paid_provider_requires_entitlement() {
        assert_eq!(
            cfg(
                AiProviderKind::Anthropic,
                "https://api.anthropic.com/v1",
                "claude"
            )
            .validate(false)
            .unwrap_err(),
            AiProviderError::PaidProviderRequiresEntitlement
        );
        assert!(
            cfg(
                AiProviderKind::Anthropic,
                "https://api.anthropic.com/v1",
                "claude"
            )
            .validate(true)
            .is_ok()
        );
    }

    #[test]
    fn per_module_requires_paid() {
        let mut with_override = cfg(AiProviderKind::Custom, "https://api.example.com/v1", "m");
        with_override.per_module_override = Some("finance".to_owned());
        assert_eq!(
            with_override.validate(false).unwrap_err(),
            AiProviderError::PerModuleRequiresPaid
        );
        assert!(with_override.validate(true).is_ok());
    }

    #[test]
    fn model_and_key_validation() {
        assert!(
            cfg(AiProviderKind::OpenAi, "https://api.openai.com/v1", "")
                .validate(false)
                .is_err()
        );
        assert!(
            cfg(
                AiProviderKind::OpenAi,
                "https://api.openai.com/v1",
                "gpt-4o-mini"
            )
            .validate(false)
            .is_ok()
        );
        let mut bad = cfg(
            AiProviderKind::OpenAi,
            "https://api.openai.com/v1",
            "gpt-4o-mini",
        );
        bad.api_key_ref = "".to_owned();
        assert!(bad.validate(false).is_err());
    }

    #[test]
    fn presets_free_flags() {
        assert!(
            AI_PROVIDER_PRESETS
                .iter()
                .find(|p| p.kind == AiProviderKind::OpenAi)
                .unwrap()
                .free
        );
        assert!(
            AI_PROVIDER_PRESETS
                .iter()
                .find(|p| p.kind == AiProviderKind::Custom)
                .unwrap()
                .free
        );
        assert!(
            !AI_PROVIDER_PRESETS
                .iter()
                .find(|p| p.kind == AiProviderKind::Anthropic)
                .unwrap()
                .free
        );
        assert_eq!(AI_PROVIDER_PRESETS.len(), 6);
    }

    #[test]
    fn api_key_roundtrip() {
        let holding = Uuid::from_u128(99);
        let key = [7u8; 32];
        let cipher = encrypt_api_key(holding, "ref-a", &key, "sk-secret").unwrap();
        assert_eq!(cipher.key_reference, "ref-a");
        assert_eq!(cipher.nonce.len(), 12);
        let plain = decrypt_api_key(holding, &key, &cipher).unwrap();
        assert_eq!(plain, "sk-secret");
    }

    #[test]
    fn decode_key_accepts_hex_and_base64() {
        let hex = "0".repeat(64);
        assert!(decode_encryption_key(&hex).is_ok());
        let b64 = STANDARD.encode([1u8; 32]);
        assert!(decode_encryption_key(&b64).is_ok());
    }
}
