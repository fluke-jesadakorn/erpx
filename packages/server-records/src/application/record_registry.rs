//! Records registry list/search/page use cases.
//!
//! The composition root supplies the current serializable read-only unit, the
//! owner SQL adapter and the cross-owner source admission guard. This layer
//! validates current scope/authority, scans a bounded number of owner rows,
//! filters them through that guard and builds the page. It never opens a
//! transaction or reads HTTP.
use crate::domain::record_context::{
    RecordContextActor, RecordRow, RecordView, context_changed, module_forbidden,
};
use crate::domain::record_registry::{
    MAX_FETCH, MAX_LIST_ROWS, MAX_SCAN_ROWS, PAGE_SIZE, RegistryCursor, RegistryPage,
    RegistryQuery, decode_cursor, encode_cursor, registry_scope, registry_source_invalid,
    request_invalid, validate_registry_row,
};
use erpx_server_shared::public::BusinessError;
use std::future::Future;
use std::pin::Pin;

pub type RecordRegistryFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// SQL access to Records-owned `workspace_records` rows. The caller must
/// already be inside the serializable read-only Holding unit.
pub trait RecordRegistryRepository {
    type Error: From<BusinessError>;

    /// Recheck the same current assignment/role/version behind the actor.
    fn current_authority<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        now: i64,
    ) -> RecordRegistryFuture<'a, bool, Self::Error>;

    /// One bounded, deterministic read. `after_key`/`after_id` are an exclusive
    /// `(updated_at, id)` cursor; `descending` selects the scan direction.
    #[allow(clippy::too_many_arguments)]
    fn read_rows<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        section_id: &'a str,
        q: Option<&'a str>,
        after_key: Option<i64>,
        after_id: Option<&'a str>,
        descending: bool,
        limit: usize,
    ) -> RecordRegistryFuture<'a, Vec<RecordRow>, Self::Error>;
}

/// Cross-owner source admission supplied by the composition root.
///
/// `authorize_module` must use the owning catalogue/public ports to reject an
/// unknown module (typically `NOT_FOUND`), apply any retained module-level read
/// gate (for example Finance history or the expense workspace) and must not
/// grant access from a request label. `row_visible` must reproduce the retained
/// per-owner source read scopes with owning public ports (never SQL from this
/// crate). A `false` result filters one row; a transport or owner failure must
/// return an error so the whole read fails closed rather than silently
/// truncating the registry.
pub trait RecordRegistryGuard {
    type Error: From<BusinessError>;

    fn authorize_module<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
    ) -> RecordRegistryFuture<'a, (), Self::Error>;

    fn row_visible<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        row: &'a RecordRow,
    ) -> RecordRegistryFuture<'a, bool, Self::Error>;
}

async fn authorize<R, G>(
    repository: &mut R,
    guard: &mut G,
    actor: &RecordContextActor,
    query: &RegistryQuery,
    now: i64,
) -> Result<(), R::Error>
where
    R: RecordRegistryRepository,
    G: RecordRegistryGuard,
    R::Error: From<G::Error>,
{
    actor.require_read(now)?;
    if !actor.permits_module(query.module_id()) {
        return Err(module_forbidden().into());
    }
    if !repository.current_authority(actor, now).await? {
        return Err(context_changed().into());
    }
    guard
        .authorize_module(actor, query.module_id())
        .await
        .map_err(Into::into)
}

