//! Postgres adapter for the native Customer Authority write slice. The SQL is
//! owned by this crate; the caller executes the returned statement in its own
//! serializable unit. No second transaction and no TS fallback.
use crate::{
    application::customer_authority_wave3::{
        CustomerAuthorityWave3Command, plan_customer_authority_wave3,
    },
    domain::customer_authority_wave3::CustomerAuthorityWave3Statement,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;

pub async fn customer_authority_wave3_statement(
    connection: &mut PgConnection,
    command: &CustomerAuthorityWave3Command,
) -> Result<CustomerAuthorityWave3Statement, PgTransactionError<BusinessError>> {
    let _ = connection;
    plan_customer_authority_wave3(command).map_err(PgTransactionError::Business)
}
