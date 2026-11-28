//! PostgreSQL adapter for the native Records drawer-detail read.
//!
//! The scoped row, current-authority predicate and link admission are shared
//! with the record context adapter; this adapter adds only the bounded
//! transition-history page. The caller's serializable read-only Holding
//! transaction is required and no commit is performed.
use crate::adapters::record_context::PgRecordContext;
use crate::application::record_details::RecordDetailsRepository;
use crate::domain::record_context::HistoryRow;

impl RecordDetailsRepository for PgRecordContext<'_> {
    fn history_page<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
        offset: i64,
        limit: usize,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<HistoryRow>, Self::Error>
    {
        Box::pin(async move { self.history_offset(module_id, id, offset, limit).await })
    }
}
