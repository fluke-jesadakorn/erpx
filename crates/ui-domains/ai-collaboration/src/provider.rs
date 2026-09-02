use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKindView {
    OpenAi,
    Custom,
    Anthropic,
    Gemini,
    Azure,
    Ollama,
}

impl AiProviderKindView {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProviderPresetView {
    pub kind: AiProviderKindView,
    pub label: String,
    pub default_endpoint: String,
    pub default_model: String,
    pub free: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiProviderFormState {
    pub selected_kind: AiProviderKindView,
    pub endpoint_url: String,
    pub model: String,
    pub api_key: String,
    pub per_module_override: Option<String>,
}

impl AiProviderFormState {
    pub fn for_preset(preset: &AiProviderPresetView) -> Self {
        Self {
            selected_kind: preset.kind,
            endpoint_url: preset.default_endpoint.clone(),
            model: preset.default_model.clone(),
            api_key: String::new(),
            per_module_override: None,
        }
    }

    pub fn validate(&self, has_flow_entitlement: bool) -> Result<(), String> {
        if self.endpoint_url.trim().is_empty() {
            return Err("endpoint is required".into());
        }
        if !(self.endpoint_url.starts_with("https://")
            || self.endpoint_url.starts_with("http://127.0.0.1")
            || self.endpoint_url.starts_with("http://localhost"))
        {
            return Err("endpoint must be https or loopback http".into());
        }
        if self.model.trim().is_empty() {
            return Err("model is required".into());
        }
        if self.api_key.trim().is_empty() {
            return Err("api key is required".into());
        }
        if !self.selected_kind.is_free() && !has_flow_entitlement {
            return Err("paid provider requires Flow AI entitlement".into());
        }
        if self.per_module_override.is_some() && !has_flow_entitlement {
            return Err("per-module override requires Flow AI entitlement".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiSuggestionState {
    Idle,
    Loading,
    Suggestions { items: Vec<String> },
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowAnalysisState {
    Idle,
    Loading,
    Result {
        summary: String,
        recommendations: Vec<String>,
    },
    Unavailable {
        reason: String,
    },
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset_openai() -> AiProviderPresetView {
        AiProviderPresetView {
            kind: AiProviderKindView::OpenAi,
            label: "OpenAI".into(),
            default_endpoint: "https://api.openai.com/v1".into(),
            default_model: "gpt-4o-mini".into(),
            free: true,
        }
    }

    #[test]
    fn form_validates_free_provider() {
        let preset = preset_openai();
        let mut form = AiProviderFormState::for_preset(&preset);
        form.api_key = "sk-test".into();
        assert!(form.validate(false).is_ok());
        form.selected_kind = AiProviderKindView::Anthropic;
        assert!(form.validate(false).is_err());
        assert!(form.validate(true).is_ok());
    }

    #[test]
    fn per_module_requires_paid() {
        let preset = preset_openai();
        let mut form = AiProviderFormState::for_preset(&preset);
        form.api_key = "sk-test".into();
        form.per_module_override = Some("finance".into());
        assert!(form.validate(false).is_err());
        assert!(form.validate(true).is_ok());
    }
}
