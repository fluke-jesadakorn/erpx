//! Native Brand attachment-source image contract.
//!
//! This mirrors the retained TypeScript read in
//! `packages/server-brand/src/adapters/attachment-read-sources.ts`
//! (`brandAttachmentReadSourceImage`): the same brand-profile and storefront
//! rows for the same `holding_brand` / `branch_brand` /
//! `company_storefront_brand` target, read on the caller's transaction. The
//! organization snapshot stays with the Organization owner; this port owns
//! only the brand image. Unknown target kinds fail closed.
use erpx_server_shared::public::BusinessError;

/// Brand attachment read targets, byte-identical to the retained
/// `isBrandAttachmentReadTarget` predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrandAttachmentKind {
    HoldingBrand,
    BranchBrand,
    CompanyStorefrontBrand,
}

impl BrandAttachmentKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "holding_brand" => Some(Self::HoldingBrand),
            "branch_brand" => Some(Self::BranchBrand),
            "company_storefront_brand" => Some(Self::CompanyStorefrontBrand),
            _ => None,
        }
    }

    /// Retained scope-kind mapping: only `holding_brand` reads the holding
    /// scope; the other targets read the branch scope (the storefront row is
    /// additionally gated on the target kind itself).
    pub fn scope_kind(self) -> &'static str {
        match self {
            Self::HoldingBrand => "holding",
            Self::BranchBrand | Self::CompanyStorefrontBrand => "branch",
        }
    }

    /// Transport spelling, byte-identical to the retained predicate.
    pub fn as_target(self) -> &'static str {
        match self {
            Self::HoldingBrand => "holding_brand",
            Self::BranchBrand => "branch_brand",
            Self::CompanyStorefrontBrand => "company_storefront_brand",
        }
    }
}

/// Validated brand attachment-source read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandAttachmentRead {
    pub kind: BrandAttachmentKind,
    pub scope_id: Option<String>,
    pub company_id: String,
}

pub fn validate_read(
    kind: &str,
    scope_id: Option<&str>,
    company_id: &str,
) -> Result<BrandAttachmentRead, BusinessError> {
    let kind = BrandAttachmentKind::parse(kind).ok_or_else(|| {
        BusinessError::new(
            erpx_server_shared::public::FailureKind::InvalidInput,
            "BRAND_ATTACHMENT_TARGET_UNKNOWN",
            "Unknown Brand attachment read target.",
        )
    })?;
    if company_id.is_empty() {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::InvalidInput,
            "BRAND_ATTACHMENT_COMPANY_REQUIRED",
            "A Company scope is required.",
        ));
    }
    Ok(BrandAttachmentRead {
        kind,
        scope_id: scope_id.map(str::to_owned),
        company_id: company_id.to_owned(),
    })
}
