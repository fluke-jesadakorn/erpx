//! Query port for the native Files work-evidence read. No SQL lives here: the
//! owner postgres adapter owns every statement and the projection.

use std::{future::Future, pin::Pin};

use erpx_platform_shared_server::public::BusinessError;

use super::domain::{
    PersonnelEvidenceKind, PersonnelEvidenceResponse, WorkEvidenceKind, WorkEvidenceResponse,
};

pub type WorkEvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The reader owns the
/// SQL, the Company scope and the complete response; the router only transports
/// the result. A missing or failing native reader has no TypeScript fallback.
pub trait WorkEvidenceRead: Send + Sync {
    fn read<'a>(
        &'a self,
        company: &'a str,
        record_id: &'a str,
        kind: WorkEvidenceKind,
        read_only: bool,
    ) -> WorkEvidenceFuture<'a, WorkEvidenceResponse>;
}

/// Bounded identities for one personnel-evidence read. The router fills only
/// the fields the selected kind needs; the rest are empty strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersonnelEvidenceRequest<'a> {
    pub company: &'a str,
    pub assessment: &'a str,
    pub source: &'a str,
    pub record: &'a str,
}

pub type PersonnelEvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner port for the Files-owned personnel-evidence reads. No SQL lives here.
pub trait PersonnelEvidenceRead: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PersonnelEvidenceRequest<'a>,
        kind: PersonnelEvidenceKind,
    ) -> PersonnelEvidenceFuture<'a, PersonnelEvidenceResponse>;
}
