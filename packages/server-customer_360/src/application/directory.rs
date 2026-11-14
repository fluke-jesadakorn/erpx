//! Native Customer 360 customer directory read use case.
use crate::domain::directory::{
    CustomerDirectoryCursor, CustomerDirectoryEntry, CustomerDirectoryPage, CustomerDirectoryQuery,
    CustomerDirectoryView, decode_directory_cursor, directory_invalid, directory_view,
    encode_directory_cursor,
};
use crate::domain::timeline::CustomerTimelineActor;
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub type CustomerDirectoryFuture<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait CustomerDirectoryRepository: Send {
    type Error: From<BusinessError> + Send;

    fn customers<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerDirectoryView,
    ) -> CustomerDirectoryFuture<'a, Vec<CustomerDirectoryEntry>, Self::Error>;

    /// Return at most `limit` entries strictly after the exclusive `(name,id)`
    /// cursor under the source database collation. The caller owns the
    /// serializable transaction and scope; read-only.
    fn customers_page<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerDirectoryView,
        cursor: Option<&'a CustomerDirectoryCursor>,
        limit: usize,
    ) -> CustomerDirectoryFuture<'a, Vec<CustomerDirectoryEntry>, Self::Error> {
        let _ = (cursor, limit);
        self.customers(actor, view)
    }
}

pub async fn customer_directory<R>(
    actor: &CustomerTimelineActor,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Vec<CustomerDirectoryEntry>, R::Error>
where
    R: CustomerDirectoryRepository,
{
    actor.require_directory_read(now)?;
    let view = directory_view(actor);
    let entries = repository.customers(actor, view).await?;
    for entry in &entries {
        entry.validate().map_err(R::Error::from)?;
    }
    if entries.len() > 201 {
        return Err(directory_invalid().into());
    }
    Ok(entries)
}

/// Bounded keyset page over the same Company/branch/department and source
/// gates as the first page. Pagination is `(name,id)` ascending under the
/// source database collation; the opaque cursor is bound to the current scope
/// and view, so it cannot be replayed in another workspace or role.
pub async fn customer_directory_page<R>(
    actor: &CustomerTimelineActor,
    query: &CustomerDirectoryQuery,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CustomerDirectoryPage, R::Error>
where
    R: CustomerDirectoryRepository,
{
    actor.require_directory_read(now)?;
    query.validate()?;
    let view = directory_view(actor);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_directory_cursor(value, actor, view))
        .transpose()?;
    let limit = query.normalized_limit()?;
    let mut entries = repository
        .customers_page(actor, view, cursor.as_ref(), limit + 1)
        .await?;
    let mut seen = BTreeSet::new();
    for entry in &entries {
        entry.validate().map_err(R::Error::from)?;
        if !seen.insert(entry.id.clone()) {
            return Err(directory_invalid().into());
        }
    }
    let has_more = entries.len() > limit;
    if has_more {
        entries.truncate(limit);
    }
    if has_more && entries.is_empty() {
        return Err(directory_invalid().into());
    }
    let next_cursor = if has_more {
        entries
            .last()
            .map(|entry| {
                encode_directory_cursor(
                    actor,
                    view,
                    &CustomerDirectoryCursor {
                        name: entry.name.clone(),
                        id: entry.id.clone(),
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(CustomerDirectoryPage {
        items: entries,
        has_more,
        next_cursor,
    })
}
