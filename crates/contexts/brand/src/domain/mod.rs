use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CommandId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_DISPLAY_NAME_BYTES: usize = 96;
pub const MAX_SHORT_NAME_BYTES: usize = 24;
pub const MAX_DATA_PROFILE_REFERENCE_BYTES: usize = 128;
pub const MAX_ACTOR_REFERENCE_BYTES: usize = 255;
pub const MAX_OBJECT_KEY_BYTES: usize = 512;
pub const MAX_BRAND_ASSET_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_LOGO_ASSET_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BrandProfileId(Uuid);

impl BrandProfileId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, BrandDomainError> {
        if value.is_nil() {
            return Err(BrandDomainError::NilIdentifier {
                field: "brand_profile_id",
            });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for BrandProfileId {
    type Error = BrandDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<BrandProfileId> for Uuid {
    fn from(value: BrandProfileId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BusinessUnitKey {
    Operations,
    Finance,
    Sales,
    Procurement,
    Inventory,
    Assets,
    Maintenance,
    Quality,
    MarketingCrm,
    HrPayroll,
    PointOfSale,
    Legal,
}

impl BusinessUnitKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Operations => "operations",
            Self::Finance => "finance",
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Assets => "assets",
            Self::Maintenance => "maintenance",
            Self::Quality => "quality",
            Self::MarketingCrm => "marketing-crm",
            Self::HrPayroll => "hr-payroll",
            Self::PointOfSale => "point-of-sale",
            Self::Legal => "legal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrandScope {
    Platform,
    Holding {
        holding_id: Uuid,
    },
    BusinessUnit {
        holding_id: Uuid,
        business_unit: BusinessUnitKey,
    },
}

impl BrandScope {
    pub fn validate(&self) -> Result<(), BrandDomainError> {
        match self {
            Self::Platform => Ok(()),
            Self::Holding { holding_id } | Self::BusinessUnit { holding_id, .. } => {
                if holding_id.is_nil() {
                    return Err(BrandDomainError::NilIdentifier {
                        field: "holding_id",
                    });
                }
                Ok(())
            }
        }
    }

    pub fn authority_scope(&self) -> Result<AuthorityScope, BrandDomainError> {
        self.validate()?;
        match self {
            Self::Platform => Ok(AuthorityScope::Platform),
            Self::Holding { holding_id, .. } | Self::BusinessUnit { holding_id, .. } => {
                Ok(AuthorityScope::Holding {
                    holding_id: *holding_id,
                })
            }
        }
    }

    pub const fn holding_id(&self) -> Option<Uuid> {
        match self {
            Self::Platform => None,
            Self::Holding { holding_id, .. } | Self::BusinessUnit { holding_id, .. } => {
                Some(*holding_id)
            }
        }
    }

    pub const fn business_unit(&self) -> Option<BusinessUnitKey> {
        match self {
            Self::BusinessUnit { business_unit, .. } => Some(*business_unit),
            Self::Platform | Self::Holding { .. } => None,
        }
    }

    pub fn canonical_key(&self) -> String {
        match self {
            Self::Platform => "platform".into(),
            Self::Holding { holding_id } => format!("holding:{holding_id}"),
            Self::BusinessUnit {
                holding_id,
                business_unit,
            } => format!("business-unit:{holding_id}:{}", business_unit.as_str()),
        }
    }

    pub fn is_direct_child_of(&self, parent: &Self) -> bool {
        match (parent, self) {
            (Self::Platform, Self::Holding { .. }) => true,
            (
                Self::Holding {
                    holding_id: parent_holding,
                },
                Self::BusinessUnit { holding_id, .. },
            ) => parent_holding == holding_id,
            _ => false,
        }
    }

    fn asset_prefix(&self) -> String {
        match self {
            Self::Platform => "branding/platform/".into(),
            Self::Holding { holding_id, .. } => format!("branding/holding/{holding_id}/"),
            Self::BusinessUnit {
                holding_id,
                business_unit,
                ..
            } => format!(
                "branding/business-unit/{holding_id}/{}/",
                business_unit.as_str()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandLifecycle {
    Draft,
    Published,
}

impl BrandLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandCategory {
    Identity,
    Color,
    Typography,
    Shape,
    Elevation,
    Density,
    Navigation,
    Motion,
}

impl BrandCategory {
    pub const ALL: [Self; 8] = [
        Self::Identity,
        Self::Color,
        Self::Typography,
        Self::Shape,
        Self::Elevation,
        Self::Density,
        Self::Navigation,
        Self::Motion,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Color => "color",
            Self::Typography => "typography",
            Self::Shape => "shape",
            Self::Elevation => "elevation",
            Self::Density => "density",
            Self::Navigation => "navigation",
            Self::Motion => "motion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandFontPack {
    ErpxSans,
    CorporateSans,
    ThaiFormal,
    SystemUi,
}

impl BrandFontPack {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ErpxSans => "erpx_sans",
            Self::CorporateSans => "corporate_sans",
            Self::ThaiFormal => "thai_formal",
            Self::SystemUi => "system_ui",
        }
    }
}

macro_rules! presentation_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }
    };
}

presentation_enum!(BrandDensity {
    Compact => "compact",
    Comfortable => "comfortable",
    Spacious => "spacious",
});
presentation_enum!(BrandRadius {
    Square => "square",
    Soft => "soft",
    Rounded => "rounded",
});
presentation_enum!(BrandElevation {
    Flat => "flat",
    Subtle => "subtle",
    Elevated => "elevated",
});
presentation_enum!(BrandNavigation {
    Solid => "solid",
    Tinted => "tinted",
});
presentation_enum!(BrandMotion {
    None => "none",
    Subtle => "subtle",
    Standard => "standard",
});

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HexColor(String);

impl HexColor {
    pub fn try_new(value: impl Into<String>) -> Result<Self, BrandDomainError> {
        let value = value.into();
        if value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(BrandDomainError::InvalidColor)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn contrast_ratio(&self, other: &Self) -> f64 {
        let first = relative_luminance(self.rgb());
        let second = relative_luminance(other.rgb());
        let lighter = first.max(second);
        let darker = first.min(second);
        (lighter + 0.05) / (darker + 0.05)
    }

    fn rgb(&self) -> [u8; 3] {
        let bytes = self.0.as_bytes();
        [
            parse_hex_pair(bytes[1], bytes[2]),
            parse_hex_pair(bytes[3], bytes[4]),
            parse_hex_pair(bytes[5], bytes[6]),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandPalette {
    pub primary: HexColor,
    pub on_primary: HexColor,
    pub accent: HexColor,
    pub on_accent: HexColor,
    pub canvas: HexColor,
    pub surface: HexColor,
    pub surface_raised: HexColor,
    pub border: HexColor,
    pub text: HexColor,
    pub text_muted: HexColor,
    pub link: HexColor,
    pub focus: HexColor,
}

impl BrandPalette {
    pub fn validate(&self) -> Result<(), BrandDomainError> {
        for (foreground, background, minimum) in [
            (&self.on_primary, &self.primary, 4.5),
            (&self.on_accent, &self.accent, 4.5),
            (&self.text, &self.canvas, 4.5),
            (&self.text, &self.surface, 4.5),
            (&self.text_muted, &self.canvas, 4.5),
            (&self.link, &self.canvas, 4.5),
            // Base borders are intentionally subtle. Product CSS derives the
            // non-text control tier from the validated muted-text token.
            (&self.focus, &self.canvas, 3.0),
        ] {
            if foreground.contrast_ratio(background) + f64::EPSILON < minimum {
                return Err(BrandDomainError::InsufficientContrast);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandTokenSet {
    pub light: BrandPalette,
    pub dark: BrandPalette,
    pub font_pack: BrandFontPack,
    pub heading_scale_percent: u16,
    pub body_scale_percent: u16,
    pub line_height_percent: u16,
    pub density: BrandDensity,
    pub radius: BrandRadius,
    pub elevation: BrandElevation,
    pub navigation: BrandNavigation,
    pub navigation_expanded_by_default: bool,
    pub motion: BrandMotion,
}

impl BrandTokenSet {
    pub fn validate_structure(&self) -> Result<(), BrandDomainError> {
        if !(90..=125).contains(&self.heading_scale_percent)
            || !(90..=120).contains(&self.body_scale_percent)
            || !(120..=180).contains(&self.line_height_percent)
        {
            return Err(BrandDomainError::InvalidTypography);
        }
        Ok(())
    }

    pub fn validate_for_publication(&self) -> Result<(), BrandDomainError> {
        self.validate_structure()?;
        self.light.validate()?;
        self.dark.validate()
    }

    pub fn neutral_erpx() -> Self {
        Self {
            light: neutral_light_palette(),
            dark: neutral_dark_palette(),
            font_pack: BrandFontPack::ErpxSans,
            heading_scale_percent: 100,
            body_scale_percent: 100,
            line_height_percent: 150,
            density: BrandDensity::Comfortable,
            radius: BrandRadius::Soft,
            elevation: BrandElevation::Subtle,
            navigation: BrandNavigation::Tinted,
            navigation_expanded_by_default: true,
            motion: BrandMotion::Subtle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandAssetKind {
    LogoMark,
    Wordmark,
    Favicon,
    LoginArtwork,
}

impl BrandAssetKind {
    pub const fn maximum_bytes(self) -> u64 {
        match self {
            Self::LogoMark | Self::Wordmark | Self::Favicon => MAX_LOGO_ASSET_BYTES,
            Self::LoginArtwork => MAX_BRAND_ASSET_BYTES,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LogoMark => "logo_mark",
            Self::Wordmark => "wordmark",
            Self::Favicon => "favicon",
            Self::LoginArtwork => "login_artwork",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandAssetReference {
    pub kind: BrandAssetKind,
    /// Immutable object key in the scope-owned Brand namespace; never a URL or credential.
    pub object_key: String,
    pub sha256: String,
    pub media_type: String,
    pub byte_length: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl BrandAssetReference {
    pub fn validate_for_scope(&self, scope: &BrandScope) -> Result<(), BrandDomainError> {
        if !self.object_key.starts_with(&scope.asset_prefix())
            || self.object_key.len() > MAX_OBJECT_KEY_BYTES
            || self.object_key.contains("..")
            || self.object_key.contains('\0')
            || self.object_key.contains('?')
            || self.object_key.contains('#')
            || self.object_key.contains("://")
            || self.object_key.split('/').any(str::is_empty)
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.byte_length == 0
            || self.byte_length > self.kind.maximum_bytes()
            || self.width.is_some_and(|value| value == 0 || value > 4_096)
            || self.height.is_some_and(|value| value == 0 || value > 4_096)
            || !allowed_media_type(self.kind, &self.media_type)
        {
            return Err(BrandDomainError::InvalidAsset);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandAssetManifest {
    pub logo_mark: Option<BrandAssetReference>,
    pub wordmark: Option<BrandAssetReference>,
    pub favicon: Option<BrandAssetReference>,
    pub login_artwork: Option<BrandAssetReference>,
}

impl BrandAssetManifest {
    pub fn validate_for_scope(&self, scope: &BrandScope) -> Result<(), BrandDomainError> {
        for (reference, expected) in [
            (self.logo_mark.as_ref(), BrandAssetKind::LogoMark),
            (self.wordmark.as_ref(), BrandAssetKind::Wordmark),
            (self.favicon.as_ref(), BrandAssetKind::Favicon),
            (self.login_artwork.as_ref(), BrandAssetKind::LoginArtwork),
        ] {
            if let Some(reference) = reference {
                if reference.kind != expected {
                    return Err(BrandDomainError::InvalidAsset);
                }
                reference.validate_for_scope(scope)?;
            }
        }
        Ok(())
    }

    /// Returns every immutable object reference currently carried by this
    /// revision. The order is stable so owner-side facts and rebuilds are
    /// deterministic.
    pub fn references(&self) -> impl Iterator<Item = &BrandAssetReference> {
        [
            self.logo_mark.as_ref(),
            self.wordmark.as_ref(),
            self.favicon.as_ref(),
            self.login_artwork.as_ref(),
        ]
        .into_iter()
        .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandDefinition {
    pub display_name: String,
    pub short_name: String,
    pub tokens: BrandTokenSet,
    pub assets: BrandAssetManifest,
    pub overridden_categories: BTreeSet<BrandCategory>,
    pub locked_categories: BTreeSet<BrandCategory>,
}

impl BrandDefinition {
    fn validate(&self, scope: &BrandScope, publication: bool) -> Result<(), BrandDomainError> {
        validate_safe_text(&self.display_name, MAX_DISPLAY_NAME_BYTES, "display_name")?;
        validate_safe_text(&self.short_name, MAX_SHORT_NAME_BYTES, "short_name")?;
        self.tokens.validate_structure()?;
        if publication {
            self.tokens.validate_for_publication()?;
        }
        self.assets.validate_for_scope(scope)?;
        if matches!(scope, BrandScope::Platform)
            && self.overridden_categories != BrandCategory::ALL.into_iter().collect()
        {
            return Err(BrandDomainError::PlatformMustDefineEveryCategory);
        }
        if matches!(scope, BrandScope::BusinessUnit { .. }) && !self.locked_categories.is_empty() {
            return Err(BrandDomainError::BusinessUnitCannotLock);
        }
        Ok(())
    }

    pub fn neutral_platform() -> Self {
        Self {
            display_name: "ERPX Platform".into(),
            short_name: "ERPX".into(),
            tokens: BrandTokenSet::neutral_erpx(),
            assets: BrandAssetManifest::default(),
            overridden_categories: BrandCategory::ALL.into_iter().collect(),
            locked_categories: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentBrandWitness {
    pub scope: BrandScope,
    pub published_revision: u64,
    pub locked_categories: BTreeSet<BrandCategory>,
}

impl ParentBrandWitness {
    fn validate_for_child(&self, child: &BrandScope) -> Result<(), BrandDomainError> {
        self.scope.validate()?;
        if self.published_revision == 0 || !child.is_direct_child_of(&self.scope) {
            return Err(BrandDomainError::InvalidParent);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandRevision {
    revision: u64,
    lifecycle: BrandLifecycle,
    definition: BrandDefinition,
    parent_published_revision: Option<u64>,
    updated_by: String,
    updated_at: DateTime<Utc>,
}

impl BrandRevision {
    pub fn try_rehydrate(
        scope: &BrandScope,
        revision: u64,
        lifecycle: BrandLifecycle,
        definition: BrandDefinition,
        parent_published_revision: Option<u64>,
        updated_by: impl Into<String>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, BrandDomainError> {
        let updated_by = updated_by.into();
        if revision == 0 || parent_published_revision == Some(0) {
            return Err(BrandDomainError::InvalidAggregate);
        }
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        definition.validate(scope, lifecycle == BrandLifecycle::Published)?;
        Ok(Self {
            revision,
            lifecycle,
            definition,
            parent_published_revision,
            updated_by,
            updated_at,
        })
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn lifecycle(&self) -> BrandLifecycle {
        self.lifecycle
    }

    pub const fn definition(&self) -> &BrandDefinition {
        &self.definition
    }

    pub const fn parent_published_revision(&self) -> Option<u64> {
        self.parent_published_revision
    }

    pub fn updated_by(&self) -> &str {
        &self.updated_by
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedBrandRevision {
    pub profile_id: BrandProfileId,
    pub scope: BrandScope,
    pub source_revision: u64,
    pub definition: BrandDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandProfile {
    id: BrandProfileId,
    version: u64,
    scope: BrandScope,
    draft: Option<BrandRevision>,
    published: Option<BrandRevision>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl BrandProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: BrandProfileId,
        version: u64,
        scope: BrandScope,
        draft: Option<BrandRevision>,
        published: Option<BrandRevision>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, BrandDomainError> {
        let value = Self {
            id,
            version,
            scope,
            draft,
            published,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create_draft(
        id: BrandProfileId,
        scope: BrandScope,
        definition: BrandDefinition,
        parent: Option<&ParentBrandWitness>,
        updated_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, BrandDomainError> {
        scope.validate()?;
        validate_definition_with_parent(&scope, &definition, parent, false)?;
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        let revision = BrandRevision {
            revision: 1,
            lifecycle: BrandLifecycle::Draft,
            definition,
            parent_published_revision: parent.map(|value| value.published_revision),
            updated_by,
            updated_at: at,
        };
        let mut value = Self {
            id,
            version: 1,
            scope,
            draft: Some(revision),
            published: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.uncommitted_events.push(DomainEvent::DraftSaved);
        Ok(value)
    }

    pub fn save_draft(
        &mut self,
        definition: BrandDefinition,
        parent: Option<&ParentBrandWitness>,
        updated_by: String,
        at: DateTime<Utc>,
    ) -> Result<(), BrandDomainError> {
        self.ensure_time(at)?;
        validate_definition_with_parent(&self.scope, &definition, parent, false)?;
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.definition == definition && draft.updated_by == updated_by)
        {
            return Err(BrandDomainError::NoChange);
        }
        let revision = self.next_revision()?;
        self.bump(at)?;
        self.draft = Some(BrandRevision {
            revision,
            lifecycle: BrandLifecycle::Draft,
            definition,
            parent_published_revision: parent.map(|value| value.published_revision),
            updated_by,
            updated_at: at,
        });
        self.uncommitted_events.push(DomainEvent::DraftSaved);
        Ok(())
    }

    pub fn publish_draft(
        &mut self,
        parent: Option<&ParentBrandWitness>,
        decision: &HumanDecisionProof,
        command_id: CommandId,
        at: DateTime<Utc>,
    ) -> Result<(), BrandDomainError> {
        self.ensure_time(at)?;
        let target_scope = self.scope.authority_scope()?;
        if !decision.authority_scope().contains(&target_scope)
            || decision.decision_ref() != brand_publish_decision_ref(command_id, self.id)
            || decision.decided_at() != at
        {
            return Err(BrandDomainError::HumanDecisionRequired);
        }
        let updated_by = decision.principal_ref().to_owned();
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        let mut draft = self.draft.clone().ok_or(BrandDomainError::DraftNotFound)?;
        validate_definition_with_parent(&self.scope, &draft.definition, parent, true)?;
        if draft.parent_published_revision != parent.map(|value| value.published_revision) {
            return Err(BrandDomainError::StaleParent);
        }
        self.bump(at)?;
        draft.lifecycle = BrandLifecycle::Published;
        draft.updated_by = updated_by;
        draft.updated_at = at;
        self.published = Some(draft);
        self.draft = None;
        self.uncommitted_events.push(DomainEvent::Published {
            state: self.published_state(),
            at,
        });
        Ok(())
    }

    pub fn rollback(
        &mut self,
        source: ArchivedBrandRevision,
        parent: Option<&ParentBrandWitness>,
        updated_by: String,
        at: DateTime<Utc>,
    ) -> Result<(), BrandDomainError> {
        self.ensure_time(at)?;
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        if self.draft.is_some() {
            return Err(BrandDomainError::DraftBlocksRollback);
        }
        if source.profile_id != self.id || source.scope != self.scope || source.source_revision == 0
        {
            return Err(BrandDomainError::InvalidRollbackSource);
        }
        validate_definition_with_parent(&self.scope, &source.definition, parent, true)?;
        let revision = self.next_revision()?;
        self.bump(at)?;
        self.published = Some(BrandRevision {
            revision,
            lifecycle: BrandLifecycle::Published,
            definition: source.definition,
            parent_published_revision: parent.map(|value| value.published_revision),
            updated_by,
            updated_at: at,
        });
        self.uncommitted_events.push(DomainEvent::RolledBack {
            source_revision: source.source_revision,
            state: self.published_state(),
            at,
        });
        Ok(())
    }

    pub fn set_category_locks(
        &mut self,
        locked_categories: BTreeSet<BrandCategory>,
        parent: Option<&ParentBrandWitness>,
        updated_by: String,
        at: DateTime<Utc>,
    ) -> Result<(), BrandDomainError> {
        self.ensure_time(at)?;
        validate_reference(&updated_by, MAX_ACTOR_REFERENCE_BYTES, "updated_by")?;
        if matches!(self.scope, BrandScope::BusinessUnit { .. }) {
            return Err(BrandDomainError::BusinessUnitCannotLock);
        }
        if self.draft.is_some() {
            return Err(BrandDomainError::DraftBlocksLockChange);
        }
        let mut definition = self
            .published
            .as_ref()
            .ok_or(BrandDomainError::PublishedNotFound)?
            .definition
            .clone();
        validate_definition_with_parent(&self.scope, &definition, parent, true)?;
        if self
            .published
            .as_ref()
            .and_then(BrandRevision::parent_published_revision)
            != parent.map(|value| value.published_revision)
        {
            return Err(BrandDomainError::StaleParent);
        }
        if definition.locked_categories == locked_categories {
            return Err(BrandDomainError::NoChange);
        }
        definition.locked_categories = locked_categories;
        definition.validate(&self.scope, true)?;
        let revision = self.next_revision()?;
        let parent_published_revision = self
            .published
            .as_ref()
            .and_then(BrandRevision::parent_published_revision);
        self.bump(at)?;
        self.published = Some(BrandRevision {
            revision,
            lifecycle: BrandLifecycle::Published,
            definition,
            parent_published_revision,
            updated_by,
            updated_at: at,
        });
        self.uncommitted_events
            .push(DomainEvent::CategoryLocksChanged {
                state: self.published_state(),
                at,
            });
        Ok(())
    }

    pub fn validate(&self) -> Result<(), BrandDomainError> {
        self.scope.validate()?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(BrandDomainError::InvalidAggregate);
        }
        if self.draft.is_none() && self.published.is_none() {
            return Err(BrandDomainError::InvalidAggregate);
        }
        if let Some(draft) = &self.draft {
            if draft.revision == 0 || draft.lifecycle != BrandLifecycle::Draft {
                return Err(BrandDomainError::InvalidAggregate);
            }
            draft.definition.validate(&self.scope, false)?;
        }
        if let Some(published) = &self.published {
            if published.revision == 0 || published.lifecycle != BrandLifecycle::Published {
                return Err(BrandDomainError::InvalidAggregate);
            }
            published.definition.validate(&self.scope, true)?;
        }
        Ok(())
    }

    pub const fn id(&self) -> BrandProfileId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub const fn scope(&self) -> &BrandScope {
        &self.scope
    }

    pub const fn draft(&self) -> Option<&BrandRevision> {
        self.draft.as_ref()
    }

    pub const fn published(&self) -> Option<&BrandRevision> {
        self.published.as_ref()
    }

    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }

    fn next_revision(&self) -> Result<u64, BrandDomainError> {
        self.draft
            .iter()
            .chain(self.published.iter())
            .map(BrandRevision::revision)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(BrandDomainError::VersionOverflow)
    }

    fn bump(&mut self, at: DateTime<Utc>) -> Result<(), BrandDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(BrandDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    fn ensure_time(&self, at: DateTime<Utc>) -> Result<(), BrandDomainError> {
        if at < self.updated_at {
            Err(BrandDomainError::InvalidTimestamp)
        } else {
            Ok(())
        }
    }

    fn published_state(&self) -> PublishedBrandState {
        let published = self
            .published
            .as_ref()
            .expect("published events require a published revision");
        PublishedBrandState {
            profile_id: self.id,
            scope: self.scope.clone(),
            revision: published.revision,
            display_name: published.definition.display_name.clone(),
        }
    }
}

pub fn brand_publish_decision_ref(command_id: CommandId, profile_id: BrandProfileId) -> String {
    format!(
        "brand.draft.publish:{}:{}",
        command_id.as_uuid(),
        profile_id.as_uuid()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedBrandState {
    pub profile_id: BrandProfileId,
    pub scope: BrandScope,
    pub revision: u64,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    DraftSaved,
    Published {
        state: PublishedBrandState,
        at: DateTime<Utc>,
    },
    RolledBack {
        source_revision: u64,
        state: PublishedBrandState,
        at: DateTime<Utc>,
    },
    CategoryLocksChanged {
        state: PublishedBrandState,
        at: DateTime<Utc>,
    },
}

/// Pure hierarchy resolution used by projection adapters. It consumes only
/// published aggregate snapshots and never performs a write-side query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedBrandDefinition {
    pub scope: BrandScope,
    pub published_revision: u64,
    pub definition: BrandDefinition,
    pub inherited_categories: BTreeSet<BrandCategory>,
    pub effective_locked_categories: BTreeSet<BrandCategory>,
}

pub fn resolve_published_brand(
    platform: &BrandProfile,
    holding: Option<&BrandProfile>,
    business_unit: Option<&BrandProfile>,
) -> Result<ResolvedBrandDefinition, BrandDomainError> {
    platform.validate()?;
    if platform.scope != BrandScope::Platform {
        return Err(BrandDomainError::InvalidScope);
    }
    let published = platform
        .published
        .as_ref()
        .ok_or(BrandDomainError::PublishedNotFound)?;
    let mut resolved = ResolvedBrandDefinition {
        scope: BrandScope::Platform,
        published_revision: published.revision,
        definition: published.definition.clone(),
        inherited_categories: BTreeSet::new(),
        effective_locked_categories: published.definition.locked_categories.clone(),
    };

    if let Some(holding) = holding {
        apply_published_child(&mut resolved, platform, holding)?;
    }
    if let Some(business_unit) = business_unit {
        let holding = holding.ok_or(BrandDomainError::InvalidParent)?;
        apply_published_child(&mut resolved, holding, business_unit)?;
    }
    Ok(resolved)
}

fn apply_published_child(
    resolved: &mut ResolvedBrandDefinition,
    parent: &BrandProfile,
    child: &BrandProfile,
) -> Result<(), BrandDomainError> {
    parent.validate()?;
    child.validate()?;
    if !child.scope.is_direct_child_of(&parent.scope) {
        return Err(BrandDomainError::InvalidParent);
    }
    parent
        .published
        .as_ref()
        .ok_or(BrandDomainError::PublishedNotFound)?;
    let child_revision = child
        .published
        .as_ref()
        .ok_or(BrandDomainError::PublishedNotFound)?;
    let active_overrides = child_revision
        .definition
        .overridden_categories
        .difference(&resolved.effective_locked_categories)
        .copied()
        .collect::<BTreeSet<_>>();
    let child_definition = &child_revision.definition;
    if active_overrides.contains(&BrandCategory::Identity) {
        resolved
            .definition
            .display_name
            .clone_from(&child_definition.display_name);
        resolved
            .definition
            .short_name
            .clone_from(&child_definition.short_name);
        resolved
            .definition
            .assets
            .clone_from(&child_definition.assets);
    }
    if active_overrides.contains(&BrandCategory::Color) {
        resolved
            .definition
            .tokens
            .light
            .clone_from(&child_definition.tokens.light);
        resolved
            .definition
            .tokens
            .dark
            .clone_from(&child_definition.tokens.dark);
    }
    if active_overrides.contains(&BrandCategory::Typography) {
        resolved.definition.tokens.font_pack = child_definition.tokens.font_pack;
        resolved.definition.tokens.heading_scale_percent =
            child_definition.tokens.heading_scale_percent;
        resolved.definition.tokens.body_scale_percent = child_definition.tokens.body_scale_percent;
        resolved.definition.tokens.line_height_percent =
            child_definition.tokens.line_height_percent;
    }
    if active_overrides.contains(&BrandCategory::Shape) {
        resolved.definition.tokens.radius = child_definition.tokens.radius;
    }
    if active_overrides.contains(&BrandCategory::Elevation) {
        resolved.definition.tokens.elevation = child_definition.tokens.elevation;
    }
    if active_overrides.contains(&BrandCategory::Density) {
        resolved.definition.tokens.density = child_definition.tokens.density;
    }
    if active_overrides.contains(&BrandCategory::Navigation) {
        resolved.definition.tokens.navigation = child_definition.tokens.navigation;
        resolved.definition.tokens.navigation_expanded_by_default =
            child_definition.tokens.navigation_expanded_by_default;
    }
    if active_overrides.contains(&BrandCategory::Motion) {
        resolved.definition.tokens.motion = child_definition.tokens.motion;
    }

    resolved
        .definition
        .overridden_categories
        .clone_from(&child_definition.overridden_categories);
    resolved
        .definition
        .locked_categories
        .clone_from(&child_definition.locked_categories);
    resolved.scope.clone_from(&child.scope);
    resolved.published_revision = child_revision.revision;
    resolved
        .effective_locked_categories
        .extend(child_definition.locked_categories.iter().copied());
    resolved.inherited_categories = BrandCategory::ALL
        .into_iter()
        .filter(|category| !active_overrides.contains(category))
        .collect();
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BrandDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is invalid")]
    InvalidReference { field: &'static str },
    #[error("{field} is empty, too long, or contains prohibited markup")]
    UnsafeText { field: &'static str },
    #[error("brand scope is invalid")]
    InvalidScope,
    #[error("brand aggregate state is invalid")]
    InvalidAggregate,
    #[error("brand parent is missing or outside the direct hierarchy")]
    InvalidParent,
    #[error("brand draft references a stale published parent")]
    StaleParent,
    #[error("a child cannot override a category locked by its parent")]
    CategoryLockedByParent,
    #[error("the platform brand must define every category")]
    PlatformMustDefineEveryCategory,
    #[error("a business-unit brand cannot lock categories")]
    BusinessUnitCannotLock,
    #[error("brand color must be #RRGGBB")]
    InvalidColor,
    #[error("brand palette does not satisfy governed WCAG contrast")]
    InsufficientContrast,
    #[error("brand typography is outside the governed range")]
    InvalidTypography,
    #[error("brand asset reference is invalid or outside its scope namespace")]
    InvalidAsset,
    #[error("brand draft does not exist")]
    DraftNotFound,
    #[error("brand has not been published")]
    PublishedNotFound,
    #[error("a draft must be published or discarded before rollback")]
    DraftBlocksRollback,
    #[error("a draft must be published or discarded before changing category locks")]
    DraftBlocksLockChange,
    #[error("brand rollback source does not belong to this profile and scope")]
    InvalidRollbackSource,
    #[error("brand mutation would not change the aggregate")]
    NoChange,
    #[error("brand publication requires exact authenticated-human evidence")]
    HumanDecisionRequired,
    #[error("brand mutation timestamp predates the aggregate")]
    InvalidTimestamp,
    #[error("brand version overflowed")]
    VersionOverflow,
}

fn validate_definition_with_parent(
    scope: &BrandScope,
    definition: &BrandDefinition,
    parent: Option<&ParentBrandWitness>,
    publication: bool,
) -> Result<(), BrandDomainError> {
    definition.validate(scope, publication)?;
    match scope {
        BrandScope::Platform if parent.is_none() => Ok(()),
        BrandScope::Platform => Err(BrandDomainError::InvalidParent),
        BrandScope::Holding { .. } | BrandScope::BusinessUnit { .. } => {
            let parent = parent.ok_or(BrandDomainError::InvalidParent)?;
            parent.validate_for_child(scope)?;
            if definition
                .overridden_categories
                .iter()
                .any(|category| parent.locked_categories.contains(category))
            {
                return Err(BrandDomainError::CategoryLockedByParent);
            }
            Ok(())
        }
    }
}

fn validate_reference(
    value: &str,
    maximum: usize,
    field: &'static str,
) -> Result<(), BrandDomainError> {
    let lower = value.to_ascii_lowercase();
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(char::is_control)
        || lower.contains("secret")
        || lower.contains("token")
    {
        Err(BrandDomainError::InvalidReference { field })
    } else {
        Ok(())
    }
}

fn validate_safe_text(
    value: &str,
    maximum: usize,
    field: &'static str,
) -> Result<(), BrandDomainError> {
    let lower = value.to_ascii_lowercase();
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(char::is_control)
        || lower.contains('<')
        || lower.contains('>')
        || lower.contains("javascript:")
        || lower.contains("data:")
    {
        Err(BrandDomainError::UnsafeText { field })
    } else {
        Ok(())
    }
}

fn allowed_media_type(kind: BrandAssetKind, media_type: &str) -> bool {
    match kind {
        BrandAssetKind::LogoMark | BrandAssetKind::Wordmark => {
            matches!(media_type, "image/png" | "image/webp" | "image/svg+xml")
        }
        BrandAssetKind::Favicon => matches!(media_type, "image/png" | "image/webp"),
        BrandAssetKind::LoginArtwork => {
            matches!(media_type, "image/png" | "image/webp" | "image/jpeg")
        }
    }
}

fn color(value: &str) -> HexColor {
    HexColor::try_new(value).expect("built-in palette must contain valid colors")
}

fn neutral_light_palette() -> BrandPalette {
    BrandPalette {
        primary: color("#0b6b8a"),
        on_primary: color("#ffffff"),
        accent: color("#0f766e"),
        on_accent: color("#ffffff"),
        canvas: color("#f4f7f9"),
        surface: color("#ffffff"),
        surface_raised: color("#edf2f5"),
        border: color("#ccd7df"),
        text: color("#14202a"),
        text_muted: color("#5b6c78"),
        link: color("#0b6b8a"),
        focus: color("#0b6b8a"),
    }
}

fn neutral_dark_palette() -> BrandPalette {
    BrandPalette {
        primary: color("#66d9ef"),
        on_primary: color("#062a33"),
        accent: color("#5eead4"),
        on_accent: color("#042f2e"),
        canvas: color("#080d12"),
        surface: color("#0f161e"),
        surface_raised: color("#16212b"),
        border: color("#2a3a47"),
        text: color("#f4f7fa"),
        text_muted: color("#9eafbc"),
        link: color("#66d9ef"),
        focus: color("#b8f3ff"),
    }
}

fn parse_hex_pair(high: u8, low: u8) -> u8 {
    (hex_nibble(high) << 4) | hex_nibble(low)
}

const fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}

fn relative_luminance(rgb: [u8; 3]) -> f64 {
    let channels = rgb.map(|value| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

#[cfg(test)]
mod tests;
