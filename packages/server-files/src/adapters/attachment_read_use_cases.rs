//! Applications for the bounded native Files attachment metadata read.
//!
//! The use cases open no transaction and read no HTTP request. They authorize
//! the owning record through the composition port, preserve draft visibility,
//! write the Files-owned status access audit, and reject a changed projection
//! before returning owner data.
use crate::domain::attachment_read::{
    AttachmentLinkStatusView, AttachmentListFilter, AttachmentListFilter as Filter,
    AttachmentReader, AttachmentSetFacts, AttachmentSetView, AttachmentStatusFacts,
    AttachmentTarget, AttachmentTargetScope, MAX_SETS, require_current, validate_set,
    validate_status,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

pub type Error = PgTransactionError<BusinessError>;

/// The owning-record authority, implemented at the composition root from the
/// current owner ports. It rechecks IAM/current selection and returns the
/// owning record's exact Company/scope/version; it never reads a request.
pub trait AttachmentTargetAuthority: Send + Sync {
    fn authorize_read<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        target: &'a AttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, AttachmentTargetScope, BusinessError>;
}

/// Composition-owned owner-unit port. All SQL remains in the Files adapter;
/// the record authority remains with the owning business owner.
pub trait AttachmentReadPorts: Send + Sync {
    fn authorize_target<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        target: &'a AttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, AttachmentTargetScope, BusinessError>;

    fn list_sets<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        filter: &'a AttachmentListFilter,
        read_only: bool,
    ) -> PgWorkFuture<'a, Vec<AttachmentSetFacts>, BusinessError>;

    fn read_status<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        link_id: &'a str,
        read_only: bool,
    ) -> PgWorkFuture<'a, Option<AttachmentStatusFacts>, BusinessError>;

    /// Files-owned allowed status evidence. Called only after authority passes.
    fn record_status_access<'a>(
        &'a self,
        c: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        link_id: &'a str,
        attachment_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// List the exact Company-scoped set ledger for one owning record. Draft sets
/// are visible to their creator unless Leave review or rollback read-only mode
/// applies. An owner denial aborts the complete list; rows are never skipped.
pub async fn list_attachment_sets<P: AttachmentReadPorts>(
    c: &mut PgConnection,
    ports: &P,
    reader: &AttachmentReader,
    filter: &Filter,
    read_only: bool,
    now: i64,
) -> Result<Vec<AttachmentSetView>, Error> {
    require_current(reader, now)?;
    let facts = ports.list_sets(c, reader, filter, read_only).await?;
    // The SQL limit is MAX_SETS + 1 so an oversized record fails closed rather
    // than silently truncating owner evidence.
    if facts.len() > MAX_SETS {
        return Err(erpx_server_shared::public::BusinessError::invalid(
            "ATTACHMENT_LIST_LIMIT",
            "This record has too many file sets to show at once. Narrow the file purpose.",
        )
        .into());
    }
    let mut approved = std::collections::BTreeSet::new();
    let mut views = Vec::with_capacity(facts.len());
    for fact in facts {
        let key = (
            fact.target_type.clone(),
            fact.target_id.clone(),
            fact.purpose_key.clone(),
            fact.component_kind.clone(),
        );
        if approved.insert(key) {
            let target = AttachmentTarget {
                target_type: &fact.target_type,
                target_id: &fact.target_id,
                purpose_key: &fact.purpose_key,
                component_kind: &fact.component_kind,
            };
            let scope = ports.authorize_target(c, reader, &target).await?;
            if scope.company_id != *reader.company_id() {
                return Err(erpx_server_shared::public::BusinessError::new(
                    erpx_server_shared::public::FailureKind::Forbidden,
                    "ATTACHMENT_CROSS_COMPANY",
                    "Files cannot cross Companies.",
                )
                .into());
            }
        }
        views.push(validate_set(reader, filter, fact, now)?);
    }
    require_current(reader, now)?;
    Ok(views)
}

/// Read one exact link status. The same-Company Files row and the owning-record
/// authority are both required. A successful read records Files-owned access
/// evidence and then re-reads the projection; any change rejects the response.
pub async fn attachment_link_status<P: AttachmentReadPorts>(
    c: &mut PgConnection,
    ports: &P,
    reader: &AttachmentReader,
    link_id: &str,
    read_only: bool,
    now: i64,
) -> Result<AttachmentLinkStatusView, Error> {
    require_current(reader, now)?;
    if link_id.is_empty()
        || link_id.len() > crate::domain::attachment_read::MAX_IDENTIFIER_BYTES
        || link_id.chars().any(char::is_control)
        || link_id.contains('/')
        || link_id.contains('\\')
    {
        return Err(erpx_server_shared::public::BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "ATTACHMENT_LINK_NOT_FOUND",
            "File link not found.",
        )
        .into());
    }
    let before = ports
        .read_status(c, reader, link_id, read_only)
        .await?
        .ok_or_else(|| {
            erpx_server_shared::public::BusinessError::new(
                erpx_server_shared::public::FailureKind::NotFound,
                "ATTACHMENT_LINK_NOT_FOUND",
                "File link not found.",
            )
        })?;
    if !crate::domain::attachment_read::may_read_status(reader, &before) {
        return Err(erpx_server_shared::public::BusinessError::new(
            erpx_server_shared::public::FailureKind::NotFound,
            "ATTACHMENT_LINK_NOT_FOUND",
            "File link not found.",
        )
        .into());
    }
    let target = AttachmentTarget {
        target_type: &before.target_type,
        target_id: &before.target_id,
        purpose_key: &before.purpose_key,
        component_kind: &before.component_kind,
    };
    let scope = ports.authorize_target(c, reader, &target).await?;
    if scope.company_id != *reader.company_id() {
        return Err(erpx_server_shared::public::BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "ATTACHMENT_CROSS_COMPANY",
            "Files cannot cross Companies.",
        )
        .into());
    }
    // The owner authority must be current after its own async read. A stale
    // selected context cannot retain the status projection it just observed.
    require_current(reader, now)?;
    if let Some(source_id) = before.source_id.as_deref() {
        ports
            .record_status_access(c, reader, link_id, source_id, now)
            .await?;
    }
    let after = ports
        .read_status(c, reader, link_id, read_only)
        .await?
        .ok_or_else(|| {
            erpx_server_shared::public::BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "ATTACHMENT_READ_CHANGED",
                "The file metadata changed while it was being read.",
            )
        })?;
    if after != before {
        return Err(erpx_server_shared::public::BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "ATTACHMENT_READ_CHANGED",
            "The file metadata changed while it was being read.",
        )
        .into());
    }
    validate_status(reader, after, &scope, now).map_err(Into::into)
}
