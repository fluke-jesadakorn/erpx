//! Application seam for the lane 1 document preview reads. No SQL here.

use super::domain::{
    DocPreviewKind, DocPreviewProjection, PreviewAccountRow, PreviewJournalLineRow,
    PreviewStockConfigurationRow,
};
use crate::adapters::native_lane_1::domain::{CompanyScope, valid_identifier};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{future::Future, pin::Pin, sync::Arc};

pub type DocPreviewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait DocPreviewReader: Send + Sync {
    fn accounts<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Vec<PreviewAccountRow>>;

    fn journal_lines<'a>(
        &'a self,
        scope: &'a CompanyScope,
        journal_id: &'a str,
    ) -> DocPreviewFuture<'a, Vec<PreviewJournalLineRow>>;

    fn stock_configuration<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Option<PreviewStockConfigurationRow>>;
}

impl DocPreviewReader for Arc<dyn DocPreviewReader> {
    fn accounts<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Vec<PreviewAccountRow>> {
        (**self).accounts(scope)
    }

    fn journal_lines<'a>(
        &'a self,
        scope: &'a CompanyScope,
        journal_id: &'a str,
    ) -> DocPreviewFuture<'a, Vec<PreviewJournalLineRow>> {
        (**self).journal_lines(scope, journal_id)
    }

    fn stock_configuration<'a>(
        &'a self,
        scope: &'a CompanyScope,
    ) -> DocPreviewFuture<'a, Option<PreviewStockConfigurationRow>> {
        (**self).stock_configuration(scope)
    }
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

pub struct DocPreviewService<R: DocPreviewReader> {
    reader: R,
}

impl<R: DocPreviewReader> DocPreviewService<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    pub fn read<'a>(
        &'a self,
        scope: &'a CompanyScope,
        kind: DocPreviewKind,
        journal_id: Option<&'a str>,
    ) -> DocPreviewFuture<'a, DocPreviewProjection> {
        Box::pin(async move {
            if !valid_identifier(&scope.company_id) {
                return Err(invalid(
                    "LANE_01_DOC_PREVIEW_COMPANY_INVALID",
                    "Use a bounded registered Company.",
                ));
            }
            match kind {
                DocPreviewKind::Accounts => {
                    if journal_id.is_some() {
                        return Err(invalid(
                            "LANE_01_DOC_PREVIEW_REQUEST_INVALID",
                            "The accounts read takes no journal id.",
                        ));
                    }
                    Ok(DocPreviewProjection::Accounts(
                        self.reader.accounts(scope).await?,
                    ))
                }
                DocPreviewKind::JournalLines => {
                    let id = journal_id
                        .filter(|id| valid_identifier(id))
                        .ok_or_else(|| {
                            invalid(
                                "LANE_01_DOC_PREVIEW_JOURNAL_INVALID",
                                "Use a bounded Finance journal id.",
                            )
                        })?;
                    Ok(DocPreviewProjection::JournalLines(
                        self.reader.journal_lines(scope, id).await?,
                    ))
                }
                DocPreviewKind::StockConfiguration => {
                    if journal_id.is_some() {
                        return Err(invalid(
                            "LANE_01_DOC_PREVIEW_REQUEST_INVALID",
                            "The configuration read takes no journal id.",
                        ));
                    }
                    Ok(DocPreviewProjection::StockConfiguration(
                        self.reader.stock_configuration(scope).await?,
                    ))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub;

    impl DocPreviewReader for Stub {
        fn accounts<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> DocPreviewFuture<'a, Vec<PreviewAccountRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn journal_lines<'a>(
            &'a self,
            _scope: &'a CompanyScope,
            _journal_id: &'a str,
        ) -> DocPreviewFuture<'a, Vec<PreviewJournalLineRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn stock_configuration<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> DocPreviewFuture<'a, Option<PreviewStockConfigurationRow>> {
            Box::pin(async { Ok(None) })
        }
    }

    #[tokio::test]
    async fn validation_precedes_reader_dispatch() {
        let service = DocPreviewService::new(Stub);
        let scope = CompanyScope {
            company_id: "company-1".into(),
        };
        assert!(
            service
                .read(&scope, DocPreviewKind::Accounts, None)
                .await
                .is_ok()
        );
        assert_eq!(
            service
                .read(&scope, DocPreviewKind::JournalLines, None)
                .await
                .unwrap_err()
                .code,
            "LANE_01_DOC_PREVIEW_JOURNAL_INVALID"
        );
        assert_eq!(
            service
                .read(&scope, DocPreviewKind::Accounts, Some("journal-1"))
                .await
                .unwrap_err()
                .code,
            "LANE_01_DOC_PREVIEW_REQUEST_INVALID"
        );
        assert_eq!(
            service
                .read(
                    &CompanyScope {
                        company_id: "bad/id".into()
                    },
                    DocPreviewKind::Accounts,
                    None
                )
                .await
                .unwrap_err()
                .code,
            "LANE_01_DOC_PREVIEW_COMPANY_INVALID"
        );
    }
}
