//! PostgreSQL adapter for the native Ecommerce listing draft/create/change
//! commands.
//!
//! SQL is Ecommerce-owned (`ecommerce_listings`, read with `ecommerce_channels`
//! for Company scope) plus the existing shared `command_receipts`, `audit_events`
//! and `outbox_events` records written through the same owner command boundary as
//! the channel slice. Product activity, the functional-currency configuration and
//! the unit-price Money snapshot are consumed through the injected Commerce and
//! Finance producer ports; this adapter never reads or writes another owner's
//! tables. The caller's serializable Holding transaction is required and no
//! commit is performed here.
use crate::adapters::command::PgEcommerceCommand;
use crate::application::listing::EcommerceListingRepository;
use crate::application::{
    AuditEntry, EcommerceCommandRepository, EcommerceFuture, OutboxEntry, ReceiptEntry,
};
use crate::domain::actor::EcommerceActor;
use crate::domain::command::CommandReceipt;
use crate::domain::listing::{
    EcommerceFunctionalCurrency, EcommerceListingMoneyRequest, EcommerceListingMoneySnapshot,
    ListingDraft, ListingRecord, ValidatedListingChange, listing_exists, money_source_unavailable,
    source_port_unavailable,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
use std::pin::Pin;

type Error = PgTransactionError<BusinessError>;

/// Commerce-owned source witness for one listing draft: an active product in the
/// current Company and the Company's configured functional currency.
pub trait PgEcommerceListingCommerceSource: Send + Sync {
    fn active_product<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BusinessError>> + Send + 'a>>;

    fn functional_currency<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        company: &'a CompanyId,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<EcommerceFunctionalCurrency>, BusinessError>>
                + Send
                + 'a,
        >,
    >;
}

/// Finance-owned `snapshotMoney` producer composed into the Ecommerce
/// transaction. The implementation resolves FX evidence, enforces the Company
/// functional currency and retains the `ecommerce_listing` unit-price row.
pub trait PgEcommerceListingMoneyCapture: Send + Sync {
    fn capture<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        now: TimestampMillis,
    ) -> Pin<Box<dyn Future<Output = Result<EcommerceListingMoneySnapshot, BusinessError>> + Send + 'a>>;
}

pub struct PgEcommerceListing<'c> {
    connection: &'c mut PgConnection,
    commerce: Option<&'c dyn PgEcommerceListingCommerceSource>,
    money: Option<&'c dyn PgEcommerceListingMoneyCapture>,
}

