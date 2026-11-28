//! sqlx execution of the native assistant source read. This is the only place
//! the retired TypeScript source query lives: it is executed on a
//! Company/user/assignment/scope-bound read unit, with no TypeScript
//! prepare/batch fallback.

use super::application::{AssistantSourceQuery, AssistantSourcesFuture, AssistantSourcesRead};
use super::domain::{SQL_ASSISTANT_SOURCES, AssistantSourceRef, unavailable};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use sqlx::Row;

pub struct PgAssistantSourcesRead {
    pool: DatabasePool,
}

impl PgAssistantSourcesRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AssistantSourcesRead for PgAssistantSourcesRead {
    fn saved<'a>(
        &'a self,
        query: &'a AssistantSourceQuery,
    ) -> AssistantSourcesFuture<'a, Vec<AssistantSourceRef>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let query = query.clone();
        Box::pin(async move {
            let rows = unit
                .run::<_, erpx_platform_shared_server::public::BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let query = query.clone();
                        Box::pin(async move {
                            let rows = sqlx::query(SQL_ASSISTANT_SOURCES)
                                .bind(query.company_id.as_str())
                                .bind(query.user_id.as_str())
                                .bind(query.assignment_id.as_str())
                                .bind(query.scope_key.as_str())
                                .bind(query.path.as_str())
                                .fetch_all(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            let mut references = Vec::new();
                            for row in rows {
                                let raw: Option<String> = row
                                    .try_get("sources_json")
                                    .map_err(|_| unavailable())?;
                                let Some(raw) = raw else { continue };
                                let sources: Vec<serde_json::Value> =
                                    serde_json::from_str(&raw).map_err(|_| unavailable())?;
                                for value in sources {
                                    references.push(
                                        serde_json::from_value::<AssistantSourceRef>(value)
                                            .map_err(|_| unavailable())?,
                                    );
                                }
                            }
                            Ok(references)
                        })
                    },
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            Ok(rows)
        })
    }
}
