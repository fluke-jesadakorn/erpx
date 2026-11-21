//! PostgreSQL read adapter for the native Ecommerce storefront catalog.
//!
//! Only `ecommerce_channels` and `ecommerce_listings` are read. Product,
//! customer, warehouse, Sales-order and package facts remain on their public
//! owner reads. The caller owns the retained serializable Holding transaction;
//! this adapter never creates, reviews, opens a cart or posts a cross-owner row.
use crate::application::{EcommerceCatalogRepository, EcommerceFuture};
use crate::domain::catalog::{CATALOG_LIMIT, EcommerceChannelRow, EcommerceListingRow};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("ECOMMERCE_CATALOG_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ECOMMERCE_CATALOG_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgEcommerceCatalogRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgEcommerceCatalogRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn channels(
        &mut self,
        company: &CompanyId,
    ) -> Result<Vec<EcommerceChannelRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT id,company_id,name,channel_type,configuration_json,status,created_by,\
             reviewed_by,version,created_at,updated_at \
             FROM ecommerce_channels WHERE company_id=$1 \
             ORDER BY updated_at DESC,id DESC LIMIT $2",
        )
        .bind(company.as_str())
        .bind((CATALOG_LIMIT + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(EcommerceChannelRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    channel_type: row.try_get("channel_type")?,
                    configuration_json: row.try_get("configuration_json")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    pub async fn listings(
        &mut self,
        company: &CompanyId,
    ) -> Result<Vec<EcommerceListingRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "SELECT l.id,l.channel_id,l.product_id,l.title,l.currency,l.price_minor,l.status,\
             l.created_by,l.reviewed_by,l.version,l.created_at,l.updated_at \
             FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
             WHERE c.company_id=$1 ORDER BY l.updated_at DESC,l.id DESC LIMIT $2",
        )
        .bind(company.as_str())
        .bind((CATALOG_LIMIT + 1) as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(EcommerceListingRow {
                    id: row.try_get("id")?,
                    channel_id: row.try_get("channel_id")?,
                    product_id: row.try_get("product_id")?,
                    title: row.try_get("title")?,
                    currency: row.try_get("currency")?,
                    price_minor: row.try_get("price_minor")?,
                    status: row.try_get("status")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }
}

impl EcommerceCatalogRepository for PgEcommerceCatalogRead<'_> {
    type Error = Error;

    fn channels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceChannelRow>, Self::Error> {
        Box::pin(async move { PgEcommerceCatalogRead::channels(self, company).await })
    }

    fn listings<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Vec<EcommerceListingRow>, Self::Error> {
        Box::pin(async move { PgEcommerceCatalogRead::listings(self, company).await })
    }
}