pub async fn record_registry_page<R, G>(
    repository: &mut R,
    guard: &mut G,
    actor: &RecordContextActor,
    query: &RegistryQuery,
    now: i64,
) -> Result<RegistryPage, R::Error>
where
    R: RecordRegistryRepository,
    G: RecordRegistryGuard,
    R::Error: From<G::Error>,
{
    authorize(repository, guard, actor, query, now).await?;
    let scope = registry_scope(actor, query);
    let cursor = query
        .cursor()
        .map(|raw| decode_cursor(raw, &scope))
        .transpose()?;
    let has_cursor = cursor.is_some();
    let reverse = cursor.as_ref().is_some_and(|value| value.reverse);
    let descending = !reverse;
    let mut after = cursor.map(|value| (value.key, value.id));
    let mut items: Vec<RecordView> = Vec::with_capacity(PAGE_SIZE + 1);
    let mut overflow = false;
    let mut scanned = 0usize;
    loop {
        let remaining = PAGE_SIZE + 1 - items.len();
        if remaining == 0 {
            overflow = true;
            break;
        }
        let limit = remaining.min(MAX_FETCH);
        let rows = repository
            .read_rows(
                actor,
                query.module_id(),
                query.section_id(),
                query.q(),
                after.as_ref().map(|(key, _)| *key),
                after.as_ref().map(|(_, id)| id.as_str()),
                descending,
                limit,
            )
            .await?;
        if rows.is_empty() {
            break;
        }
        let exhausted = rows.len() < limit;
        for row in rows {
            scanned += 1;
            if scanned > MAX_SCAN_ROWS {
                return Err(registry_source_invalid().into());
            }
            let view = validate_registry_row(&row, actor, query.module_id(), query.section_id())?;
            after = Some((row.updated_at, row.id.clone()));
            if guard.row_visible(actor, &row).await? {
                items.push(view);
                if items.len() > PAGE_SIZE {
                    overflow = true;
                    break;
                }
            }
        }
        if overflow || exhausted {
            break;
        }
    }
    if items.len() > PAGE_SIZE {
        items.pop();
        overflow = true;
    }
    if reverse {
        items.reverse();
    }
    let previous_cursor = items
        .first()
        .filter(|_| if reverse { overflow } else { has_cursor })
        .map(|item| {
            encode_cursor(&RegistryCursor {
                key: item.updated_at,
                id: item.id.clone(),
                reverse: true,
                scope: scope.clone(),
            })
        });
    let next_cursor = items
        .last()
        .filter(|_| if reverse { has_cursor } else { overflow })
        .map(|item| {
            encode_cursor(&RegistryCursor {
                key: item.updated_at,
                id: item.id.clone(),
                reverse: false,
                scope,
            })
        });
    let has_more = next_cursor.is_some();
    Ok(RegistryPage {
        items,
        next_cursor,
        previous_cursor,
        has_more,
    })
}

pub async fn record_registry_list<R, G>(
    repository: &mut R,
    guard: &mut G,
    actor: &RecordContextActor,
    query: &RegistryQuery,
    now: i64,
) -> Result<Vec<RecordView>, R::Error>
where
    R: RecordRegistryRepository,
    G: RecordRegistryGuard,
    R::Error: From<G::Error>,
{
    if query.q().is_some() || query.cursor().is_some() {
        return Err(request_invalid().into());
    }
    authorize(repository, guard, actor, query, now).await?;
    let mut items: Vec<RecordView> = Vec::new();
    let mut after: Option<(i64, String)> = None;
    let mut scanned = 0usize;
    loop {
        let rows = repository
            .read_rows(
                actor,
                query.module_id(),
                query.section_id(),
                None,
                after.as_ref().map(|(key, _)| *key),
                after.as_ref().map(|(_, id)| id.as_str()),
                true,
                MAX_FETCH,
            )
            .await?;
        if rows.is_empty() {
            break;
        }
        let exhausted = rows.len() < MAX_FETCH;
        for row in rows {
            scanned += 1;
            if scanned > MAX_SCAN_ROWS {
                return Err(registry_source_invalid().into());
            }
            let view = validate_registry_row(&row, actor, query.module_id(), query.section_id())?;
            after = Some((row.updated_at, row.id.clone()));
            if guard.row_visible(actor, &row).await? {
                items.push(view);
                if items.len() > MAX_LIST_ROWS {
                    return Err(registry_source_invalid().into());
                }
            }
        }
        if exhausted {
            break;
        }
    }
    Ok(items)
}
