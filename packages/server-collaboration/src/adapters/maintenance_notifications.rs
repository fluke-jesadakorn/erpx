//! Borrowed, owner-bound insertion plus exact precomputed notification images.
use crate::domain::maintenance_notifications::{
    MaintenanceNotification, MaintenanceNotificationFacts, MaintenanceNotificationKind,
    derive_maintenance_notification,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Actual owner-local source implementation is mandatory. Before must retain
/// original owner/current actor facts without moving late version guards. After
/// must prove the complete intended Work/event after-image, never a label or a
/// permissive current boolean. The enclosing command retains this source after
/// the notification's owned receipt is extracted.
pub trait PgMaintenanceNotificationSource: Sync {
    fn facts(&self) -> &MaintenanceNotificationFacts;
    fn require_before(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
        now: TimestampMillis,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

pub async fn lock_maintenance_notification_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    // Utility-only write intent; call before the first Holding snapshot. This
    // does not establish any owner/source permission or notification absence.
    sqlx::query(
        "LOCK TABLE staff_notifications,inbox_events,inbox_deliveries IN ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

#[derive(PartialEq, Eq)]
struct Unit(String, i32, String, String);
impl Unit {
    async fn current(c: &mut PgConnection) -> Result<Self, Error> {
        if !c.is_in_transaction() {
            return Err(changed());
        }
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?;
        if isolation != "serializable" {
            return Err(changed());
        }
        let (database,pid,xid,postmaster):(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
        Ok(Self(database, pid, xid, postmaster))
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if &Self::current(c).await? != self {
            return Err(changed());
        }
        Ok(())
    }
}

/// Private constructor and pre-effect images. A prepared value is not a grant.
pub struct PreparedMaintenanceNotification<'a, S: PgMaintenanceNotificationSource> {
    source: &'a S,
    unit: Unit,
    notification: MaintenanceNotification,
    images: Images,
}
/// Owned original preparation spanning Workflow prepare/apply, never a source
/// proof. Persistence requires reborrowing the actual original owner source.
pub struct OwnedPreparedMaintenanceNotification {
    unit: Unit,
    notification: MaintenanceNotification,
    images: Images,
}

pub async fn prepare_maintenance_notification<'a, S: PgMaintenanceNotificationSource>(
    c: &mut PgConnection,
    source: &'a S,
    key: &CommandKey,
    kind: MaintenanceNotificationKind,
    guard_now: TimestampMillis,
) -> Result<PreparedMaintenanceNotification<'a, S>, Error> {
    source.require_before(c, guard_now).await?;
    let unit = Unit::current(c).await?;
    let notification = derive_maintenance_notification(source.facts(), key, kind)?;
    let images = Images::expected(&notification);
    Ok(PreparedMaintenanceNotification {
        source,
        unit,
        notification,
        images,
    })
}
impl<S: PgMaintenanceNotificationSource> PreparedMaintenanceNotification<'_, S> {
    pub fn notification(&self) -> &MaintenanceNotification {
        &self.notification
    }
    pub fn into_owned(self) -> OwnedPreparedMaintenanceNotification {
        OwnedPreparedMaintenanceNotification {
            unit: self.unit,
            notification: self.notification,
            images: self.images,
        }
    }
    /// Call only after the actual Work/event writes. The original 0312 trigger
    /// performs Inbox bridging; no duplicate Inbox/Workflow publisher runs.
    pub async fn persist(
        self,
        c: &mut PgConnection,
        guard_now: TimestampMillis,
    ) -> Result<RetainedMaintenanceNotification, Error> {
        let source = self.source;
        self.into_owned().persist(c, source, guard_now).await
    }
}
impl OwnedPreparedMaintenanceNotification {
    pub fn notification(&self) -> &MaintenanceNotification {
        &self.notification
    }
    pub async fn persist<S: PgMaintenanceNotificationSource>(
        self,
        c: &mut PgConnection,
        source: &S,
        guard_now: TimestampMillis,
    ) -> Result<RetainedMaintenanceNotification, Error> {
        self.unit.require_current(c).await?;
        if source.facts() != self.notification.facts() {
            return Err(changed());
        }
        source.require_after(c, guard_now).await?;
        if self.notification.facts().work_version_after > 9_007_199_254_740_991 {
            return Err(changed());
        }
        let n = &self.notification;
        let f = n.facts();
        let inserted=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,$2,$3,'maintenance_work',$4,$5,$6,$7)")
            .bind(n.id().as_str()).bind(f.company_id.as_str()).bind(f.owner_user_id.as_str())
            .bind(f.work_id.as_str()).bind(f.work_version_after).bind(n.message())
            .bind(f.updated_at_after.get()).execute(&mut *c).await?;
        if inserted.rows_affected() != 1 {
            return Err(changed());
        }
        self.images.require_current(c).await?;
        source.require_after(c, guard_now).await?;
        if source.facts() != self.notification.facts() {
            return Err(changed());
        }
        let retained = RetainedMaintenanceNotification {
            unit: self.unit,
            notification: self.notification,
            images: self.images,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}

/// Owned Collaboration-only receipt. The actual owner/IAM/purchase checks must
/// still run after audit/outbox and deferred triggers in the enclosing unit.
pub struct RetainedMaintenanceNotification {
    unit: Unit,
    notification: MaintenanceNotification,
    images: Images,
}
impl RetainedMaintenanceNotification {
    pub fn notification(&self) -> &MaintenanceNotification {
        &self.notification
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.unit.require_current(c).await?;
        self.images.require_current(c).await
    }
}

struct Images {
    notification: Value,
    event: Value,
    delivery: Value,
}
impl Images {
    fn expected(n: &MaintenanceNotification) -> Self {
        let f = n.facts();
        Self {
            notification: json!({"id":n.id().as_str(),"company_id":f.company_id.as_str(),"user_id":f.owner_user_id.as_str(),"record_type":"maintenance_work","record_id":f.work_id.as_str(),"record_version":f.work_version_after,"message":n.message(),"created_at":f.updated_at_after.get(),"read_at":null}),
            event: json!({"id":n.event_id(),"company_id":f.company_id.as_str(),"event_key":n.event_id(),"source_type":"maintenance_work","source_id":f.work_id.as_str(),"source_version":f.work_version_after,"actor_user_id":null,"category":"work","preview":n.message(),"created_at":f.updated_at_after.get()}),
            delivery: json!({"id":n.id().as_str(),"event_id":n.event_id(),"company_id":f.company_id.as_str(),"user_id":f.owner_user_id.as_str(),"delivered_at":f.updated_at_after.get(),"read_at":null,"archived_at":null}),
        }
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let id = self.notification["id"].as_str().ok_or_else(changed)?;
        let company = self.notification["company_id"]
            .as_str()
            .ok_or_else(changed)?;
        let event_id = self.event["id"].as_str().ok_or_else(changed)?;
        // Compare complete after-images inside PostgreSQL; never transfer a bounded
        // row image whose length could reject an otherwise valid owner reference.
        let n: Option<bool> = sqlx::query_scalar("SELECT to_jsonb(n)=$3::jsonb FROM staff_notifications n WHERE id=$1 AND company_id=$2 FOR SHARE OF n").bind(id).bind(company).bind(self.notification.to_string()).fetch_optional(&mut *c).await?;
        let e: Option<bool> = sqlx::query_scalar("SELECT to_jsonb(e)=$3::jsonb FROM inbox_events e WHERE id=$1 AND company_id=$2 FOR SHARE OF e").bind(event_id).bind(company).bind(self.event.to_string()).fetch_optional(&mut *c).await?;
        let d: Option<bool> = sqlx::query_scalar("SELECT to_jsonb(d)=$3::jsonb FROM inbox_deliveries d WHERE id=$1 AND company_id=$2 FOR SHARE OF d").bind(id).bind(company).bind(self.delivery.to_string()).fetch_optional(&mut *c).await?;
        if [n, e, d].iter().any(|value| *value != Some(true)) {
            return Err(changed());
        }
        Ok(())
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED",
        "The original Work-bound notification evidence changed.",
    )
    .into()
}
