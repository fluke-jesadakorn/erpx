//! Borrowed, owner-bound insertion plus exact precomputed notification images.
use crate::domain::field_service_notifications::{
    FieldServiceNotification, FieldServiceNotificationFacts,
    derive_field_service_created_notification, derive_field_service_notification,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;

/// Actual owner-local source implementation is mandatory. Before must retain
/// original owner/current actor facts without moving late version guards. After
/// must prove the complete intended order/event after-image, never a label or a
/// permissive current boolean. The enclosing command retains this source after
/// the notification's owned receipt is extracted.
pub trait PgFieldServiceNotificationSource: Sync {
    fn facts(&self) -> &FieldServiceNotificationFacts;
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

pub async fn lock_field_service_notification_sources(c: &mut PgConnection) -> Result<(), Error> {
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
pub struct PreparedFieldServiceNotification<'a, S: PgFieldServiceNotificationSource> {
    source: &'a S,
    unit: Unit,
    notification: FieldServiceNotification,
    images: Images,
}
/// Owned original preparation spanning Workflow prepare/apply, never a source
/// proof. Persistence requires reborrowing the actual original owner source.
pub struct OwnedPreparedFieldServiceNotification {
    unit: Unit,
    notification: FieldServiceNotification,
    images: Images,
}

pub async fn prepare_field_service_notification<'a, S: PgFieldServiceNotificationSource>(
    c: &mut PgConnection,
    source: &'a S,
    key: &CommandKey,
    guard_now: TimestampMillis,
) -> Result<PreparedFieldServiceNotification<'a, S>, Error> {
    source.require_before(c, guard_now).await?;
    let unit = Unit::current(c).await?;
    let notification = derive_field_service_notification(source.facts(), key)?;
    let images = Images::expected(&notification);
    Ok(PreparedFieldServiceNotification {
        source,
        unit,
        notification,
        images,
    })
}
/// The creation source must prove the actual version-one owner plan and after-image.
pub async fn prepare_field_service_created_notification<'a, S: PgFieldServiceNotificationSource>(
    c: &mut PgConnection,
    source: &'a S,
    key: &CommandKey,
    guard_now: TimestampMillis,
) -> Result<PreparedFieldServiceNotification<'a, S>, Error> {
    source.require_before(c, guard_now).await?;
    let unit = Unit::current(c).await?;
    let notification = derive_field_service_created_notification(source.facts(), key)?;
    let images = Images::expected(&notification);
    Ok(PreparedFieldServiceNotification {
        source,
        unit,
        notification,
        images,
    })
}
impl<S: PgFieldServiceNotificationSource> PreparedFieldServiceNotification<'_, S> {
    pub fn notification(&self) -> &FieldServiceNotification {
        &self.notification
    }
    pub fn into_owned(self) -> OwnedPreparedFieldServiceNotification {
        OwnedPreparedFieldServiceNotification {
            unit: self.unit,
            notification: self.notification,
            images: self.images,
        }
    }
    /// Call only after the actual order/event writes. The original 0312 trigger
    /// performs Inbox bridging; no duplicate Inbox/Workflow publisher runs.
    pub async fn persist(
        self,
        c: &mut PgConnection,
        guard_now: TimestampMillis,
    ) -> Result<RetainedFieldServiceNotification, Error> {
        let source = self.source;
        self.into_owned().persist(c, source, guard_now).await
    }
}
impl OwnedPreparedFieldServiceNotification {
    pub fn notification(&self) -> &FieldServiceNotification {
        &self.notification
    }
    pub async fn persist<S: PgFieldServiceNotificationSource>(
        self,
        c: &mut PgConnection,
        source: &S,
        guard_now: TimestampMillis,
    ) -> Result<RetainedFieldServiceNotification, Error> {
        self.unit.require_current(c).await?;
        if source.facts() != self.notification.facts() {
            return Err(changed());
        }
        source.require_after(c, guard_now).await?;
        if self.notification.facts().order_version_after > 9_007_199_254_740_991 {
            return Err(changed());
        }
        let n = &self.notification;
        let f = n.facts();
        let inserted=sqlx::query("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)VALUES($1,$2,$3,'field_service_order',$4,$5,$6,$7)")
            .bind(n.id()).bind(f.company_id.as_str()).bind(f.technician_user_id.as_str())
            .bind(f.order_id.as_str()).bind(f.order_version_after).bind(n.message())
            .bind(f.updated_at_after.get()).execute(&mut *c).await?;
        if inserted.rows_affected() != 1 {
            return Err(changed());
        }
        self.images.require_current(c).await?;
        source.require_after(c, guard_now).await?;
        if source.facts() != self.notification.facts() {
            return Err(changed());
        }
        let retained = RetainedFieldServiceNotification {
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
pub struct RetainedFieldServiceNotification {
    unit: Unit,
    notification: FieldServiceNotification,
    images: Images,
}
impl RetainedFieldServiceNotification {
    pub fn notification(&self) -> &FieldServiceNotification {
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
    fn expected(n: &FieldServiceNotification) -> Self {
        let f = n.facts();
        Self {
            notification: json!({"id":n.id(),"company_id":f.company_id.as_str(),"user_id":f.technician_user_id.as_str(),"record_type":"field_service_order","record_id":f.order_id.as_str(),"record_version":f.order_version_after,"message":n.message(),"created_at":f.updated_at_after.get(),"read_at":null}),
            event: json!({"id":n.event_id(),"company_id":f.company_id.as_str(),"event_key":n.event_id(),"source_type":"field_service_order","source_id":f.order_id.as_str(),"source_version":f.order_version_after,"actor_user_id":null,"category":"work","preview":n.message(),"created_at":f.updated_at_after.get()}),
            delivery: json!({"id":n.id(),"event_id":n.event_id(),"company_id":f.company_id.as_str(),"user_id":f.technician_user_id.as_str(),"delivered_at":f.updated_at_after.get(),"read_at":null,"archived_at":null}),
        }
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let id = self.notification["id"].as_str().ok_or_else(changed)?;
        let company = self.notification["company_id"]
            .as_str()
            .ok_or_else(changed)?;
        let event_id = self.event["id"].as_str().ok_or_else(changed)?;
        // Compare the immutable intended JSON against complete actual rows
        // inside PostgreSQL, without a new identifier-sized transfer limit.
        let n:Option<bool>=sqlx::query_scalar("SELECT sha256(convert_to(to_jsonb(n)::text,'UTF8'))=sha256(convert_to($3::jsonb::text,'UTF8')) FROM staff_notifications n WHERE id=$1 AND company_id=$2 FOR SHARE OF n").bind(id).bind(company).bind(self.notification.to_string()).fetch_optional(&mut *c).await?;
        let e:Option<bool>=sqlx::query_scalar("SELECT sha256(convert_to(to_jsonb(e)::text,'UTF8'))=sha256(convert_to($3::jsonb::text,'UTF8')) FROM inbox_events e WHERE id=$1 AND company_id=$2 FOR SHARE OF e").bind(event_id).bind(company).bind(self.event.to_string()).fetch_optional(&mut *c).await?;
        let d:Option<bool>=sqlx::query_scalar("SELECT sha256(convert_to(to_jsonb(d)::text,'UTF8'))=sha256(convert_to($3::jsonb::text,'UTF8')) FROM inbox_deliveries d WHERE id=$1 AND company_id=$2 FOR SHARE OF d").bind(id).bind(company).bind(self.delivery.to_string()).fetch_optional(&mut *c).await?;
        if [n, e, d].iter().any(|value| *value != Some(true)) {
            return Err(changed());
        }
        Ok(())
    }
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_NOTIFICATION_SOURCE_CHANGED",
        "The original order-bound notification evidence changed.",
    )
    .into()
}
