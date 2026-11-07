//! Brand identity rows, wire inputs and the current validation rules.
//!
//! The retained TypeScript owner accepted these values through strict Zod
//! objects: inner identity objects reject unknown keys, strings are trimmed
//! before their bounded length is checked, numbers must be integers, and the
//! accent colour is normalised to lower case. This module preserves those
//! accepted values and their canonical JSON order; it performs no I/O.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const MAX_SHORT_NAME_UNITS: usize = 40;
pub const MAX_TAGLINE_UNITS: usize = 160;
pub const MIN_REASON_UNITS: usize = 3;
pub const MAX_REASON_UNITS: usize = 4000;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

fn validation(message: &'static str) -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", message)
}

/// ECMAScript string length used by the retained Zod bounds.
fn js_len(value: &str) -> usize {
    value.encode_utf16().count()
}

fn exact_object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))).then_some(object)
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            let value = number.as_f64()?;
            (value.is_finite()
                && value.fract() == 0.0
                && value.abs() <= MAX_SAFE_INTEGER as f64)
                .then_some(value as i64)
        }),
        _ => None,
    }
}

fn text<'a>(value: &'a Value, message: &'static str) -> Result<&'a str, BusinessError> {
    value.as_str().ok_or_else(|| validation(message))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentitySection {
    pub short_name: String,
    pub tagline: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSection {
    pub accent_color: String,
    pub logo_publication_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandIdentity {
    pub identity: IdentitySection,
    pub visual: VisualSection,
}

impl BrandIdentity {
    /// Strictly parse the `{identity,visual}` object exactly as the retained
    /// Zod schema did, including trimming and colour normalisation.
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let root = exact_object(value, &["identity", "visual"])
            .ok_or_else(|| validation("Provide a complete Brand identity."))?;
        let identity = root
            .get("identity")
            .and_then(|value| value.as_object())
            .ok_or_else(|| validation("Provide a Brand identity name."))?;
        if identity
            .keys()
            .any(|key| key != "shortName" && key != "tagline")
        {
            return Err(validation("Provide only supported Brand identity fields."));
        }
        let short_name = identity
            .get("shortName")
            .ok_or_else(|| validation("Provide a Brand short name."))?;
        let short_name = text(short_name, "Provide a Brand short name.")?.trim();
        let short_name_units = js_len(short_name);
        if short_name_units == 0 {
            return Err(validation("Provide a Brand short name."));
        }
        if short_name_units > MAX_SHORT_NAME_UNITS {
            return Err(validation(
                "Use a Brand short name of at most 40 characters.",
            ));
        }
        let tagline = match identity.get("tagline") {
            None => "",
            Some(value) => text(value, "Provide a Brand tagline.")?.trim(),
        };
        if js_len(tagline) > MAX_TAGLINE_UNITS {
            return Err(validation("Use a Brand tagline of at most 160 characters."));
        }
        let visual = root
            .get("visual")
            .and_then(|value| value.as_object())
            .ok_or_else(|| validation("Provide complete Brand visual values."))?;
        if visual
            .keys()
            .any(|key| key != "accentColor" && key != "logoPublicationId")
        {
            return Err(validation("Provide only supported Brand visual fields."));
        }
        let accent_color = visual
            .get("accentColor")
            .ok_or_else(|| validation("Provide a Brand accent color."))?;
        let accent_color = text(accent_color, "Provide a Brand accent color.")?;
        if accent_color.len() != 7
            || !accent_color.starts_with('#')
            || !accent_color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(validation(
                "Use a six-digit hex Brand accent color such as #224488.",
            ));
        }
        let logo = visual
            .get("logoPublicationId")
            .ok_or_else(|| validation("Choose an approved logo publication or none."))?;
        let logo_publication_id = match logo {
            Value::Null => None,
            Value::String(id) if !id.is_empty() => Some(id.clone()),
            _ => return Err(validation("Choose an approved logo publication or none.")),
        };
        Ok(Self {
            identity: IdentitySection {
                short_name: short_name.to_owned(),
                tagline: tagline.to_owned(),
            },
            visual: VisualSection {
                accent_color: accent_color.to_ascii_lowercase(),
                logo_publication_id,
            },
        })
    }

    /// Stored identity JSON. Field order is the retained Zod object order.
    pub fn canonical_json(&self) -> Result<String, BusinessError> {
        serde_json::to_string(self)
            .map_err(|_| validation("The Brand identity could not be encoded."))
    }

    pub fn from_json(value: &str) -> Result<Self, BusinessError> {
        let parsed: Value = serde_json::from_str(value)
            .map_err(|_| unavailable("BRAND_IDENTITY_UNAVAILABLE", "The stored Brand identity is unavailable."))?;
        Self::parse(&parsed)
    }

    pub fn category(&self, category: BrandCategory) -> Value {
        match category {
            BrandCategory::Identity => json!(self.identity),
            BrandCategory::Visual => json!(self.visual),
        }
    }
}

pub fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Unavailable,
        code,
        message,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrandCategory {
    Identity,
    Visual,
}

impl BrandCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Visual => "visual",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "identity" => Some(Self::Identity),
            "visual" => Some(Self::Visual),
            _ => None,
        }
    }
}

