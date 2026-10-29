use super::account_group_recovery::{
    BrowserDiscovery, browser_discovery, browser_key, create_key, recoverable_browser,
    recovery_snapshot, rotate_key,
};
use super::account_holding_bindings::ConfiguredHoldingRegistry;
use super::account_revocation_postgres::invalidate_children;
use crate::application::account_revocation::AccountWorkContextInvalidation;
use crate::{
    adapters::account_sessions::{
        device_snapshot, live_accounts, lock_session_reads, lock_session_writes, persistence,
        read_session,
    },
    application::account_ports::*,
    domain::account_auth::{
        CredentialSnapshot, DeviceAccounts, EmailAddress, MAX_DEVICE_REVISION,
        MAXIMUM_SAVED_ACCOUNTS, PasswordHash, RequestAudience, SESSION_LIFETIME_MS,
        SIGN_IN_RESERVATION_MS, SecretDigest, denied, stale_device,
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, SessionLock, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

pub(crate) type TxResult<T> = Result<T, PgTransactionError<BusinessError>>;

/// Account persistence. Child retirement retains a non-retrying Control writer
/// through monotonic Holding fences; a later Control failure never undoes them.
pub struct PostgresAccountRepository {
    unit: PgUnitOfWork,
    holdings: Arc<ConfiguredHoldingRegistry>,
    environment: String,
    require_verification: bool,
    invalidation: Arc<dyn AccountWorkContextInvalidation>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountRepository {
    pub fn from_control_database(
        pool: DatabasePool,
        environment: String,
        require_verification: bool,
        invalidation: Arc<dyn AccountWorkContextInvalidation>,
        clock: Arc<dyn Clock + Send + Sync>,
        holdings: Arc<ConfiguredHoldingRegistry>,
    ) -> Result<Self, BusinessError> {
        if environment != "development" {
            return Err(BusinessError::invalid(
                "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
                "The Account environment has not been enabled.",
            ));
        }
        Ok(Self {
            unit: PgUnitOfWork::new(pool),
            holdings,
            environment,
            require_verification,
            invalidation,
            clock,
        })
    }
}

pub(crate) fn options() -> TransactionOptions {
    TransactionOptions {
        session_lock: Some(SessionLock {
            namespace: "erpx-account-authority-v1".into(),
            key: "control".into(),
        }),
        ..Default::default()
    }
}
pub(crate) fn finish<T>(result: TxResult<T>) -> Result<T, BusinessError> {
    result.map_err(|error| match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => persistence(),
    })
}
pub(crate) struct Browser {
    revision: i64,
    active: Option<String>,
    pending: Option<String>,
    until: Option<i64>,
}
pub(crate) async fn browser(
    connection: &mut PgConnection,
    id: &SecretDigest,
) -> TxResult<Option<Browser>> {
    let row = sqlx::query("SELECT revision,active_session_id,pending_nonce,pending_until FROM account_device_browsers WHERE id=$1").bind(id.as_str()).fetch_optional(connection).await?;
    row.map(|row| {
        Ok(Browser {
            revision: row.try_get("revision")?,
            active: row.try_get("active_session_id")?,
            pending: row.try_get("pending_nonce")?,
            until: row.try_get("pending_until")?,
        })
    })
    .transpose()
}
pub(crate) struct PreparedBrowser {
    pub id: SecretDigest,
    pub cookie: SecretDigest,
    pub created: bool,
    pub recovery_expires_at: Option<i64>,
}
/// Chosen by the trusted operation, never deserialized from browser intent.
/// A fresh candidate grants nothing until credential/proof completion succeeds.
#[derive(Clone, Copy)]
pub(crate) enum BrowserEnrollment {
    RecoverExisting,
    FreshAuthenticated,
}
pub(crate) async fn ensure_browser(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    candidate: &SecretDigest,
    environment: &str,
    expected_revision: Option<i64>,
    now: i64,
    enrollment: BrowserEnrollment,
) -> TxResult<PreparedBrowser> {
    if let Some(id) = browser_key(connection, proof.browser.as_ref()).await? {
        return Ok(PreparedBrowser {
            id,
            cookie: proof.browser.clone().ok_or_else(denied)?,
            created: false,
            recovery_expires_at: None,
        });
    }
    let discovered = browser_discovery(connection, proof, environment, now).await?;
    let group = match discovered {
        BrowserDiscovery::Recoverable(group) => Some(group),
        BrowserDiscovery::None => None,
        BrowserDiscovery::Ambiguous => match enrollment {
            BrowserEnrollment::RecoverExisting => return Err(stale_device().into()),
            // Credential-only isolation cannot adopt, select or retire an old group.
            BrowserEnrollment::FreshAuthenticated => None,
        },
    };
    if let Some(group) = group {
        let row = browser(connection, &group.id)
            .await?
            .ok_or_else(stale_device)?;
        let expected = expected_revision.ok_or_else(stale_device)?;
        revision(&row, Some(expected), now)?;
        return Ok(PreparedBrowser {
            id: group.id,
            cookie: candidate.clone(),
            created: false,
            recovery_expires_at: Some(group.proof_expires_at),
        });
    }
    if expected_revision.is_some_and(|value| value != 0) {
        return Err(stale_device().into());
    }
    sqlx::query("INSERT INTO account_device_browsers(id,revision,updated_at) VALUES($1,0,$2)")
        .bind(candidate.as_str())
        .bind(now)
        .execute(&mut *connection)
        .await?;
    create_key(connection, candidate, candidate, now).await?;
    Ok(PreparedBrowser {
        id: candidate.clone(),
        cookie: candidate.clone(),
        created: true,
        recovery_expires_at: None,
    })
}
pub(crate) fn revision(row: &Browser, expected: Option<i64>, now: i64) -> TxResult<()> {
    if !(0..MAX_DEVICE_REVISION).contains(&row.revision)
        || expected.is_some_and(|value| value != row.revision)
    {
        return Err(stale_device().into());
    }
    if row.pending.is_some() && row.until.is_some_and(|until| until > now) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "ACCOUNT_SIGN_IN_PENDING",
            "Finish the current sign-in before changing saved accounts.",
        )
        .into());
    }
    Ok(())
}
pub(crate) async fn credential(
    connection: &mut PgConnection,
    email: &EmailAddress,
) -> TxResult<Option<CredentialSnapshot>> {
    let rows = sqlx::query(r#"SELECT u.id,u.name,u.email,u.image,u."emailVerified" verified,a.password,COALESCE(p.status,'active') status FROM "user" u JOIN account a ON a."userId"=u.id AND a."providerId"='credential' AND a."accountId"=u.id AND a.issuer='local:credential' LEFT JOIN principals p ON p.user_id=u.id WHERE lower(u.email)=$1 LIMIT 2"#).bind(email.as_str()).fetch_all(connection).await?;
    if rows.len() != 1 {
        return Ok(None);
    }
    let row = &rows[0];
    let Some(hash) = row.try_get::<Option<String>, _>("password")? else {
        return Ok(None);
    };
    let Ok(password_hash) = PasswordHash::new(hash) else {
        return Ok(None);
    };
    Ok(Some(CredentialSnapshot {
        user_id: row.try_get("id")?,
        name: row.try_get("name")?,
        email: row.try_get("email")?,
        image: row.try_get("image")?,
        email_verified: row.try_get::<i64, _>("verified")? == 1,
        active: row.try_get::<String, _>("status")? == "active",
        password_hash,
    }))
}
async fn throttle(connection: &mut PgConnection, email: &EmailAddress, now: i64) -> TxResult<()> {
    let key = format!(
        "rust-account-sign-in:{}",
        hex::encode(Sha256::digest(email.as_str().as_bytes()))
    );
    let count: i64 = sqlx::query_scalar(r#"INSERT INTO "rateLimit"(id,key,count,"lastRequest") VALUES($1,$1,1,$2) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN "rateLimit"."lastRequest"<=$2-60000 THEN 1 ELSE "rateLimit".count+1 END,"lastRequest"=$2 RETURNING count"#).bind(key).bind(now).fetch_one(connection).await?;
    if count > 10 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TOO_MANY_SIGN_IN_ATTEMPTS",
            "Wait before trying to sign in again.",
        )
        .into());
    }
    Ok(())
}
pub(crate) async fn revoke_children(
    connection: &mut PgConnection,
    browser_id: &SecretDigest,
    environment: &str,
    invalidation: &dyn AccountWorkContextInvalidation,
) -> TxResult<()> {
    let parents = sqlx::query_scalar::<_, String>(
        "SELECT session_id FROM account_device_accounts WHERE browser_id=$1",
    )
    .bind(browser_id.as_str())
    .fetch_all(&mut *connection)
    .await?;
    invalidate_children(connection, &parents, environment, invalidation).await?;
    sqlx::query("DELETE FROM session WHERE id IN (SELECT b.session_id FROM sso_session_bindings b JOIN account_device_accounts a ON a.session_id=b.parent_session_id WHERE a.browser_id=$1)").bind(browser_id.as_str()).execute(connection).await?;
    Ok(())
}
pub(crate) async fn select_browser(
    connection: &mut PgConnection,
    id: &SecretDigest,
    active: Option<&str>,
    now: i64,
    environment: &str,
    invalidation: &dyn AccountWorkContextInvalidation,
) -> TxResult<()> {
    revoke_children(connection, id, environment, invalidation).await?;
    let count = sqlx::query("UPDATE account_device_browsers SET active_session_id=$2,revision=revision+1,pending_nonce=NULL,pending_until=NULL,updated_at=$3 WHERE id=$1 AND revision<$4").bind(id.as_str()).bind(active).bind(now).bind(MAX_DEVICE_REVISION).execute(connection).await?.rows_affected();
    if count != 1 {
        return Err(stale_device().into());
    }
    Ok(())
}
pub(crate) async fn expire_family(
    connection: &mut PgConnection,
    session_id: &str,
    environment: &str,
    invalidation: &dyn AccountWorkContextInvalidation,
) -> TxResult<()> {
    invalidate_children(
        connection,
        &[session_id.to_owned()],
        environment,
        invalidation,
    )
    .await?;
    sqlx::query("DELETE FROM session WHERE id IN (SELECT session_id FROM sso_session_bindings WHERE parent_session_id=$1) OR id=$1").bind(session_id).execute(&mut *connection).await?;
    sqlx::query("DELETE FROM account_device_accounts WHERE session_id=$1")
        .bind(session_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM sso_session_bindings WHERE parent_session_id=$1 OR session_id=$1")
        .bind(session_id)
        .execute(connection)
        .await?;
    Ok(())
}

impl AccountRepository for PostgresAccountRepository {
    fn reserve_sign_in(&self, input: ReserveSignIn) -> AccountFuture<'_, SignInReservation> {
        Box::pin(async move {
            finish(self.unit.run(options(), |connection| { let input=input.clone(); let environment=self.environment.clone(); Box::pin(async move {
            lock_session_writes(connection).await?;
            throttle(connection, &input.email, input.now).await?;
            let prepared = ensure_browser(connection, &input.proof, &input.candidate_browser, &environment, input.expected_revision, input.now, BrowserEnrollment::FreshAuthenticated).await?;
            let id=prepared.id;
            let row = browser(connection, &id).await?.ok_or_else(denied)?;
            revision(&row, input.expected_revision, input.now)?;
            let credential = credential(connection, &input.email).await?;
            // Capacity is a browser invariant, including sessions whose saved proof was not supplied.
            let accounts=sqlx::query_scalar::<_,String>("SELECT s.\"userId\" FROM account_device_accounts a JOIN session s ON s.id=a.session_id JOIN sso_session_bindings b ON b.session_id=s.id WHERE a.browser_id=$1 AND b.audience='account' AND b.environment=$2 AND b.parent_session_id IS NULL AND b.absolute_expires_at>$3 AND s.\"expiresAt\"::timestamptz>to_timestamp($3::double precision/1000)").bind(id.as_str()).bind(&environment).bind(input.now).fetch_all(&mut *connection).await?;
            if accounts.len() >= MAXIMUM_SAVED_ACCOUNTS && !credential.as_ref().is_some_and(|credential| accounts.contains(&credential.user_id)) { return Err(BusinessError::new(FailureKind::Conflict,"DEVICE_ACCOUNT_LIMIT","Remove a saved account before adding another.").into()); }
            let until = input.now.checked_add(SIGN_IN_RESERVATION_MS).ok_or_else(denied)?.min(prepared.recovery_expires_at.unwrap_or(i64::MAX));
            sqlx::query("UPDATE account_device_browsers SET pending_nonce=$2,pending_until=$3,updated_at=$4 WHERE id=$1").bind(id.as_str()).bind(input.nonce.as_str()).bind(until).bind(input.now).execute(connection).await?;
            Ok(SignInReservation { browser:id,cookie:prepared.cookie,created_browser:prepared.created,recovery_expires_at:prepared.recovery_expires_at,original_revision:row.revision,nonce:input.nonce,credential })
        }) }).await)
        })
    }
    fn complete_sign_in(&self, input: CompleteSignIn) -> AccountFuture<'_, CompletedSignIn> {
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        let require_verification = self.require_verification;
                        let invalidation = self.invalidation.clone();
                        let clock = self.clock.clone();
                        let holdings = self.holdings.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let now = clock.now().get();
                            let row = browser(connection, &input.reservation.browser)
                                .await?
                                .ok_or_else(denied)?;
                            if row.revision != input.reservation.original_revision
                                || row.pending.as_deref() != Some(input.reservation.nonce.as_str())
                                || !row.until.is_some_and(|until| until > now)
                            {
                                return Err(stale_device().into());
                            }
                            let email = EmailAddress::new(&input.credential.email)?;
                            let current = credential(connection, &email)
                                .await?
                                .filter(|value| {
                                    value.user_id == input.credential.user_id
                                        && value.password_hash == input.credential.password_hash
                                        && value.can_authenticate(require_verification)
                                })
                                .ok_or_else(denied)?;
                            if input.reservation.recovery_expires_at.is_some() {
                                let group = recoverable_browser(
                                    connection,
                                    &input.proof,
                                    &environment,
                                    now,
                                )
                                .await?
                                .ok_or_else(stale_device)?;
                                if group.id != input.reservation.browser {
                                    return Err(stale_device().into());
                                }
                                rotate_key(
                                    connection,
                                    &input.reservation.browser,
                                    &input.reservation.cookie,
                                    now,
                                )
                                .await?;
                            }
                            let completed = issue_account_session(
                                connection,
                                IssuedAccountSession {
                                    proof: input.proof,
                                    browser: input.reservation.browser,
                                    cookie_digest: input.reservation.cookie,
                                    session_id: input.session_id,
                                    session_digest: input.session_digest,
                                    handle: input.handle,
                                    user_id: current.user_id,
                                    now,
                                },
                                &environment,
                                invalidation.as_ref(),
                                clock.as_ref(),
                                &holdings,
                            )
                            .await?;
                            if input
                                .reservation
                                .recovery_expires_at
                                .is_some_and(|expiry| expiry <= clock.now().get())
                                || !row.until.is_some_and(|until| until > clock.now().get())
                                || completed.session.expires_at <= clock.now().get()
                            {
                                return Err(stale_device().into());
                            }
                            Ok(completed)
                        })
                    })
                    .await,
            )
        })
    }
    fn release_sign_in(&self, reservation: SignInReservation) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(self.unit.run(options(), |connection| { let value=reservation.clone(); Box::pin(async move {
            lock_session_writes(connection).await?;
            sqlx::query("UPDATE account_device_browsers SET pending_nonce=NULL,pending_until=NULL WHERE id=$1 AND pending_nonce=$2").bind(value.browser.as_str()).bind(value.nonce.as_str()).execute(&mut *connection).await?;
            if value.created_browser { sqlx::query("DELETE FROM account_device_browsers d WHERE id=$1 AND pending_nonce IS NULL AND active_session_id IS NULL AND NOT EXISTS(SELECT 1 FROM account_device_accounts a WHERE a.browser_id=d.id)").bind(value.browser.as_str()).execute(connection).await?; }
            Ok(())
        }) }).await)
        })
    }
    fn session(
        &self,
        proof: DeviceProof,
        audience: RequestAudience,
        now: i64,
    ) -> AccountFuture<'_, Option<crate::domain::account_auth::AccountSession>> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let proof = proof.clone();
                        let audience = audience.clone();
                        let holdings = self.holdings.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_reads(connection).await?;
                            read_session(
                                connection,
                                &proof,
                                &audience,
                                &environment,
                                now,
                                &holdings,
                            )
                            .await
                        })
                    })
                    .await,
            )
        })
    }
    fn devices(&self, proof: DeviceProof, now: i64) -> AccountFuture<'_, DeviceAccounts> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let proof = proof.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_reads(connection).await?;
                            recovery_snapshot(connection, &proof, &environment, now).await
                        })
                    })
                    .await,
            )
        })
    }
    fn initialize_device(
        &self,
        proof: DeviceProof,
        candidate: SecretDigest,
        expected_revision: Option<i64>,
        _now: i64,
    ) -> AccountFuture<'_, (DeviceAccounts, bool)> {
        Box::pin(async move {
            finish(self.unit.run_once(options(), |connection| {
                let mut proof=proof.clone(); let candidate=candidate.clone();
                let environment=self.environment.clone();let invalidation=self.invalidation.clone();let clock=self.clock.clone();
                Box::pin(async move {
                    lock_session_writes(connection).await?;
                    let now=clock.now().get();
                    let prepared=ensure_browser(connection,&proof,&candidate,&environment,expected_revision,now,BrowserEnrollment::RecoverExisting).await?;
                    let row=browser(connection,&prepared.id).await?.ok_or_else(stale_device)?;
                    revision(&row,expected_revision,now)?;
                    if let Some(expiry)=prepared.recovery_expires_at {
                        revoke_children(connection,&prepared.id,&environment,invalidation.as_ref()).await?;
                        if expiry<=clock.now().get(){return Err(stale_device().into());}
                        let changed=sqlx::query("UPDATE account_device_browsers SET revision=revision+1,pending_nonce=NULL,pending_until=NULL,updated_at=$3 WHERE id=$1 AND revision=$2 AND revision<$4").bind(prepared.id.as_str()).bind(row.revision).bind(clock.now().get()).bind(MAX_DEVICE_REVISION).execute(&mut *connection).await?.rows_affected();
                        if changed!=1{return Err(stale_device().into());}
                        rotate_key(connection,&prepared.id,&prepared.cookie,clock.now().get()).await?;
                        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
                        // Resample after every Holding wait and the final Control constraint boundary.
                        let current=recoverable_browser(connection,&proof,&environment,clock.now().get()).await?.ok_or_else(stale_device)?;
                        if current.id!=prepared.id || expiry<=clock.now().get(){return Err(stale_device().into());}
                    }
                    proof.browser=Some(prepared.cookie);
                    Ok((device_snapshot(connection,&proof,&environment,clock.now().get()).await?,prepared.created||prepared.recovery_expires_at.is_some()))
                })
            }).await)
        })
    }
    fn select_device(&self, input: DeviceSelection) -> AccountFuture<'_, SelectedDevice> {
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        let invalidation = self.invalidation.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let now = clock.now().get();
                            let id = browser_key(connection, input.proof.browser.as_ref())
                                .await?
                                .ok_or_else(denied)?;
                            let row = browser(connection, &id).await?.ok_or_else(denied)?;
                            revision(&row, Some(input.expected_revision), now)?;
                            let selected =
                                live_accounts(connection, &input.proof, &environment, now)
                                    .await?
                                    .into_iter()
                                    .find(|account| account.handle == input.handle)
                                    .ok_or_else(denied)?;
                            if row.active.as_deref() == Some(selected.session.id.as_str()) {
                                return Ok(SelectedDevice {
                                    devices: device_snapshot(
                                        connection,
                                        &input.proof,
                                        &environment,
                                        now,
                                    )
                                    .await?,
                                    session_digest: SecretDigest::new(selected.digest)?,
                                });
                            }
                            select_browser(
                                connection,
                                &id,
                                Some(&selected.session.id),
                                now,
                                &environment,
                                invalidation.as_ref(),
                            )
                            .await?;
                            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
                                .execute(&mut *connection)
                                .await?;
                            let state = browser(connection, &id).await?.ok_or_else(stale_device)?;
                            if state.active.as_deref() != Some(selected.session.id.as_str())
                                || state.revision != row.revision + 1
                                || state.pending.is_some()
                            {
                                return Err(stale_device().into());
                            }
                            let now = clock.now().get();
                            if !live_accounts(connection, &input.proof, &environment, now)
                                .await?
                                .iter()
                                .any(|account| account.session.id == selected.session.id)
                            {
                                return Err(stale_device().into());
                            }
                            Ok(SelectedDevice {
                                devices: device_snapshot(
                                    connection,
                                    &input.proof,
                                    &environment,
                                    now,
                                )
                                .await?,
                                session_digest: SecretDigest::new(selected.digest)?,
                            })
                        })
                    })
                    .await,
            )
        })
    }
    fn remove_device(&self, input: DeviceSelection) -> AccountFuture<'_, DeviceAccounts> {
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        let invalidation=self.invalidation.clone();
                        let clock=self.clock.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let now=clock.now().get();
                            let id = browser_key(connection,input.proof.browser.as_ref()).await?.ok_or_else(denied)?;
                            let row = browser(connection, &id).await?.ok_or_else(denied)?;
                            revision(&row, Some(input.expected_revision), now)?;
                            let target =
                                live_accounts(connection, &input.proof, &environment, now)
                                    .await?
                                    .into_iter()
                                    .find(|account| account.handle == input.handle)
                                    .ok_or_else(denied)?;
                            expire_family(connection, &target.session.id, &environment, invalidation.as_ref()).await?;
                            if row.active.as_deref() == Some(target.session.id.as_str()) {
                                select_browser(connection, &id, None, now, &environment, invalidation.as_ref()).await?;
                            } else {
                                // Removing an inactive family does not select another account.
                                // Retain the current parent's children and advance only the CAS revision.
                                let changed=sqlx::query("UPDATE account_device_browsers SET revision=revision+1,updated_at=$2 WHERE id=$1 AND revision=$3 AND revision<$4").bind(id.as_str()).bind(now).bind(input.expected_revision).bind(MAX_DEVICE_REVISION).execute(&mut *connection).await?.rows_affected();
                                if changed != 1 {return Err(stale_device().into());}
                            }
                            Ok(
                                device_snapshot(connection, &input.proof, &environment, clock.now().get())
                                    .await?,
                            )
                        })
                    })
                    .await,
            )
        })
    }
    fn sign_out(
        &self,
        proof: DeviceProof,
        audience: RequestAudience,
        now: i64,
    ) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), |connection| {
                        let proof = proof.clone();
                        let audience = audience.clone();
                        let holdings = self.holdings.clone();
                        let environment = self.environment.clone();
                        let invalidation=self.invalidation.clone();
                        let clock=self.clock.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let now=clock.now().get().max(now);
                            if let Some(session) =
                                read_session(connection, &proof, &audience, &environment, now, &holdings)
                                    .await?
                            {
                                match audience {
                                    RequestAudience::Account => {
                                        let id = browser_key(connection,proof.browser.as_ref()).await?.ok_or_else(denied)?;
                                        expire_family(connection, &session.id, &environment, invalidation.as_ref()).await?;
                                        select_browser(connection, &id, None, now, &environment, invalidation.as_ref()).await?;
                                    }
                                    RequestAudience::Holding { .. } => {
                                        let parent:String=sqlx::query_scalar("SELECT parent_session_id FROM sso_session_bindings WHERE session_id=$1 AND environment=$2").bind(&session.id).bind(&environment).fetch_one(&mut *connection).await?;
                                        let browser:String=sqlx::query_scalar("SELECT browser_id FROM account_device_accounts WHERE session_id=$1").bind(&parent).fetch_one(&mut *connection).await?;
                                        expire_family(connection,&parent,&environment,invalidation.as_ref()).await?;
                                        select_browser(connection,&SecretDigest::new(browser)?,None,now,&environment,invalidation.as_ref()).await?;
                                    }
                                }
                            }
                            Ok(())
                        })
                    })
                    .await,
            )
        })
    }
}

