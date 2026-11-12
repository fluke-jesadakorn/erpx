//! Branding profiles, storefront values and media references.
//!
//! These are the native owner contracts. The retained TypeScript read helpers
//! keep their accepted shapes; all mutations are parsed here and executed by
//! the Brand owner transaction.
use crate::domain::identity::ActiveBrandIdentity;
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

/// Rendered workspace brand for the front-app shell and document headers.
/// Mirrors the retained `WorkspaceBrand` values without exposing SQL readers.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBrand {
    pub scope_kind: String,
    pub scope_id: String,
    pub label: String,
    pub initials: String,
    pub primary_url: Option<String>,
    pub inverse_url: Option<String>,
    pub document_mode: String,
}

/// Rendered Company storefront values. The published Brand identity overlays the
/// retained storefront row without changing its stored values.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorefrontAppBrand {
    pub company_id: String,
    pub holding_slug: String,
    pub company_slug: String,
    pub company_name: String,
    pub short_name: String,
    pub accent_color: String,
    pub logo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_revision: Option<i64>,
}

/// Holding and Branch brand choice for a business document.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentBrand {
    pub mode: String,
    pub company_name: String,
    pub branch_name: Option<String>,
    pub holding: WorkspaceBrand,
    pub branch: Option<WorkspaceBrand>,
}

/// Branding administration projection: the Holding profile, every active Branch
/// profile and the Company storefront in one read.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrandingProfiles {
    pub holding: WorkspaceBrand,
    pub branches: Vec<WorkspaceBrand>,
    pub storefront: StorefrontAppBrand,
}

/// Retained executive Brand evidence row. The field names stay the accepted
/// snake_case reader shape consumed by the retained runtime evidence types.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BrandProfileEvidence {
    pub id: String,
    pub branch_name: String,
    pub document_mode: String,
    pub updated_at: i64,
    pub primary_attachment_id: Option<String>,
    pub inverse_attachment_id: Option<String>,
}

fn invalid_presentation() -> BusinessError {
    invalid(
        "BRAND_PRESENTATION_INVALID",
        "The Brand presentation context is unavailable.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

fn valid_workspace_base(value: &str) -> bool {
    value.len() > 1
        && value.len() <= 512
        && value.starts_with('/')
        && !value.ends_with('/')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'-' | b'_' | b'.' | b'~' | b':' | b'%' | b'!' | b'*' | b'\'' | b'(' | b')'
                )
        })
}

