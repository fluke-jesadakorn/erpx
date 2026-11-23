//! Native binding seam for the Asset attachment descriptor.
//!
//! The Assets owner exposes `observe_asset_attachment_descriptor`, which reads
//! the exact Company-scoped `operational_assets` descriptor under the shared
//! owner-unit lock and proves source freshness. Files must not own SQL for a
//! sibling owner. This module defines the consumer port and the binding that
//! composes an owner-native descriptor with the Files `asset` read policy
//! (`asset_descriptor_read_scope`). No request flag or selected-work-context
//! row is accepted as the descriptor here.
//!
//! Mount (composition root, lead-owned): implement [`AssetDescriptorPort`] by
//! wrapping `erpx_server_assets::public::asset_purchase_source::
//! observe_asset_attachment_descriptor` and its returned
//! `CurrentAssetAttachmentDescriptor`, then call
//! [`bind_asset_attachment_target_scope`] from an
//! [`crate::public::attachment_read::AttachmentTargetAuthority`] for
//! `target_type == "asset"`; delegate all other target types to the existing
//! current-authority implementation. The IAM facts must be the already retained
//! current Identity facts, never request input.

use crate::domain::attachment_read::{
    AssetReadDescriptor, AttachmentTargetScope, asset_descriptor_read_scope,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};

/// Owner-native Asset descriptor data copied out of the Assets owner so Files
/// carries no dependency on the Assets crate. `version` is the exact PostgreSQL
/// bigint; the Files policy narrows it to the JS Number domain only for parity
/// decisions, while the returned read scope keeps the exact bigint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetDescriptorFacts {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

/// An owner-observed descriptor plus its post-read freshness proof. The
/// composition implementation wraps the Assets `CurrentAssetAttachmentDescriptor`
/// and rechecks it after the Files reads.
pub trait AssetDescriptorEvidence<C>: Send {
    fn data(&self) -> Option<&AssetDescriptorFacts>;
    fn require_current<'a>(
        &'a self,
        c: &'a mut C,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// The owner port. Implemented once at the composition root with the Assets
/// native descriptor reader; the connection is generic so the binding is
/// unit-testable without a live database.
pub trait AssetDescriptorPort<C>: Send + Sync {
    fn observe<'a>(
        &'a self,
        c: &'a mut C,
        company: &'a CompanyId,
        target_id: &'a str,
    ) -> PgWorkFuture<'a, Box<dyn AssetDescriptorEvidence<C>>, BusinessError>;
}

/// Already retained current Identity facts for the actor. These are never read
/// from the request; callers pass facts captured from the current work context.
#[derive(Clone, Copy, Debug)]
pub struct AssetReadAccess<'a> {
    pub has_current_records_read: bool,
    pub has_current_assets_module: bool,
    pub actor_branch: Option<&'a str>,
    pub actor_department: Option<&'a str>,
}

fn missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The file owner was not found in this Company.",
    )
}

/// Bind an owner-native Asset descriptor to the Files `asset` read authority.
/// The descriptor is observed in the caller's unit of work, filtered by the
/// Files Asset policy, and re-proved current before its scope is returned. A
/// denial, absence, scope mismatch or source change aborts the whole read.
pub async fn bind_asset_attachment_target_scope<C, P>(
    c: &mut C,
    port: &P,
    access: AssetReadAccess<'_>,
    company: &CompanyId,
    target_id: &str,
) -> Result<AttachmentTargetScope, PgTransactionError<BusinessError>>
where
    P: AssetDescriptorPort<C> + ?Sized,
{
    let evidence = port.observe(c, company, target_id).await?;
    let (descriptor, version) = match evidence.data() {
        Some(d) => (
            Some(AssetReadDescriptor {
                company_id: &d.company_id,
                branch_id: d.branch_id.as_deref(),
                department_id: d.department_id.as_deref(),
                version: d.version,
            }),
            Some(d.version),
        ),
        None => (None, None),
    };
    let scope = asset_descriptor_read_scope(
        access.has_current_records_read,
        access.has_current_assets_module,
        access.actor_branch,
        access.actor_department,
        descriptor,
    )
    .map_err(PgTransactionError::Business)?;
    let branch_id = scope.branch_id.map(str::to_owned);
    let department_id = scope.department_id.map(str::to_owned);
    // The Files reads and the owner descriptor must observe one current source.
    evidence.require_current(c).await?;
    let version = version.ok_or_else(|| PgTransactionError::Business(missing()))?;
    AttachmentTargetScope::new(company.clone(), branch_id, department_id, version)
        .map_err(PgTransactionError::Business)
}
