//! Files-owned bounded set/manifest witness and its bind mutation.
use crate::domain::field_visit_bind::{FieldBindManifestItem, FieldBindSet};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "File set not found.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_SET_CHANGED",
        "The original file set changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let level: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if level != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<(FieldBindSet, String, String), Error> {
    let row=sqlx::query("WITH src AS MATERIALIZED (SELECT s.* FROM attachment_sets s WHERE s.id=$1 AND s.company_id=$2), links AS MATERIALIZED (SELECT l.*,to_jsonb(a) attachment,a.source_state,a.checksum_sha256,a.media_type,a.bytes FROM attachment_links l JOIN src s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id), valueset AS (SELECT jsonb_build_object('id',s.id,'branch_id',s.branch_id,'department_id',s.department_id,'target_id',s.target_id,'target_type',s.target_type,'component_kind',s.component_kind,'component_ref',s.component_ref,'purpose_key',s.purpose_key,'expected_version',s.expected_target_version,'policy_version',s.policy_version,'required_count',s.required_count,'state',s.state,'bound_command_id',s.bound_command_id,'links',coalesce((SELECT jsonb_agg(jsonb_build_object('id',l.id,'state',l.state,'source_id',l.attachment_id,'source_state',l.source_state,'checksum',l.checksum_sha256,'media_type',l.media_type,'bytes',l.bytes,'binding_type',l.binding_type,'binding_id',l.binding_id) ORDER BY l.created_at,l.id) FROM links l),'[]'::jsonb)) payload,jsonb_build_array(to_jsonb(s),(SELECT coalesce(jsonb_agg(to_jsonb(l) ORDER BY l.created_at,l.id),'[]'::jsonb) FROM links l)) image,jsonb_build_array(to_jsonb(s)-'state'-'bound_at'-'bound_command_id'-'updated_at',(SELECT coalesce(jsonb_agg(to_jsonb(l)-'state'-'bound_at'-'binding_type'-'binding_id' ORDER BY l.created_at,l.id),'[]'::jsonb) FROM links l)) stable FROM src s) SELECT CASE WHEN octet_length(payload::text)+octet_length(image::text)<=2097152 THEN payload::text END payload,encode(sha256(convert_to(image::text,'UTF8')),'hex') digest,encode(sha256(convert_to(stable::text,'UTF8')),'hex') stable FROM valueset").bind(id).bind(company.as_str()).fetch_optional(c).await?.ok_or_else(missing)?;
    let raw: Option<String> = row.try_get("payload")?;
    let data: FieldBindSet =
        serde_json::from_str(&raw.ok_or_else(changed)?).map_err(|_| changed())?;
    if !crate::domain::field_visit_link::valid_field_purpose(
        &data.target_type,
        &data.component_kind,
        &data.purpose_key,
    ) {
        return Err(missing());
    }
    Ok((data, row.try_get("digest")?, row.try_get("stable")?))
}
pub struct CurrentFieldBindSet {
    company: CompanyId,
    unit: (String, i32, String, String),
    data: FieldBindSet,
    digest: String,
    stable: String,
}
impl CurrentFieldBindSet {
    pub fn data(&self) -> &FieldBindSet {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.data.id).await?
                != (self.data.clone(), self.digest.clone(), self.stable.clone())
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply(
        &self,
        c: &mut PgConnection,
        manifest: &[FieldBindManifestItem],
        actor: &str,
        key: &str,
        now: i64,
    ) -> Result<(), Error> {
        self.require_current(c).await?;
        for item in manifest {
            let result=sqlx::query("UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='record_document',binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'").bind(now).bind(&self.data.target_id).bind(&item.link_id).bind(&self.data.id).execute(&mut *c).await?;
            if result.rows_affected() != 1 {
                return Err(changed());
            }
        }
        let result=sqlx::query("UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'").bind(now).bind(key).bind(&self.data.id).bind(self.company.as_str()).execute(&mut *c).await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        let hash = hex::encode(Sha256::digest(
            format!("{}\0{}", key, self.data.id).as_bytes(),
        ));
        let event = format!("attachment-bind-{}", &hash[..32]);
        let details=serde_json::json!({"bindingType":"record_document","bindingId":self.data.target_id,"manifest":manifest}).to_string();
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.bound',$5,$6)").bind(event).bind(self.company.as_str()).bind(&self.data.id).bind(actor).bind(details).bind(now).execute(&mut *c).await?;
        let (after, _, stable) = image(c, &self.company, &self.data.id).await?;
        let mut expected = self.data.clone();
        expected.state = "bound".into();
        expected.bound_command_id = Some(key.into());
        for item in manifest {
            let Some(link) = expected.links.iter_mut().find(|l| l.id == item.link_id) else {
                return Err(changed());
            };
            link.state = "bound".into();
            link.binding_type = Some("record_document".into());
            link.binding_id = Some(self.data.target_id.clone());
        }
        if after != expected || stable != self.stable {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_field_bind_set(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentFieldBindSet, Error> {
    let unit = unit(c).await?;
    let (data, digest, stable) = image(c, company, id).await?;
    Ok(CurrentFieldBindSet {
        company: company.clone(),
        unit,
        data,
        digest,
        stable,
    })
}
/// Receipt replay is admitted only against the actual bound Files evidence.
pub async fn verify_field_bind_replay(
    c: &mut PgConnection,
    source: &CurrentFieldBindSet,
    key: &str,
    actor: &str,
    manifest: &[FieldBindManifestItem],
) -> Result<(), Error> {
    source.require_current(c).await?;
    let set = source.data();
    if set.state != "bound" || set.bound_command_id.as_deref() != Some(key) || manifest.is_empty() {
        return Err(changed());
    }
    let bound: Vec<_> = set
        .links
        .iter()
        .filter(|l| {
            l.state == "bound"
                && l.binding_type.as_deref() == Some("record_document")
                && l.binding_id.as_deref() == Some(&set.target_id)
        })
        .collect();
    if bound.len() != manifest.len() {
        return Err(changed());
    }
    for (l, m) in bound.iter().zip(manifest) {
        if l.id != m.link_id
            || l.source_id.as_deref() != Some(&m.source_id)
            || l.checksum.as_deref() != Some(&m.checksum_sha256)
            || l.media_type.as_deref() != Some(&m.media_type)
            || l.bytes != Some(m.bytes)
            || m.purpose_key != set.purpose_key
        {
            return Err(changed());
        }
    }
    let hash = hex::encode(Sha256::digest(format!("{}\0{}", key, set.id).as_bytes()));
    let event = format!("attachment-bind-{}", &hash[..32]);
    let details = serde_json::json!({"bindingType":"record_document","bindingId":set.target_id,"manifest":manifest});
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1 AND company_id=$2 AND set_id=$3 AND actor_id=$4 AND action='set.bound' AND details_json::jsonb=$5)").bind(event).bind(source.company.as_str()).bind(&set.id).bind(actor).bind(details).fetch_one(c).await?;
    if !valid {
        return Err(changed());
    }
    Ok(())
}
