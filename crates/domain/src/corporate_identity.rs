//! Governed three-layer corporate identity contracts.
//!
//! Corporate identity is public presentation metadata owned by the ERPX control
//! plane. It is deliberately represented as typed values: executable CSS,
//! JavaScript, HTML, remote fonts, database credentials, and customer data-plane
//! references cannot be expressed by these contracts.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::data_plane::BusinessUnitId;

pub const PLATFORM_BRAND_READ: &str = "platform.brand.read";
pub const PLATFORM_BRAND_MANAGE: &str = "platform.brand.manage";
pub const PLATFORM_BRAND_PUBLISH: &str = "platform.brand.publish";
pub const PLATFORM_BRAND_LOCK: &str = "platform.brand.lock";
pub const TENANT_BRAND_READ: &str = "tenant.brand.read";
pub const TENANT_BRAND_MANAGE: &str = "tenant.brand.manage";
pub const TENANT_BRAND_PUBLISH: &str = "tenant.brand.publish";
pub const TENANT_BRAND_LOCK: &str = "tenant.brand.lock";
pub const BUSINESS_UNIT_BRAND_READ: &str = "business-unit.brand.read";
pub const BUSINESS_UNIT_BRAND_MANAGE: &str = "business-unit.brand.manage";
pub const BUSINESS_UNIT_BRAND_PUBLISH: &str = "business-unit.brand.publish";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorporateIdentityRoleTemplate {
    PlatformBrandAdmin,
    CorporateIdentityAdmin,
    BusinessUnitBrandAdmin,
}

impl CorporateIdentityRoleTemplate {
    pub const ALL: [Self; 3] = [
        Self::PlatformBrandAdmin,
        Self::CorporateIdentityAdmin,
        Self::BusinessUnitBrandAdmin,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlatformBrandAdmin => "platform_brand_admin",
            Self::CorporateIdentityAdmin => "corporate_identity_admin",
            Self::BusinessUnitBrandAdmin => "business_unit_brand_admin",
        }
    }

    pub fn permissions(self) -> BTreeSet<&'static str> {
        match self {
            Self::PlatformBrandAdmin => [
                PLATFORM_BRAND_READ,
                PLATFORM_BRAND_MANAGE,
                PLATFORM_BRAND_PUBLISH,
                PLATFORM_BRAND_LOCK,
            ]
            .into_iter()
            .collect(),
            Self::CorporateIdentityAdmin => [
                TENANT_BRAND_READ,
                TENANT_BRAND_MANAGE,
                TENANT_BRAND_PUBLISH,
                TENANT_BRAND_LOCK,
            ]
            .into_iter()
            .collect(),
            Self::BusinessUnitBrandAdmin => [
                BUSINESS_UNIT_BRAND_READ,
                BUSINESS_UNIT_BRAND_MANAGE,
                BUSINESS_UNIT_BRAND_PUBLISH,
            ]
            .into_iter()
            .collect(),
        }
    }

    pub fn parse(value: &str) -> Result<Self, CorporateIdentityError> {
        Self::ALL
            .into_iter()
            .find(|template| template.as_str() == value)
            .ok_or(CorporateIdentityError::InvalidRoleTemplate)
    }
}

pub const MAX_BRAND_DISPLAY_NAME_BYTES: usize = 96;
pub const MAX_BRAND_SHORT_NAME_BYTES: usize = 24;
pub const MAX_BRAND_ASSET_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_LOGO_ASSET_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_BRAND_BOOTSTRAP_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrandScope {
    Platform,
    Holding {
        tenant_id: String,
        holding_id: Uuid,
    },
    BusinessUnit {
        tenant_id: String,
        holding_id: Uuid,
        business_unit: BusinessUnitId,
    },
}

