//! Collaboration-owned Quality notices. The enclosing composition must attest
//! the saved Quality hold and complete current Identity recipient set.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const MESSAGE: &str = "Quality handoff: review the source record";
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_NOTIFICATION_CHANGED",
        "The Quality notification source changed.",
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
        return Err(PersistenceError::new("QUALITY_NOTIFICATION_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Utility-only write intent; acquire along with source locks before snapshot.
pub async fn lock_quality_notifications(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query(
        "LOCK TABLE staff_notifications,inbox_events,inbox_deliveries IN ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
/// Acquire before the transaction's first data snapshot, with other owner locks.
/// This serializes notification and inbox writers until retained verification.
pub async fn lock_quality_count_notices(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query("LOCK TABLE staff_notifications,inbox_events,inbox_deliveries IN SHARE ROW EXCLUSIVE MODE NOWAIT")
        .execute(c).await.map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}
async fn require_quality_count_fence(c: &mut PgConnection) -> Result<(), Error> {
    let held:bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=3 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation IN ('staff_notifications'::regclass,'inbox_events'::regclass,'inbox_deliveries'::regclass) AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')")
        .fetch_one(c).await?;
    if !held {
        return Err(changed());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityNotificationFacts {
    pub command_key: String,
    pub company_id: String,
    pub hold_id: String,
    pub version: i64,
    pub updated_at: i64,
    pub recipients: Vec<String>,
}
/// Concrete composition implementation must retain and recheck the saved
/// Quality after-image and exact complete Identity recipient witness in this
/// unit. Supplying these facts alone never proves either.
pub trait PgQualityNotificationSource: Sync {
    fn facts(&self) -> &QualityNotificationFacts;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
async fn conflicts(
    c: &mut PgConnection,
    f: &QualityNotificationFacts,
    recipient: &str,
) -> Result<Vec<Value>, Error> {
    let id = format!("{}-{}", f.command_key, recipient);
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(n) FROM staff_notifications n WHERE n.id=$1 OR (n.user_id=$2 AND n.record_type='quality_hold' AND n.record_id=$3 AND n.record_version=$4 AND n.message=$5) ORDER BY n.id FOR SHARE")
        .bind(id).bind(recipient).bind(&f.hold_id).bind(f.version).bind(MESSAGE).fetch_all(c).await?;
    Ok(rows)
}
async fn bridge(c: &mut PgConnection, id: &str) -> Result<(Option<Value>, Option<Value>), Error> {
    let event = sqlx::query_scalar("SELECT to_jsonb(e) FROM inbox_events e WHERE id=$1 FOR SHARE")
        .bind(format!("legacy:{id}"))
        .fetch_optional(&mut *c)
        .await?;
    let delivery =
        sqlx::query_scalar("SELECT to_jsonb(d) FROM inbox_deliveries d WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(c)
            .await?;
    Ok((event, delivery))
}
/// Run only after the actual Quality hold effect, inside the caller's retained
/// transaction. Existing ID or unique-tuple conflicts remain no-ops as in TS.
pub async fn stage_quality_notifications<S: PgQualityNotificationSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedQualityNotifications, Error> {
    source.require_current(c).await?;
    let u = unit(c).await?;
    let f = source.facts();
    if f.command_key.is_empty()
        || f.company_id.is_empty()
        || f.hold_id.is_empty()
        || f.version < 1
        || f.updated_at < 0
        || f.recipients.iter().any(|r| r.is_empty())
    {
        return Err(changed());
    }
    let mut seen = std::collections::BTreeSet::new();
    if f.recipients.iter().any(|r| !seen.insert(r)) {
        return Err(changed());
    }
    let mut retained = Vec::with_capacity(f.recipients.len());
    for recipient in &f.recipients {
        let id = format!("{}-{}", f.command_key, recipient);
        let before = conflicts(c, f, recipient).await?;
        let bridge_before = bridge(c, &id).await?;
        let inserted=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'quality_hold',$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(&id).bind(&f.company_id).bind(recipient).bind(&f.hold_id).bind(f.version).bind(MESSAGE).bind(f.updated_at).execute(&mut *c).await?.rows_affected();
        if inserted != u64::from(before.is_empty()) {
            return Err(changed());
        }
        let after = conflicts(c, f, recipient).await?;
        let bridge_after = bridge(c, &id).await?;
        if inserted == 0 && (after != before || bridge_after != bridge_before) {
            return Err(changed());
        }
        if inserted == 1 {
            let expected = json!({"id":id,"company_id":f.company_id,"user_id":recipient,"record_type":"quality_hold","record_id":f.hold_id,"record_version":f.version,"message":MESSAGE,"created_at":f.updated_at,"read_at":null});
            if after != vec![expected] {
                return Err(changed());
            }
            let expected_event = json!({"id":format!("legacy:{id}"),"company_id":f.company_id,"event_key":format!("legacy:{id}"),"source_type":"quality_hold","source_id":f.hold_id,"source_version":f.version,"actor_user_id":null,"category":"work","preview":MESSAGE,"created_at":f.updated_at});
            let expected_delivery = json!({"id":id,"event_id":format!("legacy:{id}"),"company_id":f.company_id,"user_id":recipient,"delivered_at":f.updated_at,"read_at":null,"archived_at":null});
            if bridge_before.0.is_none() && bridge_after.0.as_ref() != Some(&expected_event)
                || bridge_before.1.is_none() && bridge_after.1.as_ref() != Some(&expected_delivery)
            {
                return Err(changed());
            }
            if bridge_before.0.is_some() && bridge_after.0 != bridge_before.0
                || bridge_before.1.is_some() && bridge_after.1 != bridge_before.1
            {
                return Err(changed());
            }
        }
        retained.push((recipient.clone(), after, bridge_after));
    }
    source.require_current(c).await?;
    Ok(RetainedQualityNotifications {
        unit: u,
        facts: f.clone(),
        retained,
    })
}
pub struct RetainedQualityNotifications {
    unit: Unit,
    facts: QualityNotificationFacts,
    retained: Vec<(String, Vec<Value>, (Option<Value>, Option<Value>))>,
}
impl RetainedQualityNotifications {
    pub async fn require_current<S: PgQualityNotificationSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || source.facts() != &self.facts {
            return Err(changed());
        }
        source.require_current(c).await?;
        for (recipient, expected, expected_bridge) in &self.retained {
            if conflicts(c, &self.facts, recipient).await? != *expected
                || bridge(c, &format!("{}-{}", self.facts.command_key, recipient)).await?
                    != *expected_bridge
            {
                return Err(changed());
            }
        }
        Ok(())
    }
}

/// Legacy Count disposition notifications. Source must be composed from the
/// actual staged Quality/Inventory after-image and, for a child, current IAM
/// Count-reader witness in the same serializable transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityCountNoticeKind {
    RemainingChild,
    Requester,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCountNoticeFacts {
    pub kind: QualityCountNoticeKind,
    pub command_key: String,
    pub company_id: String,
    pub hold_id: String,
    pub version: i64,
    pub created_at: i64,
    pub recipients: Vec<String>,
}
pub trait PgQualityCountNoticeSource: Sync {
    fn facts(&self) -> &QualityCountNoticeFacts;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
fn count_message(f: &QualityCountNoticeFacts) -> &'static str {
    match f.kind {
        QualityCountNoticeKind::RemainingChild => {
            "Partial scrap posted; remaining returned stock requires a new quality decision"
        }
        QualityCountNoticeKind::Requester => {
            "Quality scrap posted with inventory and accounting evidence"
        }
    }
}
fn count_notice_id(f: &QualityCountNoticeFacts, recipient: &str) -> String {
    match f.kind {
        QualityCountNoticeKind::RemainingChild => format!("{}-notice-{}", f.hold_id, recipient),
        QualityCountNoticeKind::Requester => format!("{}-quality-requester", f.command_key),
    }
}
async fn count_conflicts(
    c: &mut PgConnection,
    f: &QualityCountNoticeFacts,
    recipient: &str,
) -> Result<Vec<Value>, Error> {
    Ok(sqlx::query_scalar("SELECT to_jsonb(n) FROM staff_notifications n WHERE n.id=$1 OR (n.user_id=$2 AND n.record_type='quality_hold' AND n.record_id=$3 AND n.record_version=$4 AND n.message=$5) ORDER BY n.id FOR SHARE")
        .bind(count_notice_id(f,recipient)).bind(recipient).bind(&f.hold_id).bind(f.version).bind(count_message(f)).fetch_all(c).await?)
}
async fn count_bridge(
    c: &mut PgConnection,
    id: &str,
    company: &str,
    recipient: &str,
) -> Result<(Vec<Value>, Vec<Value>), Error> {
    let event_id = format!("legacy:{id}");
    let events:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(e) FROM inbox_events e WHERE e.id=$1 OR (e.company_id=$2 AND e.event_key=$1) ORDER BY e.id FOR SHARE")
        .bind(&event_id).bind(company).fetch_all(&mut *c).await?;
    let deliveries:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(d) FROM inbox_deliveries d WHERE d.id=$1 OR (d.event_id=$2 AND d.user_id=$3) ORDER BY d.id FOR SHARE")
        .bind(id).bind(&event_id).bind(recipient).fetch_all(c).await?;
    Ok((events, deliveries))
}

async fn child_prefix_image(
    c: &mut PgConnection,
    f: &QualityCountNoticeFacts,
) -> Result<Vec<Value>, Error> {
    if f.kind != QualityCountNoticeKind::RemainingChild {
        return Ok(Vec::new());
    }
    let prefix = format!("{}-notice-", f.hold_id);
    // Preserve the complete preexisting prefix and bridge rows exactly. The
    // witness retains all rows in memory; no synthetic row/byte ceiling is
    // imposed on historical records which legacy insertion leaves untouched.
    let rows: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('notice',to_jsonb(n),'event',(SELECT to_jsonb(e) FROM inbox_events e WHERE e.id='legacy:'||n.id),'delivery',(SELECT to_jsonb(d) FROM inbox_deliveries d WHERE d.id=n.id)) FROM staff_notifications n WHERE left(n.id,length($1))=$1 ORDER BY n.id")
        .bind(prefix).fetch_all(c).await?;
    Ok(rows)
}

pub async fn stage_quality_count_notices<S: PgQualityCountNoticeSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedQualityCountNotices, Error> {
    let mut sp = c.begin().await?;
    match stage_quality_count_notices_inner(&mut sp, source).await {
        Ok(retained) => {
            sp.commit().await?;
            Ok(retained)
        }
        Err(error) => {
            sp.rollback().await?;
            Err(error)
        }
    }
}

async fn stage_quality_count_notices_inner<S: PgQualityCountNoticeSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedQualityCountNotices, Error> {
    require_quality_count_fence(c).await?;
    source.require_current(c).await?;
    let u = unit(c).await?;
    let f = source.facts();
    let mut seen = std::collections::BTreeSet::new();
    if f.command_key.is_empty()
        || f.company_id.is_empty()
        || f.hold_id.is_empty()
        || f.version < 1
        || f.created_at < 0
        || (f.kind == QualityCountNoticeKind::RemainingChild && f.version != 1)
        || (f.kind == QualityCountNoticeKind::Requester && f.recipients.len() != 1)
        || f.recipients.iter().any(|r| r.is_empty() || !seen.insert(r))
    {
        return Err(changed());
    }
    let prefix_before = child_prefix_image(c, f).await?;
    let mut retained = Vec::new();
    for recipient in &f.recipients {
        let id = count_notice_id(f, recipient);
        let prior = count_conflicts(c, f, recipient).await?;
        let bridge_before = count_bridge(c, &id, &f.company_id, recipient).await?;
        let inserted = sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES($1,$2,$3,'quality_hold',$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(&id).bind(&f.company_id).bind(recipient).bind(&f.hold_id).bind(f.version).bind(count_message(f)).bind(f.created_at).execute(&mut *c).await?.rows_affected();
        if inserted != u64::from(prior.is_empty()) {
            return Err(changed());
        }
        let after = count_conflicts(c, f, recipient).await?;
        let bridge_after = count_bridge(c, &id, &f.company_id, recipient).await?;
        if inserted == 0 && (prior != after || bridge_before != bridge_after) {
            return Err(changed());
        }
        if inserted == 1 {
            let expected = json!({"id":id,"company_id":f.company_id,"user_id":recipient,"record_type":"quality_hold","record_id":f.hold_id,"record_version":f.version,"message":count_message(f),"created_at":f.created_at,"read_at":null});
            let event = json!({"id":format!("legacy:{id}"),"company_id":f.company_id,"event_key":format!("legacy:{id}"),"source_type":"quality_hold","source_id":f.hold_id,"source_version":f.version,"actor_user_id":null,"category":"work","preview":count_message(f),"created_at":f.created_at});
            let delivery = json!({"id":id,"event_id":format!("legacy:{id}"),"company_id":f.company_id,"user_id":recipient,"delivered_at":f.created_at,"read_at":null,"archived_at":null});
            if after != vec![expected]
                || (bridge_before.0.is_empty() && bridge_after.0 != vec![event])
                || (bridge_before.1.is_empty() && bridge_after.1 != vec![delivery])
                || (!bridge_before.0.is_empty() && bridge_after.0 != bridge_before.0)
                || (!bridge_before.1.is_empty() && bridge_after.1 != bridge_before.1)
            {
                return Err(changed());
            }
        }
        retained.push((recipient.clone(), after, bridge_after));
    }
    let prefix_after = child_prefix_image(c, f).await?;
    let mut before_by_id = std::collections::BTreeMap::new();
    for row in &prefix_before {
        let id = row["notice"]["id"].as_str().ok_or_else(changed)?;
        if before_by_id.insert(id, row).is_some() {
            return Err(changed());
        }
    }
    let mut after_by_id = std::collections::BTreeMap::new();
    for row in &prefix_after {
        let id = row["notice"]["id"].as_str().ok_or_else(changed)?;
        if after_by_id.insert(id, row).is_some() {
            return Err(changed());
        }
    }
    let allowed: std::collections::BTreeSet<String> =
        f.recipients.iter().map(|r| count_notice_id(f, r)).collect();
    for (id, before) in &before_by_id {
        if after_by_id.get(id) != Some(before) {
            return Err(changed());
        }
    }
    for id in after_by_id.keys() {
        if !before_by_id.contains_key(id) && !allowed.contains(*id) {
            return Err(changed());
        }
    }
    source.require_current(c).await?;
    Ok(RetainedQualityCountNotices {
        unit: u,
        facts: f.clone(),
        retained,
        child_prefix: prefix_after,
    })
}
pub struct RetainedQualityCountNotices {
    unit: Unit,
    facts: QualityCountNoticeFacts,
    retained: Vec<(String, Vec<Value>, (Vec<Value>, Vec<Value>))>,
    child_prefix: Vec<Value>,
}
impl RetainedQualityCountNotices {
    pub async fn require_current<S: PgQualityCountNoticeSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit || source.facts() != &self.facts {
            return Err(changed());
        }
        require_quality_count_fence(c).await?;
        source.require_current(c).await?;
        if child_prefix_image(c, &self.facts).await? != self.child_prefix {
            return Err(changed());
        }
        for (recipient, expected, expected_bridge) in &self.retained {
            if count_conflicts(c, &self.facts, recipient).await? != *expected
                || count_bridge(
                    c,
                    &count_notice_id(&self.facts, recipient),
                    &self.facts.company_id,
                    recipient,
                )
                .await?
                    != *expected_bridge
            {
                return Err(changed());
            }
        }
        Ok(())
    }
}
