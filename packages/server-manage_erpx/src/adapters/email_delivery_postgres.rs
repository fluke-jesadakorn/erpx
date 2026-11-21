//! PostgreSQL adapter for the native sandbox email delivery reservation. The
//! caller owns one Control transaction; all SQL lives here in the owner crate.
use crate::application::email_delivery::EmailDeliveryRepository;
use crate::domain::email_delivery::EmailDeliveryInput;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

pub type EmailDeliveryError = PgTransactionError<BusinessError>;

pub struct PgEmailDelivery<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgEmailDelivery<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }
}

impl EmailDeliveryRepository for PgEmailDelivery<'_> {
    type Error = EmailDeliveryError;

    async fn reserve(&mut self, input: &EmailDeliveryInput) -> Result<(), Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("EMAIL_DELIVERY_TRANSACTION_REQUIRED").into());
        }
        sqlx::query(
            "INSERT INTO sandbox_email_deliveries(id,delivery_key,recipient,subject,body_text,\
             body_ciphertext,provider_id,created_at,metadata_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9::jsonb) ON CONFLICT(delivery_key) DO NOTHING",
        )
        .bind(&input.id)
        .bind(&input.delivery_key)
        .bind(&input.recipient)
        .bind(&input.subject)
        .bind(&input.body_text)
        .bind(&input.body_ciphertext)
        .bind(&input.provider_id)
        .bind(input.created_at)
        .bind(&input.metadata_json)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}
