//! Field-only push queue. Caller retains one transaction for cursor + fanout,
//! and establishes current Account/Company session authority before admission.
use sqlx::{PgConnection, Row};
#[derive(Clone)]
pub struct FieldPushSubscription {
    pub id: String,
    pub created_at: i64,
    pub company: String,
    pub user: String,
    pub session: String,
    pub origin: String,
    pub sealed: String,
    pub inbox_path: String,
}
pub async fn subscriptions(
    c: &mut PgConnection,
) -> Result<Vec<FieldPushSubscription>, sqlx::Error> {
    let rows=sqlx::query("SELECT s.id,s.created_at,s.company_id,s.user_id,s.session_id,s.origin,s.sealed_subscription,s.inbox_path FROM push_subscriptions s JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id LEFT JOIN native_field_push_cursors n ON n.subscription_id=s.id WHERE p.enabled ORDER BY coalesce(n.scanned_at,0),s.id LIMIT 25").fetch_all(c).await?;
    rows.into_iter()
        .map(|r| {
            Ok(FieldPushSubscription {
                id: r.try_get("id")?,
                created_at: r.try_get("created_at")?,
                company: r.try_get("company_id")?,
                user: r.try_get("user_id")?,
                session: r.try_get("session_id")?,
                origin: r.try_get("origin")?,
                sealed: r.try_get("sealed_subscription")?,
                inbox_path: r.try_get("inbox_path")?,
            })
        })
        .collect()
}
/// Subscription lock serializes cursor admission across overlapping worker turns.
/// Existing inbox/subscription unique identity records both eligible and
/// suppressed observations. The cursor is diagnostic: it never excludes a late
/// commit with an earlier delivery timestamp. Pre-subscription history is excluded.
pub async fn fanout(
    c: &mut PgConnection,
    s: &FieldPushSubscription,
    now: i64,
) -> Result<u64, sqlx::Error> {
    let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM push_subscriptions WHERE id=$1 AND company_id=$2 AND user_id=$3 AND session_id=$4 AND origin=$5 AND sealed_subscription=$6 AND created_at=$7 FOR UPDATE)").bind(&s.id).bind(&s.company).bind(&s.user).bind(&s.session).bind(&s.origin).bind(&s.sealed).bind(s.created_at).fetch_one(&mut *c).await?;
    if !current {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query(
        "INSERT INTO native_field_push_cursors(subscription_id)VALUES($1)ON CONFLICT DO NOTHING",
    )
    .bind(&s.id)
    .execute(&mut *c)
    .await?;
    let (at,id):(i64,String)=sqlx::query_as("SELECT cursor_at,cursor_id FROM native_field_push_cursors WHERE subscription_id=$1 FOR UPDATE").bind(&s.id).fetch_one(&mut *c).await?;
    let rows=sqlx::query("SELECT d.id,d.delivered_at,e.category FROM inbox_deliveries d JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id WHERE d.company_id=$1 AND d.user_id=$2 AND erpx_field_push_source(e.source_type,e.source_id,e.company_id) AND d.delivered_at>=$3 AND NOT EXISTS(SELECT 1 FROM push_delivery_jobs j WHERE j.inbox_delivery_id=d.id AND j.subscription_id=$4) ORDER BY d.delivered_at,d.id LIMIT 50").bind(&s.company).bind(&s.user).bind(s.created_at).bind(&s.id).fetch_all(&mut *c).await?;
    sqlx::query("SELECT set_config('erpx.push_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    let mut queued = 0;
    for row in &rows {
        let delivery: String = row.try_get("id")?;
        let category: String = row.try_get("category")?;
        queued+=sqlx::query_scalar::<_, i64>("WITH observed AS (INSERT INTO push_delivery_jobs(id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at,status) SELECT $1,$2,$3,$4,$5,$5,CASE WHEN d.read_at IS NULL AND d.archived_at IS NULL AND p.enabled AND CASE $7 WHEN 'work' THEN p.work_enabled WHEN 'mention' THEN p.mention_enabled WHEN 'system' THEN p.system_enabled WHEN 'calendar' THEN p.calendar_enabled WHEN 'notice' THEN p.notice_enabled ELSE false END THEN 'pending' ELSE 'suppressed' END FROM inbox_deliveries d JOIN push_preferences p ON p.company_id=d.company_id AND p.user_id=d.user_id WHERE d.id=$3 AND d.company_id=$2 AND d.user_id=$6 ON CONFLICT(inbox_delivery_id,subscription_id)DO NOTHING RETURNING status) SELECT count(*) FROM observed WHERE status='pending'").bind(format!("field-push-{}-{delivery}",s.id)).bind(&s.company).bind(delivery).bind(&s.id).bind(now).bind(&s.user).bind(category).fetch_one(&mut *c).await? as u64;
    }
    let next = rows
        .last()
        .map(|r| {
            Ok::<_, sqlx::Error>((
                r.try_get::<i64, _>("delivered_at")?,
                r.try_get::<String, _>("id")?,
            ))
        })
        .transpose()?
        .unwrap_or((at, id.clone()));
    let next = std::cmp::max(next, (at, id));
    sqlx::query("UPDATE native_field_push_cursors SET cursor_at=$2,cursor_id=$3,scanned_at=$4 WHERE subscription_id=$1").bind(&s.id).bind(next.0).bind(next.1).bind(now).execute(c).await?;
    Ok(queued)
}

/// Claim one native job. Expired leases recover without changing its stable tag.
/// Caller commits before delivery and uses an unpredictable, per-claim token.
pub async fn claim(
    c: &mut PgConnection,
    token: &str,
    now: i64,
) -> Result<Option<(String, i32)>, sqlx::Error> {
    sqlx::query("SELECT set_config('erpx.push_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    sqlx::query_as("UPDATE push_delivery_jobs SET lease_token=$1,lease_until=$2,attempts=attempts+1 WHERE id=(SELECT j.id FROM push_delivery_jobs j WHERE j.status='pending' AND j.next_attempt_at<=$3 AND (j.lease_until IS NULL OR j.lease_until<$3) AND erpx_field_push_job(j) ORDER BY j.next_attempt_at,j.id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id,attempts")
        .bind(token).bind(now.saturating_add(30_000)).bind(now).fetch_optional(c).await
}

/// A lost/expired lease can never acknowledge a newer worker's attempt.
pub async fn settle(
    c: &mut PgConnection,
    id: &str,
    token: &str,
    now: i64,
    status: Option<u16>,
    attempts: u32,
) -> Result<bool, sqlx::Error> {
    use crate::domain::web_push::{PushRetry, retry};
    sqlx::query("SELECT set_config('erpx.push_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    let owned:Option<String>=sqlx::query_scalar("SELECT subscription_id FROM push_delivery_jobs j WHERE id=$1 AND lease_token=$2 AND lease_until>=$3 AND status='pending' AND attempts=$4 AND erpx_field_push_job(j) FOR UPDATE").bind(id).bind(token).bind(now).bind(i64::from(attempts)).fetch_optional(&mut *c).await?;
    let Some(subscription) = owned else {
        return Ok(false);
    };
    if status.is_some_and(|s| (200..300).contains(&s)) {
        sqlx::query("UPDATE push_delivery_jobs SET status='sent',sent_at=$2,last_status=$3,lease_token=NULL,lease_until=NULL WHERE id=$1").bind(id).bind(now).bind(status.map(i32::from)).execute(c).await?;
    } else {
        match retry(attempts, status, now) {
            PushRetry::RemoveSubscription => {
                sqlx::query("DELETE FROM push_subscriptions WHERE id=$1")
                    .bind(subscription)
                    .execute(c)
                    .await?;
            }
            outcome => {
                let (state, next) = match outcome {
                    PushRetry::Dead => ("dead", now),
                    PushRetry::RetryAt(next) => ("pending", next),
                    _ => unreachable!(),
                };
                sqlx::query("UPDATE push_delivery_jobs SET status=$2,next_attempt_at=$3,last_status=$4,lease_token=NULL,lease_until=NULL WHERE id=$1").bind(id).bind(state).bind(next).bind(status.map(i32::from)).execute(c).await?;
            }
        }
    }
    Ok(true)
}

pub struct FieldPushDelivery {
    pub subscription: FieldPushSubscription,
    pub category: String,
    pub visible: bool,
    pub preferences: crate::domain::web_push::PushPreferences,
}
/// Re-read live preference and Inbox state after claiming; no preview/content is
/// returned to the provider. Company/user equality is required on every join.
pub async fn delivery(
    c: &mut PgConnection,
    id: &str,
    token: &str,
    now: i64,
) -> Result<Option<FieldPushDelivery>, sqlx::Error> {
    let row=sqlx::query("SELECT s.id,s.created_at,s.company_id,s.user_id,s.session_id,s.origin,s.sealed_subscription,s.inbox_path,p.enabled,p.work_enabled,p.mention_enabled,p.system_enabled,p.calendar_enabled,p.notice_enabled,p.quiet_start_minute,p.quiet_end_minute,d.delivered_at,d.read_at,d.archived_at,e.category FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id AND s.company_id=j.company_id JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id JOIN inbox_deliveries d ON d.id=j.inbox_delivery_id AND d.company_id=s.company_id AND d.user_id=s.user_id JOIN inbox_events e ON e.id=d.event_id AND e.company_id=d.company_id WHERE j.id=$1 AND j.lease_token=$2 AND j.status='pending' AND j.lease_until>=$3 AND erpx_field_push_job(j)").bind(id).bind(token).bind(now).fetch_optional(c).await?;
    row.map(|r| {
        Ok(FieldPushDelivery {
            subscription: FieldPushSubscription {
                id: r.try_get("id")?,
                created_at: r.try_get("created_at")?,
                company: r.try_get("company_id")?,
                user: r.try_get("user_id")?,
                session: r.try_get("session_id")?,
                origin: r.try_get("origin")?,
                sealed: r.try_get("sealed_subscription")?,
                inbox_path: r.try_get("inbox_path")?,
            },
            category: r.try_get("category")?,
            visible: r.try_get::<i64, _>("delivered_at")? >= r.try_get::<i64, _>("created_at")?
                && r.try_get::<Option<i64>, _>("read_at")?.is_none()
                && r.try_get::<Option<i64>, _>("archived_at")?.is_none(),
            preferences: crate::domain::web_push::PushPreferences {
                enabled: r.try_get("enabled")?,
                work: r.try_get("work_enabled")?,
                mention: r.try_get("mention_enabled")?,
                system: r.try_get("system_enabled")?,
                calendar: r.try_get("calendar_enabled")?,
                notice: r.try_get("notice_enabled")?,
                quiet_start: r
                    .try_get::<Option<i32>, _>("quiet_start_minute")?
                    .map(|v| v as u16),
                quiet_end: r
                    .try_get::<Option<i32>, _>("quiet_end_minute")?
                    .map(|v| v as u16),
            },
        })
    })
    .transpose()
}
/// Delay a quiet-hours claim without consuming an attempt; otherwise suppress a
/// currently ineligible job. Both actions retain the claimed lease identity.
pub async fn suppress(
    c: &mut PgConnection,
    id: &str,
    token: &str,
    now: i64,
    quiet_until: Option<i64>,
) -> Result<bool, sqlx::Error> {
    sqlx::query("SELECT set_config('erpx.push_runtime','native-rust',true)")
        .execute(&mut *c)
        .await?;
    Ok(sqlx::query("UPDATE push_delivery_jobs j SET status=CASE WHEN $4::bigint IS NULL THEN 'suppressed' ELSE 'pending' END,next_attempt_at=coalesce($4,next_attempt_at),attempts=CASE WHEN $4::bigint IS NULL THEN attempts ELSE greatest(0,attempts-1) END,lease_token=NULL,lease_until=NULL WHERE j.id=$1 AND j.lease_token=$2 AND j.lease_until>=$3 AND j.status='pending' AND erpx_field_push_job(j)").bind(id).bind(token).bind(now).bind(quiet_until).execute(c).await?.rows_affected()==1)
}

/// Rotate inactive subscription checks without advancing their delivery cursor.
/// Otherwise 25 expired sessions can starve every later valid subscription.
pub async fn visited(c: &mut PgConnection, id: &str, now: i64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO native_field_push_cursors(subscription_id,scanned_at) SELECT id,$2 FROM push_subscriptions WHERE id=$1 ON CONFLICT(subscription_id)DO UPDATE SET scanned_at=excluded.scanned_at").bind(id).bind(now).execute(c).await?;
    Ok(())
}
