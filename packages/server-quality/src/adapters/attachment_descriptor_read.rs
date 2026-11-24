//! Owner-local, read-only Quality attachment descriptor reads.
//!
//! This adapter consumes the existing Quality `attachment_descriptor` owner
//! port to expose the Company-scoped descriptor row for the Quality attachment
//! target types admitted by the retained TypeScript descriptor read
//! (`policyQualityFacts.readDescriptor` / `qualityAttachmentScopeReadSql`).
//! It grants no attachment authority, performs no composition and changes no
//! Files consumer; it is a native read port for a later owner-only cutover.
use crate::adapters::attachment_descriptor::{
    CurrentQualityAttachmentDescriptor, inspect_quality_attachment_descriptor,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// The exact descriptor row shape retained by the Quality TS descriptor read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityAttachmentDescriptorRow {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

/// Closed set of Quality attachment target types with a descriptor read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QualityAttachmentDescriptorTarget {
    QualityHold,
    InventoryQualityHold,
}

impl QualityAttachmentDescriptorTarget {
    /// Mirror the retained TS `descriptorTargets` set: unknown types have no
    /// descriptor and return `None`, exactly as the TS read does.
    pub fn from_target_type(value: &str) -> Option<Self> {
        match value {
            "quality_hold" => Some(Self::QualityHold),
            "inventory_quality_hold" => Some(Self::InventoryQualityHold),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::QualityHold => "quality_hold",
            Self::InventoryQualityHold => "inventory_quality_hold",
        }
    }
}

/// Read the descriptor for a Quality attachment target through the existing
/// owner port. The caller owns the same bounded, source-fenced read unit that
/// the owner port requires; this adapter adds no lock authority of its own.
pub async fn read_quality_attachment_descriptor(
    c: &mut PgConnection,
    company: &CompanyId,
    target_type: &str,
    target_id: &str,
) -> Result<Option<QualityAttachmentDescriptorRow>, Error> {
    if QualityAttachmentDescriptorTarget::from_target_type(target_type).is_none() {
        return Ok(None);
    }
    let current: CurrentQualityAttachmentDescriptor =
        inspect_quality_attachment_descriptor(c, company, target_id).await?;
    Ok(current.value().map(|value| QualityAttachmentDescriptorRow {
        company_id: value.company_id.clone(),
        branch_id: value.branch_id.clone(),
        department_id: value.department_id.clone(),
        version: value.version,
    }))
}

#[cfg(test)]
mod tests {
    use super::QualityAttachmentDescriptorTarget as T;

    #[test]
    fn descriptor_targets_match_retained_typescript_set() {
        assert_eq!(T::from_target_type("quality_hold"), Some(T::QualityHold));
        assert_eq!(
            T::from_target_type("inventory_quality_hold"),
            Some(T::InventoryQualityHold)
        );
        assert_eq!(T::from_target_type("inventory_quality_spec"), None);
        assert_eq!(T::from_target_type("quality_attachment"), None);
        assert_eq!(T::from_target_type(""), None);
        assert_eq!(T::QualityHold.as_str(), "quality_hold");
        assert_eq!(T::InventoryQualityHold.as_str(), "inventory_quality_hold");
    }
}
