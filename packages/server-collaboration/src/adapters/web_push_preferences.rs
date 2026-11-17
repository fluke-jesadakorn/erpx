//! Same-unit owner mutation for selected-user web-push preferences.
//! Composition supplies a current Account/Holding witness; these inputs are
//! never themselves an authentication or authorization grant.
use crate::domain::web_push::PushPreferenceInput;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub trait CurrentPushPreferenceAuthority: Sync {
    fn company_id(&self) -> &str;
    fn user_id(&self) -> &str;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PUSH_CONTEXT_CHANGED",
        "The current push preferences or work context changed.",
    )
    .into()
}

/// Take with Identity fences before the transaction's first protected snapshot.
pub async fn lock_push_preference_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("PUSH_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE push_preferences IN SHARE ROW EXCLUSIVE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("PUSH_SERIALIZABLE_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND relation='push_preferences'::regclass AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("PUSH_PREFERENCE_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

pub struct RetainedPushPreferences {
    unit: Unit,
    company: String,
    user: String,
    image: String,
}
impl RetainedPushPreferences {
    pub async fn require_current<A: CurrentPushPreferenceAuthority>(
        &self,
        c: &mut PgConnection,
        authority: &A,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || authority.company_id() != self.company
            || authority.user_id() != self.user
        {
            return Err(changed());
        }
        authority.require_current(c).await?;
        let image: Option<String> = sqlx::query_scalar(
            "SELECT to_jsonb(p)::text FROM push_preferences p WHERE company_id=$1 AND user_id=$2",
        )
        .bind(&self.company)
        .bind(&self.user)
        .fetch_optional(&mut *c)
        .await?;
        if image.as_deref() != Some(self.image.as_str()) {
            return Err(changed());
        }
        Ok(())
    }
}

/// Caller retains and rechecks this proof after all other same-unit effects.
pub async fn save_push_preferences<A: CurrentPushPreferenceAuthority>(
    c: &mut PgConnection,
    authority: &A,
    input: &PushPreferenceInput,
    now: i64,
) -> Result<RetainedPushPreferences, Error> {
    if input.quiet_start.is_some() != input.quiet_end.is_some()
        || input.quiet_start.is_some() && input.quiet_start == input.quiet_end
        || input.quiet_start.is_some_and(|n| !(0..=1439).contains(&n))
        || input.quiet_end.is_some_and(|n| !(0..=1439).contains(&n))
    {
        return Err(BusinessError::invalid(
            "INVALID_PUSH_PREFERENCES",
            "Choose valid push preferences and quiet hours.",
        )
        .into());
    }
    let unit = unit(c).await?;
    authority.require_current(c).await?;
    let company = authority.company_id().to_owned();
    let user = authority.user_id().to_owned();
    if company.is_empty() || user.is_empty() {
        return Err(changed());
    }
    let enabled: Option<bool> = sqlx::query_scalar(
        "SELECT enabled FROM push_preferences WHERE company_id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(&company)
    .bind(&user)
    .fetch_optional(&mut *c)
    .await?;
    sqlx::query("INSERT INTO push_preferences(company_id,user_id,enabled,work_enabled,mention_enabled,system_enabled,calendar_enabled,notice_enabled,quiet_start_minute,quiet_end_minute,updated_at) VALUES($1,$2,false,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(company_id,user_id) DO UPDATE SET work_enabled=excluded.work_enabled,mention_enabled=excluded.mention_enabled,system_enabled=excluded.system_enabled,calendar_enabled=excluded.calendar_enabled,notice_enabled=excluded.notice_enabled,quiet_start_minute=excluded.quiet_start_minute,quiet_end_minute=excluded.quiet_end_minute,updated_at=excluded.updated_at")
        .bind(&company).bind(&user).bind(input.work).bind(input.mention).bind(input.system).bind(input.calendar).bind(input.notice).bind(input.quiet_start).bind(input.quiet_end).bind(now).execute(&mut *c).await?;
    let exact: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM push_preferences WHERE company_id=$1 AND user_id=$2 AND enabled=$3 AND work_enabled=$4 AND mention_enabled=$5 AND system_enabled=$6 AND calendar_enabled=$7 AND notice_enabled=$8 AND quiet_start_minute IS NOT DISTINCT FROM $9 AND quiet_end_minute IS NOT DISTINCT FROM $10 AND updated_at=$11)")
        .bind(&company).bind(&user).bind(enabled.unwrap_or(false)).bind(input.work).bind(input.mention).bind(input.system).bind(input.calendar).bind(input.notice).bind(input.quiet_start).bind(input.quiet_end).bind(now).fetch_one(&mut *c).await?;
    if !exact {
        return Err(changed());
    }
    let image: String = sqlx::query_scalar(
        "SELECT to_jsonb(p)::text FROM push_preferences p WHERE company_id=$1 AND user_id=$2",
    )
    .bind(&company)
    .bind(&user)
    .fetch_one(&mut *c)
    .await?;
    let retained = RetainedPushPreferences {
        unit,
        company,
        user,
        image,
    };
    retained.require_current(c, authority).await?;
    Ok(retained)
}
