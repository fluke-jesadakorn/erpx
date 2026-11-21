//! PostgreSQL read adapter for the native Manufacturing BOM register.
//!
//! The caller owns the retained serializable Holding transaction. The queries
//! read only Manufacturing-owned specification, component-line, routing and
//! resource relations; Commerce product labels and other owner facts are not
//! joined here. No specification, line, operation or resource row is created,
//! reviewed or deleted by this read.
use crate::application::bom::{BomReadFuture, BomReadRepository};
use crate::domain::bom::{
    BomLine, BomOperation, BomRegister, BomSpecification, MAX_LINES_PER_SPECIFICATION,
    MAX_OPERATIONS_PER_SPECIFICATION, MAX_SPECIFICATIONS,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeMap;

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("MANUFACTURING_BOM_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("MANUFACTURING_BOM_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgBomRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBomRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn register(&mut self, company: &CompanyId) -> Result<BomRegister, Error> {
        serializable(&mut *self.connection).await?;

        let specification_rows = sqlx::query(
            "SELECT id,company_id,name,mode,output_product_id,batch_quantity_milli,\
             expected_yield_bps,status,created_by,reviewed_by,version,created_at,updated_at \
             FROM manufacturing_specifications \
             WHERE company_id=$1 ORDER BY updated_at DESC,id DESC LIMIT $2",
        )
        .bind(company.as_str())
        .bind(MAX_SPECIFICATIONS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let line_rows = sqlx::query(
            "SELECT line.id,line.specification_id,line.product_id,line.quantity_milli,\
             line.substitution_group,line.ordinal \
             FROM manufacturing_specification_lines line \
             JOIN manufacturing_specifications specification \
               ON specification.id=line.specification_id \
             WHERE specification.company_id=$1 \
             ORDER BY line.specification_id,line.ordinal,line.id LIMIT $2",
        )
        .bind(company.as_str())
        .bind(MAX_LINES_PER_SPECIFICATION as i64 * MAX_SPECIFICATIONS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let operation_rows = sqlx::query(
            "SELECT operation.id,operation.specification_id,operation.resource_id,\
             resource.name AS resource_name,operation.name,operation.setup_minutes,\
             operation.run_minutes_per_batch,operation.ordinal \
             FROM manufacturing_specification_operations operation \
             JOIN manufacturing_specifications specification \
               ON specification.id=operation.specification_id \
             JOIN production_resources resource ON resource.id=operation.resource_id \
             WHERE specification.company_id=$1 \
             ORDER BY operation.specification_id,operation.ordinal,operation.id LIMIT $2",
        )
        .bind(company.as_str())
        .bind(MAX_OPERATIONS_PER_SPECIFICATION as i64 * MAX_SPECIFICATIONS as i64 + 1)
        .fetch_all(&mut *self.connection)
        .await?;

        let mut lines: BTreeMap<String, Vec<BomLine>> = BTreeMap::new();
        for row in line_rows {
            let specification_id: String = row.try_get("specification_id")?;
            lines.entry(specification_id).or_default().push(BomLine {
                id: row.try_get("id")?,
                specification_id: row.try_get("specification_id")?,
                product_id: row.try_get("product_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                substitution_group: row.try_get("substitution_group")?,
                ordinal: row.try_get("ordinal")?,
            });
        }

        let mut operations: BTreeMap<String, Vec<BomOperation>> = BTreeMap::new();
        for row in operation_rows {
            let specification_id: String = row.try_get("specification_id")?;
            operations
                .entry(specification_id)
                .or_default()
                .push(BomOperation {
                    id: row.try_get("id")?,
                    specification_id: row.try_get("specification_id")?,
                    resource_id: row.try_get("resource_id")?,
                    resource_name: row.try_get("resource_name")?,
                    name: row.try_get("name")?,
                    setup_minutes: row.try_get("setup_minutes")?,
                    run_minutes_per_batch: row.try_get("run_minutes_per_batch")?,
                    ordinal: row.try_get("ordinal")?,
                });
        }

        let specifications = specification_rows
            .into_iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                Ok(BomSpecification {
                    lines: lines.remove(&id).unwrap_or_default(),
                    operations: operations.remove(&id).unwrap_or_default(),
                    id,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    mode: row.try_get("mode")?,
                    output_product_id: row.try_get("output_product_id")?,
                    batch_quantity_milli: row.try_get("batch_quantity_milli")?,
                    expected_yield_bps: row.try_get("expected_yield_bps")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(BomRegister { specifications })
    }
}

impl BomReadRepository for PgBomRead<'_> {
    type Error = Error;

    fn specifications<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> BomReadFuture<'a, Vec<BomSpecification>, Self::Error> {
        Box::pin(async move { Ok(PgBomRead::register(self, company).await?.specifications) })
    }
}
