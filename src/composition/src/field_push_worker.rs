//! Field push composition. IAM owns session decisions, Collaboration owns its
//! queue and delivery policy, and the provider sees only a stable tag/Inbox path.
use erpx_server_collaboration::public::web_push::{self as queue, NativePushProvider};
use erpx_server_identity_access::public::push_session;
use erpx_server_shared::public::Clock;
use sqlx::PgPool;

pub struct FieldPushTarget {
    pub control: PgPool,
    pub holding: PgPool,
    pub audience: String,
    pub origin: String,
    pub environment: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushTurn {
    Disabled,
    Enabled,
}

async fn current(
    target: &FieldPushTarget,
    c: &mut sqlx::PgConnection,
    s: &queue::FieldPushSubscription,
    now: i64,
) -> Result<bool, sqlx::Error> {
    if s.origin != target.origin {
        return Ok(false);
    }
    let mut control = target.control.begin().await?;
    let valid = push_session::current_account(
        &mut control,
        &s.session,
        &s.user,
        &target.audience,
        &target.environment,
        now,
    )
    .await?;
    control.commit().await?;
    Ok(valid && push_session::current_company(c, &s.session, &s.user, &s.company, now).await?)
}

/// An absent provider leaves both cursors and pending jobs untouched. Verification
/// must use an absent provider unless an explicit local synthetic sender is set.
pub async fn run_once(
    target: &FieldPushTarget,
    provider: Option<&NativePushProvider>,
    clock: &(dyn Clock + Send + Sync),
) -> Result<PushTurn, sqlx::Error> {
    let Some(provider) = provider else {
        return Ok(PushTurn::Disabled);
    };
    let mut tx = target.holding.begin().await?;
    let subscriptions = queue::subscriptions(&mut tx).await?;
    tx.commit().await?;
    for s in subscriptions {
        let now = clock.now().get();
        let mut tx = target.holding.begin().await?;
        if current(target, &mut tx, &s, now).await? {
            queue::fanout(&mut tx, &s, now).await?;
        } else {
            queue::visited(&mut tx, &s.id, now).await?;
        }
        tx.commit().await?;
    }
    for _ in 0..25 {
        let now = clock.now().get();
        let token = uuid::Uuid::new_v4().to_string();
        let mut tx = target.holding.begin().await?;
        let claimed = queue::claim(&mut tx, &token, now).await?;
        tx.commit().await?;
        let Some((id, attempts)) = claimed else { break };
        let mut tx = target.holding.begin().await?;
        let row = queue::delivery(&mut tx, &id, &token, now).await?;
        let Some(row) = row else {
            tx.commit().await?;
            continue;
        };
        let s = &row.subscription;
        let valid = row.visible
            && row.preferences.permits(&row.category)
            && s.inbox_path.starts_with('/')
            && !s.inbox_path.starts_with("//")
            && !s.inbox_path.contains('\\')
            && s.inbox_path.len() <= 2048
            && !s.inbox_path.chars().any(char::is_control)
            && current(target, &mut tx, s, now).await?;
        let delay = row.preferences.quiet_delay(now);
        if !valid || delay > 0 {
            queue::suppress(
                &mut tx,
                &id,
                &token,
                now,
                if valid {
                    Some(now.saturating_add(delay))
                } else {
                    None
                },
            )
            .await?;
            tx.commit().await?;
            continue;
        }
        let payload = serde_json::to_vec(&serde_json::json!({"tag":id,"path":s.inbox_path}))
            .expect("fixed JSON strings");
        let request = provider.prepare(&s.sealed, &payload, now);
        tx.commit().await?;
        let status = match request {
            Ok(request) => provider.send(request).await.ok(),
            Err(_) => Some(400),
        };
        // The clock is sampled after network I/O so an expired lease cannot be
        // acknowledged merely because the worker began sending before expiry.
        let finished = clock.now().get();
        let mut tx = target.holding.begin().await?;
        queue::settle(&mut tx, &id, &token, finished, status, attempts as u32).await?;
        tx.commit().await?;
    }
    Ok(PushTurn::Enabled)
}

/// Partial/invalid provider configuration is an explicit startup error. Completely
/// absent configuration disables this channel without consuming queued work.
pub fn configured_provider() -> Result<Option<NativePushProvider>, &'static str> {
    let names = [
        "ERPX_PUSH_VAPID_PUBLIC_KEY",
        "ERPX_PUSH_VAPID_PRIVATE_KEY",
        "ERPX_PUSH_VAPID_SUBJECT",
        "ERPX_PUSH_DATA_KEY",
    ];
    let values = names.map(|name| std::env::var(name).ok());
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [Some(public), Some(private), Some(subject), Some(data)] = values else {
        return Err("FIELD_PUSH_CONFIGURATION_INVALID");
    };
    NativePushProvider::new(&public, &private, &subject, &data)
        .map(Some)
        .map_err(|_| "FIELD_PUSH_CONFIGURATION_INVALID")
}
