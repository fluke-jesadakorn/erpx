//! Branding profiles, storefront values and media references.
//!
//! These are the native owner contracts. The retained TypeScript read helpers
//! keep their accepted shapes; all mutations are parsed here and executed by
//! the Brand owner transaction.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn text<'a>(value: Option<&'a Value>, code: &'static str) -> Result<&'a str, BusinessError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(code, "Provide a valid branding value."))
}

fn optional_text(value: Option<&Value>, code: &'static str) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(invalid(code, "Provide a valid branding value.")),
    }
}

fn optional_integer(value: Option<&Value>) -> Result<Option<i64>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let value = value
                .as_i64()
                .or_else(|| value.as_f64().filter(|value| value.fract() == 0.0).map(|value| value as i64))
                .filter(|value| (0..=MAX_SAFE_INTEGER).contains(value))
                .ok_or_else(|| invalid("BRAND_VERSION_INVALID", "Use a valid branding version."))?;
            Ok(Some(value))
        }
    }
}

fn optional_bool(value: Option<&Value>) -> Result<Option<bool>, BusinessError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(invalid("STOREFRONT_CHANGE_REQUIRED", "Choose a valid storefront change.")),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrandScopeKind {
    Holding,
    Branch,
}

impl BrandScopeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Branch => "branch",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "holding" => Some(Self::Holding),
            "branch" => Some(Self::Branch),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrandVariant {
    Primary,
    Inverse,
}

impl BrandVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Inverse => "inverse",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "primary" => Some(Self::Primary),
            "inverse" => Some(Self::Inverse),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentMode {
    Holding,
    Branch,
    Both,
}

impl DocumentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Branch => "branch",
            Self::Both => "both",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "holding" => Some(Self::Holding),
            "branch" => Some(Self::Branch),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileInput {
    pub scope_kind: BrandScopeKind,
    pub scope_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_mode: Option<DocumentMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<BrandVariant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_updated_at: Option<i64>,
}

