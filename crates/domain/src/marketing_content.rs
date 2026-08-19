//! Holding-scoped Marketing content and digital-experience contracts.
//!
//! This module contains metadata and validated opaque references only. It
//! never reads or writes object storage, resolves DNS, serves a preview, or
//! submits a form. Those effects belong to later authorized infrastructure
//! adapters which can consume these typed contracts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::AuthorityScope;

pub const MARKETING_CONTENT_CONTRACT_VERSION: u16 = 1;
pub const MAX_CONTENT_NAME_BYTES: usize = 255;
pub const MAX_CONTENT_SLUG_BYTES: usize = 128;
pub const MAX_CONTENT_BODY_BYTES: usize = 64 * 1024;
pub const MAX_CONTENT_OBJECT_REF_BYTES: usize = 512;
pub const MAX_CONTENT_MEDIA_TYPE_BYTES: usize = 128;
pub const MAX_CONTENT_ASSET_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_CONTENT_ASSET_VERSIONS: usize = 1_024;
pub const MAX_CONTENT_ASSETS_PER_VERSION: usize = 256;
pub const MAX_CONTENT_FORM_FIELDS: usize = 128;
pub const MAX_CONTENT_FORM_VALUE_BYTES: usize = 16 * 1024;
/// The maximum deterministic ASCII email representation accepted by a form.
pub const MAX_CONTENT_EMAIL_BYTES: usize = 254;
/// The minimum number of digits in the canonical international phone form.
pub const MIN_CONTENT_PHONE_DIGITS: usize = 7;
/// The maximum number of digits in the canonical international phone form.
pub const MAX_CONTENT_PHONE_DIGITS: usize = 15;
/// The maximum plain-decimal representation accepted for a number field.
pub const MAX_CONTENT_NUMBER_BYTES: usize = 64;
pub const MAX_CONTENT_PREVIEW_DURATION: Duration = Duration::hours(24);
pub const MAX_CONTENT_SEO_KEYWORDS: usize = 32;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingContentError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} contains an unsafe value")]
    UnsafeValue { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("content must be holding-scoped")]
    InvalidScope,
    #[error("content reference is outside its owning holding scope")]
    ScopeMismatch,
    #[error("content reference does not match the owning aggregate")]
    ReferenceMismatch,
    #[error("content version must be the next contiguous version")]
    InvalidVersion,
    #[error("content version limit was reached")]
    VersionLimit,
    #[error("content version number overflowed")]
    VersionOverflow,
    #[error("content template requires exactly Thai and English variants")]
    MissingLocale,
    #[error("content template contains an unsupported or duplicate locale")]
    InvalidLocale,
    #[error("content lifecycle transition from {from:?} to {to:?} is not allowed")]
    InvalidTransition {
        from: PublicationStatus,
        to: PublicationStatus,
    },
    #[error("the requested content version is not the current candidate")]
    NotCurrentCandidate,
    #[error("approval must be performed by a different principal")]
    ApprovalSeparation,
    #[error("rollback target must be an older published version")]
    InvalidRollback,
    #[error("preview expiry is invalid or exceeds the maximum duration")]
    InvalidPreviewExpiry,
    #[error("preview is expired or revoked")]
    PreviewExpired,
    #[error("preview is already revoked")]
    PreviewAlreadyRevoked,
    #[error("custom domain hostname is invalid")]
    InvalidHostname,
    #[error("custom domain cannot serve content outside its holding scope")]
    DomainScopeMismatch,
    #[error("duplicate form field")]
    DuplicateFormField,
    #[error("form field is unknown")]
    UnknownFormField,
    #[error("form contains too many fields")]
    TooManyFormFields,
    #[error("form value exceeds the bounded maximum")]
    FormValueTooLong,
    #[error("form value is invalid for the {kind:?} field")]
    InvalidFormValue { kind: FormFieldKind },
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), MarketingContentError> {
    if value.trim().is_empty() {
        return Err(MarketingContentError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingContentError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingContentError::ControlCharacter { field });
    }
    Ok(())
}

fn identifier(value: Uuid, field: &'static str) -> Result<(), MarketingContentError> {
    if value.is_nil() {
        return Err(MarketingContentError::NilIdentifier { field });
    }
    Ok(())
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), MarketingContentError> {
    match scope {
        AuthorityScope::Platform => Err(MarketingContentError::InvalidScope),
        AuthorityScope::Holding { holding_id } => identifier(*holding_id, "holding_id"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => {
            identifier(*holding_id, "holding_id")?;
            identifier(*company_id, "company_id")
        }
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            identifier(*holding_id, "holding_id")?;
            identifier(*company_id, "company_id")?;
            identifier(*branch_id, "branch_id")
        }
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => {
            identifier(*holding_id, "holding_id")?;
            identifier(*organization_node_id, "organization_node_id")
        }
    }
}

fn exact_scope(
    expected: &AuthorityScope,
    actual: &AuthorityScope,
) -> Result<(), MarketingContentError> {
    validate_scope(expected)?;
    validate_scope(actual)?;
    if expected == actual {
        Ok(())
    } else {
        Err(MarketingContentError::ScopeMismatch)
    }
}

fn safe_slug(value: String, field: &'static str) -> Result<String, MarketingContentError> {
    bounded(&value, field, MAX_CONTENT_SLUG_BYTES)?;
    if value != value.to_ascii_lowercase()
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || value
            .chars()
            .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-')
    {
        return Err(MarketingContentError::UnsafeValue { field });
    }
    Ok(value)
}

/// The only locales supported by the first digital-experience contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentLocale {
    En,
    Th,
}

impl ContentLocale {
    pub const ALL: [Self; 2] = [Self::En, Self::Th];
}

/// A required bilingual value. Keeping the two locales explicit prevents a
/// later adapter from silently falling back to a different holding's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedText {
    pub en: String,
    pub th: String,
}

impl LocalizedText {
    pub fn new(
        en: impl Into<String>,
        th: impl Into<String>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            en: en.into(),
            th: th.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        bounded(&self.en, "english_text", MAX_CONTENT_BODY_BYTES)?;
        bounded(&self.th, "thai_text", MAX_CONTENT_BODY_BYTES)
    }

