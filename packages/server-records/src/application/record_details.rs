//! Records drawer-detail (supplement) read use case.
//!
//! The composition root supplies the current serializable read-only unit, the
//! owner SQL adapter and the cross-owner link admission guard. This layer
//! rechecks the same current authority as the record context read, bounds the
//! timeline page and materializes only visible link targets. It never opens a
//! transaction or reads HTTP.
use crate::application::record_context::{
    RecordContextFuture, RecordContextGuard, RecordContextRepository,
};
use crate::domain::record_context::{
    HistoryRow, context_changed, record_not_found, validate_record, validate_reference_edges,
    validate_request,
};
use crate::domain::record_details::{
    RecordDetailsQuery, details_wire, history_page, require_current,
};

/// Owner SQL access for the bounded detail read. It extends the context
/// repository so the identity/authority predicate and scoped row read stay
/// singular; only the paginated timeline is new.
pub trait RecordDetailsRepository: RecordContextRepository {
    /// One bounded, deterministic transition-history slice ordered newest first.
    fn history_page<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
        offset: i64,
        limit: usize,
    ) -> RecordContextFuture<'a, Vec<HistoryRow>, Self::Error>;
}

pub async fn record_details<R: RecordDetailsRepository, G: RecordContextGuard>(
    repository: &mut R,
    guard: &mut G,
    actor: &crate::domain::record_context::RecordContextActor,
    query: &RecordDetailsQuery,
    now: i64,
) -> Result<serde_json::Value, R::Error>
where
    R::Error: From<G::Error>,
{
    require_current(actor, now, query.module_id())?;
    validate_request(query.module_id(), query.record_id())?;
    if !repository.current_authority(actor, now).await? {
        return Err(context_changed().into());
    }
    guard
        .authorize_source(actor, query.module_id(), query.record_id())
        .await?;
    let row = repository
        .record(actor, query.module_id(), query.record_id())
        .await?
        .ok_or_else(record_not_found)?;
    let record = validate_record(&row, actor, query.module_id(), query.record_id())?;

    let references = validate_reference_edges(
        repository
            .references(query.module_id(), query.record_id())
            .await?,
    )?;
    let mut related = Vec::with_capacity(references.len());
    for edge in references {
        if guard
            .target_visible(actor, &edge.target_module, &edge.target_id)
            .await?
        {
            related.push((edge.target_module, edge.target_id));
        }
    }

    let rows = repository
        .history_page(
            query.module_id(),
            query.record_id(),
            query.offset(),
            query.fetch_limit(),
        )
        .await?;
    let (history, has_next) = history_page(rows)?;

    Ok(details_wire(
        record,
        related,
        history,
        query.page(),
        has_next,
    ))
}
