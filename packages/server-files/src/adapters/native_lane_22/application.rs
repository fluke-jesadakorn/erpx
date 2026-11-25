//! Query port for the native Files work-evidence read. No SQL lives here: the
//! owner postgres adapter owns every statement and the projection.

use std::{future::Future, pin::Pin};

use erpx_server_shared::public::BusinessError;

use super::domain::{WorkEvidenceKind, WorkEvidenceResponse};

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
