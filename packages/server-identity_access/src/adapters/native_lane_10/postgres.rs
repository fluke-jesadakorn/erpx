//! sqlx execution for lane 10's account invitation reads.
//!
//! These are the three byte-identical successors of the deleted TypeScript
//! `prepare()` calls in `adapters/account-access.ts`. Only the placeholder
//! syntax is translated (`?` to `$n`); predicates, bind order and projections
//! are unchanged. Every statement runs inside one owner transaction and every
//! persistence fault maps to an explicit 503.

use crate::adapters::native_lane_10::application::{
    AccountAccessRead, AccountAccessReadFuture,
};
use crate::adapters::native_lane_10::domain::{
    Invitation, InvitationFields, invalid_result, unavailable,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Executed statements (`$n` placeholders).
pub const SQL_EMAIL_EXISTS: &str = r#"SELECT 1 FROM "user" WHERE lower(email)=lower($1)"#;
pub const SQL_INVITATION_STATUS: &str =
    "SELECT status FROM account_invitations WHERE id=$1 AND holding_id=$2 AND company_id=$3 AND person_id=$4";
pub const SQL_INVITATION_BY_TOKEN: &str = "SELECT * FROM account_invitations WHERE token_hash=$1 AND (status='pending' OR (status='accepted' AND accepted_at IS NULL)) AND expires_at>$2";

/// Pre-migration TypeScript statements (`?` placeholders), kept only as the
/// byte-identical result oracle for the inventory and parity tests.
pub const TS_ORACLE_SQL: [&str; 3] = [
    r#"SELECT 1 FROM "user" WHERE lower(email)=lower(?)"#,
    "SELECT status FROM account_invitations WHERE id=? AND holding_id=? AND company_id=? AND person_id=?",
    "SELECT * FROM account_invitations WHERE token_hash=? AND (status='pending' OR (status='accepted' AND accepted_at IS NULL)) AND expires_at>?",
];

pub struct PgAccountAccessRead {
    control: DatabasePool,
}

impl PgAccountAccessRead {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

impl AccountAccessRead for PgAccountAccessRead {
    fn email_exists<'a>(&'a self, email: &'a str) -> AccountAccessReadFuture<'a, bool> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let email = email.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let email = email.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_EMAIL_EXISTS)
                        .bind(email.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(row.is_some())
                })
            })
            .await
            .map_err(persistence)
        })
    }

    fn invitation_status<'a>(
        &'a self,
        holding_id: &'a str,
        company_id: &'a str,
        person_id: &'a str,
        invitation_id: &'a str,
    ) -> AccountAccessReadFuture<'a, Option<String>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let holding_id = holding_id.to_owned();
        let company_id = company_id.to_owned();
        let person_id = person_id.to_owned();
        let invitation_id = invitation_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let holding_id = holding_id.clone();
                let company_id = company_id.clone();
                let person_id = person_id.clone();
                let invitation_id = invitation_id.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_INVITATION_STATUS)
                        .bind(invitation_id.as_str())
                        .bind(holding_id.as_str())
                        .bind(company_id.as_str())
                        .bind(person_id.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        row.try_get::<String, _>("status")
                            .map_err(|_| invalid_result())
                    })
                    .transpose()
                    .map_err(PgTransactionError::Business)
                })
            })
            .await
            .map_err(persistence)
        })
    }

    fn invitation_by_token<'a>(
        &'a self,
        token_hash: &'a str,
        now: i64,
    ) -> AccountAccessReadFuture<'a, Option<Invitation>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let token_hash = token_hash.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let token_hash = token_hash.clone();
                Box::pin(async move {
                    let row = sqlx::query(SQL_INVITATION_BY_TOKEN)
                        .bind(token_hash.as_str())
                        .bind(now)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    row.map(|row| {
                        InvitationFields {
                            id: row.try_get("id").map_err(|_| invalid_result())?,
                            holding_id: row.try_get("holding_id").map_err(|_| invalid_result())?,
                            company_id: row.try_get("company_id").map_err(|_| invalid_result())?,
                            person_id: row.try_get("person_id").map_err(|_| invalid_result())?,
                            email: row.try_get("email").map_err(|_| invalid_result())?,
                            name: row.try_get("name").map_err(|_| invalid_result())?,
                            role: row.try_get("role").map_err(|_| invalid_result())?,
                            scope_kind: row.try_get("scope_kind").map_err(|_| invalid_result())?,
                            branch_id: row.try_get("branch_id").map_err(|_| invalid_result())?,
                            department_id: row.try_get("department_id").map_err(|_| invalid_result())?,
                            created_by: row.try_get("created_by").map_err(|_| invalid_result())?,
                            assignment_id: row.try_get("assignment_id").map_err(|_| invalid_result())?,
                            expires_at: row.try_get("expires_at").map_err(|_| invalid_result())?,
                            status: row.try_get("status").map_err(|_| invalid_result())?,
                        }
                        .into_invitation()
                    })
                    .transpose()
                    .map_err(PgTransactionError::Business)
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
    use crate::adapters::native_lane_10::domain::AccountAccessReadResult;
    use sha2::{Digest, Sha256};

    fn hash(value: &str) -> String {
        hex::encode(Sha256::digest(value.as_bytes()))
    }

    #[test]
    fn executed_sql_keeps_the_legacy_predicate_and_bind_order() {
        assert_eq!(
            hash(SQL_EMAIL_EXISTS),
            "54e54cbbe2e6c74f0320c389daa3b8bc38cf89e689215b45fc26ab58b5262b09"
        );
        assert_eq!(
            hash(SQL_INVITATION_STATUS),
            "def79196c50b708c7c6f3c325a6700fad1d057327d923b7c09617f8baef42c50"
        );
        assert_eq!(
            hash(SQL_INVITATION_BY_TOKEN),
            "368e3db1e375b2ae7fb13b0b710e622b0e4219c2b405cee22b63d31b4f0f612a"
        );
    }

    #[test]
    fn legacy_oracle_bundle_hash_is_pinned() {
        assert_eq!(
            hash(&serde_json::to_string(&TS_ORACLE_SQL).expect("json")),
            "0d36e9c87c85df5a2342fd0e394769dd73101e1731ffeed374b6453e3c6e4f26"
        );
    }

    #[test]
    fn result_oracle_hash_is_pinned() {
        let result = AccountAccessReadResult::Status(Some("pending".into())).into_json();
        assert_eq!(hash(&serde_json::to_string(&result).expect("json")), hash("\"pending\""));
    }
}
