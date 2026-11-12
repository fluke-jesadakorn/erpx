//! PMS-owned read ports for the Workflow PMS social projector.
//!
//! Exact port of the `payoutSource` and `source` units in
//! `packages/server-pms/src/adapters/pms/social-events.ts`. Both statements run
//! on the caller's `&mut sqlx::PgConnection`; this adapter never opens or
//! commits a transaction.
use crate::domain::social_sources::PMS_SOURCES_SQL;
use erpx_server_runtime::public::postgres::PersistenceError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{PgConnection, Row};

/// One row from the retained `pmsSourcesSql` union.
#[derive(Clone, Debug, PartialEq)]
pub struct PmsSocialSource {
    pub id: String,
    pub kind: String,
    pub company_id: String,
    pub branch_id: String,
    pub property_name: String,
    pub business_date: String,
    pub data_json: Value,
    pub version: i64,
}

/// Exact retained payout source lookup for `pms.refund.pay`.
pub async fn pms_social_payout_source(
    connection: &mut PgConnection,
    id: &str,
    property_id: &str,
) -> Result<Option<String>, BusinessError> {
    let row = sqlx::query(
        "SELECT data_json::jsonb->>'originalId' id FROM pms_records \
         WHERE id=$1 AND property_id=$2 AND kind='entry'",
    )
    .bind(id)
    .bind(property_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(read_failed)?;
    row.map(|row| row.try_get::<Option<String>, _>("id").map_err(read_failed))
        .transpose()
        .map(Option::flatten)
}

/// Exact retained source lookup over the `pmsSourcesSql` union.
pub async fn pms_social_source(
    connection: &mut PgConnection,
    id: Option<&str>,
    company_id: &str,
    property_id: &str,
) -> Result<Option<PmsSocialSource>, BusinessError> {
    let sql = format!(
        "SELECT pr.id,pr.property_id,pr.kind,pr.data data_json,pr.version,pr.company_id,pr.branch_id,\
         pr.property_name,pr.business_date FROM ({PMS_SOURCES_SQL}) pr \
         WHERE pr.id=$1 AND pr.company_id=$2 AND pr.property_id=$3"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(company_id)
        .bind(property_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(read_failed)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(PmsSocialSource {
        id: row.try_get("id").map_err(read_failed)?,
        kind: row.try_get("kind").map_err(read_failed)?,
        company_id: row.try_get("company_id").map_err(read_failed)?,
        branch_id: row.try_get("branch_id").map_err(read_failed)?,
        property_name: row.try_get("property_name").map_err(read_failed)?,
        business_date: row.try_get("business_date").map_err(read_failed)?,
        data_json: row.try_get("data_json").map_err(read_failed)?,
        version: row.try_get("version").map_err(read_failed)?,
    }))
}

fn read_failed(error: sqlx::Error) -> BusinessError {
    let persistence = PersistenceError::from(error);
    if persistence.retryable() {
        BusinessError::new(
            FailureKind::Conflict,
            "PMS_SOCIAL_RETRY_REQUIRED",
            "The PMS social source must be retried.",
        )
    } else {
        BusinessError::new(
            FailureKind::Unavailable,
            "PMS_SOCIAL_READ_FAILED",
            "The PMS social source could not be read.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_union_keeps_all_three_retained_branches() {
        assert_eq!(PMS_SOURCES_SQL.matches("UNION ALL").count(), 2);
        assert!(PMS_SOURCES_SQL.contains("FROM pms_records r JOIN pms_properties p"));
        assert!(PMS_SOURCES_SQL.contains("FROM pms_transfer_requests t"));
        assert!(PMS_SOURCES_SQL.contains("FROM pms_charge_corrections c"));
    }
}
