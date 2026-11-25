//! sqlx execution for the native field-command routing read.
//!
//! Both statements are byte-identical successors of the two `prepare` calls in
//! `adapters/field-command-owner.ts`; only the `?` binds were renumbered to
//! `$1,$2` in the original order. Both tables are collaboration-owned and carry
//! the Company fence.

use crate::adapters::native_lane_15::field_command::{
    application::{FieldCommandFuture, FieldCommandQuery},
    domain::{unavailable, CardSource, RelationSource},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

pub const RELATION_SQL: &str =
    "SELECT source_type,target_type FROM collaboration_record_relations WHERE id=$1 AND company_id=$2";
pub const CARD_SQL: &str =
    "SELECT source_type FROM collaboration_action_cards WHERE id=$1 AND company_id=$2";

pub struct PgFieldCommandQuery {
    pool: DatabasePool,
}

impl PgFieldCommandQuery {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl FieldCommandQuery for PgFieldCommandQuery {
    fn relation<'a>(
        &'a self,
        id: &'a str,
        company: &'a str,
    ) -> FieldCommandFuture<'a, Option<RelationSource>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let id = id.to_owned();
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let id = id.clone();
                let company = company.clone();
                Box::pin(async move {
                    let row = sqlx::query(RELATION_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|r| {
                        Ok(RelationSource {
                            source_type: r.try_get("source_type").map_err(|_| unavailable())?,
                            target_type: r.try_get("target_type").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }

    fn card<'a>(
        &'a self,
        id: &'a str,
        company: &'a str,
    ) -> FieldCommandFuture<'a, Option<CardSource>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let id = id.to_owned();
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let id = id.clone();
                let company = company.clone();
                Box::pin(async move {
                    let row = sqlx::query(CARD_SQL)
                        .bind(id.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|r| {
                        Ok(CardSource {
                            source_type: r.try_get("source_type").map_err(|_| unavailable())?,
                        })
                    })
                    .transpose()
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENDERED_TS_RELATION: &str =
        "SELECT source_type,target_type FROM collaboration_record_relations WHERE id=? AND company_id=?";
    const RENDERED_TS_CARD: &str =
        "SELECT source_type FROM collaboration_action_cards WHERE id=? AND company_id=?";

    fn renumber(sql: &str) -> String {
        let mut n = 0;
        sql.split('?')
            .map(|part| {
                if n == 0 {
                    n += 1;
                    part.to_owned()
                } else {
                    let out = format!("${n}{part}");
                    n += 1;
                    out
                }
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn native_sql_is_the_rendered_ts_sql_with_numbered_binds() {
        assert_eq!(renumber(RENDERED_TS_RELATION), RELATION_SQL);
        assert_eq!(renumber(RENDERED_TS_CARD), CARD_SQL);
        assert_eq!(RELATION_SQL.matches('$').count(), 2);
        assert_eq!(CARD_SQL.matches('$').count(), 2);
    }

    #[test]
    fn both_reads_stay_on_collaboration_owned_tables() {
        assert!(RELATION_SQL.contains("collaboration_record_relations"));
        assert!(CARD_SQL.contains("collaboration_action_cards"));
        assert!(RELATION_SQL.contains("company_id=$2"));
        assert!(CARD_SQL.contains("company_id=$2"));
    }
}