    pub fn get(&self, locale: ContentLocale) -> &str {
        match locale {
            ContentLocale::En => &self.en,
            ContentLocale::Th => &self.th,
        }
    }
}

/// An opaque, holding-bound pointer to an object stored by a future MinIO
/// adapter. It is not a URL, credential, or downloadable bearer token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageObjectRef {
    pub scope: AuthorityScope,
    pub object_key: String,
}

impl StorageObjectRef {
    pub fn new(
        scope: AuthorityScope,
        object_key: impl Into<String>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            scope,
            object_key: object_key.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        validate_scope(&self.scope)?;
        bounded(&self.object_key, "object_key", MAX_CONTENT_OBJECT_REF_BYTES)?;
        if self.object_key.starts_with('/')
            || self.object_key.contains("..")
            || self.object_key.contains("://")
            || self.object_key.contains('?')
            || self.object_key.contains('#')
        {
            return Err(MarketingContentError::UnsafeValue {
                field: "object_key",
            });
        }
        Ok(())
    }
}

/// A versioned asset descriptor. The bytes remain outside the domain crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentAssetVersion {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub scope: AuthorityScope,
    pub version: u64,
    pub object_ref: StorageObjectRef,
    pub content_sha256: String,
    pub byte_length: u64,
    pub media_type: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl ContentAssetVersion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        asset_id: Uuid,
        scope: AuthorityScope,
        version: u64,
        object_ref: StorageObjectRef,
        content_sha256: impl Into<String>,
        byte_length: u64,
        media_type: impl Into<String>,
        created_by: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            id,
            asset_id,
            scope,
            version,
            object_ref,
            content_sha256: content_sha256.into().to_ascii_lowercase(),
            byte_length,
            media_type: media_type.into(),
            created_by: created_by.into(),
            created_at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "asset_version_id")?;
        identifier(self.asset_id, "asset_id")?;
        validate_scope(&self.scope)?;
        if self.version == 0 {
            return Err(MarketingContentError::InvalidVersion);
        }
        exact_scope(&self.scope, &self.object_ref.scope)?;
        if self.byte_length == 0 || self.byte_length > MAX_CONTENT_ASSET_BYTES {
            return Err(MarketingContentError::UnsafeValue {
                field: "byte_length",
            });
        }
        if self.content_sha256.len() != 64
            || !self.content_sha256.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(MarketingContentError::UnsafeValue {
                field: "content_sha256",
            });
        }
        bounded(&self.media_type, "media_type", MAX_CONTENT_MEDIA_TYPE_BYTES)?;
        if !self.media_type.contains('/') || self.media_type.contains(';') {
            return Err(MarketingContentError::UnsafeValue {
                field: "media_type",
            });
        }
        bounded(&self.created_by, "created_by", MAX_CONTENT_NAME_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentAssetRef {
    pub asset_id: Uuid,
    pub version: u64,
    pub scope: AuthorityScope,
}

impl ContentAssetRef {
    pub fn new(
        asset_id: Uuid,
        version: u64,
        scope: AuthorityScope,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            asset_id,
            version,
            scope,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.asset_id, "asset_id")?;
        if self.version == 0 {
            return Err(MarketingContentError::InvalidVersion);
        }
        validate_scope(&self.scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentAsset {
    pub id: Uuid,
    pub scope: AuthorityScope,
    pub name: String,
    pub versions: BTreeMap<u64, ContentAssetVersion>,
}

impl ContentAsset {
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        name: impl Into<String>,
        initial_version: ContentAssetVersion,
    ) -> Result<Self, MarketingContentError> {
        identifier(id, "asset_id")?;
        validate_scope(&scope)?;
        let name = name.into();
        bounded(&name, "asset_name", MAX_CONTENT_NAME_BYTES)?;
        if initial_version.version != 1 {
            return Err(MarketingContentError::InvalidVersion);
        }
        let mut value = Self {
            id,
            scope,
            name,
            versions: BTreeMap::new(),
        };
        value.add_version(initial_version)?;
        Ok(value)
    }

    pub fn add_version(
        &mut self,
        version: ContentAssetVersion,
    ) -> Result<(), MarketingContentError> {
        version.validate()?;
        if version.asset_id != self.id {
            return Err(MarketingContentError::ReferenceMismatch);
        }
        exact_scope(&self.scope, &version.scope)?;
        let next = self
            .versions
            .last_key_value()
            .map_or(Ok(1_u64), |(current, _)| {
                current
                    .checked_add(1)
                    .ok_or(MarketingContentError::VersionOverflow)
            })?;
        if self.versions.len() >= MAX_CONTENT_ASSET_VERSIONS {
            return Err(MarketingContentError::VersionLimit);
        }
        if version.version != next {
            return Err(MarketingContentError::InvalidVersion);
        }
        self.versions.insert(version.version, version);
        Ok(())
    }

    pub fn reference(&self, version: u64) -> Result<ContentAssetRef, MarketingContentError> {
        if !self.versions.contains_key(&version) {
            return Err(MarketingContentError::InvalidVersion);
        }
        ContentAssetRef::new(self.id, version, self.scope.clone())
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "asset_id")?;
        validate_scope(&self.scope)?;
        bounded(&self.name, "asset_name", MAX_CONTENT_NAME_BYTES)?;
        if self.versions.is_empty() || self.versions.len() > MAX_CONTENT_ASSET_VERSIONS {
            return Err(MarketingContentError::InvalidVersion);
        }
        for (expected, version) in self.versions.values().enumerate() {
            let expected = u64::try_from(expected + 1).unwrap_or(u64::MAX);
            version.validate()?;
            if version.version != expected
                || version.asset_id != self.id
                || version.scope != self.scope
            {
                return Err(MarketingContentError::ReferenceMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeoMetadata {
    pub title: LocalizedText,
    pub description: LocalizedText,
    pub canonical_path: String,
    pub no_index: bool,
    pub keywords: Vec<String>,
}

impl SeoMetadata {
    pub fn new(
        title: LocalizedText,
        description: LocalizedText,
        canonical_path: impl Into<String>,
        no_index: bool,
        keywords: Vec<String>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            title,
            description,
            canonical_path: canonical_path.into(),
            no_index,
            keywords,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        self.title.validate()?;
        self.description.validate()?;
        bounded(
            &self.canonical_path,
            "canonical_path",
            MAX_CONTENT_SLUG_BYTES,
        )?;
        if !self.canonical_path.starts_with('/')
            || self.canonical_path.contains("://")
            || self.canonical_path.contains("..")
        {
            return Err(MarketingContentError::UnsafeValue {
                field: "canonical_path",
            });
        }
        if self.keywords.len() > MAX_CONTENT_SEO_KEYWORDS {
            return Err(MarketingContentError::TooLong {
                field: "keywords",
                max: MAX_CONTENT_SEO_KEYWORDS,
            });
        }
        for keyword in &self.keywords {
            bounded(keyword, "seo_keyword", MAX_CONTENT_NAME_BYTES)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialMetadata {
    pub title: LocalizedText,
    pub description: LocalizedText,
    pub image: Option<ContentAssetRef>,
}

impl SocialMetadata {
    pub fn new(
        title: LocalizedText,
        description: LocalizedText,
        image: Option<ContentAssetRef>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            title,
            description,
            image,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        self.title.validate()?;
        self.description.validate()?;
        if let Some(image) = &self.image {
            image.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateVariant {
    pub title: String,
    pub body: String,
}

impl TemplateVariant {
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            title: title.into(),
            body: body.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        bounded(&self.title, "template_title", MAX_CONTENT_NAME_BYTES)?;
        bounded(&self.body, "template_body", MAX_CONTENT_BODY_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTemplateVersion {
    pub id: Uuid,
    pub template_id: Uuid,
    pub scope: AuthorityScope,
    pub version: u64,
    pub variants: BTreeMap<ContentLocale, TemplateVariant>,
    pub seo: SeoMetadata,
    pub social: SocialMetadata,
    pub assets: Vec<ContentAssetRef>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl ContentTemplateVersion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        template_id: Uuid,
        scope: AuthorityScope,
        version: u64,
        variants: BTreeMap<ContentLocale, TemplateVariant>,
        seo: SeoMetadata,
        social: SocialMetadata,
        assets: Vec<ContentAssetRef>,
        created_by: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            id,
            template_id,
            scope,
            version,
            variants,
            seo,
            social,
            assets,
            created_by: created_by.into(),
            created_at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "template_version_id")?;
        identifier(self.template_id, "template_id")?;
        validate_scope(&self.scope)?;
        if self.version == 0 {
            return Err(MarketingContentError::InvalidVersion);
        }
        if self.variants.len() != ContentLocale::ALL.len()
            || !ContentLocale::ALL
                .iter()
                .all(|locale| self.variants.contains_key(locale))
        {
            return Err(MarketingContentError::MissingLocale);
        }
        for (locale, variant) in &self.variants {
            if !ContentLocale::ALL.contains(locale) {
                return Err(MarketingContentError::InvalidLocale);
            }
            variant.validate()?;
        }
        self.seo.validate()?;
        self.social.validate()?;
        if self.assets.len() > MAX_CONTENT_ASSETS_PER_VERSION {
            return Err(MarketingContentError::TooLong {
                field: "assets",
                max: MAX_CONTENT_ASSETS_PER_VERSION,
            });
        }
        let mut seen = BTreeSet::new();
        for asset in &self.assets {
            asset.validate()?;
            exact_scope(&self.scope, &asset.scope)?;
            if !seen.insert((asset.asset_id, asset.version)) {
                return Err(MarketingContentError::ReferenceMismatch);
            }
        }
        if let Some(image) = &self.social.image {
            exact_scope(&self.scope, &image.scope)?;
        }
        bounded(&self.created_by, "created_by", MAX_CONTENT_NAME_BYTES)
    }

    pub fn variant(&self, locale: ContentLocale) -> &TemplateVariant {
        self.variants
            .get(&locale)
            .expect("validated template versions contain Thai and English")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStatus {
    Draft,
    InReview,
    Approved,
    Scheduled,
    Published,
    RolledBack,
    Archived,
}

impl PublicationStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::InReview)
                | (Self::InReview, Self::Draft | Self::Approved)
                | (Self::Approved, Self::Scheduled | Self::Published)
                | (Self::Scheduled, Self::Published | Self::Draft)
                | (Self::Published, Self::RolledBack | Self::Archived)
                | (Self::RolledBack, Self::InReview | Self::Archived)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTemplateRef {
    pub template_id: Uuid,
    pub version: u64,
    pub scope: AuthorityScope,
}

impl ContentTemplateRef {
    pub fn new(
        template_id: Uuid,
        version: u64,
        scope: AuthorityScope,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            template_id,
            version,
            scope,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.template_id, "template_id")?;
        if self.version == 0 {
            return Err(MarketingContentError::InvalidVersion);
        }
        validate_scope(&self.scope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTemplate {
    pub id: Uuid,
    pub scope: AuthorityScope,
    pub slug: String,
    pub versions: BTreeMap<u64, ContentTemplateVersion>,
    pub status: PublicationStatus,
    pub latest_version: u64,
    pub approved_version: Option<u64>,
    pub published_version: Option<u64>,
    pub rolled_back_from: Option<u64>,
    pub last_published_at: Option<DateTime<Utc>>,
}

impl ContentTemplate {
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        slug: impl Into<String>,
        initial_version: ContentTemplateVersion,
    ) -> Result<Self, MarketingContentError> {
        identifier(id, "template_id")?;
        validate_scope(&scope)?;
        let slug = safe_slug(slug.into(), "template_slug")?;
        if initial_version.template_id != id || initial_version.version != 1 {
            return Err(MarketingContentError::ReferenceMismatch);
        }
        exact_scope(&scope, &initial_version.scope)?;
        let mut value = Self {
            id,
            scope,
            slug,
            versions: BTreeMap::new(),
            status: PublicationStatus::Draft,
            latest_version: 0,
            approved_version: None,
            published_version: None,
            rolled_back_from: None,
            last_published_at: None,
        };
        value.add_version(initial_version)?;
        Ok(value)
    }

    pub fn add_version(
        &mut self,
        version: ContentTemplateVersion,
    ) -> Result<(), MarketingContentError> {
        version.validate()?;
        if self.status == PublicationStatus::Archived {
            return Err(MarketingContentError::InvalidTransition {
                from: self.status,
                to: PublicationStatus::Draft,
            });
        }
        if version.template_id != self.id {
            return Err(MarketingContentError::ReferenceMismatch);
        }
        exact_scope(&self.scope, &version.scope)?;
        let next = self
            .latest_version
            .checked_add(1)
            .ok_or(MarketingContentError::VersionOverflow)?;
        if self.versions.len() >= MAX_CONTENT_ASSET_VERSIONS {
            return Err(MarketingContentError::VersionLimit);
        }
        if version.version != next {
            return Err(MarketingContentError::InvalidVersion);
        }
        self.versions.insert(version.version, version);
        self.latest_version = next;
        self.approved_version = None;
        self.rolled_back_from = None;
        self.status = PublicationStatus::Draft;
        Ok(())
    }

    pub fn reference(&self, version: u64) -> Result<ContentTemplateRef, MarketingContentError> {
        if !self.versions.contains_key(&version) {
            return Err(MarketingContentError::InvalidVersion);
        }
        ContentTemplateRef::new(self.id, version, self.scope.clone())
    }

    pub fn submit_for_review(&mut self, version: u64) -> Result<(), MarketingContentError> {
        self.require_candidate(version)?;
        self.transition(PublicationStatus::InReview)?;
        Ok(())
    }

    pub fn approve(
        &mut self,
        version: u64,
        approver: impl Into<String>,
    ) -> Result<(), MarketingContentError> {
        self.require_candidate(version)?;
        let approver = approver.into();
        bounded(&approver, "approver", MAX_CONTENT_NAME_BYTES)?;
        if approver == self.versions[&version].created_by {
            return Err(MarketingContentError::ApprovalSeparation);
        }
        self.transition(PublicationStatus::Approved)?;
        self.approved_version = Some(version);
        Ok(())
    }

    pub fn schedule(&mut self, version: u64) -> Result<(), MarketingContentError> {
        self.require_candidate(version)?;
        if self.approved_version != Some(version) {
            return Err(MarketingContentError::NotCurrentCandidate);
        }
        self.transition(PublicationStatus::Scheduled)
    }

    pub fn publish(
        &mut self,
        version: u64,
        published_at: DateTime<Utc>,
    ) -> Result<(), MarketingContentError> {
        self.require_candidate(version)?;
        if self.approved_version != Some(version) {
            return Err(MarketingContentError::NotCurrentCandidate);
        }
        if !matches!(
            self.status,
            PublicationStatus::Approved | PublicationStatus::Scheduled
        ) {
            return Err(MarketingContentError::InvalidTransition {
                from: self.status,
                to: PublicationStatus::Published,
            });
        }
        self.transition(PublicationStatus::Published)?;
        self.published_version = Some(version);
        self.last_published_at = Some(published_at);
        self.rolled_back_from = None;
        Ok(())
    }

    pub fn rollback(
        &mut self,
        target_version: u64,
        rolled_back_at: DateTime<Utc>,
    ) -> Result<(), MarketingContentError> {
        let current = self
            .published_version
            .ok_or(MarketingContentError::InvalidRollback)?;
        if !matches!(
            self.status,
            PublicationStatus::Published | PublicationStatus::RolledBack
        ) || target_version >= current
            || !self.versions.contains_key(&target_version)
        {
            return Err(MarketingContentError::InvalidRollback);
        }
        self.transition(PublicationStatus::RolledBack)?;
        self.published_version = Some(target_version);
        self.rolled_back_from = Some(current);
        self.last_published_at = Some(rolled_back_at);
        Ok(())
    }

    pub fn archive(&mut self) -> Result<(), MarketingContentError> {
        self.transition(PublicationStatus::Archived)
    }

    fn require_candidate(&self, version: u64) -> Result<(), MarketingContentError> {
        if version != self.latest_version || !self.versions.contains_key(&version) {
            return Err(MarketingContentError::NotCurrentCandidate);
        }
        Ok(())
    }

    fn transition(&mut self, next: PublicationStatus) -> Result<(), MarketingContentError> {
        if !self.status.can_transition_to(next) {
            return Err(MarketingContentError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        self.status = next;
        Ok(())
    }

    pub fn issue_preview(
        &self,
        version: u64,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<PreviewGrant, MarketingContentError> {
        let template_ref = self.reference(version)?;
        PreviewGrant::new(Uuid::new_v4(), template_ref, issued_at, expires_at)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "template_id")?;
        validate_scope(&self.scope)?;
        safe_slug(self.slug.clone(), "template_slug")?;
        if self.versions.is_empty() || self.versions.len() > MAX_CONTENT_ASSET_VERSIONS {
            return Err(MarketingContentError::InvalidVersion);
        }
        for (expected, version) in self.versions.values().enumerate() {
            let expected = u64::try_from(expected + 1).unwrap_or(u64::MAX);
            version.validate()?;
            if version.version != expected
                || version.template_id != self.id
                || version.scope != self.scope
            {
                return Err(MarketingContentError::ReferenceMismatch);
            }
        }
        if self.latest_version != self.versions.len() as u64
            || self.latest_version == 0
            || self
                .approved_version
                .is_some_and(|version| version > self.latest_version)
            || self
                .published_version
                .is_some_and(|version| version > self.latest_version)
            || self
                .rolled_back_from
                .is_some_and(|version| version > self.latest_version)
        {
            return Err(MarketingContentError::InvalidVersion);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewGrant {
    pub id: Uuid,
    pub template: ContentTemplateRef,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl PreviewGrant {
    pub fn new(
        id: Uuid,
        template: ContentTemplateRef,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, MarketingContentError> {
        let value = Self {
            id,
            template,
            issued_at,
            expires_at,
            revoked_at: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "preview_id")?;
        self.template.validate()?;
        let duration = self.expires_at.signed_duration_since(self.issued_at);
        if duration <= Duration::zero() || duration > MAX_CONTENT_PREVIEW_DURATION {
            return Err(MarketingContentError::InvalidPreviewExpiry);
        }
        if self.revoked_at.is_some_and(|at| at < self.issued_at) {
            return Err(MarketingContentError::InvalidPreviewExpiry);
        }
        Ok(())
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.issued_at <= at
            && at < self.expires_at
            && self.revoked_at.is_none_or(|revoked_at| at < revoked_at)
    }

    pub fn authorize_at(&self, at: DateTime<Utc>) -> Result<(), MarketingContentError> {
        if self.is_active_at(at) {
            Ok(())
        } else {
            Err(MarketingContentError::PreviewExpired)
        }
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) -> Result<(), MarketingContentError> {
        if at < self.issued_at {
            return Err(MarketingContentError::InvalidPreviewExpiry);
        }
        if self.revoked_at.is_some() {
            return Err(MarketingContentError::PreviewAlreadyRevoked);
        }
        self.revoked_at = Some(at);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormFieldKind {
    Text,
    Email,
    Phone,
    Number,
    Consent,
}

fn invalid_form_value(kind: FormFieldKind) -> MarketingContentError {
    // Deliberately retain only the field kind. Validation errors must not
    // carry submitted values into logs, events, or transport responses.
    MarketingContentError::InvalidFormValue { kind }
}

fn validate_email_value(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_CONTENT_EMAIL_BYTES
        || !value.is_ascii()
        || value.chars().any(char::is_control)
    {
        return false;
    }

    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    if parts.next().is_some()
        || local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "!#$%&'*+-/=?^_`{|}~.".contains(ch))
    {
        return false;
    }

    if domain.len() > 253 || !domain.contains('.') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    })
}

/// Accept only the canonical `+` followed by 7–15 ASCII digits form. This is
/// intentionally stricter than accepting display separators or extensions so
/// that whitespace and control-character injection cannot be normalized into a
/// different phone number by a later adapter.
fn validate_phone_value(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('+') else {
        return false;
    };
    (MIN_CONTENT_PHONE_DIGITS..=MAX_CONTENT_PHONE_DIGITS).contains(&digits.len())
        && digits
            .as_bytes()
            .first()
            .is_some_and(|digit| *digit != b'0')
        && digits.bytes().all(|digit| digit.is_ascii_digit())
}

/// Accept a bounded plain decimal: an optional leading minus, digits, and at
/// most one decimal point with digits on both sides when present. Exponents,
/// non-finite spellings, plus signs, and whitespace are intentionally absent
/// from this canonical representation.
fn validate_number_value(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_CONTENT_NUMBER_BYTES
        || !value.is_ascii()
        || value.chars().any(char::is_control)
    {
        return false;
    }

    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if unsigned.is_empty() || unsigned.starts_with('+') {
        return false;
    }
    let mut parts = unsigned.split('.');
    let Some(integer) = parts.next() else {
        return false;
    };
    let fraction = parts.next();
    if parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|digit| digit.is_ascii_digit())
        || fraction.is_some_and(|digits| {
            digits.is_empty() || !digits.bytes().all(|digit| digit.is_ascii_digit())
        })
    {
        return false;
    }

    value.parse::<f64>().is_ok_and(f64::is_finite)
}

fn validate_form_field_value(
    kind: FormFieldKind,
    value: &str,
) -> Result<(), MarketingContentError> {
    let valid = match kind {
        FormFieldKind::Text => true,
        FormFieldKind::Email => validate_email_value(value),
        FormFieldKind::Phone => validate_phone_value(value),
        FormFieldKind::Number => validate_number_value(value),
        // Consent is intentionally not truthy parsing: only this exact,
        // lower-case ASCII representation means the user opted in.
        FormFieldKind::Consent => value == "true",
    };
    valid.then_some(()).ok_or_else(|| invalid_form_value(kind))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    pub key: String,
    pub label: LocalizedText,
    pub kind: FormFieldKind,
    pub required: bool,
    pub max_length: usize,
}

impl FormField {
    pub fn new(
        key: impl Into<String>,
        label: LocalizedText,
        kind: FormFieldKind,
        required: bool,
        max_length: usize,
    ) -> Result<Self, MarketingContentError> {
        let key = key.into();
        bounded(&key, "form_field_key", MAX_CONTENT_SLUG_BYTES)?;
        if key != key.to_ascii_lowercase()
            || key
                .chars()
                .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '_')
        {
            return Err(MarketingContentError::UnsafeValue {
                field: "form_field_key",
            });
        }
        if max_length == 0 || max_length > MAX_CONTENT_FORM_VALUE_BYTES {
            return Err(MarketingContentError::FormValueTooLong);
        }
        label.validate()?;
        Ok(Self {
            key,
            label,
            kind,
            required,
            max_length,
        })
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        bounded(&self.key, "form_field_key", MAX_CONTENT_SLUG_BYTES)?;
        if self.key != self.key.to_ascii_lowercase()
            || self
                .key
                .chars()
                .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '_')
        {
            return Err(MarketingContentError::UnsafeValue {
                field: "form_field_key",
            });
        }
        if self.max_length == 0 || self.max_length > MAX_CONTENT_FORM_VALUE_BYTES {
            return Err(MarketingContentError::FormValueTooLong);
        }
        self.label.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormStatus {
    Draft,
    InReview,
    Published,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentFormRef {
    pub form_id: Uuid,
    pub scope: AuthorityScope,
}

impl ContentFormRef {
    pub fn new(form_id: Uuid, scope: AuthorityScope) -> Result<Self, MarketingContentError> {
        identifier(form_id, "form_id")?;
        validate_scope(&scope)?;
        Ok(Self { form_id, scope })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentForm {
    pub id: Uuid,
    pub scope: AuthorityScope,
    pub slug: String,
    pub fields: Vec<FormField>,
    pub status: FormStatus,
}

impl ContentForm {
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        slug: impl Into<String>,
        fields: Vec<FormField>,
    ) -> Result<Self, MarketingContentError> {
        identifier(id, "form_id")?;
        validate_scope(&scope)?;
        let slug = safe_slug(slug.into(), "form_slug")?;
        if fields.is_empty() || fields.len() > MAX_CONTENT_FORM_FIELDS {
            return Err(MarketingContentError::TooManyFormFields);
        }
        let mut seen = BTreeSet::new();
        for field in &fields {
            if !seen.insert(field.key.clone()) {
                return Err(MarketingContentError::DuplicateFormField);
            }
        }
        Ok(Self {
            id,
            scope,
            slug,
            fields,
            status: FormStatus::Draft,
        })
    }

    pub fn reference(&self) -> Result<ContentFormRef, MarketingContentError> {
        ContentFormRef::new(self.id, self.scope.clone())
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "form_id")?;
        validate_scope(&self.scope)?;
        safe_slug(self.slug.clone(), "form_slug")?;
        if self.fields.is_empty() || self.fields.len() > MAX_CONTENT_FORM_FIELDS {
            return Err(MarketingContentError::TooManyFormFields);
        }
        let mut seen = BTreeSet::new();
        for field in &self.fields {
            if !seen.insert(field.key.clone()) {
                return Err(MarketingContentError::DuplicateFormField);
            }
            field.validate()?;
        }
        Ok(())
    }

    pub fn submit_for_review(&mut self) -> Result<(), MarketingContentError> {
        if self.status != FormStatus::Draft {
            return Err(MarketingContentError::InvalidTransition {
                from: PublicationStatus::Draft,
                to: PublicationStatus::InReview,
            });
        }
        self.status = FormStatus::InReview;
        Ok(())
    }

    pub fn publish(&mut self) -> Result<(), MarketingContentError> {
        if self.status != FormStatus::InReview {
            return Err(MarketingContentError::InvalidTransition {
                from: PublicationStatus::Draft,
                to: PublicationStatus::Published,
            });
        }
        self.status = FormStatus::Published;
        Ok(())
    }

    pub fn archive(&mut self) -> Result<(), MarketingContentError> {
        if self.status != FormStatus::Published {
            return Err(MarketingContentError::InvalidTransition {
                from: PublicationStatus::Published,
                to: PublicationStatus::Archived,
            });
        }
        self.status = FormStatus::Archived;
        Ok(())
    }

    pub fn validate_submission(
        &self,
        form: &ContentFormRef,
        values: &BTreeMap<String, String>,
    ) -> Result<(), MarketingContentError> {
        self.validate()?;
        if form.form_id != self.id || form.scope != self.scope {
            return Err(MarketingContentError::ScopeMismatch);
        }
        if self.status != FormStatus::Published {
            return Err(MarketingContentError::InvalidTransition {
                from: PublicationStatus::Draft,
                to: PublicationStatus::Published,
            });
        }
        for field in &self.fields {
            match values.get(&field.key) {
                Some(value) if field.required && value.trim().is_empty() => {
                    return Err(MarketingContentError::Required {
                        field: "form_value",
                    });
                }
                Some(value) if value.len() <= field.max_length => {}
                Some(_) => return Err(MarketingContentError::FormValueTooLong),
                None if field.required => {
                    return Err(MarketingContentError::Required {
                        field: "form_value",
                    });
                }
                None => {}
            }
        }
        if values
            .keys()
            .any(|key| !self.fields.iter().any(|field| field.key == *key))
        {
            return Err(MarketingContentError::UnknownFormField);
        }
        for field in &self.fields {
            if let Some(value) = values.get(&field.key) {
                validate_form_field_value(field.kind, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomDomainStatus {
    PendingVerification,
    Active,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDomain {
    pub id: Uuid,
    pub scope: AuthorityScope,
    pub hostname: String,
    pub status: CustomDomainStatus,
}

impl CustomDomain {
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        hostname: impl Into<String>,
    ) -> Result<Self, MarketingContentError> {
        identifier(id, "custom_domain_id")?;
        validate_scope(&scope)?;
        let hostname = normalize_hostname(hostname.into())?;
        Ok(Self {
            id,
            scope,
            hostname,
            status: CustomDomainStatus::PendingVerification,
        })
    }

    pub fn activate(&mut self) {
        self.status = CustomDomainStatus::Active;
    }

    pub fn suspend(&mut self) {
        self.status = CustomDomainStatus::Suspended;
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        identifier(self.id, "custom_domain_id")?;
        validate_scope(&self.scope)?;
        if normalize_hostname(self.hostname.clone())? != self.hostname {
            return Err(MarketingContentError::InvalidHostname);
        }
        Ok(())
    }

    pub fn bind_template(
        &self,
        template: &ContentTemplateRef,
        bound_at: DateTime<Utc>,
    ) -> Result<CustomDomainBinding, MarketingContentError> {
        template.validate()?;
        if !self.scope.contains(&template.scope) {
            return Err(MarketingContentError::DomainScopeMismatch);
        }
        CustomDomainBinding::new(self, template, bound_at)
    }
}

fn normalize_hostname(value: String) -> Result<String, MarketingContentError> {
    bounded(&value, "hostname", 253)?;
    let value = value.trim_end_matches('.').to_ascii_lowercase();
    if value.is_empty()
        || value == "localhost"
        || value.contains("://")
        || value.contains('/')
        || value.contains('*')
        || value.parse::<std::net::IpAddr>().is_ok()
        || !value.contains('.')
    {
        return Err(MarketingContentError::InvalidHostname);
    }
    for label in value.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || label
                .chars()
                .any(|ch| !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-')
        {
            return Err(MarketingContentError::InvalidHostname);
        }
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomDomainBinding {
    pub domain_id: Uuid,
    pub domain_scope: AuthorityScope,
    pub template: ContentTemplateRef,
    pub bound_at: DateTime<Utc>,
}

impl CustomDomainBinding {
    fn new(
        domain: &CustomDomain,
        template: &ContentTemplateRef,
        bound_at: DateTime<Utc>,
    ) -> Result<Self, MarketingContentError> {
        identifier(domain.id, "custom_domain_id")?;
        if !domain.scope.contains(&template.scope) {
            return Err(MarketingContentError::DomainScopeMismatch);
        }
        Ok(Self {
            domain_id: domain.id,
            domain_scope: domain.scope.clone(),
            template: template.clone(),
            bound_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentTemplateCommand {
    SubmitForReview {
        template: ContentTemplateRef,
    },
    Approve {
        template: ContentTemplateRef,
        approver: String,
    },
    Publish {
        template: ContentTemplateRef,
        published_at: DateTime<Utc>,
    },
    Rollback {
        template: ContentTemplateRef,
        target_version: u64,
        rolled_back_at: DateTime<Utc>,
    },
}

impl ContentTemplateCommand {
    pub fn template(&self) -> &ContentTemplateRef {
        match self {
            Self::SubmitForReview { template }
            | Self::Approve { template, .. }
            | Self::Publish { template, .. }
            | Self::Rollback { template, .. } => template,
        }
    }

    pub fn validate(&self) -> Result<(), MarketingContentError> {
        self.template().validate()?;
        if let Self::Approve { approver, .. } = self {
            bounded(approver, "approver", MAX_CONTENT_NAME_BYTES)?;
        }
        if let Self::Rollback { target_version, .. } = self
            && *target_version == 0
        {
            return Err(MarketingContentError::InvalidRollback);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(holding_id: Uuid) -> AuthorityScope {
        AuthorityScope::Holding { holding_id }
    }

    fn template_version(
        template_id: Uuid,
        active_scope: AuthorityScope,
        version: u64,
        created_by: &str,
    ) -> ContentTemplateVersion {
        let mut variants = BTreeMap::new();
        variants.insert(
            ContentLocale::En,
            TemplateVariant::new("English title", "English body").unwrap(),
        );
        variants.insert(
            ContentLocale::Th,
            TemplateVariant::new("ชื่อภาษาไทย", "เนื้อหาภาษาไทย").unwrap(),
        );
        let title = LocalizedText::new("SEO title", "ชื่อ SEO").unwrap();
        let description = LocalizedText::new("SEO description", "คำอธิบาย SEO").unwrap();
        ContentTemplateVersion::new(
            Uuid::new_v4(),
            template_id,
            active_scope,
            version,
            variants,
            SeoMetadata::new(
                title.clone(),
                description.clone(),
                "/landing",
                false,
                vec![],
            )
            .unwrap(),
            SocialMetadata::new(title, description, None).unwrap(),
            vec![],
            created_by,
            Utc::now(),
        )
        .unwrap()
    }

    fn template() -> ContentTemplate {
        let id = Uuid::new_v4();
        let active_scope = scope(Uuid::new_v4());
        ContentTemplate::new(
            id,
            active_scope.clone(),
            "launch-page",
            template_version(id, active_scope, 1, "author"),
        )
        .unwrap()
    }

    fn form_field(key: &str, kind: FormFieldKind, required: bool, max_length: usize) -> FormField {
        FormField::new(
            key,
            LocalizedText::new("Field", "ฟิลด์").unwrap(),
            kind,
            required,
            max_length,
        )
        .unwrap()
    }

    fn published_form(field: FormField) -> (ContentForm, ContentFormRef) {
        let mut form = ContentForm::new(
            Uuid::new_v4(),
            scope(Uuid::new_v4()),
            "lead-form",
            vec![field],
        )
        .unwrap();
        form.submit_for_review().unwrap();
        form.publish().unwrap();
        let reference = form.reference().unwrap();
        (form, reference)
    }

    #[test]
    fn publication_lifecycle_requires_review_and_separated_approval() {
        let mut content = template();
        assert_eq!(content.status, PublicationStatus::Draft);
        assert!(content.submit_for_review(1).is_ok());
        assert_eq!(content.status, PublicationStatus::InReview);
        assert_eq!(
            content.approve(1, "author"),
            Err(MarketingContentError::ApprovalSeparation)
        );
        content.approve(1, "reviewer").unwrap();
        content.publish(1, Utc::now()).unwrap();
        assert_eq!(content.status, PublicationStatus::Published);
        assert_eq!(content.published_version, Some(1));
    }

    #[test]
    fn rollback_restores_an_older_published_version_and_new_work_is_draft() {
        let mut content = template();
        content.submit_for_review(1).unwrap();
        content.approve(1, "reviewer").unwrap();
        content.publish(1, Utc::now()).unwrap();
        let version_two = template_version(content.id, content.scope.clone(), 2, "author");
        content.add_version(version_two).unwrap();
        content.submit_for_review(2).unwrap();
        content.approve(2, "reviewer").unwrap();
        content.publish(2, Utc::now()).unwrap();
        content.rollback(1, Utc::now()).unwrap();
        assert_eq!(content.status, PublicationStatus::RolledBack);
        assert_eq!(content.published_version, Some(1));
        assert_eq!(content.rolled_back_from, Some(2));
        assert_eq!(
            content.rollback(2, Utc::now()),
            Err(MarketingContentError::InvalidRollback)
        );
    }

    #[test]
    fn preview_is_active_before_expiry_and_rejected_at_exact_expiry() {
        let content = template();
        let issued_at = Utc::now();
        let expires_at = issued_at + Duration::hours(1);
        let mut preview = content.issue_preview(1, issued_at, expires_at).unwrap();
        assert!(preview.authorize_at(issued_at).is_ok());
        assert_eq!(
            preview.authorize_at(expires_at),
            Err(MarketingContentError::PreviewExpired)
        );
        preview.revoke(issued_at + Duration::minutes(5)).unwrap();
        assert_eq!(
            preview.authorize_at(issued_at + Duration::minutes(6)),
            Err(MarketingContentError::PreviewExpired)
        );
    }

    #[test]
    fn form_submission_validates_email_without_external_lookup() {
        let (form, reference) = published_form(form_field(
            "email",
            FormFieldKind::Email,
            true,
            MAX_CONTENT_EMAIL_BYTES,
        ));
        let mut values = BTreeMap::from([(
            String::from("email"),
            String::from("a.person+1@example.com"),
        )]);
        assert!(form.validate_submission(&reference, &values).is_ok());

        for invalid in [
            "a.person.example.com",
            "a.person@@example.com",
            "a person@example.com",
            "a..person@example.com",
            "a@example",
            "a@example.com\n",
        ] {
            values.insert(String::from("email"), invalid.to_owned());
            assert_eq!(
                form.validate_submission(&reference, &values),
                Err(MarketingContentError::InvalidFormValue {
                    kind: FormFieldKind::Email,
                }),
                "invalid email should be rejected: {invalid:?}"
            );
        }
    }

    #[test]
    fn form_submission_accepts_only_bounded_international_phone_digits() {
        let (form, reference) = published_form(form_field(
            "phone",
            FormFieldKind::Phone,
            true,
            MAX_CONTENT_PHONE_DIGITS + 2,
        ));
        let mut values = BTreeMap::from([(String::from("phone"), String::from("+1234567"))]);
        assert!(form.validate_submission(&reference, &values).is_ok());

        values.insert(
            String::from("phone"),
            format!("+{}", "1".repeat(MIN_CONTENT_PHONE_DIGITS - 1)),
        );
        assert_eq!(
            form.validate_submission(&reference, &values),
            Err(MarketingContentError::InvalidFormValue {
                kind: FormFieldKind::Phone,
            })
        );
        values.insert(
            String::from("phone"),
            format!("+{}", "1".repeat(MAX_CONTENT_PHONE_DIGITS)),
        );
        assert!(form.validate_submission(&reference, &values).is_ok());

        for invalid in ["1234567", "+1234567890123456", "+1 234567", "+123-4567"] {
            values.insert(String::from("phone"), invalid.to_owned());
            assert_eq!(
                form.validate_submission(&reference, &values),
                Err(MarketingContentError::InvalidFormValue {
                    kind: FormFieldKind::Phone,
                })
            );
        }
    }

    #[test]
    fn form_submission_accepts_only_finite_bounded_plain_decimals() {
        let (form, reference) = published_form(form_field(
            "amount",
            FormFieldKind::Number,
            true,
            MAX_CONTENT_FORM_VALUE_BYTES,
        ));
        let mut values = BTreeMap::from([(String::from("amount"), String::from("-12.50"))]);
        assert!(form.validate_submission(&reference, &values).is_ok());

        values.insert(String::from("amount"), "9".repeat(MAX_CONTENT_NUMBER_BYTES));
        assert!(form.validate_submission(&reference, &values).is_ok());

        for invalid in ["1e3", "NaN", "inf", "1.", ".5", "+1"] {
            values.insert(String::from("amount"), invalid.to_owned());
            assert_eq!(
                form.validate_submission(&reference, &values),
                Err(MarketingContentError::InvalidFormValue {
                    kind: FormFieldKind::Number,
                })
            );
        }
        values.insert(
            String::from("amount"),
            "9".repeat(MAX_CONTENT_NUMBER_BYTES + 1),
        );
        assert_eq!(
            form.validate_submission(&reference, &values),
            Err(MarketingContentError::InvalidFormValue {
                kind: FormFieldKind::Number,
            })
        );
    }

    #[test]
    fn form_submission_requires_exact_lowercase_true_consent() {
        let (form, reference) =
            published_form(form_field("consent", FormFieldKind::Consent, true, 5));
        let mut values = BTreeMap::from([(String::from("consent"), String::from("true"))]);
        assert!(form.validate_submission(&reference, &values).is_ok());

        for invalid in ["false", "TRUE", " true", "true ", "1"] {
            values.insert(String::from("consent"), invalid.to_owned());
            assert_eq!(
                form.validate_submission(&reference, &values),
                Err(MarketingContentError::InvalidFormValue {
                    kind: FormFieldKind::Consent,
                })
            );
        }
    }

    #[test]
    fn required_blank_values_are_missing_and_text_length_boundary_is_preserved() {
        let (form, reference) = published_form(form_field("name", FormFieldKind::Text, true, 4));
        let mut values = BTreeMap::from([(String::from("name"), String::from(" \t"))]);
        assert_eq!(
            form.validate_submission(&reference, &values),
            Err(MarketingContentError::Required {
                field: "form_value",
            })
        );

        values.insert(String::from("name"), String::from("word"));
        assert!(form.validate_submission(&reference, &values).is_ok());
        values.insert(String::from("name"), String::from("words"));
        assert_eq!(
            form.validate_submission(&reference, &values),
            Err(MarketingContentError::FormValueTooLong)
        );
    }

    #[test]
    fn template_requires_both_locales_and_preserves_locale_selection() {
        let en = LocalizedText::new("English", "ภาษาไทย").unwrap();
        assert_eq!(en.get(ContentLocale::En), "English");
        assert_eq!(en.get(ContentLocale::Th), "ภาษาไทย");

        let id = Uuid::new_v4();
        let mut variants = BTreeMap::new();
        variants.insert(
            ContentLocale::En,
            TemplateVariant::new("Only English", "Body").unwrap(),
        );
        let title = LocalizedText::new("Title", "ชื่อ").unwrap();
        let description = LocalizedText::new("Description", "คำอธิบาย").unwrap();
        assert_eq!(
            ContentTemplateVersion::new(
                Uuid::new_v4(),
                id,
                scope(Uuid::new_v4()),
                1,
                variants,
                SeoMetadata::new(title.clone(), description.clone(), "/x", false, vec![]).unwrap(),
                SocialMetadata::new(title, description, None).unwrap(),
                vec![],
                "author",
                Utc::now(),
            )
            .unwrap_err(),
            MarketingContentError::MissingLocale
        );
    }

    #[test]
    fn custom_domain_rejects_cross_holding_template_scope() {
        let holding_a = Uuid::new_v4();
        let holding_b = Uuid::new_v4();
        let domain =
            CustomDomain::new(Uuid::new_v4(), scope(holding_a), "campaign.example.com").unwrap();
        let content = template_version(Uuid::new_v4(), scope(holding_b), 1, "author");
        let template_ref = ContentTemplateRef::new(content.template_id, 1, content.scope).unwrap();
        assert_eq!(
            domain.bind_template(&template_ref, Utc::now()),
            Err(MarketingContentError::DomainScopeMismatch)
        );
    }
}