impl UpdateProfileInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| invalid("BRANDING_COMMAND_REQUIRED", "Choose a branding change."))?;
        let scope_kind = text(object.get("scopeKind"), "BRANDING_COMMAND_REQUIRED")
            .and_then(|value| BrandScopeKind::parse(value).ok_or_else(|| invalid("BRANDING_COMMAND_REQUIRED", "Choose a branding scope.")))?;
        let scope_id = text(object.get("scopeId"), "BRANDING_COMMAND_REQUIRED")?.to_owned();
        let document_mode = match object.get("documentMode") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .and_then(DocumentMode::parse)
                    .ok_or_else(|| invalid("BRAND_MODE_INVALID", "Choose a valid document branding mode."))?,
            ),
        };
        let remove = match object.get("remove") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_str()
                    .and_then(BrandVariant::parse)
                    .ok_or_else(|| invalid("BRANDING_COMMAND_REQUIRED", "Choose a branding variant."))?,
            ),
        };
        if document_mode.is_none() && remove.is_none() {
            return Err(invalid("BRANDING_COMMAND_REQUIRED", "Choose a document mode or a media removal."));
        }
        if scope_kind == BrandScopeKind::Holding && document_mode.is_some_and(|mode| mode != DocumentMode::Holding) {
            return Err(invalid("BRAND_MODE_INVALID", "Holding branding uses Holding mode."));
        }
        Ok(Self {
            scope_kind,
            scope_id,
            document_mode,
            remove,
            expected_updated_at: optional_integer(object.get("expectedUpdatedAt"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStorefrontInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_logo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_updated_at: Option<i64>,
}

impl UpdateStorefrontInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| invalid("STOREFRONT_CHANGE_REQUIRED", "Choose a storefront change."))?;
        let short_name = optional_text(object.get("shortName"), "STOREFRONT_NAME_INVALID")?
            .map(|value| value.trim().to_owned());
        if let Some(value) = &short_name {
            let units = value.chars().count();
            if units < 1 || units > 40 {
                return Err(invalid("STOREFRONT_NAME_INVALID", "Use a storefront name between 1 and 40 characters."));
            }
        }
        let accent_color = optional_text(object.get("accentColor"), "STOREFRONT_COLOR_INVALID")?;
        if let Some(value) = &accent_color {
            let bytes = value.as_bytes();
            if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
                return Err(invalid("STOREFRONT_COLOR_INVALID", "Use a six-digit hex color such as #3659c9."));
            }
        }
        let remove_logo = optional_bool(object.get("removeLogo"))?;
        if short_name.is_none() && accent_color.is_none() && !remove_logo.unwrap_or(false) {
            return Err(invalid("STOREFRONT_CHANGE_REQUIRED", "Choose a name, color, or logo change."));
        }
        Ok(Self {
            short_name,
            accent_color: accent_color.map(|value| value.to_ascii_lowercase()),
            remove_logo,
            expected_updated_at: optional_integer(object.get("expectedUpdatedAt"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandMediaInput {
    pub scope_kind: BrandScopeKind,
    pub scope_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<BrandVariant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_updated_at: Option<i64>,
}

impl BrandMediaInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| invalid("BRAND_MEDIA_INVALID", "Choose approved Brand media."))?;
        let scope_kind = text(object.get("scopeKind"), "BRAND_MEDIA_INVALID")
            .and_then(|value| BrandScopeKind::parse(value).ok_or_else(|| invalid("BRAND_MEDIA_INVALID", "Choose a branding scope.")))?;
        let scope_id = text(object.get("scopeId"), "BRAND_MEDIA_INVALID")?.to_owned();
        let variant = match object.get("variant") {
            None | Some(Value::Null) => None,
            Some(value) => Some(value.as_str().and_then(BrandVariant::parse).ok_or_else(|| invalid("BRAND_MEDIA_INVALID", "Choose a branding variant."))?),
        };
        let attachment_id = optional_text(object.get("attachmentId"), "BRAND_MEDIA_INVALID")?;
        let publication_id = optional_text(object.get("publicationId"), "BRAND_MEDIA_INVALID")?;
        if attachment_id.is_none() && publication_id.is_none() {
            return Err(invalid("BRAND_MEDIA_INVALID", "Choose approved Brand media."));
        }
        if scope_kind == BrandScopeKind::Branch && variant.is_none() {
            return Err(invalid("BRAND_MEDIA_INVALID", "Choose a Branch branding variant."));
        }
        Ok(Self {
            scope_kind,
            scope_id,
            variant,
            attachment_id,
            publication_id,
            expected_updated_at: optional_integer(object.get("expectedUpdatedAt"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingProfile {
    pub scope_kind: String,
    pub scope_id: String,
    pub company_id: Option<String>,
    pub primary_attachment_id: Option<String>,
    pub inverse_attachment_id: Option<String>,
    pub primary_publication_id: Option<String>,
    pub inverse_publication_id: Option<String>,
    pub document_mode: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingStorefront {
    pub company_id: String,
    pub company_slug: String,
    pub company_name: String,
    pub short_name: Option<String>,
    pub accent_color: Option<String>,
    pub logo_attachment_id: Option<String>,
    pub logo_publication_id: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingBranch {
    pub id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingAttachment {
    pub id: String,
    pub company_id: Option<String>,
    pub owner_type: String,
    pub owner_id: String,
    pub purpose: String,
    pub revoked_at: Option<i64>,
    pub object_key: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingPublication {
    pub id: String,
    pub company_id: String,
    pub target_type: String,
    pub target_id: String,
    pub state: String,
    pub source_attachment_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingWorkspace {
    pub holding_id: String,
    pub holding_name: String,
    pub company_id: String,
    pub company_slug: String,
    pub company_name: String,
    pub holding_profile: Option<BrandingProfile>,
    pub branches: Vec<(BrandingBranch, Option<BrandingProfile>)>,
    pub storefront: BrandingStorefront,
    pub published_identity: Option<Value>,
}