pub(crate) struct IssuedAccountSession {
    pub proof: DeviceProof,
    pub browser: SecretDigest,
    pub cookie_digest: SecretDigest,
    pub session_id: String,
    pub session_digest: SecretDigest,
    pub handle: String,
    pub user_id: String,
    pub now: i64,
}
pub(crate) async fn issue_account_session(
    connection: &mut PgConnection,
    input: IssuedAccountSession,
    environment: &str,
    invalidation: &dyn AccountWorkContextInvalidation,
    clock: &(dyn Clock + Send + Sync),
    holdings: &ConfiguredHoldingRegistry,
) -> TxResult<CompletedSignIn> {
    let accounts=sqlx::query_scalar::<_,String>("SELECT s.\"userId\" FROM account_device_accounts a JOIN session s ON s.id=a.session_id JOIN sso_session_bindings b ON b.session_id=s.id WHERE a.browser_id=$1 AND b.audience='account' AND b.environment=$2 AND b.parent_session_id IS NULL AND b.absolute_expires_at>$3 AND s.\"expiresAt\"::timestamptz>to_timestamp($3::double precision/1000)").bind(input.browser.as_str()).bind(environment).bind(input.now).fetch_all(&mut *connection).await?;
    if accounts.len() >= MAXIMUM_SAVED_ACCOUNTS && !accounts.contains(&input.user_id) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "DEVICE_ACCOUNT_LIMIT",
            "Remove a saved account before adding another.",
        )
        .into());
    }
    let old=sqlx::query_scalar::<_,String>("SELECT a.session_id FROM account_device_accounts a JOIN session s ON s.id=a.session_id WHERE a.browser_id=$1 AND s.\"userId\"=$2").bind(input.browser.as_str()).bind(&input.user_id).fetch_all(&mut *connection).await?;
    for id in old {
        expire_family(connection, &id, environment, invalidation).await?;
    }
    let expires = input
        .now
        .checked_add(SESSION_LIFETIME_MS)
        .ok_or_else(denied)?;
    sqlx::query(r#"INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","userId") VALUES($1,to_char(to_timestamp($2::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),$3,to_char(to_timestamp($4::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),to_char(to_timestamp($4::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),$5)"#).bind(&input.session_id).bind(expires).bind(input.session_digest.as_str()).bind(input.now).bind(&input.user_id).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,authenticated_at,absolute_expires_at) VALUES($1,'account',$2,$3,$4)").bind(&input.session_id).bind(environment).bind(input.now).bind(expires).execute(&mut *connection).await?;
    sqlx::query(
        "INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES($1,$2,$3)",
    )
    .bind(&input.session_id)
    .bind(input.browser.as_str())
    .bind(&input.handle)
    .execute(&mut *connection)
    .await?;
    select_browser(
        connection,
        &input.browser,
        Some(&input.session_id),
        input.now,
        environment,
        invalidation,
    )
    .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *connection)
        .await?;
    let now = clock.now().get();
    let mut proof = input.proof;
    proof.browser = Some(input.cookie_digest);
    proof.active = Some(input.session_digest.clone());
    proof.saved.push((input.handle, input.session_digest));
    let devices = device_snapshot(connection, &proof, environment, now).await?;
    let session = read_session(
        connection,
        &proof,
        &RequestAudience::Account,
        environment,
        now,
        holdings,
    )
    .await?
    .ok_or_else(denied)?;
    Ok(CompletedSignIn { session, devices })
}
