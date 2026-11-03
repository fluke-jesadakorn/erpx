//! Runtime owns the audit table. The caller retains source and IAM witnesses
//! on this same selected SERIALIZABLE unit, including after deferred effects.
use crate::domain::executive_access::*;
use crate::public::postgres::PersistenceError;
use sqlx::{Connection, PgConnection};
pub async fn record_executive_evidence_access(
    c: &mut PgConnection,
    input: ExecutiveEvidenceAccess<'_>,
) -> Result<(), PersistenceError> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new(
            "EXECUTIVE_ACCESS_TRANSACTION_REQUIRED",
        ));
    }
    if input.rows.is_empty() && matches!(input.action, EvidenceAccessAction::Search) {
        return Ok(());
    }
    if matches!(input.action, EvidenceAccessAction::Open) && input.rows.len() != 1
        || input.rows.len() > 50
        || input.query_hash.len() != 64
        || !input
            .query_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || input
            .rows
            .iter()
            .any(|row| row.id.is_empty() || row.version < 1)
    {
        return Err(PersistenceError::new("EXECUTIVE_ACCESS_INPUT_INVALID"));
    }
    let rows = serde_json::Value::Array(
        input
            .rows
            .iter()
            .map(|row| serde_json::json!({"record_id":row.id,"record_version":row.version}))
            .collect(),
    );
    sqlx::query("WITH events AS MATERIALIZED(SELECT 'executive-evidence-'||gen_random_uuid()::text id,$1::text company_id,$2::text actor_id,$3::text assignment_id,$4::text provider_id,$5::text metric_id,$6::text action,item.record_id,item.record_version,$7::jsonb field_ids,$8::text query_hash,$9::bigint created_at FROM jsonb_to_recordset($10::jsonb) AS item(record_id text,record_version bigint)),marker AS MATERIALIZED(SELECT set_config('erpx.native_field_evidence',jsonb_build_object('unit',pg_current_xact_id()::text,'rows',coalesce(jsonb_agg(to_jsonb(events)),'[]'::jsonb))::text,true) FROM events) INSERT INTO executive_evidence_access_events(id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at) SELECT events.* FROM events CROSS JOIN marker")
        .bind(input.company).bind(input.user).bind(input.assignment).bind(input.provider).bind(input.metric).bind(input.action.as_str()).bind(serde_json::json!(input.fields)).bind(input.query_hash).bind(input.at).bind(rows).execute(c).await?;
    Ok(())
}