impl BrandScope {
    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        match self {
            Self::Platform => Ok(()),
            Self::Holding {
                tenant_id,
                holding_id,
            }
            | Self::BusinessUnit {
                tenant_id,
                holding_id,
                ..
            } => {
                validate_reference(tenant_id, 128)?;
                if holding_id.is_nil() {
                    return Err(CorporateIdentityError::InvalidScope);
                }
                Ok(())
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

    pub fn tenant_id(&self) -> Option<&str> {
        match self {
            Self::Platform => None,
            Self::Holding { tenant_id, .. } | Self::BusinessUnit { tenant_id, .. } => {
                Some(tenant_id)
            }
        }
    }

    pub fn business_unit(&self) -> Option<BusinessUnitId> {
        match self {
            Self::BusinessUnit { business_unit, .. } => Some(*business_unit),
            Self::Platform | Self::Holding { .. } => None,
        }
    }

    pub fn canonical_key(&self) -> String {
        match self {
            Self::Platform => "platform".to_owned(),
            Self::Holding {
                tenant_id,
                holding_id,
            } => format!("holding:{tenant_id}:{holding_id}"),
            Self::BusinessUnit {
                tenant_id,
                holding_id,
                business_unit,
            } => format!(
                "business-unit:{tenant_id}:{holding_id}:{}",
                business_unit.as_str()
            ),
        }
    }

    pub fn is_direct_child_of(&self, parent: &Self) -> bool {
        match (parent, self) {
            (Self::Platform, Self::Holding { .. }) => true,
            (
                Self::Holding {
                    tenant_id: parent_tenant,
                    holding_id: parent_holding,
                },
                Self::BusinessUnit {
                    tenant_id,
                    holding_id,
                    ..
                },
            ) => parent_tenant == tenant_id && parent_holding == holding_id,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrandLifecycle {
    Draft,
    Published,
    Archived,
    Suspended,
}

impl BrandLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
            Self::Suspended => "suspended",
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

    pub fn parse(value: &str) -> Result<Self, CorporateIdentityError> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
            .ok_or(CorporateIdentityError::InvalidCategory)
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

    pub const fn css_stack(self) -> &'static str {
        match self {
            Self::ErpxSans => "Inter, 'Noto Sans Thai', ui-sans-serif, system-ui, sans-serif",
            Self::CorporateSans => {
                "'IBM Plex Sans', 'Noto Sans Thai', ui-sans-serif, system-ui, sans-serif"
            }
            Self::ThaiFormal => "Sarabun, ui-sans-serif, system-ui, sans-serif",
            Self::SystemUi => "ui-sans-serif, system-ui, -apple-system, sans-serif",
        }
    }

    pub fn parse(value: &str) -> Result<Self, CorporateIdentityError> {
        match value {
            "erpx_sans" => Ok(Self::ErpxSans),
            "corporate_sans" => Ok(Self::CorporateSans),
            "thai_formal" => Ok(Self::ThaiFormal),
            "system_ui" => Ok(Self::SystemUi),
            _ => Err(CorporateIdentityError::UnsupportedFont),
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
    pub fn new(value: impl Into<String>) -> Result<Self, CorporateIdentityError> {
        let value = value.into();
        if value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(CorporateIdentityError::InvalidColor)
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
    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        for (foreground, background, minimum) in [
            (&self.on_primary, &self.primary, 4.5),
            (&self.on_accent, &self.accent, 4.5),
            (&self.text, &self.canvas, 4.5),
            (&self.text, &self.surface, 4.5),
            (&self.text_muted, &self.canvas, 4.5),
            (&self.link, &self.canvas, 4.5),
            (&self.border, &self.canvas, 3.0),
            (&self.focus, &self.canvas, 3.0),
        ] {
            if foreground.contrast_ratio(background) + f64::EPSILON < minimum {
                return Err(CorporateIdentityError::InsufficientContrast);
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
    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        self.light.validate()?;
        self.dark.validate()?;
        self.validate_structure()
    }

    pub fn validate_structure(&self) -> Result<(), CorporateIdentityError> {
        if !(90..=125).contains(&self.heading_scale_percent)
            || !(90..=120).contains(&self.body_scale_percent)
            || !(120..=180).contains(&self.line_height_percent)
        {
            return Err(CorporateIdentityError::InvalidTypography);
        }
        Ok(())
    }

    pub fn neutral_erpx() -> Self {
        Self {
            light: BrandPalette {
                primary: color("#075985"),
                on_primary: color("#ffffff"),
                accent: color("#0f766e"),
                on_accent: color("#ffffff"),
                canvas: color("#f5f7f9"),
                surface: color("#ffffff"),
                surface_raised: color("#edf2f5"),
                border: color("#7b8b98"),
                text: color("#111827"),
                text_muted: color("#4b5b68"),
                link: color("#075985"),
                focus: color("#075985"),
            },
            dark: BrandPalette {
                primary: color("#67e8f9"),
                on_primary: color("#082f49"),
                accent: color("#5eead4"),
                on_accent: color("#042f2e"),
                canvas: color("#0b1117"),
                surface: color("#111a22"),
                surface_raised: color("#1b2833"),
                border: color("#687b89"),
                text: color("#f1f5f9"),
                text_muted: color("#a8bac7"),
                link: color("#67e8f9"),
                focus: color("#a5f3fc"),
            },
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandAssetReference {
    pub kind: BrandAssetKind,
    /// Immutable control-plane object key. Never a URL or credential.
    pub object_key: String,
    pub sha256: String,
    pub media_type: String,
    pub byte_length: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl BrandAssetReference {
    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        validate_asset_key(&self.object_key)?;
        if self.sha256.len() != 64
            || !self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || self.byte_length == 0
            || self.byte_length > self.kind.maximum_bytes()
        {
            return Err(CorporateIdentityError::InvalidAsset);
        }
        if !brand_asset_media_allowed(self.kind, &self.media_type)
            || self.width.is_some_and(|width| width == 0 || width > 4_096)
            || self
                .height
                .is_some_and(|height| height == 0 || height > 4_096)
        {
            return Err(CorporateIdentityError::InvalidAsset);
        }
        Ok(())
    }
}

fn brand_asset_media_allowed(kind: BrandAssetKind, media_type: &str) -> bool {
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

/// Validate upload bytes before any asset is admitted to the control-plane
/// object namespace. Raster callers must additionally decode and re-encode the
/// image before storage so metadata and malformed trailing chunks are removed.
pub fn validate_brand_asset_bytes(
    kind: BrandAssetKind,
    media_type: &str,
    bytes: &[u8],
) -> Result<String, CorporateIdentityError> {
    if bytes.is_empty()
        || u64::try_from(bytes.len()).map_err(|_| CorporateIdentityError::InvalidAsset)?
            > kind.maximum_bytes()
        || !brand_asset_media_allowed(kind, media_type)
    {
        return Err(CorporateIdentityError::InvalidAsset);
    }
    let valid_signature = match media_type {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]) && bytes.ends_with(&[0xff, 0xd9]),
        "image/webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "image/svg+xml" => validate_safe_svg(bytes).is_ok(),
        _ => false,
    };
    if !valid_signature {
        return Err(CorporateIdentityError::InvalidAsset);
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_safe_svg(bytes: &[u8]) -> Result<(), CorporateIdentityError> {
    let source = std::str::from_utf8(bytes).map_err(|_| CorporateIdentityError::InvalidAsset)?;
    if source
        .chars()
        .any(|character| character.is_control() && !character.is_ascii_whitespace())
    {
        return Err(CorporateIdentityError::InvalidAsset);
    }
    let source = source.trim();
    let lower = source.to_ascii_lowercase();
    if !lower.starts_with("<svg") || !lower.ends_with("</svg>") {
        return Err(CorporateIdentityError::InvalidAsset);
    }
    let normalized = lower.replace("xmlns=\"http://www.w3.org/2000/svg\"", "xmlns=\"svg\"");
    for prohibited in [
        "<!",
        "<?",
        "script",
        "style",
        "foreignobject",
        "animate",
        "<set",
        "iframe",
        "object",
        "embed",
        "<image",
        "<use",
        "filter",
        "mask",
        "pattern",
        "marker",
        "cursor",
        "font",
        "metadata",
        "href",
        "url(",
        "@import",
        "data:",
        "http:",
        "https:",
        "javascript:",
        "expression(",
    ] {
        if normalized.contains(prohibited) {
            return Err(CorporateIdentityError::InvalidAsset);
        }
    }

    const ALLOWED_TAGS: &[&str] = &[
        "svg", "g", "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "title",
        "desc",
    ];
    for fragment in normalized.split('<').skip(1) {
        let tag = fragment
            .trim_start_matches('/')
            .split(|character: char| {
                character.is_ascii_whitespace() || character == '>' || character == '/'
            })
            .next()
            .unwrap_or_default();
        if tag.is_empty() || !ALLOWED_TAGS.contains(&tag) {
            return Err(CorporateIdentityError::InvalidAsset);
        }
    }

    const ALLOWED_ATTRIBUTES: &[&str] = &[
        "xmlns",
        "viewbox",
        "width",
        "height",
        "x",
        "y",
        "x1",
        "y1",
        "x2",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "d",
        "points",
        "fill",
        "fill-rule",
        "fill-opacity",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "stroke-opacity",
        "opacity",
        "transform",
        "role",
        "aria-label",
        "focusable",
        "preserveaspectratio",
    ];
    for token in normalized.split(|character: char| {
        character.is_ascii_whitespace() || character == '<' || character == '>'
    }) {
        let Some((attribute, _)) = token.split_once('=') else {
            continue;
        };
        let attribute = attribute.trim_matches('/');
        if attribute.starts_with("on") || !ALLOWED_ATTRIBUTES.contains(&attribute) {
            return Err(CorporateIdentityError::InvalidAsset);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrandAssetManifest {
    pub logo_mark: Option<BrandAssetReference>,
    pub wordmark: Option<BrandAssetReference>,
    pub favicon: Option<BrandAssetReference>,
    pub login_artwork: Option<BrandAssetReference>,
}

impl BrandAssetManifest {
    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        for (reference, expected_kind) in [
            (self.logo_mark.as_ref(), BrandAssetKind::LogoMark),
            (self.wordmark.as_ref(), BrandAssetKind::Wordmark),
            (self.favicon.as_ref(), BrandAssetKind::Favicon),
            (self.login_artwork.as_ref(), BrandAssetKind::LoginArtwork),
        ] {
            if let Some(reference) = reference {
                reference.validate()?;
                if reference.kind != expected_kind {
                    return Err(CorporateIdentityError::InvalidAsset);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorporateIdentityProfile {
    pub profile_id: Uuid,
    pub scope: BrandScope,
    pub version: u64,
    pub parent_published_version: Option<u64>,
    pub lifecycle: BrandLifecycle,
    pub display_name: String,
    pub short_name: String,
    pub tokens: BrandTokenSet,
    pub assets: BrandAssetManifest,
    pub overridden_categories: BTreeSet<BrandCategory>,
    pub locked_categories: BTreeSet<BrandCategory>,
    pub updated_by: String,
}

impl CorporateIdentityProfile {
    pub fn neutral_platform() -> Self {
        Self {
            profile_id: Uuid::nil(),
            scope: BrandScope::Platform,
            version: 1,
            parent_published_version: None,
            lifecycle: BrandLifecycle::Published,
            display_name: "ERPX Platform".to_owned(),
            short_name: "ERPX".to_owned(),
            tokens: BrandTokenSet::neutral_erpx(),
            assets: BrandAssetManifest::default(),
            overridden_categories: BrandCategory::ALL.into_iter().collect(),
            locked_categories: BTreeSet::new(),
            updated_by: "erpx-system".to_owned(),
        }
    }

    pub fn validate(&self, for_publication: bool) -> Result<(), CorporateIdentityError> {
        self.scope.validate()?;
        if self.profile_id.is_nil() && !matches!(self.scope, BrandScope::Platform) {
            return Err(CorporateIdentityError::InvalidProfile);
        }
        if self.version == 0
            || self.display_name.trim().is_empty()
            || self.display_name.len() > MAX_BRAND_DISPLAY_NAME_BYTES
            || self.short_name.trim().is_empty()
            || self.short_name.len() > MAX_BRAND_SHORT_NAME_BYTES
        {
            return Err(CorporateIdentityError::InvalidProfile);
        }
        validate_safe_text(&self.display_name)?;
        validate_safe_text(&self.short_name)?;
        validate_reference(&self.updated_by, 255)?;
        if for_publication {
            self.tokens.validate()?;
        } else {
            self.tokens.validate_structure()?;
        }
        self.assets.validate()?;
        if matches!(self.scope, BrandScope::Platform)
            && self.overridden_categories != BrandCategory::ALL.into_iter().collect()
        {
            return Err(CorporateIdentityError::PlatformMustDefineEveryCategory);
        }
        if matches!(self.scope, BrandScope::BusinessUnit { .. })
            && !self.locked_categories.is_empty()
        {
            return Err(CorporateIdentityError::BusinessUnitCannotLock);
        }
        if for_publication && self.lifecycle != BrandLifecycle::Published {
            return Err(CorporateIdentityError::InvalidLifecycle);
        }
        Ok(())
    }

    pub fn assert_parent(
        &self,
        parent: &CorporateIdentityProfile,
    ) -> Result<(), CorporateIdentityError> {
        if !self.scope.is_direct_child_of(&parent.scope)
            || self.parent_published_version != Some(parent.version)
            || parent.lifecycle != BrandLifecycle::Published
        {
            return Err(CorporateIdentityError::InvalidParent);
        }
        if self
            .overridden_categories
            .iter()
            .any(|category| parent.locked_categories.contains(category))
        {
            return Err(CorporateIdentityError::CategoryLockedByParent);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCorporateIdentity {
    pub scope: BrandScope,
    pub published_version: u64,
    pub display_name: String,
    pub short_name: String,
    pub tokens: BrandTokenSet,
    pub assets: BrandAssetManifest,
    pub inherited_categories: BTreeSet<BrandCategory>,
    pub locked_categories: BTreeSet<BrandCategory>,
    pub digest: String,
}

impl ResolvedCorporateIdentity {
    pub fn neutral_erpx() -> Self {
        resolve_corporate_identity(&CorporateIdentityProfile::neutral_platform(), None, None)
            .expect("built-in ERPX identity must remain valid")
    }

    pub fn validate(&self) -> Result<(), CorporateIdentityError> {
        self.scope.validate()?;
        if self.published_version == 0
            || self.digest.len() != 64
            || !self.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(CorporateIdentityError::InvalidResolvedIdentity);
        }
        self.tokens.validate()?;
        self.assets.validate()
    }
}

pub fn resolve_corporate_identity(
    platform: &CorporateIdentityProfile,
    holding: Option<&CorporateIdentityProfile>,
    business_unit: Option<&CorporateIdentityProfile>,
) -> Result<ResolvedCorporateIdentity, CorporateIdentityError> {
    platform.validate(true)?;
    if !matches!(platform.scope, BrandScope::Platform) {
        return Err(CorporateIdentityError::InvalidScope);
    }

    let mut resolved = ResolvedCorporateIdentity {
        scope: BrandScope::Platform,
        published_version: platform.version,
        display_name: platform.display_name.clone(),
        short_name: platform.short_name.clone(),
        tokens: platform.tokens.clone(),
        assets: platform.assets.clone(),
        inherited_categories: BTreeSet::new(),
        locked_categories: platform.locked_categories.clone(),
        digest: String::new(),
    };

    if let Some(holding) = holding.filter(|profile| profile.lifecycle == BrandLifecycle::Published)
    {
        holding.validate(true)?;
        apply_child(&mut resolved, platform, holding)?;
    }
    if let Some(business_unit) =
        business_unit.filter(|profile| profile.lifecycle == BrandLifecycle::Published)
    {
        business_unit.validate(true)?;
        let parent = holding.ok_or(CorporateIdentityError::InvalidParent)?;
        apply_child(&mut resolved, parent, business_unit)?;
    }
    resolved.digest = resolved_digest(&resolved)?;
    resolved.validate()?;
    Ok(resolved)
}

fn apply_child(
    resolved: &mut ResolvedCorporateIdentity,
    parent: &CorporateIdentityProfile,
    child: &CorporateIdentityProfile,
) -> Result<(), CorporateIdentityError> {
    if !child.scope.is_direct_child_of(&parent.scope) {
        return Err(CorporateIdentityError::InvalidParent);
    }
    let active_overrides = child
        .overridden_categories
        .difference(&parent.locked_categories)
        .copied()
        .collect::<BTreeSet<_>>();

    if active_overrides.contains(&BrandCategory::Identity) {
        resolved.display_name.clone_from(&child.display_name);
        resolved.short_name.clone_from(&child.short_name);
        resolved.assets.clone_from(&child.assets);
    }
    if active_overrides.contains(&BrandCategory::Color) {
        resolved.tokens.light.clone_from(&child.tokens.light);
        resolved.tokens.dark.clone_from(&child.tokens.dark);
    }
    if active_overrides.contains(&BrandCategory::Typography) {
        resolved.tokens.font_pack = child.tokens.font_pack;
        resolved.tokens.heading_scale_percent = child.tokens.heading_scale_percent;
        resolved.tokens.body_scale_percent = child.tokens.body_scale_percent;
        resolved.tokens.line_height_percent = child.tokens.line_height_percent;
    }
    if active_overrides.contains(&BrandCategory::Shape) {
        resolved.tokens.radius = child.tokens.radius;
    }
    if active_overrides.contains(&BrandCategory::Elevation) {
        resolved.tokens.elevation = child.tokens.elevation;
    }
    if active_overrides.contains(&BrandCategory::Density) {
        resolved.tokens.density = child.tokens.density;
    }
    if active_overrides.contains(&BrandCategory::Navigation) {
        resolved.tokens.navigation = child.tokens.navigation;
        resolved.tokens.navigation_expanded_by_default =
            child.tokens.navigation_expanded_by_default;
    }
    if active_overrides.contains(&BrandCategory::Motion) {
        resolved.tokens.motion = child.tokens.motion;
    }

    resolved.scope.clone_from(&child.scope);
    resolved.published_version = child.version;
    resolved
        .locked_categories
        .extend(parent.locked_categories.iter());
    resolved
        .locked_categories
        .extend(child.locked_categories.iter());
    resolved.inherited_categories = BrandCategory::ALL
        .into_iter()
        .filter(|category| !active_overrides.contains(category))
        .collect();
    Ok(())
}

fn resolved_digest(value: &ResolvedCorporateIdentity) -> Result<String, CorporateIdentityError> {
    let mut canonical = value.clone();
    canonical.digest.clear();
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|_| CorporateIdentityError::InvalidResolvedIdentity)?;
    if bytes.len() > MAX_BRAND_BOOTSTRAP_BYTES {
        return Err(CorporateIdentityError::BootstrapTooLarge);
    }
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_reference(value: &str, maximum: usize) -> Result<(), CorporateIdentityError> {
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(char::is_control)
        || value.to_ascii_lowercase().contains("secret")
        || value.to_ascii_lowercase().contains("token")
    {
        Err(CorporateIdentityError::InvalidReference)
    } else {
        Ok(())
    }
}

fn validate_safe_text(value: &str) -> Result<(), CorporateIdentityError> {
    let lower = value.to_ascii_lowercase();
    if value.chars().any(char::is_control)
        || lower.contains('<')
        || lower.contains('>')
        || lower.contains("javascript:")
        || lower.contains("data:")
    {
        Err(CorporateIdentityError::UnsafeText)
    } else {
        Ok(())
    }
}

fn validate_asset_key(value: &str) -> Result<(), CorporateIdentityError> {
    if !value.starts_with("branding/")
        || value.len() > 512
        || value.contains("..")
        || value.contains('\0')
        || value.contains('?')
        || value.contains('#')
        || value.contains("://")
        || value.split('/').any(|segment| segment.is_empty())
    {
        Err(CorporateIdentityError::InvalidAsset)
    } else {
        Ok(())
    }
}

fn color(value: &str) -> HexColor {
    HexColor::new(value).expect("built-in palette must contain valid hex colors")
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CorporateIdentityError {
    #[error("corporate identity scope is invalid")]
    InvalidScope,
    #[error("corporate identity profile is invalid")]
    InvalidProfile,
    #[error("corporate identity lifecycle is invalid")]
    InvalidLifecycle,
    #[error("corporate identity parent is missing, stale, or outside scope")]
    InvalidParent,
    #[error("corporate identity category is invalid")]
    InvalidCategory,
    #[error("corporate identity category is locked by its parent")]
    CategoryLockedByParent,
    #[error("business-unit corporate identity cannot lock child categories")]
    BusinessUnitCannotLock,
    #[error("the platform corporate identity must define every category")]
    PlatformMustDefineEveryCategory,
    #[error("corporate identity color must be #RRGGBB")]
    InvalidColor,
    #[error("corporate identity palette does not satisfy WCAG contrast")]
    InsufficientContrast,
    #[error("corporate identity typography is outside the governed range")]
    InvalidTypography,
    #[error("corporate identity font is not in the bundled catalog")]
    UnsupportedFont,
    #[error("corporate identity asset is invalid")]
    InvalidAsset,
    #[error("corporate identity reference is invalid")]
    InvalidReference,
    #[error("corporate identity text contains prohibited markup")]
    UnsafeText,
    #[error("resolved corporate identity is invalid")]
    InvalidResolvedIdentity,
    #[error("public corporate identity bootstrap exceeds its bounded contract")]
    BootstrapTooLarge,
    #[error("corporate identity role template is not allow-listed")]
    InvalidRoleTemplate,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn published_child(scope: BrandScope, parent_version: u64) -> CorporateIdentityProfile {
        CorporateIdentityProfile {
            profile_id: Uuid::new_v4(),
            scope,
            version: 1,
            parent_published_version: Some(parent_version),
            lifecycle: BrandLifecycle::Published,
            display_name: "Acme Holding".to_owned(),
            short_name: "ACME".to_owned(),
            tokens: BrandTokenSet::neutral_erpx(),
            assets: BrandAssetManifest::default(),
            overridden_categories: [BrandCategory::Identity, BrandCategory::Shape]
                .into_iter()
                .collect(),
            locked_categories: BTreeSet::new(),
            updated_by: "brand-admin".to_owned(),
        }
    }

    #[test]
    fn neutral_identity_is_valid_and_bounded() {
        let resolved = ResolvedCorporateIdentity::neutral_erpx();
        resolved.validate().expect("neutral identity");
        assert_eq!(resolved.display_name, "ERPX Platform");
        assert_eq!(resolved.digest.len(), 64);
        assert!(serde_json::to_vec(&resolved).expect("json").len() < MAX_BRAND_BOOTSTRAP_BYTES);
    }

    #[test]
    fn holding_and_business_unit_resolve_only_unlocked_categories() {
        let mut platform = CorporateIdentityProfile::neutral_platform();
        platform.locked_categories.insert(BrandCategory::Shape);
        let holding_id = Uuid::new_v4();
        let holding_scope = BrandScope::Holding {
            tenant_id: "tenant-acme".to_owned(),
            holding_id,
        };
        let holding = published_child(holding_scope, platform.version);
        let unit = published_child(
            BrandScope::BusinessUnit {
                tenant_id: "tenant-acme".to_owned(),
                holding_id,
                business_unit: BusinessUnitId::Finance,
            },
            holding.version,
        );

        let resolved = resolve_corporate_identity(&platform, Some(&holding), Some(&unit))
            .expect("resolved identity");
        assert_eq!(resolved.display_name, "Acme Holding");
        assert_eq!(resolved.tokens.radius, BrandRadius::Soft);
        assert!(resolved.locked_categories.contains(&BrandCategory::Shape));
    }

    #[test]
    fn publication_parent_check_rejects_locked_override() {
        let mut platform = CorporateIdentityProfile::neutral_platform();
        platform.locked_categories.insert(BrandCategory::Identity);
        let holding = published_child(
            BrandScope::Holding {
                tenant_id: "tenant-acme".to_owned(),
                holding_id: Uuid::new_v4(),
            },
            platform.version,
        );
        assert_eq!(
            holding.assert_parent(&platform),
            Err(CorporateIdentityError::CategoryLockedByParent)
        );
    }

    #[test]
    fn low_contrast_and_executable_values_are_rejected() {
        let mut tokens = BrandTokenSet::neutral_erpx();
        tokens.light.text = tokens.light.canvas.clone();
        assert_eq!(
            tokens.validate(),
            Err(CorporateIdentityError::InsufficientContrast)
        );
        assert!(HexColor::new("red").is_err());
        assert!(validate_safe_text("<script>alert(1)</script>").is_err());
    }

    #[test]
    fn asset_contract_rejects_urls_and_cross_namespace_keys() {
        let reference = BrandAssetReference {
            kind: BrandAssetKind::LogoMark,
            object_key: "https://evil.invalid/logo.svg".to_owned(),
            sha256: "a".repeat(64),
            media_type: "image/svg+xml".to_owned(),
            byte_length: 128,
            width: None,
            height: None,
        };
        assert_eq!(
            reference.validate(),
            Err(CorporateIdentityError::InvalidAsset)
        );
    }

    #[test]
    fn asset_upload_validation_rejects_mime_spoofing_and_active_svg() {
        assert_eq!(
            validate_brand_asset_bytes(BrandAssetKind::LogoMark, "image/png", b"not-a-png"),
            Err(CorporateIdentityError::InvalidAsset)
        );
        for malicious in [
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.as_slice(),
            br#"<svg xmlns="http://www.w3.org/2000/svg"><image href="https://evil.invalid/x"/></svg>"#.as_slice(),
            br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><path d="M0 0"/></svg>"#.as_slice(),
            br#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject/></svg>"#.as_slice(),
        ] {
            assert_eq!(
                validate_brand_asset_bytes(
                    BrandAssetKind::LogoMark,
                    "image/svg+xml",
                    malicious,
                ),
                Err(CorporateIdentityError::InvalidAsset)
            );
        }
    }

    #[test]
    fn conservative_svg_geometry_subset_is_digest_addressable() {
        let safe = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" role="img" aria-label="Acme"><title>Acme</title><path d="M2 2 L22 2 L12 22 Z" fill="#075985"/></svg>"##;
        let digest = validate_brand_asset_bytes(BrandAssetKind::LogoMark, "image/svg+xml", safe)
            .expect("strict geometry-only SVG");
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn corporate_identity_role_templates_are_exact_and_layered() {
        let platform = CorporateIdentityRoleTemplate::PlatformBrandAdmin.permissions();
        assert!(platform.contains(PLATFORM_BRAND_LOCK));
        assert!(!platform.contains(TENANT_BRAND_MANAGE));
        let unit = CorporateIdentityRoleTemplate::BusinessUnitBrandAdmin.permissions();
        assert!(unit.contains(BUSINESS_UNIT_BRAND_PUBLISH));
        assert!(!unit.contains(TENANT_BRAND_LOCK));
        assert!(CorporateIdentityRoleTemplate::parse("custom_brand_admin").is_err());
    }
}
