//! Records context read use case.
//!
//! The composition root supplies the current serializable unit, the owner SQL
//! adapter and the cross-owner record admission guard. This layer validates
//! current scope/authority, filters link targets through that guard, and builds
//! the bounded document. It never opens a transaction or reads HTTP.
use crate::domain::record_context::{
    BacklinkEdge, BacklinkView, HistoryRow, LinkTarget, MentionRow, RecordContextActor,
    RecordContextDocument, RecordRow, ReferenceEdge, ReferenceView, SuggestionRow, context_changed,
    module_forbidden, record_not_found, validate_backlinks, validate_document, validate_history,
    validate_link_target, validate_mentions, validate_record, validate_reference_edges,
    validate_request, validate_suggestions,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type RecordContextFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// SQL access to Records-owned `workspace_records` and its link/timeline tables.
/// The caller must already be inside the serializable Holding unit.
pub trait RecordContextRepository {
    type Error: From<BusinessError>;

    /// Recheck the same current assignment/role/version behind the actor.
    fn current_authority<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        now: i64,
    ) -> RecordContextFuture<'a, bool, Self::Error>;
    fn record<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Option<RecordRow>, Self::Error>;
    fn references<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Vec<ReferenceEdge>, Self::Error>;
    fn backlinks<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Vec<BacklinkEdge>, Self::Error>;
    fn mentions<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Vec<MentionRow>, Self::Error>;
    fn suggestions<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Vec<SuggestionRow>, Self::Error>;
    fn history<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Vec<HistoryRow>, Self::Error>;
    /// Scoped title/version for one already-admitted link target.
    fn link_target<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, Option<LinkTarget>, Self::Error>;
}

/// Cross-owner record admission supplied by the composition root.
///
/// A `false` result hides one link target, preserving the retained drawer's
/// "Unavailable record" reference and hidden-backlink behavior. A transport or
/// owner failure must return an error so the whole read fails closed rather than
/// silently hiding a source outage. Implementations must use the owning public
/// ports; this crate never issues cross-owner SQL.
pub trait RecordContextGuard {
    type Error: From<BusinessError>;

    fn authorize_source<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, (), Self::Error>;
    fn target_visible<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> RecordContextFuture<'a, bool, Self::Error>;
}

pub async fn record_context<R: RecordContextRepository, G: RecordContextGuard>(
    repository: &mut R,
    guard: &mut G,
    actor: &RecordContextActor,
    module_id: &str,
    id: &str,
    now: i64,
) -> Result<serde_json::Value, R::Error>
where
    R::Error: From<G::Error>,
{
    actor.require_read(now)?;
    validate_request(module_id, id)?;
    if !actor.permits_module(module_id) {
        return Err(module_forbidden().into());
    }
    if !repository.current_authority(actor, now).await? {
        return Err(context_changed().into());
    }
    guard.authorize_source(actor, module_id, id).await?;
    let row = repository
        .record(actor, module_id, id)
        .await?
        .ok_or_else(record_not_found)?;
    let record = validate_record(&row, actor, module_id, id)?;

    let references = validate_reference_edges(repository.references(module_id, id).await?)?;
    let mut visible_references = Vec::with_capacity(references.len());
    for edge in references {
        let visible = guard
            .target_visible(actor, &edge.target_module, &edge.target_id)
            .await?;
        let target = if visible {
            repository
                .link_target(actor, &edge.target_module, &edge.target_id)
                .await?
                .map(validate_link_target)
                .transpose()?
        } else {
            None
        };
        let (title, version, access) = match target {
            Some(target) => (target.title, target.version, "available"),
            None => ("Unavailable record".to_owned(), 0, "denied"),
        };
        visible_references.push(ReferenceView {
            target_module: edge.target_module,
            target_id: edge.target_id,
            kind: edge.kind,
            ordinal: edge.ordinal,
            title,
            version,
            access,
        });
    }

    let backlinks = validate_backlinks(repository.backlinks(module_id, id).await?)?;
    let mut visible_backlinks = Vec::new();
    for edge in backlinks {
        if !guard
            .target_visible(actor, &edge.host_module, &edge.host_id)
            .await?
        {
            continue;
        }
        let Some(target) = repository
            .link_target(actor, &edge.host_module, &edge.host_id)
            .await?
            .map(validate_link_target)
            .transpose()?
        else {
            continue;
        };
        visible_backlinks.push(BacklinkView {
            host_module: edge.host_module,
            host_id: edge.host_id,
            kind: edge.kind,
            title: target.title,
            version: target.version,
        });
    }

    let mentions = validate_mentions(repository.mentions(module_id, id).await?)?;
    let suggestions = validate_suggestions(repository.suggestions(module_id, id).await?)?;
    let history = validate_history(repository.history(module_id, id).await?)?;

    validate_document(
        RecordContextDocument {
            record,
            references: visible_references,
            backlinks: visible_backlinks,
            mentions,
            suggestions,
            history,
        },
        module_id,
        id,
    )
    .map_err(Into::into)
}
