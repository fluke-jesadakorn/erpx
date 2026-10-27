use super::{
    account_mfa_crypto::AccountMfaKeyring,
    account_postgres::{TxResult, finish, options},
    account_sessions::{lock_session_reads, read_session},
};
use crate::{
    application::{account_ports::DeviceProof, account_security::AccountMfaRepository},
    domain::{
        account_auth::{AccountSession, RequestAudience},
        account_mfa::{
            MFA_FRESH_MS, MfaEnrollment, MfaVerified, candidate_counters, fresh_until, mfa_error,
            recent_authentication, valid_code,
        },
    },
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub(crate) fn signed_out() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "SIGN_IN_REQUIRED",
        "Sign in to manage your security.",
    )
}
pub(crate) async fn current(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    audience: &RequestAudience,
    environment: &str,
    now: i64,
) -> TxResult<AccountSession> {
    lock_session_reads(connection).await?;
    if proof.saved.len() > 5 {
        return Err(signed_out().into());
    }
    let tokens = proof
        .active
        .iter()
        .map(|value| value.as_str())
        .chain(proof.saved.iter().map(|(_, value)| value.as_str()))
        .collect::<Vec<_>>();
    // Bound the shared session reader's identity projection before it transfers
    // any display field. These rows remain locked through the subsequent read.
    let oversized:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM session s JOIN \"user\" u ON u.id=s.\"userId\" WHERE s.token=ANY($1::text[]) AND (octet_length(s.id)>128 OR octet_length(u.id)>128 OR octet_length(u.name)>4096 OR octet_length(u.email)>320 OR octet_length(u.image)>4096))").bind(tokens).fetch_one(&mut *connection).await?;
    if oversized {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "ACCOUNT_SESSION_SOURCE_LIMIT",
            "The current session identity exceeds its safe read budget.",
        )
        .into());
    }
    Ok(read_session(connection, proof, audience, environment, now)
        .await?
        .ok_or_else(signed_out)?)
}
pub struct PostgresAccountMfa {
    unit: PgUnitOfWork,
    environment: String,
    keys: Arc<AccountMfaKeyring>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountMfa {
    pub fn new(
        pool: DatabasePool,
        environment: String,
        keys: Arc<AccountMfaKeyring>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Result<Self, BusinessError> {
        if environment != "development" {
            return Err(BusinessError::invalid(
                "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
                "The Account environment is not enabled.",
            ));
        }
        Ok(Self {
            unit: PgUnitOfWork::new(pool),
            environment,
            keys,
            clock,
        })
    }
    /// Counter commit is deliberately independent of the subsequent business
    /// result: malformed/incorrect codes and already-enabled enrollment count.
    async fn attempt(
        &self,
        proof: DeviceProof,
        audience: RequestAudience,
        enroll: bool,
    ) -> Result<(), BusinessError> {
        let environment = self.environment.clone();
        let clock = self.clock.clone();
        let count=finish(self.unit.run_once(options(),move|connection|Box::pin(async move{
            let actor=current(connection,&proof,&audience,&environment,clock.now().get()).await?;
            let category=if enroll{"mfa-enroll"}else{"mfa"};
            let key=hex::encode(Sha256::digest(format!("{category}:{}",actor.user.id.to_lowercase())));
            let now=clock.now().get();
            let count:i32=sqlx::query_scalar("INSERT INTO iam_throttles(key,window_start,attempts) VALUES($1,$2,1) ON CONFLICT(key) DO UPDATE SET window_start=CASE WHEN iam_throttles.window_start<$2-300000 THEN $2 ELSE iam_throttles.window_start END,attempts=CASE WHEN iam_throttles.window_start<$2-300000 THEN 1 ELSE LEAST(iam_throttles.attempts::bigint+1,2147483647)::integer END RETURNING attempts").bind(key).bind(now).fetch_one(connection).await?;
            Ok(count)
        })).await)?;
        if count > if enroll { 3 } else { 8 } {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IAM_RATE_LIMIT",
                "Too many attempts. Wait five minutes before trying again.",
            ));
        }
        Ok(())
    }
}
impl AccountMfaRepository for PostgresAccountMfa {
    fn enroll<'a>(
        &'a self,
        proof: DeviceProof,
        audience: RequestAudience,
    ) -> crate::application::account_ports::AccountFuture<'a, MfaEnrollment> {
        Box::pin(async move {
            self.attempt(proof.clone(), audience.clone(), true).await?;
            let environment = self.environment.clone();
            let clock = self.clock.clone();
            let keys = self.keys.clone();
            finish(self.unit.run_once(options(),move|connection|Box::pin(async move{
            let actor=current(connection,&proof,&audience,&environment,clock.now().get()).await?;
            sqlx::query("LOCK TABLE iam_mfa,iam_mfa_sessions IN SHARE ROW EXCLUSIVE MODE").execute(&mut *connection).await?;
            let enabled:Option<bool>=sqlx::query_scalar("SELECT enabled FROM iam_mfa WHERE user_id=$1").bind(&actor.user.id).fetch_optional(&mut *connection).await?;
            if enabled==Some(true){return Err(BusinessError::new(FailureKind::Conflict,"MFA_ALREADY_ENABLED","Your authenticator is already configured.").into());}
            recent_authentication(actor.authenticated_at,clock.now().get())?;
            let secret=keys.generate()?;let envelope=keys.seal(&actor.user.id,&secret)?;
            sqlx::query("INSERT INTO iam_mfa(user_id,secret) VALUES($1,$2) ON CONFLICT(user_id) DO UPDATE SET secret=excluded.secret WHERE iam_mfa.enabled=false").bind(&actor.user.id).bind(&envelope).execute(&mut *connection).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
            let final_actor=current(connection,&proof,&audience,&environment,clock.now().get()).await?;
            recent_authentication(final_actor.authenticated_at,clock.now().get())?;
            let stable:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM iam_mfa WHERE user_id=$1 AND secret=$2 AND enabled=false)").bind(&actor.user.id).bind(envelope).fetch_one(connection).await?;
            if !stable||final_actor.id!=actor.id||final_actor.user.id!=actor.user.id{return Err(signed_out().into());}
            let secret=secret.base32();
            Ok(MfaEnrollment{uri:crate::domain::account_mfa::enrollment_uri(&actor.user.id,&secret),secret})
        })).await)
        })
    }
    fn verify<'a>(
        &'a self,
        proof: DeviceProof,
        audience: RequestAudience,
        code: String,
    ) -> crate::application::account_ports::AccountFuture<'a, MfaVerified> {
        Box::pin(async move {
            self.attempt(proof.clone(), audience.clone(), false).await?;
            if !valid_code(&code) {
                return Err(BusinessError::invalid(
                    "MFA_INVALID",
                    "Enter a six-digit authenticator code.",
                ));
            }
            let environment = self.environment.clone();
            let clock = self.clock.clone();
            let keys = self.keys.clone();
            finish(self.unit.run_once(options(),move|connection|Box::pin(async move{
            let actor=current(connection,&proof,&audience,&environment,clock.now().get()).await?;
            sqlx::query("LOCK TABLE iam_mfa,iam_mfa_sessions IN SHARE ROW EXCLUSIVE MODE").execute(&mut *connection).await?;
            let row=sqlx::query("SELECT CASE WHEN octet_length(secret)<=256 THEN secret END secret,last_counter FROM iam_mfa WHERE user_id=$1 FOR UPDATE").bind(&actor.user.id).fetch_optional(&mut *connection).await?.ok_or_else(||BusinessError::new(FailureKind::Conflict,"MFA_NOT_ENROLLED","Set up your authenticator first."))?;
            let envelope:String=row.try_get("secret")?;let previous:i64=row.try_get("last_counter")?;
            let secret=keys.open(&actor.user.id,&envelope)?;let now=clock.now().get();
            let mut matched=None;
            for counter in candidate_counters(now){if counter>=0&&counter>previous&&bool::from(secret.code(counter)?.as_bytes().ct_eq(code.as_bytes())){matched=Some(counter);break;}}
            let counter=matched.ok_or_else(||mfa_error("MFA_INVALID","The code is invalid or already used."))?;
            // Re-seal under the current key while preserving the original secret.
            let rotated=keys.seal(&actor.user.id,&secret)?;
            let updated=sqlx::query("UPDATE iam_mfa SET enabled=true,last_counter=$1,secret=$4 WHERE user_id=$2 AND last_counter=$3 AND secret=$5").bind(counter).bind(&actor.user.id).bind(previous).bind(&rotated).bind(envelope).execute(&mut *connection).await?.rows_affected();
            if updated!=1{return Err(mfa_error("MFA_REPLAY","This code was already used.").into());}
            sqlx::query("INSERT INTO iam_mfa_sessions(session_id,user_id,verified_at) VALUES($1,$2,$3) ON CONFLICT(session_id) DO UPDATE SET verified_at=excluded.verified_at WHERE iam_mfa_sessions.user_id=excluded.user_id").bind(&actor.id).bind(&actor.user.id).bind(now).execute(&mut *connection).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
            let final_now=clock.now().get();let final_actor=current(connection,&proof,&audience,&environment,final_now).await?;
            let stable:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM iam_mfa m JOIN iam_mfa_sessions f ON f.user_id=m.user_id WHERE m.user_id=$1 AND m.enabled=true AND m.secret=$2 AND m.last_counter=$3 AND f.session_id=$4 AND f.verified_at=$5)").bind(&actor.user.id).bind(rotated).bind(counter).bind(&actor.id).bind(now).fetch_one(connection).await?;
            fresh_until(true,now,final_now,final_actor.expires_at)?;
            if !stable||final_actor.id!=actor.id||final_actor.user.id!=actor.user.id{return Err(signed_out().into());}
            Ok(MfaVerified{verified_until:now+MFA_FRESH_MS})
        })).await)
        })
    }
}

