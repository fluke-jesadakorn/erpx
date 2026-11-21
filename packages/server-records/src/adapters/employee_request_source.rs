//! PostgreSQL adapter for the Records employee-request purchase source.
//!
//! SQL is Records-owned: only `employee_requests` is selected, with
//! `FOR SHARE` so the retained row is fenced against concurrent writers
//! for the caller's unit. The exact retained selection (department
//! before branch before holding) is applied in SQL, in the same shape
//! as the Records context/registry reads. No authority is checked
//! here: `commerce.purchase` permission, workspace membership and the
//! Procurement-owned allocation/order-link facts remain caller duties.
//! The caller's serializable Holding transaction is required and no
//! commit is performed.
use crate::application::employee_request_source::EmployeeRequestSourcePort;
use crate::domain::employee_request_source::{
    EmployeeRequestGuard, EmployeeRequestRow, EmployeeRequestScope, guard_for, require_current,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const SCOPED_SELECT: &str = "SELECT id,company_id,branch_id,department_id,kind,state,version,\
 evidence_version,items_json,approval_json,title,currency,requester_user_id,created_by,\
 assignee_user_id,purchase_order_id FROM employee_requests \
 WHERE company_id=$1 AND id=$2 \
 AND (($4::text IS NOT NULL AND department_id=$4::text) \
   OR ($4::text IS NULL AND $3::text IS NOT NULL AND branch_id=$3::text) \
   OR ($3::text IS NULL AND $4::text IS NULL)) FOR SHARE";

/// Acquire the Records source locks before the first selected snapshot.
/// The union order against other owners is the composition root's duty;
/// this matches the retained TS lock sets
/// (`commerceOwnerRequestSourceReadLocks` and the Records/legal
/// employee-request lock pair).
pub async fn lock_employee_request_sources(connection: &mut PgConnection) -> Result<(), Error> {
    sqlx::query(
        "LOCK TABLE employee_requests, employee_request_events IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}

fn map_request(row: &sqlx::postgres::PgRow) -> Result<EmployeeRequestRow, sqlx::Error> {
    Ok(EmployeeRequestRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        kind: row.try_get("kind")?,
        state: row.try_get("state")?,
        version: row.try_get("version")?,
        evidence_version: row.try_get("evidence_version")?,
        items_json: row.try_get("items_json")?,
        approval_json: row.try_get("approval_json")?,
        title: row.try_get("title")?,
        currency: row.try_get("currency")?,
        requester_user_id: row.try_get("requester_user_id")?,
        created_by: row.try_get("created_by")?,
        assignee_user_id: row.try_get("assignee_user_id")?,
        purchase_order_id: row.try_get("purchase_order_id")?,
    })
}

pub struct PgEmployeeRequestSource<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgEmployeeRequestSource<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("RECORDS_REQUEST_SOURCE_TRANSACTION_REQUIRED").into(),
            );
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("RECORDS_REQUEST_SOURCE_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }

    async fn scoped_row(
        &mut self,
        scope: &EmployeeRequestScope,
        id: &str,
    ) -> Result<Option<EmployeeRequestRow>, Error> {
        let row = sqlx::query(SCOPED_SELECT)
            .bind(&scope.company_id)
            .bind(id)
            .bind(scope.branch_id.as_deref())
            .bind(scope.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.as_ref()
            .map(map_request)
            .transpose()
            .map_err(Into::into)
    }
}

impl EmployeeRequestSourcePort for PgEmployeeRequestSource<'_> {
    type Error = Error;
    type Witness = EmployeeRequestGuard;

    fn read_request(
        &mut self,
        scope: &EmployeeRequestScope,
        id: &str,
    ) -> impl Future<Output = Result<(EmployeeRequestRow, Self::Witness), Self::Error>> + Send {
        Box::pin(async move {
            self.ready().await?;
            let row = self.scoped_row(scope, id).await?.ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "REQUEST_NOT_FOUND",
                    "Request not found.",
                )
            })?;
            Ok((row.clone(), guard_for(&row)))
        })
    }

    fn require_current(
        &mut self,
        witness: &Self::Witness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        Box::pin(async move {
            self.ready().await?;
            let scope = EmployeeRequestScope {
                company_id: witness.company_id.clone(),
                branch_id: witness.branch_id.clone(),
                department_id: witness.department_id.clone(),
            };
            let row = self.scoped_row(&scope, &witness.id).await?.ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::NotFound,
                    "REQUEST_NOT_FOUND",
                    "Request not found.",
                )
            })?;
            require_current(witness, &row, &scope)?;
            Ok(())
        })
    }
}
