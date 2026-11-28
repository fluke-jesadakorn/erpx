//! Owner query port and handler for the lane-24 evaluation-reference family.
//! This layer plans no SQL: it validates and dispatches to the injected owner
//! reader, then projects the result into the strict response envelope.

use crate::adapters::native_lane_24::domain::{
    FactsEnvelope, ReadResult, envelope, validate,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type ReadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct FactsQuery {
    /// Company scope from the verified request header only.
    pub company: String,
    pub kind: String,
    /// Bounded positional owner ids (nullable where the retired SQL bound a
    /// null filter); never SQL text.
    pub values: Vec<Option<String>>,
}

/// The owner executes the read on its own Postgres pool. Composition injects
/// the Pg-backed implementation; the router only transports transport facts.
pub trait EvaluationFactsReader: Send + Sync {
    fn read(&self, query: FactsQuery) -> ReadFuture<'_, ReadResult>;
}

/// Validate once at the application boundary and assemble the response shape.
/// The reader cannot be reached with an unvalidated kind, Company or arity.
pub async fn handle(
    reader: &dyn EvaluationFactsReader,
    query: FactsQuery,
) -> Result<FactsEnvelope, BusinessError> {
    validate(&query.company, &query.kind, &query.values)?;
    let kind = query.kind.clone();
    let result = reader.read(query).await?;
    envelope(&kind, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_24::domain::{EVAL_MARKET_LEVELS, MarketLevel};

    struct Fixed;
    impl EvaluationFactsReader for Fixed {
        fn read(&self, _: FactsQuery) -> ReadFuture<'_, ReadResult> {
            Box::pin(async {
                Ok(ReadResult::MarketLevels(vec![MarketLevel {
                    id: "level-1".into(),
                }]))
            })
        }
    }

    #[tokio::test]
    async fn handler_validates_then_projects() {
        let envelope = handle(
            &Fixed,
            FactsQuery {
                company: "acme-co".into(),
                kind: EVAL_MARKET_LEVELS.into(),
                values: vec![],
            },
        )
        .await
        .unwrap();
        assert_eq!(envelope.data["rows"][0]["id"], "level-1");
    }

    #[tokio::test]
    async fn handler_rejects_unknown_kind_before_reading() {
        assert!(handle(
            &Fixed,
            FactsQuery {
                company: "acme-co".into(),
                kind: "drop_table".into(),
                values: vec![],
            },
        )
        .await
        .is_err());
    }
}