fn reason(value: Option<&Value>) -> Result<String, BusinessError> {
    let value = value.ok_or_else(|| validation("Provide a change reason."))?;
    let value = text(value, "Provide a change reason.")?.trim();
    let units = js_len(value);
    if units < MIN_REASON_UNITS {
        return Err(validation("Use a reason of at least 3 characters."));
    }
    if units > MAX_REASON_UNITS {
        return Err(validation(
            "Use a reason of at most 4000 characters.",
        ));
    }
    Ok(value.to_owned())
}

fn positive(value: Option<&Value>, message: &'static str) -> Result<i64, BusinessError> {
    let value = integer(value.ok_or_else(|| validation(message))?)
        .ok_or_else(|| validation(message))?;
    if value < 1 {
        return Err(validation(message));
    }
    Ok(value)
}

fn non_negative(value: Option<&Value>, message: &'static str) -> Result<i64, BusinessError> {
    let value = integer(value.ok_or_else(|| validation(message))?)
        .ok_or_else(|| validation(message))?;
    if value < 0 {
        return Err(validation(message));
    }
    Ok(value)
}

fn optional_positive(value: Option<&Value>, message: &'static str) -> Result<Option<i64>, BusinessError> {
    match value {
        None => Ok(None),
        Some(value) => positive(Some(value), message).map(Some),
    }
}

fn identifier(value: Option<&Value>, message: &'static str) -> Result<Option<String>, BusinessError> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(validation(message)),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandDraftInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
    pub expected_published_version: i64,
    pub identity: BrandIdentity,
    pub reason: String,
}