impl<'c> PgEcommerceListing<'c> {
    /// Without the Commerce/Finance producer ports creation fails closed; draft
    /// retitle and readback still work.
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            connection,
            commerce: None,
            money: None,
        }
    }

    pub fn with_sources(
        connection: &'c mut PgConnection,
        commerce: &'c dyn PgEcommerceListingCommerceSource,
        money: &'c dyn PgEcommerceListingMoneyCapture,
    ) -> Self {
        Self {
            connection,
            commerce: Some(commerce),
            money: Some(money),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("ECOMMERCE_LISTING_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("ECOMMERCE_LISTING_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    fn listing_record(row: sqlx::postgres::PgRow) -> Result<ListingRecord, Error> {
        Ok(ListingRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
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
    }
}

const SOURCE_SELECT: &str = "SELECT l.id,c.company_id,l.channel_id,l.product_id,l.title,l.currency,\
     l.price_minor,l.status,l.created_by,l.reviewed_by,l.version,l.created_at,l.updated_at \
     FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
     WHERE l.id=$1 AND c.company_id=$2 FOR UPDATE OF l,c";

const READBACK_SELECT: &str = "SELECT l.id,c.company_id,l.channel_id,l.product_id,l.title,l.currency,\
     l.price_minor,l.status,l.created_by,l.reviewed_by,l.version,l.created_at,l.updated_at \
     FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id \
     WHERE l.id=$1 AND c.company_id=$2";

impl EcommerceListingRepository for PgEcommerceListing<'_> {
    type Error = Error;

    fn digest<'a>(&'a mut self, value: &'a str) -> EcommerceFuture<'a, String, Self::Error> {
        Box::pin(async move {
            let mut command = PgEcommerceCommand::new(&mut *self.connection);
            EcommerceCommandRepository::digest(&mut command, value).await
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EcommerceFuture<'a, Option<CommandReceipt>, Self::Error> {
        Box::pin(async move {
            let mut command = PgEcommerceCommand::new(&mut *self.connection);
            EcommerceCommandRepository::receipt(&mut command, key).await
        })
    }

    fn active_channel<'a>(
        &'a mut self,
        company: &'a CompanyId,
        channel_id: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM ecommerce_channels \
                 WHERE id=$1 AND company_id=$2 AND status='active')",
            )
            .bind(channel_id)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn active_product<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let commerce = self.commerce.ok_or_else(source_port_unavailable)?;
            commerce
                .active_product(&mut *self.connection, company, product_id)
                .await
                .map_err(Into::into)
        })
    }

    fn functional_currency<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<EcommerceFunctionalCurrency>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let commerce = self.commerce.ok_or_else(source_port_unavailable)?;
            commerce
                .functional_currency(&mut *self.connection, company)
                .await
                .map_err(Into::into)
        })
    }

    fn capture_money<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        request: &'a EcommerceListingMoneyRequest,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, EcommerceListingMoneySnapshot, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let money = self.money.ok_or_else(money_source_unavailable)?;
            money
                .capture(&mut *self.connection, actor, request, now)
                .await
                .map_err(Into::into)
        })
    }

    fn insert_listing<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        listing: &'a ListingDraft,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let result = sqlx::query(
                "INSERT INTO ecommerce_listings\
                 (id,channel_id,product_id,title,currency,price_minor,status,created_by,\
                 reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'draft',$7,NULL,1,$8,$8) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&listing.id)
            .bind(&listing.channel_id)
            .bind(&listing.product_id)
            .bind(&listing.title)
            .bind(&listing.currency)
            .bind(listing.price_minor)
            .bind(actor.user_id.as_str())
            .bind(now.get())
            .execute(&mut *self.connection)
            .await;
            match result {
                Ok(done) => Ok(done.rows_affected() == 1),
                // The owner table keeps one listing per channel/product pair.
                Err(error)
                    if error
                        .as_database_error()
                        .and_then(|error| error.code())
                        .as_deref()
                        == Some("23505") =>
                {
                    Err(listing_exists().into())
                }
                Err(error) => Err(error.into()),
            }
        })
    }

    fn listing_source<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(SOURCE_SELECT)
                .bind(id)
                .bind(company.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(Self::listing_record).transpose()
        })
    }

    fn commit_listing_change<'a>(
        &'a mut self,
        change: &'a ValidatedListingChange,
        company: &'a CompanyId,
        actor: &'a EcommerceActor,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let changed = sqlx::query(
                "UPDATE ecommerce_listings l SET title=$3,version=l.version+1,updated_at=$4 \
                 FROM ecommerce_channels c \
                 WHERE l.channel_id=c.id AND l.id=$1 AND c.company_id=$2 \
                 AND l.version=$5 AND l.status='draft' AND l.created_by=$6",
            )
            .bind(&change.id)
            .bind(company.as_str())
            .bind(&change.title)
            .bind(now.get())
            .bind(change.expected_version)
            .bind(actor.user_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn listing_readback<'a>(
        &'a mut self,
        id: &'a str,
        company: &'a CompanyId,
    ) -> EcommerceFuture<'a, Option<ListingRecord>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(READBACK_SELECT)
                .bind(id)
                .bind(company.as_str())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(Self::listing_record).transpose()
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            let mut command = PgEcommerceCommand::new(&mut *self.connection);
            EcommerceCommandRepository::insert_receipt(&mut command, entry, now).await
        })
    }

    fn audit<'a>(
        &'a mut self,
        actor: &'a EcommerceActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut command = PgEcommerceCommand::new(&mut *self.connection);
            EcommerceCommandRepository::audit(&mut command, actor, entry, now).await
        })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> EcommerceFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut command = PgEcommerceCommand::new(&mut *self.connection);
            EcommerceCommandRepository::outbox(&mut command, entry, now).await
        })
    }
}
