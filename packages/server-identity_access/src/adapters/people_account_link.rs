//! Identity-owned account-link port for native People workforce commands.
//!
//! The account row lives in the Control database while the active assignment
//! and person-link facts live in the caller's serializable Holding
//! transaction. This adapter is the only native code that reads the Control
//! `"user"` row for a person/account link; it never writes People or
//! Organization rows and performs no commit of its own.

use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const MEMBER_LINK_INVALID: BusinessError = BusinessError::new(
    FailureKind::InvalidInput,
    "MEMBER_LINK_INVALID",
    "Choose the existing company member and their verified account email.",
);

async fn holding_ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PEOPLE_ACCOUNT_LINK_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PEOPLE_ACCOUNT_LINK_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

use erpx_server_runtime::public::postgres::PersistenceError;

/// Verify that `user_id` is an active member of `company_id` whose verified
/// Control account email matches `email`, and that no other live person in
/// the Company is already linked to that account.
pub async fn verify_person_account_link(
    control: &DatabasePool,
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    email: &str,
    person_id: &str,
    now: i64,
) -> Result<(), Error> {
    holding_ready(connection).await?;
    let account: Option<String> = sqlx::query_scalar("SELECT email FROM \"user\" WHERE id=$1")
        .bind(user_id)
        .fetch_optional(control.raw())
        .await?;
    let Some(account) = account else {
        return Err(MEMBER_LINK_INVALID.into());
    };
    if account.to_lowercase() != email.to_lowercase() {
        return Err(MEMBER_LINK_INVALID.into());
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=$1 AND a.company_id=$2 \
         AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$3) \
         AND iam_principal_active(a.user_id,a.company_id,$3) AND a.valid_from<=$3 AND a.valid_until>$3) \
         AND NOT EXISTS(SELECT 1 FROM hr_people p WHERE p.user_id=$1 AND p.company_id=$2 \
         AND p.id<>$4 AND p.merged_into_id IS NULL)",
    )
    .bind(user_id)
    .bind(company_id)
    .bind(now)
    .bind(person_id)
    .fetch_one(&mut *connection)
    .await?;
    if !active {
        return Err(MEMBER_LINK_INVALID.into());
    }
    Ok(())
}

/// Revoke every active Company assignment of the linked account. The caller
/// owns the Holding transaction; People owns the person/engagement rows.
pub async fn revoke_company_access(
    connection: &mut PgConnection,
    company_id: &str,
    user_id: &str,
    now: i64,
) -> Result<(), Error> {
    holding_ready(connection).await?;
    sqlx::query(
        "UPDATE assignments SET revoked_at=$1,version=version+1 \
         WHERE user_id=$2 AND company_id=$3 AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(user_id)
    .bind(company_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Convert the adapter error into the portable owner error used by the
/// People workforce ports.
pub fn into_business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "PEOPLE_ACCOUNT_LINK_UNAVAILABLE",
            "The Identity account-link port could not complete.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{MEMBER_LINK_INVALID, into_business};
    use erpx_server_runtime::public::postgres::PgTransactionError;

    #[test]
    fn account_link_failure_is_portable() {
        assert_eq!(MEMBER_LINK_INVALID.code, "MEMBER_LINK_INVALID");
        let business = into_business(PgTransactionError::Business(MEMBER_LINK_INVALID.clone()));
        assert_eq!(business, MEMBER_LINK_INVALID);
    }
}