impl BrandDraftInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| validation("Provide a Brand draft."))?;
        Ok(Self {
            id: identifier(object.get("id"), "Provide a valid Brand draft identifier.")?,
            expected_version: optional_positive(
                object.get("expectedVersion"),
                "Use the current positive draft version.",
            )?,
            expected_published_version: non_negative(
                object.get("expectedPublishedVersion"),
                "Review the current published identity version.",
            )?,
            identity: BrandIdentity::parse(
                object
                    .get("identity")
                    .ok_or_else(|| validation("Provide a Brand identity."))?,
            )?,
            reason: reason(object.get("reason"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandPublishInput {
    pub expected_version: i64,
    pub expected_published_version: i64,
    pub reason: String,
}

impl BrandPublishInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| validation("Provide a Brand publication review."))?;
        Ok(Self {
            expected_version: positive(
                object.get("expectedVersion"),
                "Use the current positive draft version.",
            )?,
            expected_published_version: non_negative(
                object.get("expectedPublishedVersion"),
                "Review the current published identity version.",
            )?,
            reason: reason(object.get("reason"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandRollbackInput {
    pub published_id: String,
    #[serde(rename = "expectedPublishedVersion")]
    pub expected_published_version: i64,
    pub reason: String,
}

impl BrandRollbackInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| validation("Provide a Brand rollback."))?;
        let published_id = identifier(
            object.get("publishedId"),
            "Choose a published Brand identity.",
        )?
        .ok_or_else(|| validation("Choose a published Brand identity."))?;
        Ok(Self {
            published_id,
            expected_published_version: positive(
                object.get("expectedPublishedVersion"),
                "Review the current published identity version.",
            )?,
            reason: reason(object.get("reason"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandCategoryLockInput {
    pub category: BrandCategory,
    pub locked: bool,
    pub expected_version: i64,
    pub expected_published_version: i64,
    pub reason: String,
}

impl BrandCategoryLockInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| validation("Provide a category protection change."))?;
        let category = object
            .get("category")
            .and_then(Value::as_str)
            .and_then(BrandCategory::parse)
            .ok_or_else(|| validation("Choose the identity or visual category."))?;
        let locked = object
            .get("locked")
            .and_then(Value::as_bool)
            .ok_or_else(|| validation("Choose whether the category is locked."))?;
        Ok(Self {
            category,
            locked,
            expected_version: non_negative(
                object.get("expectedVersion"),
                "Review the current category lock version.",
            )?,
            expected_published_version: positive(
                object.get("expectedPublishedVersion"),
                "Review the current published identity version.",
            )?,
            reason: reason(object.get("reason"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandCancelInput {
    pub expected_version: i64,
    pub reason: String,
}

impl BrandCancelInput {
    pub fn parse(raw: &Value) -> Result<Self, BusinessError> {
        let object = raw
            .as_object()
            .ok_or_else(|| validation("Provide a Brand draft cancellation."))?;
        Ok(Self {
            expected_version: positive(
                object.get("expectedVersion"),
                "Use the current positive draft version.",
            )?,
            reason: reason(object.get("reason"))?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrandDraft {
    pub id: String,
    pub company_id: String,
    pub based_revision: i64,
    pub identity_json: String,
    pub status: String,
    pub version: i64,
    pub created_by: String,
    pub updated_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrandPublished {
    pub id: String,
    pub company_id: String,
    pub revision: i64,
    pub identity_json: String,
    pub draft_id: String,
    pub draft_version: i64,
    pub created_by: String,
    pub published_by: String,
    pub published_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveBrandIdentity {
    pub id: String,
    pub company_id: String,
    pub revision: i64,
    pub identity_json: String,
    pub draft_id: String,
    pub draft_version: i64,
    pub created_by: String,
    pub published_by: String,
    pub published_at: i64,
    pub active_version: i64,
    pub identity: BrandIdentity,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActiveBrandIdentityRow {
    pub id: String,
    pub company_id: String,
    pub revision: i64,
    pub identity_json: String,
    pub draft_id: String,
    pub draft_version: i64,
    pub created_by: String,
    pub published_by: String,
    pub published_at: i64,
    pub active_version: i64,
}

impl ActiveBrandIdentityRow {
    pub fn into_identity(self) -> Result<ActiveBrandIdentity, BusinessError> {
        let identity = BrandIdentity::from_json(&self.identity_json)?;
        Ok(ActiveBrandIdentity {
            id: self.id,
            company_id: self.company_id,
            revision: self.revision,
            identity_json: self.identity_json,
            draft_id: self.draft_id,
            draft_version: self.draft_version,
            created_by: self.created_by,
            published_by: self.published_by,
            published_at: self.published_at,
            active_version: self.active_version,
            identity,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrandCategoryLock {
    pub company_id: String,
    pub category: String,
    pub locked: bool,
    pub approved_snapshot_json: String,
    pub version: i64,
    pub updated_by: String,
    pub reason: String,
    pub updated_at: i64,
}

impl BrandCategoryLock {
    pub fn category(&self) -> Option<BrandCategory> {
        BrandCategory::parse(&self.category)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrandIdentityEvent {
    pub id: String,
    pub company_id: String,
    pub record_id: String,
    pub action: String,
    pub version: i64,
    pub actor_id: String,
    pub reason: String,
    pub details_json: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrandMediaOption {
    pub id: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LegacyBrand {
    pub short_name: String,
    pub accent_color: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandIdentityWorkspace {
    pub drafts: Vec<BrandDraft>,
    pub versions: Vec<BrandPublished>,
    pub locks: Vec<BrandCategoryLock>,
    pub history: Vec<BrandIdentityEvent>,
    pub active: Option<ActiveBrandIdentity>,
    pub initial: BrandIdentity,
    pub media: Vec<BrandMediaOption>,
    pub actor_id: String,
    pub can_manage: bool,
    pub can_lock: bool,
}


/// Explicit row decoding; the workspace sqlx build does not enable derive macros.
macro_rules! from_row {
    ($type:ty { $($field:ident : $column:literal),+ $(,)? }) => {
        impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for $type {
            fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
                use sqlx::Row;
                Ok(Self { $($field: row.try_get($column)?),+ })
            }
        }
    };
}

from_row!(BrandDraft {
    id: "id",
    company_id: "company_id",
    based_revision: "based_revision",
    identity_json: "identity_json",
    status: "status",
    version: "version",
    created_by: "created_by",
    updated_by: "updated_by",
    created_at: "created_at",
    updated_at: "updated_at",
});

from_row!(BrandPublished {
    id: "id",
    company_id: "company_id",
    revision: "revision",
    identity_json: "identity_json",
    draft_id: "draft_id",
    draft_version: "draft_version",
    created_by: "created_by",
    published_by: "published_by",
    published_at: "published_at",
});

from_row!(ActiveBrandIdentityRow {
    id: "id",
    company_id: "company_id",
    revision: "revision",
    identity_json: "identity_json",
    draft_id: "draft_id",
    draft_version: "draft_version",
    created_by: "created_by",
    published_by: "published_by",
    published_at: "published_at",
    active_version: "active_version",
});

from_row!(BrandCategoryLock {
    company_id: "company_id",
    category: "category",
    locked: "locked",
    approved_snapshot_json: "approved_snapshot_json",
    version: "version",
    updated_by: "updated_by",
    reason: "reason",
    updated_at: "updated_at",
});

from_row!(BrandIdentityEvent {
    id: "id",
    company_id: "company_id",
    record_id: "record_id",
    action: "action",
    version: "version",
    actor_id: "actor_id",
    reason: "reason",
    details_json: "details_json",
    created_at: "created_at",
});
