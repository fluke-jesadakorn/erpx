//! sqlx execution for the lane-7 legacy people clock. The statement is the
//! retired TypeScript literal with only `?` rewritten to sqlx `$n` binds; the
//! Company predicate appears in every UNION arm and the clock is a bound value.

use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::{
    AuthorityPersonLookup, AuthorityQuery, LegacyPeopleClock, TransitionFuture, TransitionQuery,
    WorkspacePeoplePayments,
};
use super::domain::{
    AuthorityKind, AuthorityPerson, LegacyPeopleTransition, WorkspacePaymentRow,
};

/// Byte-for-byte successor of `nextLegacyPeopleTransition` in
/// `adapters/legacy-license-access.ts`: five UNION arms and the `stamp>?`
/// boundary, with positional binds numbered for sqlx.
pub const SQL_LEGACY_PEOPLE_TRANSITION: &str = "WITH boundaries AS ( SELECT effective_at AS stamp FROM people_access_cutoffs WHERE company_id=$1 AND cancelled_at IS NULL UNION ALL SELECT restored_at FROM people_access_cutoffs WHERE company_id=$2 AND cancelled_at IS NULL UNION ALL SELECT effective_at FROM people_lifecycle_cases WHERE company_id=$3 AND status IN ('scheduled','completed') UNION ALL SELECT valid_from FROM people_applied_packages WHERE company_id=$4 AND ended_at IS NULL UNION ALL SELECT valid_until FROM people_applied_packages WHERE company_id=$5 AND ended_at IS NULL ) SELECT min(stamp) AS deadline FROM boundaries WHERE stamp>$6";

/// Pg-backed owner reader. It owns the statement, the transaction and the
/// projection; no caller-supplied SQL is ever accepted.
pub struct PgLegacyPeopleClock {
    pool: DatabasePool,
}

impl PgLegacyPeopleClock {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_LANE7_READ_UNAVAILABLE",
        "The native People Payroll clock read is unavailable.",
    )
}