/// Evidence is sealed to one live session/audience AND one Control transaction.
/// It grants no Company/Platform permission. Retain the lease through the owner
/// commit and call require_current at its final authorization boundary.
pub struct FreshMfaProof {
    session: AccountSession,
    proof: DeviceProof,
    audience: RequestAudience,
    environment: String,
    backend: i32,
    transaction: String,
    database: String,
    expires_at: i64,
    source: String,
    keys: Arc<AccountMfaKeyring>,
}
impl FreshMfaProof {
    pub fn user_id(&self) -> &str {
        &self.session.user.id
    }
    pub fn session_id(&self) -> &str {
        &self.session.id
    }
    pub fn audience(&self) -> &RequestAudience {
        &self.audience
    }
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }
    pub async fn require_current(&self, connection: &mut PgConnection, now: i64) -> TxResult<()> {
        let (backend, transaction, database) = transaction_identity(connection).await?;
        if backend != self.backend
            || transaction != self.transaction
            || database != self.database
            || now >= self.expires_at
        {
            return Err(mfa_error(
                "FRESH_MFA_REQUIRED",
                "Verify your authenticator before this sensitive change.",
            )
            .into());
        }
        let proof = require_fresh_mfa_in_transaction(
            connection,
            &self.proof,
            &self.audience,
            &self.environment,
            self.keys.clone(),
            now,
        )
        .await?;
        if proof.session.id != self.session.id
            || proof.session.user.id != self.session.user.id
            || proof.session.authenticated_at != self.session.authenticated_at
            || proof.session.expires_at != self.session.expires_at
            || proof.source != self.source
        {
            return Err(signed_out().into());
        }
        Ok(())
    }
}
async fn transaction_identity(
    connection: &mut PgConnection,
) -> Result<(i32, String, String), sqlx::Error> {
    let row=sqlx::query("SELECT pg_backend_pid() backend,pg_current_xact_id()::text transaction,current_database()||'/'||pg_postmaster_start_time()::text database").fetch_one(connection).await?;
    Ok((
        row.try_get("backend")?,
        row.try_get("transaction")?,
        row.try_get("database")?,
    ))
}
pub async fn require_fresh_mfa_in_transaction(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    audience: &RequestAudience,
    environment: &str,
    keys: Arc<AccountMfaKeyring>,
    now: i64,
) -> TxResult<FreshMfaProof> {
    if environment != "development" {
        return Err(signed_out().into());
    }
    let result:TxResult<_>=async{
        let actor=current(connection,proof,audience,environment,now).await?;
        sqlx::query("LOCK TABLE iam_mfa,iam_mfa_sessions IN SHARE MODE").execute(&mut *connection).await?;
        let row=sqlx::query("SELECT m.enabled,CASE WHEN octet_length(m.secret)<=256 THEN m.secret END secret,f.verified_at,CASE WHEN octet_length(jsonb_build_object('mfa',to_jsonb(m),'verification',to_jsonb(f))::text)<=4096 THEN jsonb_build_object('mfa',to_jsonb(m),'verification',to_jsonb(f))::text END source FROM iam_mfa m JOIN iam_mfa_sessions f ON f.user_id=m.user_id WHERE m.user_id=$1 AND f.session_id=$2 FOR SHARE OF m,f").bind(&actor.user.id).bind(&actor.id).fetch_optional(&mut *connection).await?.ok_or_else(||mfa_error("FRESH_MFA_REQUIRED","Verify your authenticator before this sensitive change."))?;
        let secret:String=row.try_get("secret")?;
        keys.open(&actor.user.id,&secret)?;
        let expires_at=fresh_until(row.try_get("enabled")?,row.try_get("verified_at")?,now,actor.expires_at)?;
        let (backend,transaction,database)=transaction_identity(connection).await?;
        Ok(FreshMfaProof{session:actor,proof:proof.clone(),audience:audience.clone(),environment:environment.into(),backend,transaction,database,expires_at,source:row.try_get("source")?,keys})
    }.await;
    result
}
