//! Read-only Identity facts for native Staff Work composition.
//!
//! This calls the existing `iam_same_person` SQL function on the caller's
//! connection. It opens no transaction, writes nothing and makes no permission
//! decision; it only reports the current person mapping.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, UserId};
use sqlx::PgConnection;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IAM_FACT_UNAVAILABLE",
        "The current Identity person fact is unavailable.",
    )
}

/// True when the two users map to the same current Person in this Company.
/// Missing users and missing memberships are simply not the same person.
pub async fn same_person(
    c: &mut PgConnection,
    company_id: &str,
    user_a: &str,
    user_b: &str,
) -> Result<bool, BusinessError> {
    let company = CompanyId::new(company_id)?;
    let left = UserId::new(user_a)?;
    let right = UserId::new(user_b)?;
    sqlx::query_scalar::<_, bool>("SELECT COALESCE(iam_same_person($1,$2,$3),false)")
        .bind(left.as_str())
        .bind(right.as_str())
        .bind(company.as_str())
        .fetch_one(c)
        .await
        .map_err(|_| unavailable())
}
