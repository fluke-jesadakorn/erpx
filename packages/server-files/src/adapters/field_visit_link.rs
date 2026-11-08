//! Files-owned current link/set/source image and atomic withdrawal effects.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
pub async fn lock_field_visit_link_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE attachment_links, attachment_sets IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE attachments, attachment_scan_attempts IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// Require the already acquired Files fence without obtaining or upgrading it.
/// It proves neither snapshot timing nor authority to read a target.
pub async fn require_field_visit_link_sources_held(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());}
    let held:bool=sqlx::query_scalar("SELECT (SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('attachment_links'::regclass,'attachment_sets'::regclass) AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')) AND (SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation IN ('attachments'::regclass,'attachment_scan_attempts'::regclass) AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(c).await?;
    if !held { return Err(PersistenceError::new("FILES_FIELD_LINK_LOCKS_REQUIRED").into()); }
    Ok(())
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldVisitLink {
    pub id: String,
    pub set_id: String,
    pub target_type: String,
    pub target_id: String,
    pub component_kind: String,
    pub purpose_key: String,
    pub expected_target_version: i64,
    pub state: String,
    pub created_by: String,
    pub withdrawn_by: Option<String>,
    pub withdrawal_reason: Option<String>,
    pub attachment_id: Option<String>,
    pub source_valid: bool,
    pub source_state: Option<String>,
    pub preview_available: bool,
    pub expires_at: i64,
    pub bound_at: Option<i64>,
    pub withdrawn_at: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FieldVisitScan {
    pub attempt: i64,
    pub state: String,
    pub error_code: Option<String>,
    pub scanner_signature: Option<String>,
    pub scanner_definition_at: Option<i64>,
    pub completed_at: Option<i64>,
}
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_LINK_NOT_FOUND",
        "File link not found.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_LINK_LOCKED",
        "This file link is closed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_READ_LIMIT",
        "The file source exceeds its bounded read.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    // A rollback to a savepoint can release utility locks without changing
    // the transaction identity or snapshot. Check each owner's actual lock.
    require_field_visit_link_sources_held(&mut *c).await?;
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    owner: crate::public::field_visit_intake::IntakeOwner,
) -> Result<(FieldVisitLink, String, String), Error> {
    let row=sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(l)::text)+octet_length(to_jsonb(s)::text)+coalesce(octet_length(to_jsonb(a)::text),0)<=2097152 THEN jsonb_build_object('id',l.id,'set_id',l.set_id,'target_type',s.target_type,'target_id',s.target_id,'component_kind',s.component_kind,'purpose_key',s.purpose_key,'expected_target_version',s.expected_target_version,'state',l.state,'created_by',l.created_by,'withdrawn_by',l.withdrawn_by,'withdrawal_reason',l.withdrawal_reason,'attachment_id',l.attachment_id,'source_valid',(l.attachment_id IS NULL OR a.id IS NOT NULL),'source_state',a.source_state,'preview_available',a.preview_object_key IS NOT NULL,'expires_at',l.expires_at,'bound_at',l.bound_at,'withdrawn_at',l.withdrawn_at)::text END payload,encode(sha256(convert_to(jsonb_build_array(to_jsonb(l),to_jsonb(s),to_jsonb(a))::text,'UTF8')),'hex') digest,encode(sha256(convert_to(jsonb_build_array(to_jsonb(l)-'state'-'withdrawn_at'-'withdrawn_by'-'withdrawal_reason',to_jsonb(s),to_jsonb(a))::text,'UTF8')),'hex') stable_digest FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 FOR UPDATE OF l,s").bind(id).bind(company.as_str()).fetch_optional(c).await?.ok_or_else(missing)?;
    let payload: Option<String> = row.try_get("payload")?;
    let source: FieldVisitLink =
        serde_json::from_str(&payload.ok_or_else(limited)?).map_err(|_| limited())?;
    if !owner.supports(
        &source.target_type,
        &source.component_kind,
        &source.purpose_key,
    ) {
        return Err(missing());
    }
    Ok((
        source,
        row.try_get("digest")?,
        row.try_get("stable_digest")?,
    ))
}
pub struct CurrentFieldVisitLink {
    owner: crate::public::field_visit_intake::IntakeOwner,
    company: CompanyId,
    unit: (String, i32, String, String),
    source: FieldVisitLink,
    digest: String,
    stable_digest: String,
}
/// Retained exact withdrawal image; it grants no authorization.
pub struct RetainedFieldVisitWithdrawal {
    owner: crate::public::field_visit_intake::IntakeOwner, company: CompanyId,
    unit: (String,i32,String,String), expected: FieldVisitLink, stable_digest: String,
    event: Option<(String,String,String,i64)>, event_image: Option<(String,String)>,
}
impl RetainedFieldVisitWithdrawal {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await? != self.unit { return Err(changed()); }
        let (actual,_,stable)=image(c,&self.company,&self.expected.id,self.owner).await?;
        if actual != self.expected || stable != self.stable_digest {return Err(changed());}
        if let Some((id,expected))=&self.event_image {
            let actual:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM attachment_events e WHERE id=$1").bind(id).fetch_optional(&mut *c).await?;
            if actual.as_ref()!=Some(expected){return Err(changed());}
        }
        if let Some((event,actor,details,now))=&self.event {
            let intact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1 AND company_id=$2 AND attachment_id IS NOT DISTINCT FROM $3 AND link_id=$4 AND set_id=$5 AND actor_id=$6 AND action='link.withdrawn' AND details_json=$7 AND created_at=$8)").bind(event).bind(self.company.as_str()).bind(&self.expected.attachment_id).bind(&self.expected.id).bind(&self.expected.set_id).bind(actor).bind(details).bind(now).fetch_one(c).await?;
            if !intact{return Err(changed());}
        }
        Ok(())
    }
}
impl CurrentFieldVisitLink {
    pub fn source(&self) -> &FieldVisitLink {
        &self.source
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.source.id, self.owner).await?
                != (
                    self.source.clone(),
                    self.digest.clone(),
                    self.stable_digest.clone(),
                )
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn withdraw(
        &self,
        c: &mut PgConnection,
        actor: &str,
        reason: &str,
        key: &str,
        now: i64,
    ) -> Result<(), Error> {
        self.withdraw_retained(c,actor,reason,key,now).await.map(|_| ())
    }
    pub async fn withdraw_retained(
        &self, c:&mut PgConnection, actor:&str, reason:&str, key:&str, now:i64,
    ) -> Result<RetainedFieldVisitWithdrawal, Error> {
        self.require_current(c).await?;
        let decision = crate::application::field_visit_link::prepare_withdrawal(
            crate::application::field_visit_link::WithdrawalFacts {
                state: &self.source.state,
                creator: &self.source.created_by,
                withdrawn_by: self.source.withdrawn_by.as_deref(),
                previous_reason: self.source.withdrawal_reason.as_deref(),
                actor,
                reason,
            },
        )?;
        if decision == crate::domain::field_visit_link::WithdrawalDecision::Replay {
            return Ok(RetainedFieldVisitWithdrawal {owner:self.owner,company:self.company.clone(),unit:self.unit.clone(),expected:self.source.clone(),stable_digest:self.stable_digest.clone(),event:None,event_image:None});
        }
        let result=sqlx::query("UPDATE attachment_links SET state='withdrawn',withdrawn_at=$1,withdrawn_by=$2,withdrawal_reason=$3 WHERE id=$4 AND set_id=$5 AND state='draft'").bind(now).bind(actor).bind(reason).bind(&self.source.id).bind(&self.source.set_id).execute(&mut *c).await?;
        if result.rows_affected() != 1 {
            return Err(changed());
        }
        let hash = Sha256::digest(
            format!("{}\0{}\0{}", self.company.as_str(), self.source.id, key).as_bytes(),
        );
        let event = format!("attachment-withdraw-{}", &hex::encode(hash)[..32]);
        let details = serde_json::json!({"reason":reason}).to_string();
        let inserted=sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,$5,$6,'link.withdrawn',$7,$8) ON CONFLICT(id) DO NOTHING").bind(&event).bind(self.company.as_str()).bind(&self.source.attachment_id).bind(&self.source.id).bind(&self.source.set_id).bind(actor).bind(&details).bind(now).execute(&mut *c).await?;
        if inserted.rows_affected() == 1 {
            let intact: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1 AND company_id=$2 AND attachment_id IS NOT DISTINCT FROM $3 AND link_id=$4 AND set_id=$5 AND actor_id=$6 AND action='link.withdrawn' AND details_json=$7 AND created_at=$8)").bind(&event).bind(self.company.as_str()).bind(&self.source.attachment_id).bind(&self.source.id).bind(&self.source.set_id).bind(actor).bind(&details).bind(now).fetch_one(&mut *c).await?;
            if !intact {
                return Err(changed());
            }
        }
        let (actual, _, stable) = image(c, &self.company, &self.source.id, self.owner).await?;
        let mut expected = self.source.clone();
        expected.state = "withdrawn".into();
        expected.withdrawn_by = Some(actor.into());
        expected.withdrawal_reason = Some(reason.into());
        expected.withdrawn_at = Some(now);
        if actual != expected || stable != self.stable_digest {
            return Err(changed());
        }
        let event_digest:Option<String>=sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(e)::text,'UTF8')),'hex') FROM attachment_events e WHERE id=$1").bind(&event).fetch_optional(&mut *c).await?;
        let event_digest=event_digest.ok_or_else(changed)?;
        Ok(RetainedFieldVisitWithdrawal {owner:self.owner,company:self.company.clone(),unit:self.unit.clone(),expected,stable_digest:self.stable_digest.clone(),event:if inserted.rows_affected()==1 {Some((event.clone(),actor.to_owned(),details,now))} else {None},event_image:Some((event,event_digest))})
    }
}
pub async fn read_field_visit_link(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentFieldVisitLink, Error> {
    read_owner_link(
        c,
        company,
        id,
        crate::public::field_visit_intake::IntakeOwner::FieldVisit,
    )
    .await
}
async fn read_owner_link(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    owner: crate::public::field_visit_intake::IntakeOwner,
) -> Result<CurrentFieldVisitLink, Error> {
    let unit = unit(c).await?;
    let (source, digest, stable_digest) = image(c, company, id, owner).await?;
    Ok(CurrentFieldVisitLink {
        owner,
        company: company.clone(),
        unit,
        source,
        digest,
        stable_digest,
    })
}
/// Finite owner intake withdrawal. The same opaque authority that admitted
/// upload must prove the actual current record on this serializable unit.
pub async fn withdraw_owner_intake_link<
    A: crate::public::field_visit_intake::FieldIntakeAuthority,
>(
    c: &mut PgConnection,
    authority: &A,
    actor: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
    id: &str,
    key: &str,
    reason: &str,
    clock: &impl erpx_server_shared::public::Clock,
) -> Result<(), Error> {
    let link = read_owner_link(
        c,
        &actor.selection().workspace().company_id,
        id,
        authority.owner_kind(),
    )
    .await?;
    let source = link.source();
    crate::application::field_visit_link::check_withdrawal_actor(
        &source.state,
        &source.created_by,
        actor.user_id().as_str(),
    )?;
    let guard = crate::adapters::field_visit_intake::prepare_field_intake(
        c,
        authority,
        actor,
        &source.target_id,
        true,
        clock.now().get(),
    )
    .await?;
    guard.require_current(c, clock.now().get()).await?;
    let withdrawal = link.withdraw_retained(c, actor.user_id().as_str(), reason, key, clock.now().get())
        .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
    withdrawal.require_current(c).await?;
    guard.require_current(c, clock.now().get()).await?;
    Ok(())
}
pub async fn read_field_visit_scan(
    c: &mut PgConnection,
    id: Option<&str>,
) -> Result<Option<FieldVisitScan>, Error> {
    let Some(id) = id else { return Ok(None) };
    let value:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(a)::text)<=2097152 THEN to_jsonb(a)::text END FROM attachment_scan_attempts a WHERE attachment_id=$1 ORDER BY attempt DESC LIMIT 1").bind(id).fetch_optional(c).await?;
    value
        .map(|s| serde_json::from_str(&s.ok_or_else(limited)?).map_err(|_| limited()))
        .transpose()
}
/// Same-unit retained status audit; absence is explicit when the link has no attachment.
pub struct RetainedFieldStatusAccess {
    unit:(String,i32,String,String),
    event:Option<(String,CompanyId,String,String,String,i64)>,
}
impl RetainedFieldStatusAccess {
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error>{
        if unit(c).await? != self.unit {return Err(changed());}
        if let Some((event,company,attachment,link,actor,now))=&self.event {
            let intact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_access_events WHERE id=$1 AND company_id=$2 AND attachment_id=$3 AND link_id=$4 AND actor_id=$5 AND variant='status' AND outcome='allowed' AND reason IS NULL AND created_at=$6)").bind(event).bind(company.as_str()).bind(attachment).bind(link).bind(actor).bind(now).fetch_one(c).await?;
            if !intact{return Err(changed());}
        }
        Ok(())
    }
}
pub async fn write_status_access_retained(
    c:&mut PgConnection,company:&CompanyId,link:&FieldVisitLink,actor:&str,now:i64,
)->Result<RetainedFieldStatusAccess,Error>{
    let transaction=unit(&mut *c).await?;
    let event=if let Some(id)=&link.attachment_id {
        let event=format!("attachment-access-{}",uuid::Uuid::new_v4());
        sqlx::query("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,'status','allowed',$6)").bind(&event).bind(company.as_str()).bind(id).bind(&link.id).bind(actor).bind(now).execute(&mut *c).await?;
        Some((event,company.clone(),id.clone(),link.id.clone(),actor.to_owned(),now))
    }else{None};
    let retained=RetainedFieldStatusAccess{unit:transaction,event};
    retained.require_current(c).await?;
    Ok(retained)
}
pub async fn write_status_access(
    c:&mut PgConnection,company:&CompanyId,link:&FieldVisitLink,actor:&str,now:i64,
)->Result<(),Error>{
    write_status_access_retained(c,company,link,actor,now).await.map(|_|())
}