/// `encodeURIComponent` for path segments. Brand/attachment identifiers are
/// opaque; escaping keeps one identifier from changing the URL path.
fn encode_component(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(*byte as char);
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn present_id(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// The two retained media URL shapes. The caller supplies the current workspace
/// API base; the owner supplies the public Holding publication base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandMediaUrls {
    attachment_base: String,
    publication_base: String,
}

impl BrandMediaUrls {
    fn new(workspace_api_base: &str, holding_slug: &str) -> Self {
        Self {
            attachment_base: format!("{workspace_api_base}/attachments"),
            publication_base: format!(
                "/api/public/holdings/{}/media-publications",
                encode_component(holding_slug)
            ),
        }
    }

    pub fn attachment(&self, id: Option<&str>) -> Option<String> {
        present_id(id).map(|id| format!("{}/{}/content", self.attachment_base, encode_component(id)))
    }

    pub fn publication(&self, id: Option<&str>) -> Option<String> {
        present_id(id).map(|id| format!("{}/{}/content", self.publication_base, encode_component(id)))
    }
}

/// Presentation facts the Brand owner cannot derive: the selected Holding name
/// and public slug plus the current workspace API base and the Files owner's
/// legacy-attachment readability. Every value is validated once; a malformed
/// context fails closed instead of rendering a partial URL.
#[derive(Clone, Debug)]
pub struct BrandPresentationContext {
    holding_name: String,
    holding_slug: String,
    media: BrandMediaUrls,
    legacy_media_readable: bool,
}

impl BrandPresentationContext {
    pub fn new(
        holding_name: &str,
        holding_slug: &str,
        workspace_api_base: &str,
        legacy_media_readable: bool,
    ) -> Result<Self, BusinessError> {
        let holding_name = holding_name.trim();
        let holding_slug = holding_slug.trim();
        let workspace_api_base = workspace_api_base.trim_end_matches('/');
        if holding_name.is_empty()
            || holding_name.chars().count() > 200
            || !valid_slug(holding_slug)
            || !valid_workspace_base(workspace_api_base)
        {
            return Err(invalid_presentation());
        }
        Ok(Self {
            holding_name: holding_name.to_owned(),
            holding_slug: holding_slug.to_owned(),
            media: BrandMediaUrls::new(workspace_api_base, holding_slug),
            legacy_media_readable,
        })
    }

    pub fn holding_name(&self) -> &str {
        &self.holding_name
    }

    pub fn holding_slug(&self) -> &str {
        &self.holding_slug
    }

    pub fn media(&self) -> &BrandMediaUrls {
        &self.media
    }

    pub fn legacy_media_readable(&self) -> bool {
        self.legacy_media_readable
    }
}

/// The retained initials rule: first and last word initial, uppercase, three
/// characters, or an em dash when the label has no word.
fn brand_initials(label: &str) -> String {
    let words: Vec<&str> = label.split_whitespace().collect();
    let selected: Vec<&str> = if words.len() > 1 {
        vec![words[0], words[words.len() - 1]]
    } else {
        words
    };
    let initials: String = selected
        .into_iter()
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .take(3)
        .collect();
    if initials.is_empty() {
        "—".into()
    } else {
        initials
    }
}

/// Approved public media wins; the legacy attachment URL is only rendered while
/// the caller reports legacy reads still available.
pub fn brand_presentation(
    kind: BrandScopeKind,
    scope_id: &str,
    label: &str,
    profile: Option<&BrandingProfile>,
    urls: &BrandMediaUrls,
    legacy_media_readable: bool,
) -> WorkspaceBrand {
    let publication = |column: Option<&str>| urls.publication(column);
    let attachment = |column: Option<&str>| {
        if legacy_media_readable {
            None
        } else {
            urls.attachment(column)
        }
    };
    let primary_publication = profile.and_then(|row| row.primary_publication_id.as_deref());
    let inverse_publication = profile.and_then(|row| row.inverse_publication_id.as_deref());
    WorkspaceBrand {
        scope_kind: kind.as_str().into(),
        scope_id: scope_id.into(),
        label: label.into(),
        initials: brand_initials(label),
        primary_url: publication(primary_publication).or_else(|| {
            attachment(profile.and_then(|row| row.primary_attachment_id.as_deref()))
        }),
        inverse_url: publication(inverse_publication).or_else(|| {
            attachment(profile.and_then(|row| row.inverse_attachment_id.as_deref()))
        }),
        document_mode: match kind {
            BrandScopeKind::Holding => "holding".into(),
            BrandScopeKind::Branch => profile
                .map(|row| row.document_mode.clone())
                .unwrap_or_else(|| "branch".into()),
        },
    }
}

/// The retained storefront overlay: the active published identity replaces the
/// short name, accent and logo; the stored storefront row remains the fallback.
pub fn storefront_presentation(
    company_id: &str,
    context: &BrandPresentationContext,
    storefront: &BrandingStorefront,
    published: Option<&ActiveBrandIdentity>,
    urls: &BrandMediaUrls,
) -> StorefrontAppBrand {
    let publication = published
        .map(|active| active.identity.visual.logo_publication_id.as_deref())
        .unwrap_or(storefront.logo_publication_id.as_deref());
    let logo_url = urls.publication(publication).or_else(|| {
        if published.is_some() || context.legacy_media_readable() {
            None
        } else {
            urls.attachment(storefront.logo_attachment_id.as_deref())
        }
    });
    let short_name = match published {
        Some(active) => active.identity.identity.short_name.clone(),
        None => storefront
            .short_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| storefront.company_name.trim().chars().take(40).collect()),
    };
    let accent_color = published
        .map(|active| active.identity.visual.accent_color.clone())
        .or_else(|| storefront.accent_color.clone())
        .unwrap_or_else(|| "#3659c9".into());
    StorefrontAppBrand {
        company_id: company_id.into(),
        holding_slug: context.holding_slug().into(),
        company_slug: storefront.company_slug.clone(),
        company_name: storefront.company_name.clone(),
        short_name,
        accent_color,
        logo_url,
        identity_revision: published.map(|active| active.revision),
    }
}
