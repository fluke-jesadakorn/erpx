//! Citation-specific Legal purchase/release rules. Observations are data, not
//! grants; the composition must obtain and revalidate actual owner witnesses.
use super::record_policy::{
    LegalCommercialMode, LegalPurchasePurpose, LegalRecordActor, legal_kind_capability,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Resolves configuration only; it does not grant a Legal purchase or release.
/// The caller supplies the trusted runtime environment, never a request claim.
/// As in TypeScript, disabled and invalid modes do not consult the runtime.
pub fn resolve_legal_citation_mode<'a>(
    raw: Option<&str>,
    runtime_environment: impl FnOnce() -> Result<&'a str, BusinessError>,
) -> Result<LegalCommercialMode, BusinessError> {
    let mode = match raw.unwrap_or("disabled") {
        "disabled" => return Ok(LegalCommercialMode::Disabled),
        "shadow" => LegalCommercialMode::Shadow,
        "enforced" => LegalCommercialMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if runtime_environment()? != "local" {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

/// `Skip` means disabled mode must not call Commercial or release sources.
/// Active modes need all five history controls; shadow additionally needs the
/// underlying decisions for the retained Commercial telemetry, even though it
/// must not deny the read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegalCitationPurchasePlan {
    Skip,
    Observe {
        capabilities: [&'static str; 5],
        purposes: [LegalPurchasePurpose; 3],
        kind_capability: &'static str,
    },
}
pub fn citation_purchase_plan(
    mode: LegalCommercialMode,
    kind: &str,
) -> Result<LegalCitationPurchasePlan, BusinessError> {
    if mode == LegalCommercialMode::Disabled {
        return Ok(LegalCitationPurchasePlan::Skip);
    }
    Ok(LegalCitationPurchasePlan::Observe {
        capabilities: [
            "legal.contracts",
            "legal.documents",
            "legal.matters",
            "legal.obligations",
            "legal.access",
        ],
        purposes: [
            LegalPurchasePurpose::NewWork,
            LegalPurchasePurpose::SettleExisting,
            LegalPurchasePurpose::History,
        ],
        kind_capability: legal_kind_capability(kind)?,
    })
}

/// Values must come from the corresponding same-unit Commercial and release
/// owner witnesses. This value alone cannot establish authority or freshness.
#[derive(Clone, Copy, Debug)]
pub struct LegalCitationPurchaseObservations {
    pub kind_history: bool,
    pub documents_history: bool,
    /// Required only in enforced mode; shadow does not query release.
    pub scheduled_release: Option<bool>,
}
fn observations(
    mode: LegalCommercialMode,
    value: Option<LegalCitationPurchaseObservations>,
) -> Result<Option<LegalCitationPurchaseObservations>, BusinessError> {
    if mode == LegalCommercialMode::Disabled {
        return Ok(None);
    }
    value.map(Some).ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "LEGAL_CITATION_PURCHASE_SOURCE_REQUIRED",
            "Current Legal purchase and release sources are required.",
        )
    })
}
fn preparation(actor: &LegalRecordActor, kind: &str) -> bool {
    actor.branch_id.is_none()
        && actor.department_id.is_none()
        && actor.permissions.contains("legal.manage")
        && matches!(kind, "rule" | "template")
}
fn package_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PACKAGE_REQUIRED",
        "This Legal source requires its purchased capability or retained history.",
    )
}
fn release_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FEATURE_COMING_SOON",
        "Legal is unavailable for this Company.",
    )
}
fn law_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "LEGAL_CITATION_UNAVAILABLE",
        "The cited law source is unavailable, unapproved, conflicting or not applicable on this date.",
    )
}
fn released(value: LegalCitationPurchaseObservations) -> Result<bool, BusinessError> {
    value.scheduled_release.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "LEGAL_CITATION_RELEASE_SOURCE_REQUIRED",
            "Current Legal release source is required.",
        )
    })
}

/// First document gate models `access`: execute before checking the document's
/// disposed/frozen state. The source must already be visible and actor current.
pub fn citation_document_kind_gate(
    mode: LegalCommercialMode,
    actor: &LegalRecordActor,
    kind: &str,
    value: Option<LegalCitationPurchaseObservations>,
) -> Result<(), BusinessError> {
    let Some(value) = observations(mode, value)? else {
        return Ok(());
    };
    if mode == LegalCommercialMode::Enforced {
        if !value.kind_history {
            return Err(package_required());
        }
        if !released(value)? && !preparation(actor, kind) {
            return Err(release_unavailable());
        }
    }
    Ok(())
}
/// Run only after documentAccess has checked disposal and active destruction.
pub fn citation_document_documents_gate(
    mode: LegalCommercialMode,
    value: Option<LegalCitationPurchaseObservations>,
) -> Result<(), BusinessError> {
    let Some(value) = observations(mode, value)? else {
        return Ok(());
    };
    if mode == LegalCommercialMode::Enforced && !value.documents_history {
        return Err(package_required());
    }
    Ok(())
}
/// Law citation uses the read-scope SQL filter, concealing purchase/release
/// denial as an unavailable citation rather than document's 403 errors.
pub fn citation_law_gate(
    mode: LegalCommercialMode,
    actor: &LegalRecordActor,
    kind: &str,
    value: Option<LegalCitationPurchaseObservations>,
) -> Result<(), BusinessError> {
    let Some(value) = observations(mode, value)? else {
        return Ok(());
    };
    if mode == LegalCommercialMode::Enforced
        && (!value.kind_history
            || !value.documents_history
            || (!released(value)? && !preparation(actor, kind)))
    {
        return Err(law_unavailable());
    }
    Ok(())
}
