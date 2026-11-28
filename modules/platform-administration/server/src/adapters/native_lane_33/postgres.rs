//! PostgreSQL execution for the lane 33 release child-image read. The SQL is
//! byte-identical to the retired TypeScript `jsonb_build_object` statement;
//! only the two `?` binds become `$1`/`$2`.

use crate::adapters::native_lane_33::application::{
    ReleaseChildImageFuture, ReleaseChildImageRead, unavailable,
};
use crate::adapters::native_lane_33::domain::ReleaseChildImage;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;

/// Byte-identical to the retired TS rendering, with `?` renamed to `$n`.
pub const IMAGE_SQL: &str = "SELECT jsonb_build_object('acceptance',to_jsonb(acceptance),
 'reviews',(SELECT jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id) FROM company_release_gate_reviews review WHERE review.release_id=acceptance.id AND review.company_id=acceptance.company_id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id) FROM company_release_events event WHERE event.release_id=acceptance.id AND event.company_id=acceptance.company_id),
 'revision',(SELECT to_jsonb(revision) FROM company_release_revisions revision WHERE revision.company_id=acceptance.company_id))::text image FROM company_release_acceptances acceptance WHERE acceptance.id=$1 AND acceptance.company_id=$2";

/// The exact `?`-placeholder rendering captured from the retired TypeScript.
pub fn rendered_image_sql() -> String {
    IMAGE_SQL.replace("$1", "?").replace("$2", "?")
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub struct PgReleaseChildImageRead {
    pool: DatabasePool,
}

impl PgReleaseChildImageRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ReleaseChildImageRead for PgReleaseChildImageRead {
    fn read<'a>(
        &'a self,
        company: &'a str,
        release: &'a str,
    ) -> ReleaseChildImageFuture<'a, Option<ReleaseChildImage>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let release = release.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let release = release.clone();
                Box::pin(async move {
                    let image = sqlx::query_scalar::<_, String>(IMAGE_SQL)
                        .bind(release)
                        .bind(company)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| PgTransactionError::Business(unavailable()))?;
                    image
                        .map(|image| {
                            ReleaseChildImage::new(image)
                                .map_err(|_| PgTransactionError::Business(unavailable()))
                        })
                        .transpose()
                })
            })
            .await
            .map_err(persistence)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_lane_33::domain::SQL_SHA256;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(value.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn rendered_sql_is_byte_identical_to_the_retired_typescript() {
        let rendered = rendered_image_sql();
        assert_eq!(rendered.len(), 689);
        assert_eq!(hash(&rendered), SQL_SHA256);
        let mut executable = rendered.clone();
        let first = executable.find("=?").unwrap();
        executable.replace_range(first..first + 2, "=$1");
        let second = executable.find("=?").unwrap();
        executable.replace_range(second..second + 2, "=$2");
        assert_eq!(executable, IMAGE_SQL);
        assert!(IMAGE_SQL.contains("jsonb_agg(to_jsonb(review) ORDER BY review.criterion_id)"));
        assert!(IMAGE_SQL.contains("jsonb_agg(to_jsonb(event) ORDER BY event.version,event.id)"));
        assert!(IMAGE_SQL.contains("to_jsonb(revision)"));
        assert!(!IMAGE_SQL.contains("FOR UPDATE") && !IMAGE_SQL.contains("FOR SHARE"));
    }

    #[test]
    fn reader_does_not_run_sql_in_tests() {
        // Parity with PG is owned by the integration suite; this lane test only
        // pins the SQL text and the fail-closed projection boundary.
        assert!(ReleaseChildImage::new("{\"a\":1}".into()).is_ok());
    }
}
