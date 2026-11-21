//! PostgreSQL read adapter for the native Sales document list.
//!
//! The caller owns the retained serializable Holding transaction. The query is
//! a bounded read projection over the current Commerce order header and the
//! Finance customer display name already used by the retained Sales documents
//! page. It never creates, revises or deletes an order, customer or posting row.
use crate::application::sales_documents::{SalesDocumentReadFuture, SalesDocumentReadRepository};
use crate::domain::sales_documents::{SalesDocumentCursor, SalesDocumentKind, SalesDocumentRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_DOCUMENTS_SOURCE_INVALID",
        "The current sales document source is invalid.",
    )
}

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SALES_DOCUMENTS_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("SALES_DOCUMENTS_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgSalesDocumentRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSalesDocumentRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn page(
        &mut self,
        company: &CompanyId,
        branch_id: Option<&str>,
        department_id: Option<&str>,
        kind: SalesDocumentKind,
        search: &str,
        status: Option<&str>,
        cursor: Option<&SalesDocumentCursor>,
        limit: usize,
    ) -> Result<Vec<SalesDocumentRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.number,\
             o.party_id,p.name AS party_name,o.state,o.order_date,o.due_date,o.currency,\
             o.subtotal_minor,o.tax_minor,(o.subtotal_minor+o.tax_minor) AS total_minor,\
             o.version,o.created_at \
             FROM commerce_orders o \
             JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
             WHERE o.company_id=$1 \
             AND ($2::text IS NULL OR o.branch_id=$2) \
             AND ($3::text IS NULL OR o.department_id=$3) \
             AND o.kind=$4 \
             AND ($5='' OR strpos(lower(o.number||' '||p.name||' '||o.state),lower($5))>0) \
             AND ($6::text IS NULL OR o.state=$6) \
             AND ($7::bigint IS NULL OR (o.created_at,o.id) < ($7,$8)) \
             ORDER BY o.created_at DESC,o.id DESC LIMIT $9",
        )
        .bind(company.as_str())
        .bind(branch_id)
        .bind(department_id)
        .bind(kind.as_str())
        .bind(search)
        .bind(status)
        .bind(cursor.map(|value| value.created_at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(SalesDocumentRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    kind: row.try_get("kind")?,
                    number: row.try_get("number")?,
                    party_id: row.try_get("party_id")?,
                    party_name: row
                        .try_get::<Option<String>, _>("party_name")?
                        .ok_or_else(source_invalid)?,
                    state: row.try_get("state")?,
                    order_date: row.try_get("order_date")?,
                    due_date: row.try_get("due_date")?,
                    currency: row.try_get("currency")?,
                    subtotal_minor: row.try_get("subtotal_minor")?,
                    tax_minor: row.try_get("tax_minor")?,
                    total_minor: row.try_get("total_minor")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }
}

impl SalesDocumentReadRepository for PgSalesDocumentRead<'_> {
    type Error = Error;

    #[allow(clippy::too_many_arguments)]
    fn page<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
        kind: SalesDocumentKind,
        search: &'a str,
        status: Option<&'a str>,
        cursor: Option<&'a SalesDocumentCursor>,
        limit: usize,
    ) -> SalesDocumentReadFuture<'a, Vec<SalesDocumentRow>, Self::Error> {
        Box::pin(async move {
            PgSalesDocumentRead::page(
                self,
                company,
                branch_id,
                department_id,
                kind,
                search,
                status,
                cursor,
                limit,
            )
            .await
        })
    }
}
