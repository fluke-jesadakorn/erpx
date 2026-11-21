//! The actual immutable Inventory source set for Service completion.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
pub struct CurrentServiceCompletionConsumptions {
    order: String,
    journals: Vec<String>,
    digest: String,
    unit: Unit,
}
impl CurrentServiceCompletionConsumptions {
    pub fn journal_ids(&self) -> &[String] {
        &self.journals
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let (journals, digest) = read(c, &self.order).await?;
        if journals != self.journals || digest != self.digest {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_completion_consumptions(
    c: &mut PgConnection,
    order: &str,
) -> Result<CurrentServiceCompletionConsumptions, Error> {
    let unit = unit(c).await?;
    let (journals, digest) = read(c, order).await?;
    Ok(CurrentServiceCompletionConsumptions {
        order: order.into(),
        journals,
        digest,
        unit,
    })
}
async fn read(c: &mut PgConnection, order: &str) -> Result<(Vec<String>, String), Error> {
    let r=sqlx::query("SELECT CASE WHEN coalesce(sum(octet_length(journal_id)::bigint+16),0)<=2097152 THEN array_agg(journal_id ORDER BY id) ELSE NULL END ids,count(*)::bigint n,encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(p) ORDER BY id),'[]'::jsonb)::text,'UTF8')),'hex') digest FROM operational_inventory_consumptions p WHERE source_kind='field_service' AND source_id=$1").bind(order).fetch_one(c).await?;
    let count: i64 = r.try_get("n")?;
    let ids: Option<Vec<String>> = r.try_get("ids")?;
    let journals = if count == 0 {
        vec![]
    } else {
        ids.ok_or_else(invalid)?
    };
    Ok((journals, r.try_get("digest")?))
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("INVENTORY_SERVICE_COMPLETION_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_SOURCE_CHANGED",
        "The original Service consumption set changed.",
    )
    .into()
}
