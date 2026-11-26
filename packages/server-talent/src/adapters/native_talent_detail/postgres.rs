//! sqlx/Postgres execution of the native Talent detail read.
//!
//! This is the only layer in the slice that opens a unit of work and executes
//! SQL. The application handler never sees a connection or a row, and no
//! TypeScript prepares or executes these statements. The owner SELECTs are
//! fixed in the domain; only validated wire facts are bound.

use crate::application::native_talent_detail::{
    TalentDetailQuery, TalentDetailRead, TalentDetailReadFuture,
};
use crate::domain::native_talent_detail::{
    DETAIL_APPLICATION_SQL, DETAIL_HANDOFF_SQL, DETAIL_HISTORY_SQL, DETAIL_OFFER_SQL,
    DETAIL_PROPOSAL_SQL, TalentDetail, application_not_found, unavailable,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use serde_json::Value;

type Error = PgTransactionError<BusinessError>;

fn unavailable_error() -> Error {
    unavailable().into()
}

fn parse_row(text: Option<String>) -> Result<Value, Error> {
    let text = text.ok_or_else(unavailable_error)?;
    serde_json::from_str(&text).map_err(|_| unavailable_error())
}

fn parse_rows(text: Option<String>) -> Result<Vec<Value>, Error> {
    let value = parse_row(text)?;
    match value {
        Value::Array(rows) => Ok(rows),
        _ => Err(unavailable_error()),
    }
}

/// Postgres-backed owner reader. All five owner SELECTs execute inside one
/// serializable read unit, so the returned projection is a single consistent
/// snapshot of the application, its history, proposal, offer and handoff.
pub struct PgTalentDetailRead {
    pool: DatabasePool,
}

impl PgTalentDetailRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TalentDetailRead for PgTalentDetailRead {
    fn detail<'a>(
        &'a self,
        query: &'a TalentDetailQuery,
    ) -> TalentDetailReadFuture<'a, Option<TalentDetail>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let id = query.id.clone();
        let company_id = query.company_id.clone();
        let branch_id = query.branch_id.clone();
        let department_id = query.department_id.clone();
        let actor_id = query.actor_id.clone();
        let can_manage_documents = query.can_manage_documents;
        Box::pin(async move {
            let detail = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let id = id.clone();
                    let company_id = company_id.clone();
                    let branch_id = branch_id.clone();
                    let department_id = department_id.clone();
                    let actor_id = actor_id.clone();
                    Box::pin(async move {
                        let application_text = sqlx::query_scalar::<_, Option<String>>(
                            DETAIL_APPLICATION_SQL,
                        )
                        .bind(id.as_str())
                        .bind(company_id.as_str())
                        .bind(branch_id.as_deref())
                        .bind(department_id.as_deref())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable_error())?;
                        let application = parse_row(application_text)?;
                        if application.is_null() {
                            return Err(application_not_found().into());
                        }

                        let history_text = sqlx::query_scalar::<_, Option<String>>(
                            DETAIL_HISTORY_SQL,
                        )
                        .bind(id.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable_error())?;
                        let history = parse_rows(history_text)?;

                        let proposal_text = sqlx::query_scalar::<_, Option<String>>(
                            DETAIL_PROPOSAL_SQL,
                        )
                        .bind(id.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable_error())?;
                        let proposal = parse_row(proposal_text)?;

                        let offer_text =
                            sqlx::query_scalar::<_, Option<String>>(DETAIL_OFFER_SQL)
                                .bind(id.as_str())
                                .fetch_one(&mut *connection)
                                .await
                                .map_err(|_| unavailable_error())?;
                        let offer = parse_row(offer_text)?;

                        let handoff = if offer.is_null() {
                            Value::Null
                        } else {
                            let offer_id = offer
                                .get("id")
                                .and_then(Value::as_str)
                                .ok_or_else(unavailable_error)?;
                            let handoff_text = sqlx::query_scalar::<_, Option<String>>(
                                DETAIL_HANDOFF_SQL,
                            )
                            .bind(offer_id)
                            .fetch_one(&mut *connection)
                            .await
                            .map_err(|_| unavailable_error())?;
                            parse_row(handoff_text)?
                        };

                        Ok::<_, Error>(TalentDetail {
                            application,
                            history,
                            proposal,
                            offer,
                            handoff,
                            actor_id,
                            can_review: true,
                            can_manage_documents,
                        })
                    })
                })
                .await;
            match detail {
                Ok(value) => Ok(Some(value)),
                Err(PgTransactionError::Business(error)) => {
                    if error.code == application_not_found().code {
                        Ok(None)
                    } else {
                        Err(error)
                    }
                }
                Err(PgTransactionError::Persistence(_)) => Err(unavailable()),
            }
        })
    }
}
