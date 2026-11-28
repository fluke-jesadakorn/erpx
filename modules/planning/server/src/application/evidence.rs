//! Native Planning evidence SEARCH use case.
//!
//! The port rechecks current read admission, validates the bounded query and
//! the returned page, and delegates the owner SQL to the repository on the
//! caller's already-open unit. No audit, purchase decision, cross-owner write
//! or retry authority lives here; composition owns those seams.
use crate::domain::evidence::{
    EvidenceActor, EvidenceEligibility, EvidenceQuery, EvidenceRow, validate_rows,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type EvidenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait EvidenceRepository: Send {
    fn search<'a>(
        &'a mut self,
        actor: &'a EvidenceActor,
        eligibility: &'a EvidenceEligibility,
        query: &'a EvidenceQuery,
    ) -> EvidenceFuture<'a, Vec<EvidenceRow>>;
}

/// Execute one bounded evidence search. Returns at most `limit + 1` owner rows;
/// the caller derives `hasMore`/`nextCursor` and owns the access audit on the
/// same unit, exactly as before the cutover.
pub async fn search_evidence<R: EvidenceRepository>(
    actor: &EvidenceActor,
    eligibility: &EvidenceEligibility,
    query: &EvidenceQuery,
    repository: &mut R,
) -> Result<Vec<EvidenceRow>, BusinessError> {
    actor.require_read()?;
    let rows = repository.search(actor, eligibility, query).await?;
    validate_rows(&query.metric, &rows)?;
    Ok(rows)
}
