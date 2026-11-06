//! PostgreSQL read adapter for the native Customer 360 customer directory.
use crate::application::directory::{CustomerDirectoryFuture, CustomerDirectoryRepository};
use crate::domain::directory::{CustomerDirectoryEntry, CustomerDirectoryView, directory_invalid};
use crate::domain::timeline::CustomerTimelineActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_DIRECTORY_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CUSTOMER_DIRECTORY_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCustomerDirectoryRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCustomerDirectoryRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn customers(
        &mut self,
        actor: &CustomerTimelineActor,
        view: CustomerDirectoryView,
    ) -> Result<Vec<CustomerDirectoryEntry>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "WITH scoped_orders AS (\
               SELECT o.id,o.party_id FROM commerce_orders o \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) AND o.kind IN ('sales','quote')\
             ), scoped_leads AS (\
               SELECT l.id,v.customer_id FROM crm_leads l \
               JOIN crm_opportunities v ON v.lead_id=l.id \
               WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$2) \
               AND ($3::text IS NULL OR l.department_id=$3)\
             ), scoped_service AS (\
               SELECT g.id,g.customer_id FROM field_service_agreements g \
               WHERE $4::boolean AND g.company_id=$1 AND ($2::text IS NULL OR g.branch_id=$2) \
               AND ($3::text IS NULL OR g.department_id=$3)\
             ), scoped_invoices AS (\
               SELECT i.id,i.party_id FROM finance_invoices i \
               WHERE $5::boolean AND i.company_id=$1\
             ), scoped_legal AS (\
               SELECT i.id,n.target_id order_id FROM legal_items i \
               JOIN legal_native_references n ON n.item_id=i.id \
               WHERE $6::boolean AND i.company_id=$1 AND ($2::text IS NULL OR i.branch_id=$2) \
               AND ($3::text IS NULL OR i.department_id=$3) \
               AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$7)) \
               AND ($8::text <> 'legal_requester' OR (i.kind='request' AND i.created_by=$7)) \
               AND n.target_module='commerce_order'\
             ) \
             SELECT p.id,p.name,p.code FROM finance_parties p \
             WHERE p.company_id=$1 AND p.kind='customer' \
             AND (EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id))) \
             ORDER BY p.name,p.id LIMIT 201",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
        .bind(actor.department_id.as_ref().map(|value| value.as_str()))
        .bind(view.field_service)
        .bind(view.finance)
        .bind(view.legal)
        .bind(actor.user_id.as_str())
        .bind(actor.role.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = CustomerDirectoryEntry {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                code: row.try_get("code")?,
            };
            entry.validate().map_err(PgTransactionError::Business)?;
            entries.push(entry);
        }
        if entries.len() > 201 {
            return Err(directory_invalid().into());
        }
        Ok(entries)
    }
}

impl CustomerDirectoryRepository for PgCustomerDirectoryRead<'_> {
    type Error = Error;

    fn customers<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerDirectoryView,
    ) -> CustomerDirectoryFuture<'a, Vec<CustomerDirectoryEntry>, Self::Error> {
        Box::pin(async move { PgCustomerDirectoryRead::customers(self, actor, view).await })
    }
}
