//! Application seam for lane 1. The service owns request validation and
//! response assembly; it contains no SQL. SQL and pool access live only in
//! `postgres`, and transport only in `http`.

use super::domain::{
    CompanyScope, ExpenseProjection, JournalLineRow, PostingAccountRow, ProjectionKind, invalid,
    valid_identifier,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LaneReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition supplies the Postgres-backed adapter; the
/// service and router only handle bounded Company scopes and row shapes.
pub trait ExpenseProjectionReader: Send + Sync {
    fn journal_lines<'a>(
        &'a self,
        scope: &'a CompanyScope,
        journal_id: &'a str,
    ) -> LaneReadFuture<'a, Vec<JournalLineRow>>;

    fn posting_accounts<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> LaneReadFuture<'a, Vec<PostingAccountRow>>;
}

pub struct ExpenseProjectionService<R: ExpenseProjectionReader> {
    reader: R,
}

impl<R: ExpenseProjectionReader> ExpenseProjectionService<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Validate once here, then dispatch. A kind never selects SQL in this
    /// layer, and a journal id can never reach the accounts read.
    pub fn read<'a>(
        &'a self,
        scope: &'a CompanyScope,
        kind: ProjectionKind,
        journal_id: Option<&'a str>,
    ) -> LaneReadFuture<'a, ExpenseProjection> {
        Box::pin(async move {
            if !valid_identifier(&scope.company_id) {
                return Err(invalid(
                    "LANE_01_COMPANY_INVALID",
                    "Use a bounded registered Company.",
                ));
            }
            if kind.needs_journal() {
                let id = journal_id
                    .filter(|id| valid_identifier(id))
                    .ok_or_else(|| {
                        invalid(
                            "LANE_01_JOURNAL_INVALID",
                            "Use a bounded Finance journal id.",
                        )
                    })?;
                Ok(ExpenseProjection::JournalLines(
                    self.reader.journal_lines(scope, id).await?,
                ))
            } else {
                if journal_id.is_some() {
                    return Err(invalid(
                        "LANE_01_READ_REQUEST_INVALID",
                        "The accounts read takes no journal id.",
                    ));
                }
                Ok(ExpenseProjection::PostingAccounts(
                    self.reader.posting_accounts(scope).await?,
                ))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StubReader {
        calls: AtomicUsize,
        seen_company: std::sync::Mutex<Option<String>>,
        seen_journal: std::sync::Mutex<Option<String>>,
    }

    impl StubReader {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                seen_company: std::sync::Mutex::new(None),
                seen_journal: std::sync::Mutex::new(None),
            }
        }
    }

    impl ExpenseProjectionReader for StubReader {
        fn journal_lines<'a>(
            &'a self,
            scope: &'a CompanyScope,
            journal_id: &'a str,
        ) -> LaneReadFuture<'a, Vec<JournalLineRow>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.seen_company.lock().unwrap() = Some(scope.company_id.clone());
            *self.seen_journal.lock().unwrap() = Some(journal_id.to_owned());
            Box::pin(async {
                Ok(vec![JournalLineRow {
                    id: "line-1".into(),
                    code: "1210".into(),
                    name: "Receivable".into(),
                    debit_minor: 500,
                    credit_minor: 0,
                }])
            })
        }

        fn posting_accounts<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> LaneReadFuture<'a, Vec<PostingAccountRow>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(vec![PostingAccountRow {
                    id: "acc-1".into(),
                    code: "1210".into(),
                    name: "Receivable".into(),
                    status: "active".into(),
                    chart_id: "chart-1".into(),
                    currency: None,
                }])
            })
        }
    }

    fn scope(id: &str) -> CompanyScope {
        CompanyScope {
            company_id: id.into(),
        }
    }

    #[tokio::test]
    async fn service_dispatches_bounded_company_and_journal() {
        let reader = StubReader::new();
        let service = ExpenseProjectionService::new(reader);
        let result = service
            .read(
                &scope("company-1"),
                ProjectionKind::JournalLines,
                Some("journal-1"),
            )
            .await
            .expect("bounded read");
        assert!(
            matches!(result, ExpenseProjection::JournalLines(rows) if rows[0].debit_minor == 500)
        );
        assert_eq!(service.reader.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *service.reader.seen_company.lock().unwrap(),
            Some("company-1".into())
        );
        assert_eq!(
            *service.reader.seen_journal.lock().unwrap(),
            Some("journal-1".into())
        );
    }

    #[tokio::test]
    async fn service_rejects_bad_input_before_calling_reader() {
        let reader = StubReader::new();
        let service = ExpenseProjectionService::new(reader);

        assert_eq!(
            service
                .read(&scope("company/1"), ProjectionKind::PostingAccounts, None)
                .await
                .unwrap_err()
                .code,
            "LANE_01_COMPANY_INVALID"
        );
        assert_eq!(
            service
                .read(&scope("company-1"), ProjectionKind::JournalLines, None)
                .await
                .unwrap_err()
                .code,
            "LANE_01_JOURNAL_INVALID"
        );
        assert_eq!(
            service
                .read(
                    &scope("company-1"),
                    ProjectionKind::JournalLines,
                    Some("bad/id")
                )
                .await
                .unwrap_err()
                .code,
            "LANE_01_JOURNAL_INVALID"
        );
        assert_eq!(
            service
                .read(
                    &scope("company-1"),
                    ProjectionKind::PostingAccounts,
                    Some("journal-1")
                )
                .await
                .unwrap_err()
                .code,
            "LANE_01_READ_REQUEST_INVALID"
        );
        assert_eq!(service.reader.calls.load(Ordering::SeqCst), 0);
    }
}
