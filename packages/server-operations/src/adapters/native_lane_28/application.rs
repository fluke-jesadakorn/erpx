//! Application port and handler. No SQL lives here: the owner reader owns
//! statement execution and the handler owns validation and response assembly.

use super::domain::{LevelHistoryRow, StaffLevelFacts, validate_company, validate_engagement};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin, sync::Arc};

pub type StaffLevelReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner executes
/// the statements and applies the aggregation; the router only transports the
/// resulting facts.
pub trait StaffLevelRead: Send + Sync {
    fn facts<'a>(&'a self, company: &'a str) -> StaffLevelReadFuture<'a, StaffLevelFacts>;

    fn history<'a>(&'a self, engagement: &'a str) -> StaffLevelReadFuture<'a, Vec<LevelHistoryRow>>;
}

#[derive(Clone)]
pub struct StaffLevelHandler {
    reader: Arc<dyn StaffLevelRead>,
}

impl StaffLevelHandler {
    pub fn new(reader: Arc<dyn StaffLevelRead>) -> Self {
        Self { reader }
    }

    pub fn facts<'a>(&'a self, company: &'a str) -> StaffLevelReadFuture<'a, StaffLevelFacts> {
        let reader = self.reader.clone();
        let company = company.to_owned();
        Box::pin(async move {
            validate_company(&company)?;
            reader.facts(&company).await
        })
    }

    pub fn history<'a>(&'a self, engagement: &'a str) -> StaffLevelReadFuture<'a, Vec<LevelHistoryRow>> {
        let reader = self.reader.clone();
        let engagement = engagement.to_owned();
        Box::pin(async move {
            validate_engagement(&engagement)?;
            reader.history(&engagement).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_28::domain::StaffLevel;

    #[derive(Clone)]
    struct Probe;

    impl StaffLevelRead for Probe {
        fn facts<'a>(&'a self, company: &'a str) -> StaffLevelReadFuture<'a, StaffLevelFacts> {
            let company = company.to_owned();
            Box::pin(async move {
                Ok(StaffLevelFacts {
                    levels: vec![StaffLevel { id: format!("{company}-level"), rank: 3, level_number: 4 }],
                    active_count: 1,
                })
            })
        }

        fn history<'a>(&'a self, _engagement: &'a str) -> StaffLevelReadFuture<'a, Vec<LevelHistoryRow>> {
            Box::pin(async move { Ok(vec![]) })
        }
    }

    #[tokio::test]
    async fn handler_validates_before_delegating() {
        let handler = StaffLevelHandler::new(Arc::new(Probe));
        assert!(handler.facts("good-company").await.is_ok());
        assert!(handler.facts("").await.is_err());
        assert!(handler.facts("bad company!").await.is_err());
        assert!(handler.history("eng:1").await.is_ok());
        assert!(handler.history("").await.is_err());
    }
}
