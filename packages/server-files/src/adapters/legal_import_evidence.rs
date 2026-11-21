//! Files-owned legal import evidence. The port validates and binds one draft
//! attachment link for a Legal document or signed-original import. It reads no
//! Legal-owned relation and grants no Legal record authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalImportEvidence {
    pub link_id: String,
    pub set_id: String,
    pub source_id: String,
    pub object_key: String,
    pub checksum_sha256: String,
    pub media_type: String,
    pub original_name: String,
    pub bytes: i64,
}

fn invalid() -> Error {
    BusinessError::new(
        FailureKind::InvalidInput,
        "LEGAL_IMPORT_EVIDENCE_INVALID",
        "Choose one ready attachment uploaded for this Legal record.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_IMPORT_EVIDENCE_CHANGED",
        "The Legal import attachment changed. Reload and retry.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("LEGAL_IMPORT_EVIDENCE_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text",
    )
    .fetch_one(c)
    .await?)
}
/// Validate one draft link, its ready attachment and exact Legal set target.
/// Rows are locked until the caller commits, so the bind cannot race a withdrawal.
pub async fn inspect_legal_import_evidence(
    c: &mut PgConnection,
    company: &CompanyId,
    item_id: &str,
    link_id: &str,
) -> Result<LegalImportEvidence, Error> {
    if item_id.is_empty() || item_id.len() > 200 || link_id.is_empty() || link_id.len() > 200 {
        return Err(invalid());
    }
    unit(c).await?;
    let row = sqlx::query(
        "SELECT l.id,l.set_id,l.attachment_id,l.state link_state,\
                a.source_state,a.object_key,a.checksum_sha256,a.media_type,a.original_name,a.bytes,\
                s.state set_state,s.target_type,s.target_id,s.purpose_key,s.component_kind \
         FROM attachment_links l \
         JOIN attachments a ON a.id=l.attachment_id AND a.company_id=$2 \
         JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=$2 \
         WHERE l.id=$1 FOR UPDATE OF l,a,s",
    )
    .bind(link_id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(invalid)?;
    if row.try_get::<String, _>("link_state")? != "draft"
        || row.try_get::<String, _>("set_state")? != "draft"
        || row.try_get::<String, _>("source_state")? != "available"
        || row.try_get::<String, _>("target_type")? != "legal_item"
        || row.try_get::<String, _>("target_id")? != item_id
        || row.try_get::<String, _>("purpose_key")? != "legal_document"
        || row.try_get::<String, _>("component_kind")? != "record"
    {
        return Err(invalid());
    }
    Ok(LegalImportEvidence {
        link_id: row.try_get("id")?,
        set_id: row.try_get("set_id")?,
        source_id: row.try_get("attachment_id")?,
        object_key: row.try_get("object_key")?,
        checksum_sha256: row.try_get("checksum_sha256")?,
        media_type: row.try_get("media_type")?,
        original_name: row.try_get("original_name")?,
        bytes: row.try_get("bytes")?,
    })
}
impl LegalImportEvidence {
    pub async fn bind(
        &self,
        c: &mut PgConnection,
        company: &CompanyId,
        actor: &str,
        command_key: &str,
        binding_type: &str,
        binding_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        if actor.is_empty()
            || actor.len() > 1024
            || command_key.is_empty()
            || command_key.len() > 200
            || !matches!(binding_type, "legal_document" | "legal_executed_copy")
            || binding_id.is_empty()
            || binding_id.len() > 200
            || now < 0
        {
            return Err(invalid());
        }
        let link = sqlx::query(
            "UPDATE attachment_links SET state='bound',bound_at=$1,binding_type=$2,binding_id=$3 \
             WHERE id=$4 AND set_id=$5 AND state='draft'",
        )
        .bind(now)
        .bind(binding_type)
        .bind(binding_id)
        .bind(&self.link_id)
        .bind(&self.set_id)
        .execute(&mut *c)
        .await?;
        let set = sqlx::query(
            "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 \
             WHERE id=$3 AND company_id=$4 AND state='draft'",
        )
        .bind(now)
        .bind(command_key)
        .bind(&self.set_id)
        .bind(company.as_str())
        .execute(&mut *c)
        .await?;
        if link.rows_affected() != 1 || set.rows_affected() != 1 {
            return Err(changed());
        }
        sqlx::query(
            "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) \
             VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
        )
        .bind(event_id(command_key, &self.set_id))
        .bind(company.as_str())
        .bind(&self.set_id)
        .bind(actor)
        .bind(
            json!({
                "bindingType":binding_type,
                "bindingId":binding_id,
                "links":[{
                    "linkId":self.link_id,
                    "sourceId":self.source_id,
                    "purposeKey":"legal_document",
                    "checksumSha256":self.checksum_sha256,
                    "mediaType":self.media_type,
                    "bytes":self.bytes,
                }]
            })
            .to_string(),
        )
        .bind(now)
        .execute(&mut *c)
        .await?;
        Ok(())
    }
}
fn event_id(command_key: &str, set_id: &str) -> String {
    let digest = Sha256::digest(format!("{command_key}\0{set_id}").as_bytes());
    format!("attachment-bind-{:x}", digest)[..48].to_owned()
}
