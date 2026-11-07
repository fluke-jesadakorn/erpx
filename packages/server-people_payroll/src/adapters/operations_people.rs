//! People-owned directory names for Operations owner assignment. These are
//! display facts only; they grant no authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationsPerson {
    pub id: String,
    pub name: String,
}

/// Read active people in the selected Company on the caller's open transaction.
pub async fn read_operations_people(
    c: &mut PgConnection,
    company: &CompanyId,
) -> Result<Vec<OperationsPerson>, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("OPERATIONS_PEOPLE_TRANSACTION_REQUIRED").into());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT person_id,display_name FROM employees \
         WHERE company_id=$1 AND status='active' AND person_id IS NOT NULL \
           AND octet_length(display_name) BETWEEN 1 AND 3000 \
         ORDER BY display_name,person_id LIMIT 1001",
    )
    .bind(company.as_str())
    .fetch_all(c)
    .await?;
    if rows.len() > 1_000 {
        return Err(PersistenceError::new("OPERATIONS_PEOPLE_LIMIT").into());
    }
    Ok(rows
        .into_iter()
        .map(|(id, name)| OperationsPerson { id, name })
        .collect())
}
