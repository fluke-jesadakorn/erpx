//! Bounded structural labels for an already-authorized Commercial operator.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;
use sqlx::{Connection, PgConnection, Row};
#[derive(Serialize)]
pub struct CommercialCompany {
    pub id: String,
    pub slug: String,
    pub name: String,
}
pub async fn read_commercial_companies(
    c: &mut PgConnection,
) -> Result<Vec<CommercialCompany>, PgTransactionError<BusinessError>> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ORGANIZATION_TRANSACTION_REQUIRED").into());
    }
    let rows=sqlx::query("SELECT CASE WHEN octet_length(id)<=128 THEN id END id,CASE WHEN octet_length(slug)<=128 THEN slug END slug,CASE WHEN octet_length(display_name)<=8192 THEN display_name END name FROM organization_nodes WHERE kind='company' AND status='active' ORDER BY display_name,id LIMIT 1001").fetch_all(c).await?;
    let limit = || {
        BusinessError::new(
            FailureKind::Unavailable,
            "COMMERCIAL_COMPANY_LABEL_LIMIT",
            "Company labels exceed this bounded read.",
        )
    };
    if rows.len() > 1000 {
        return Err(limit().into());
    }
    rows.into_iter()
        .map(|r| {
            Ok(CommercialCompany {
                id: r.try_get::<Option<String>, _>("id")?.ok_or_else(limit)?,
                slug: r.try_get::<Option<String>, _>("slug")?.ok_or_else(limit)?,
                name: r.try_get::<Option<String>, _>("name")?.ok_or_else(limit)?,
            })
        })
        .collect()
}