impl LegacyPeopleClock for PgLegacyPeopleClock {
    fn next_transition<'a>(
        &'a self,
        query: TransitionQuery,
    ) -> TransitionFuture<'a, LegacyPeopleTransition> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let query = query.clone();
                Box::pin(async move {
                    let company = query.company.as_str();
                    let row = sqlx::query(SQL_LEGACY_PEOPLE_TRANSITION)
                        .bind(company)
                        .bind(company)
                        .bind(company)
                        .bind(company)
                        .bind(company)
                        .bind(query.observed_at_ms)
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let deadline = row
                        .try_get::<Option<i64>, _>("deadline")
                        .map_err(|_| unavailable())?;
                    Ok(LegacyPeopleTransition { deadline })
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

/// Byte-identical successor of `financeWorkspacePeoplePayments` in
/// `adapters/finance-workspace-sources.ts`. `?` became `$1`; the SUM is cast to
/// bigint so the numeric aggregate decodes as the same integer the TS consumer
/// already coerced with `Number(...)`.
pub const SQL_WORKSPACE_PEOPLE_PAYMENTS: &str = "SELECT b.id,b.source_type,b.source_id,b.currency,b.status,r.pay_date due_date, COALESCE(SUM(CASE WHEN i.status IN ('outstanding','failed') THEN i.amount_minor ELSE 0 END),0)::bigint amount_minor FROM hr_payment_batches b LEFT JOIN hr_payment_batch_items i ON i.batch_id=b.id LEFT JOIN hr_payroll_runs r ON r.id=b.source_id AND b.source_type='payroll' AND r.company_id=b.company_id WHERE b.company_id=$1 AND b.status IN ('draft','approved','exported','partially_settled') GROUP BY b.id,r.pay_date ORDER BY r.pay_date NULLS LAST,b.id";

/// Pg-backed owner reader for the Company People payment projection.
pub struct PgWorkspacePeoplePayments {
    pool: DatabasePool,
}

impl PgWorkspacePeoplePayments {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkspacePeoplePayments for PgWorkspacePeoplePayments {
    fn outstanding<'a>(
        &'a self,
        company: &'a str,
    ) -> TransitionFuture<'a, Vec<WorkspacePaymentRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                Box::pin(async move {
                    let rows = sqlx::query(SQL_WORKSPACE_PEOPLE_PAYMENTS)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    rows.into_iter()
                        .map(|row| {
                            Ok(WorkspacePaymentRow {
                                id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                                source_type: row
                                    .try_get::<String, _>("source_type")
                                    .map_err(|_| unavailable())?,
                                source_id: row
                                    .try_get::<String, _>("source_id")
                                    .map_err(|_| unavailable())?,
                                currency: row
                                    .try_get::<String, _>("currency")
                                    .map_err(|_| unavailable())?,
                                status: row
                                    .try_get::<String, _>("status")
                                    .map_err(|_| unavailable())?,
                                due_date: row
                                    .try_get::<Option<String>, _>("due_date")
                                    .map_err(|_| unavailable())?,
                                amount_minor: row
                                    .try_get::<i64, _>("amount_minor")
                                    .map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()
                        .map_err(PgTransactionError::Business)
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

/// Successor of `organizationAuthorityActorPerson`: same Company/active/merge
/// guards and the owner's `iam_same_person` predicate.
pub const SQL_AUTHORITY_ACTOR: &str = "SELECT id FROM hr_people WHERE company_id=$1 AND iam_same_person(user_id,$2,company_id) AND status='active' AND merged_into_id IS NULL";
/// Successor of `organizationAuthorityAppointee`; `p.user_id` is aliased to the
/// same single `id` projection as the actor lookup.
pub const SQL_AUTHORITY_APPOINTEE: &str = "SELECT p.user_id id FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=$1 AND p.company_id=$2 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND ($3::text IS NULL OR e.position_id=$4 OR $5) AND ($6<>'head' OR e.department_id=$7)";

pub struct PgAuthorityPersonLookup {
    pool: DatabasePool,
}

impl PgAuthorityPersonLookup {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AuthorityPersonLookup for PgAuthorityPersonLookup {
    fn lookup<'a>(
        &'a self,
        company: &'a str,
        query: AuthorityQuery,
    ) -> TransitionFuture<'a, AuthorityPerson> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let query = query.clone();
                Box::pin(async move {
                    let row = match query.kind {
                        AuthorityKind::Actor => sqlx::query(SQL_AUTHORITY_ACTOR)
                            .bind(company.as_str())
                            .bind(query.user_id.as_str())
                            .fetch_optional(&mut *connection)
                            .await,
                        AuthorityKind::Appointee => {
                            let position = query.position_id.as_deref();
                            sqlx::query(SQL_AUTHORITY_APPOINTEE)
                                .bind(query.person_id.as_str())
                                .bind(company.as_str())
                                .bind(position)
                                .bind(position)
                                .bind(query.self_ceo)
                                .bind(query.appointment_kind.as_str())
                                .bind(query.scope_id.as_str())
                                .fetch_optional(&mut *connection)
                                .await
                        }
                    }
                    .map_err(|_| unavailable())?;
                    let id = row
                        .as_ref()
                        .map(|row| row.try_get::<String, _>("id").map_err(|_| unavailable()))
                        .transpose()?;
                    Ok(AuthorityPerson { id })
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
    use sha2::Digest;

    #[test]
    fn statement_keeps_every_company_arm_and_one_boundary() {
        let sql = SQL_LEGACY_PEOPLE_TRANSITION;
        assert_eq!(sql.matches("company_id=$").count(), 5);
        assert_eq!(sql.matches("stamp>$6").count(), 1);
        assert_eq!(sql.matches("SELECT ").count(), 6);
        assert!(sql.contains("people_access_cutoffs"));
        assert!(sql.contains("people_lifecycle_cases"));
        assert!(sql.contains("people_applied_packages"));
        assert!(!sql.contains('?'));
    }

    #[test]
    fn pins_authority_sql_oracles() {
        let actor = sha2::Sha256::digest(SQL_AUTHORITY_ACTOR.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            actor,
            "c5264c5c857254afc0d7f10e7627a513e05a9d3aea5c250e08858d6b759a0840"
        );
        let appointee = sha2::Sha256::digest(SQL_AUTHORITY_APPOINTEE.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            appointee,
            "f579db98f3ccd908d63b13ef6c64fbb39096f7a8864cf2598dfd78bff8a5face"
        );
    }

    #[test]
    fn pins_payments_sql_oracle() {
        assert_eq!(
            SQL_WORKSPACE_PEOPLE_PAYMENTS
                .matches("company_id=$1")
                .count(),
            1
        );
        assert!(SQL_WORKSPACE_PEOPLE_PAYMENTS.contains("hr_payment_batches"));
        assert!(SQL_WORKSPACE_PEOPLE_PAYMENTS.contains("::bigint amount_minor"));
        let hex = sha2::Sha256::digest(SQL_WORKSPACE_PEOPLE_PAYMENTS.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "97047147e5dc9782b5de8e4c0659cb6c693f45a398186acb19ddb1f2b0200c2e"
        );
    }

    #[test]
    fn pins_rendered_sql_oracle() {
        // sha256 of the exact const above; the TS contract test pins the same
        // value without carrying a SQL literal.
        let hash = sha2::Sha256::digest(SQL_LEGACY_PEOPLE_TRANSITION.as_bytes());
        let hex = hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex, "79ad85c3c72c22e111f812e45e22068608eeb21fb5e000c23beaa6ec162930f2",
            "update this oracle in Rust and in tests/unit/people_payroll/native-lane-7.test.ts"
        );
    }
}
