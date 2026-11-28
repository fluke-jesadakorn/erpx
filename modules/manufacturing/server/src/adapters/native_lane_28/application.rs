//! Application port and handler. SQL stays in the owner adapter; this module
//! owns validation and response assembly only.

use super::domain::{PostingReferencesFacts, StockHistoryFacts, validate_identifier};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type StockHistoryReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port.
pub trait StockHistoryRead: Send + Sync {
    fn history<'a>(
        &'a self,
        company: &'a str,
        warehouse: &'a str,
        product: &'a str,
    ) -> StockHistoryReadFuture<'a, StockHistoryFacts>;
}

#[derive(Clone)]
pub struct StockHistoryHandler {
    reader: Arc<dyn StockHistoryRead>,
}

impl StockHistoryHandler {
    pub fn new(reader: Arc<dyn StockHistoryRead>) -> Self {
        Self { reader }
    }

    pub fn history<'a>(
        &'a self,
        company: &'a str,
        warehouse: &'a str,
        product: &'a str,
    ) -> StockHistoryReadFuture<'a, StockHistoryFacts> {
        let reader = self.reader.clone();
        let company = company.to_owned();
        let warehouse = warehouse.to_owned();
        let product = product.to_owned();
        Box::pin(async move {
            validate_identifier(&company)?;
            validate_identifier(&warehouse)?;
            validate_identifier(&product)?;
            reader.history(&company, &warehouse, &product).await
        })
    }
}

/// One owner reader serving both lane-28 read families. Composition injects a
/// single instance behind the router; tests use one probe for both routes.
pub trait Lane28Read: StockHistoryRead + PostingReferencesRead {}
impl<T: StockHistoryRead + PostingReferencesRead> Lane28Read for T {}

pub type PostingReferencesReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader for the existing-posting economic references capability.
pub trait PostingReferencesRead: Send + Sync {
    fn references<'a>(
        &'a self,
        company: &'a str,
    ) -> PostingReferencesReadFuture<'a, PostingReferencesFacts>;
}

#[derive(Clone)]
pub struct PostingReferencesHandler {
    reader: Arc<dyn PostingReferencesRead>,
}

impl PostingReferencesHandler {
    pub fn new(reader: Arc<dyn PostingReferencesRead>) -> Self {
        Self { reader }
    }

    pub fn references<'a>(
        &'a self,
        company: &'a str,
    ) -> PostingReferencesReadFuture<'a, PostingReferencesFacts> {
        let reader = self.reader.clone();
        let company = company.to_owned();
        Box::pin(async move {
            validate_identifier(&company)?;
            reader.references(&company).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Probe;

    impl StockHistoryRead for Probe {
        fn history<'a>(
            &'a self,
            company: &'a str,
            warehouse: &'a str,
            product: &'a str,
        ) -> StockHistoryReadFuture<'a, StockHistoryFacts> {
            let present = company == "company" && warehouse == "wh" && product == "product";
            Box::pin(async move { Ok(StockHistoryFacts { history: present }) })
        }
    }

    impl PostingReferencesRead for Probe {
        fn references<'a>(
            &'a self,
            company: &'a str,
        ) -> PostingReferencesReadFuture<'a, PostingReferencesFacts> {
            let valid = validate_identifier(company).is_ok();
            Box::pin(async move {
                Ok(PostingReferencesFacts {
                    journal_ids: if valid { vec!["j1".into()] } else { vec![] },
                    order_ids: if valid { vec!["o1".into()] } else { vec![] },
                    invalid: false,
                    image: "[]".into(),
                })
            })
        }
    }

    #[tokio::test]
    async fn handler_validates_all_identities() {
        let handler = StockHistoryHandler::new(Arc::new(Probe));
        assert_eq!(handler.history("company", "wh", "product").await.unwrap().history, true);
        assert!(handler.history("", "wh", "product").await.is_err());
        assert!(handler.history("company", "", "product").await.is_err());
        assert!(handler.history("company", "wh", "").await.is_err());
    }

    #[tokio::test]
    async fn posting_references_handler_validates_company() {
        let handler = PostingReferencesHandler::new(Arc::new(Probe));
        assert_eq!(handler.references("company").await.unwrap().journal_ids, vec!["j1"]);
        assert!(handler.references("").await.is_err());
        assert!(handler.references("bad company!").await.is_err());
    }
}
