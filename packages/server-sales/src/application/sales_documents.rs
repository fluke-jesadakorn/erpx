//! Native Sales document list use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application enforces the current actor policy, validates
//! the cursor scope and every bounded owner row before presentation.
use crate::domain::sales_documents::{
    SalesDocumentCursor, SalesDocumentKind, SalesDocumentReadActor, SalesDocumentRow,
    SalesDocumentsPage, cursor_scope, decode_cursor, normalize_status, validate_page,
    validate_search,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type SalesDocumentReadFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait SalesDocumentReadRepository: Send {
    type Error: From<BusinessError> + Send;
    #[allow(clippy::too_many_arguments)]
    fn page<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        kind: SalesDocumentKind,
        search: &'a str,
        status: Option<&'a str>,
        cursor: Option<&'a SalesDocumentCursor>,
        limit: usize,
    ) -> SalesDocumentReadFuture<'a, Vec<SalesDocumentRow>, Self::Error>;
}

#[allow(clippy::too_many_arguments)]
pub async fn sales_documents<R: SalesDocumentReadRepository>(
    actor: &SalesDocumentReadActor,
    repository: &mut R,
    kind: SalesDocumentKind,
    search: &str,
    status: Option<&str>,
    cursor: Option<&str>,
    now: TimestampMillis,
) -> Result<SalesDocumentsPage, R::Error> {
    actor.require_documents(now)?;
    validate_search(search)?;
    let status = normalize_status(status)?;
    let scope = cursor_scope(actor, kind, search, status.as_deref())?;
    let cursor = cursor
        .map(|value| decode_cursor(value, &scope))
        .transpose()?;
    let source = repository
        .page(
            &actor.company_id,
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
            kind,
            search,
            status.as_deref(),
            cursor.as_ref(),
            crate::domain::sales_documents::PAGE_SIZE + 1,
        )
        .await?;
    validate_page(actor, kind, cursor.as_ref(), source, &scope).map_err(Into::into)
}
