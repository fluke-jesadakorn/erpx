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
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

pub(crate) type TxResult<T> = Result<T, PgTransactionError<BusinessError>>;

/// Control-only persistence. Every write rechecks current proof and revision in one UoW.
pub struct PostgresAccountRepository {
    unit: PgUnitOfWork,
    environment: String,
    require_verification: bool,
}
impl PostgresAccountRepository {
    pub fn from_control_database(
        pool: DatabasePool,
        environment: String,
        require_verification: bool,
    ) -> Result<Self, BusinessError> {
        if environment != "development" {
            return Err(BusinessError::invalid(
                "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
                "The Account environment has not been enabled.",
            ));
        }
        Ok(Self {
            unit: PgUnitOfWork::new(pool),
            environment,
            require_verification,
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
pub(crate) async fn ensure_browser(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    candidate: &SecretDigest,
    now: i64,
) -> TxResult<(SecretDigest, bool)> {
    if let Some(id) = &proof.browser
        && browser(connection, id).await?.is_some()
    {
        return Ok((id.clone(), false));
    }
    sqlx::query("INSERT INTO account_device_browsers(id,revision,updated_at) VALUES($1,0,$2)")
        .bind(candidate.as_str())
        .bind(now)
        .execute(connection)
        .await?;
    Ok((candidate.clone(), true))
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
async fn revoke_children(connection: &mut PgConnection, browser_id: &SecretDigest) -> TxResult<()> {
    sqlx::query("DELETE FROM session WHERE id IN (SELECT b.session_id FROM sso_session_bindings b JOIN account_device_accounts a ON a.session_id=b.parent_session_id WHERE a.browser_id=$1)").bind(browser_id.as_str()).execute(connection).await?;
    Ok(())
}
pub(crate) async fn select_browser(
    connection: &mut PgConnection,
    id: &SecretDigest,
    active: Option<&str>,
    now: i64,
) -> TxResult<()> {
    revoke_children(connection, id).await?;
    let count = sqlx::query("UPDATE account_device_browsers SET active_session_id=$2,revision=revision+1,pending_nonce=NULL,pending_until=NULL,updated_at=$3 WHERE id=$1 AND revision<$4").bind(id.as_str()).bind(active).bind(now).bind(MAX_DEVICE_REVISION).execute(connection).await?.rows_affected();
    if count != 1 {
        return Err(stale_device().into());
    }
    Ok(())
}
pub(crate) async fn expire_family(connection: &mut PgConnection, session_id: &str) -> TxResult<()> {
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
            let (id, created) = ensure_browser(connection, &input.proof, &input.candidate_browser, input.now).await?;
            let row = browser(connection, &id).await?.ok_or_else(denied)?;
            revision(&row, input.expected_revision, input.now)?;
            let credential = credential(connection, &input.email).await?;
            // Capacity is a browser invariant, including sessions whose saved proof was not supplied.
            let accounts=sqlx::query_scalar::<_,String>("SELECT s.\"userId\" FROM account_device_accounts a JOIN session s ON s.id=a.session_id JOIN sso_session_bindings b ON b.session_id=s.id WHERE a.browser_id=$1 AND b.audience='account' AND b.environment=$2 AND b.parent_session_id IS NULL AND b.absolute_expires_at>$3 AND s.\"expiresAt\"::timestamptz>to_timestamp($3::double precision/1000)").bind(id.as_str()).bind(&environment).bind(input.now).fetch_all(&mut *connection).await?;
            if accounts.len() >= MAXIMUM_SAVED_ACCOUNTS && !credential.as_ref().is_some_and(|credential| accounts.contains(&credential.user_id)) { return Err(BusinessError::new(FailureKind::Conflict,"DEVICE_ACCOUNT_LIMIT","Remove a saved account before adding another.").into()); }
            let until = input.now.checked_add(SIGN_IN_RESERVATION_MS).ok_or_else(denied)?;
            sqlx::query("UPDATE account_device_browsers SET pending_nonce=$2,pending_until=$3,updated_at=$4 WHERE id=$1").bind(id.as_str()).bind(input.nonce.as_str()).bind(until).bind(input.now).execute(connection).await?;
            Ok(SignInReservation { browser:id,created_browser:created,nonce:input.nonce,credential })
        }) }).await)
        })
    }
    fn complete_sign_in(&self, input: CompleteSignIn) -> AccountFuture<'_, CompletedSignIn> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        let require_verification = self.require_verification;
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let row = browser(connection, &input.reservation.browser)
                                .await?
                                .ok_or_else(denied)?;
                            if row.pending.as_deref() != Some(input.reservation.nonce.as_str())
                                || !row.until.is_some_and(|until| until > input.now)
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
                            issue_account_session(
                                connection,
                                IssuedAccountSession {
                                    proof: input.proof,
                                    browser: input.reservation.browser,
                                    session_id: input.session_id,
                                    session_digest: input.session_digest,
                                    handle: input.handle,
                                    user_id: current.user_id,
                                    now: input.now,
                                },
                                &environment,
                            )
                            .await
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
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_reads(connection).await?;
                            Ok(
                                read_session(connection, &proof, &audience, &environment, now)
                                    .await?,
                            )
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
                            Ok(device_snapshot(connection, &proof, &environment, now).await?)
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
        now: i64,
    ) -> AccountFuture<'_, (DeviceAccounts, bool)> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let mut proof = proof.clone();
                        let candidate = candidate.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let (id, created) =
                                ensure_browser(connection, &proof, &candidate, now).await?;
                            proof.browser = Some(id);
                            Ok((
                                device_snapshot(connection, &proof, &environment, now).await?,
                                created,
                            ))
                        })
                    })
                    .await,
            )
        })
    }
    fn select_device(&self, input: DeviceSelection) -> AccountFuture<'_, SelectedDevice> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let id = input.proof.browser.as_ref().ok_or_else(denied)?;
                            let row = browser(connection, id).await?.ok_or_else(denied)?;
                            revision(&row, Some(input.expected_revision), input.now)?;
                            let selected =
                                live_accounts(connection, &input.proof, &environment, input.now)
                                    .await?
                                    .into_iter()
                                    .find(|account| account.handle == input.handle)
                                    .ok_or_else(denied)?;
                            select_browser(connection, id, Some(&selected.session.id), input.now)
                                .await?;
                            Ok(SelectedDevice {
                                devices: device_snapshot(
                                    connection,
                                    &input.proof,
                                    &environment,
                                    input.now,
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
                    .run(options(), |connection| {
                        let input = input.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            let id = input.proof.browser.as_ref().ok_or_else(denied)?;
                            let row = browser(connection, id).await?.ok_or_else(denied)?;
                            revision(&row, Some(input.expected_revision), input.now)?;
                            let target =
                                live_accounts(connection, &input.proof, &environment, input.now)
                                    .await?
                                    .into_iter()
                                    .find(|account| account.handle == input.handle)
                                    .ok_or_else(denied)?;
                            expire_family(connection, &target.session.id).await?;
                            if row.active.as_deref() == Some(target.session.id.as_str()) {
                                select_browser(connection, id, None, input.now).await?;
                            } else {
                                // Removing an inactive family does not select another account.
                                // Retain the current parent's children and advance only the CAS revision.
                                let changed=sqlx::query("UPDATE account_device_browsers SET revision=revision+1,updated_at=$2 WHERE id=$1 AND revision=$3 AND revision<$4").bind(id.as_str()).bind(input.now).bind(input.expected_revision).bind(MAX_DEVICE_REVISION).execute(&mut *connection).await?.rows_affected();
                                if changed != 1 {return Err(stale_device().into());}
                            }
                            Ok(
                                device_snapshot(connection, &input.proof, &environment, input.now)
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
                    .run(options(), |connection| {
                        let proof = proof.clone();
                        let audience = audience.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_writes(connection).await?;
                            if let Some(session) =
                                read_session(connection, &proof, &audience, &environment, now)
                                    .await?
                            {
                                match audience {
                                    RequestAudience::Account => {
                                        let id = proof.browser.as_ref().ok_or_else(denied)?;
                                        expire_family(connection, &session.id).await?;
                                        select_browser(connection, id, None, now).await?;
                                    }
                                    RequestAudience::Holding { .. } => {
                                        let parent:String=sqlx::query_scalar("SELECT parent_session_id FROM sso_session_bindings WHERE session_id=$1 AND environment=$2").bind(&session.id).bind(&environment).fetch_one(&mut *connection).await?;
                                        let browser:String=sqlx::query_scalar("SELECT browser_id FROM account_device_accounts WHERE session_id=$1").bind(&parent).fetch_one(&mut *connection).await?;
                                        expire_family(connection,&parent).await?;
                                        select_browser(connection,&SecretDigest::new(browser)?,None,now).await?;
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
        expire_family(connection, &id).await?;
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
    )
    .await?;
    let mut proof = input.proof;
    proof.browser = Some(input.browser);
    proof.active = Some(input.session_digest.clone());
    proof.saved.push((input.handle, input.session_digest));
    let devices = device_snapshot(connection, &proof, environment, input.now).await?;
    let session = read_session(
        connection,
        &proof,
        &RequestAudience::Account,
        environment,
        input.now,
    )
    .await?
    .ok_or_else(denied)?;
    Ok(CompletedSignIn { session, devices })
}
