use crate::{
    adapters::account_postgres::{credential, finish, options},
    application::{account_mail::*, account_ports::AccountFuture},
    domain::{
        account_auth::{EmailAddress, OpaqueSecret, SecretDigest},
        account_credentials::CredentialProofKind,
        account_mail::*,
    },
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgRequestLease, PgTransactionError, PgUnitOfWork,
};
use erpx_server_shared::public::{BusinessError, Clock};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
type Tx<T> = Result<T, PgTransactionError<BusinessError>>;
pub struct PostgresAccountMail {
    pool: DatabasePool,
    unit: PgUnitOfWork,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountMail {
    pub fn new(pool: DatabasePool, clock: Arc<dyn Clock + Send + Sync>) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool.clone()),
            pool,
            clock,
        }
    }
}
async fn write_locks(c: &mut PgConnection) -> Tx<()> {
    sqlx::query("LOCK TABLE \"user\",account,principals,native_account_proofs,native_account_mail_outbox,native_account_mail_events IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
async fn read_locks(c: &mut PgConnection) -> Tx<()> {
    sqlx::query("LOCK TABLE \"user\",account,principals,native_account_proofs,native_account_mail_outbox IN SHARE MODE").execute(c).await?;
    Ok(())
}
fn kind(s: &str) -> Result<CredentialProofKind, BusinessError> {
    match s {
        "email_verification" => Ok(CredentialProofKind::EmailVerification),
        "password_reset" => Ok(CredentialProofKind::PasswordReset),
        _ => Err(account_mail_unavailable()),
    }
}
async fn event(
    c: &mut PgConnection,
    id: &str,
    attempt: i64,
    action: &str,
    code: Option<&str>,
    now: i64,
) -> Tx<()> {
    sqlx::query("INSERT INTO native_account_mail_events(id,outbox_id,attempt,action,failure_code,created_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(outbox_id,attempt,action) DO NOTHING")
 .bind(format!("native-mail-{id}-{attempt}-{action}")).bind(id).bind(attempt).bind(action).bind(code).bind(now).execute(c).await?;
    Ok(())
}
async fn status(
    c: &mut PgConnection,
    target: (&str, i64),
    state: &str,
    action: &str,
    code: Option<&str>,
    next: i64,
    now: i64,
) -> Tx<()> {
    let (id, attempt) = target;
    sqlx::query("UPDATE native_account_mail_outbox SET state=$2,failure_code=$3,next_attempt_at=$4,lease_token=NULL,lease_until=NULL,updated_at=$5 WHERE id=$1").bind(id).bind(state).bind(code).bind(next).bind(now).execute(&mut *c).await?;
    event(c, id, attempt, action, code, now).await
}
async fn recover(c: &mut PgConnection, now: i64) -> Tx<()> {
    let rows=sqlx::query("SELECT id,attempts,expires_at,provider_kind,dispatch_started_at,idempotency_started_at FROM native_account_mail_outbox WHERE state='running' AND lease_until<=$1 ORDER BY created_at,id LIMIT 100 FOR UPDATE").bind(now).fetch_all(&mut *c).await?;
    for row in rows {
        let id: String = row.try_get("id")?;
        let attempts: i64 = row.try_get("attempts")?;
        let expires: i64 = row.try_get("expires_at")?;
        let started: Option<i64> = row.try_get("dispatch_started_at")?;
        let resend = row
            .try_get::<Option<String>, _>("provider_kind")?
            .as_deref()
            == Some("resend");
        let retry_unknown = resend
            && row
                .try_get::<Option<i64>, _>("idempotency_started_at")?
                .is_some_and(|first| {
                    account_mail_can_retry_unknown(
                        AccountMailProviderKind::Resend,
                        first,
                        expires,
                        now,
                    )
                });
        if expires <= now {
            status(
                c,
                (&id, attempts),
                "cancelled",
                "cancelled",
                Some(AccountMailFailure::ProofUnavailable.code()),
                now,
                now,
            )
            .await?;
        } else if started.is_some() && !retry_unknown {
            status(
                c,
                (&id, attempts),
                "failed",
                "uncertain",
                Some(AccountMailFailure::DeliveryUncertain.code()),
                now,
                now,
            )
            .await?;
        } else if attempts >= ACCOUNT_MAIL_MAX_ATTEMPTS {
            status(
                c,
                (&id, attempts),
                "failed",
                "failed",
                Some("EMAIL_DELIVERY_INTERRUPTED"),
                now,
                now,
            )
            .await?;
        } else {
            status(
                c,
                (&id, attempts),
                "queued",
                "retry_scheduled",
                Some("EMAIL_DELIVERY_INTERRUPTED"),
                now,
                now,
            )
            .await?;
        }
    }
    Ok(())
}
fn lease_digest(job: &AccountMailAttempt) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erpx-account-mail-dispatch-lease-v1:");
    digest.update(job.lease.expose().as_bytes());
    hex::encode(digest.finalize())
}
async fn cancel_if_owned(c: &mut PgConnection, job: &AccountMailAttempt, now: i64) -> Tx<()> {
    let changed=sqlx::query("UPDATE native_account_mail_outbox SET state='cancelled',failure_code='ACCOUNT_MAIL_PROOF_UNAVAILABLE',lease_token=NULL,lease_until=NULL,updated_at=$4 WHERE id=$1 AND lease_token=$2 AND attempts=$3 AND state='running'").bind(&job.id).bind(job.lease.expose()).bind(job.attempts).bind(now).execute(&mut *c).await?.rows_affected();
    if changed == 1 {
        event(
            c,
            &job.id,
            job.attempts,
            "cancelled",
            Some(AccountMailFailure::ProofUnavailable.code()),
            now,
        )
        .await?;
    }
    Ok(())
}
async fn current(c: &mut PgConnection, job: &AccountMailAttempt, now: i64) -> Tx<bool> {
    let row=sqlx::query("SELECT p.*,o.payload_ciphertext,o.kind outbox_kind,o.user_id outbox_user,o.expires_at outbox_expires FROM native_account_mail_outbox o JOIN native_account_proofs p ON p.id=o.proof_id WHERE o.id=$1 AND o.state='running' AND o.lease_token=$2 AND o.attempts=$3 AND o.lease_until>$4")
 .bind(&job.id).bind(job.lease.expose()).bind(job.attempts).bind(now).fetch_optional(&mut *c).await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let email = EmailAddress::new(&row.try_get::<String, _>("email")?)?;
    let user = credential(c, &email).await?;
    let proof_kind = kind(&row.try_get::<String, _>("kind")?)?;
    let stable = row.try_get::<String, _>("id")? == job.proof_id
        && row.try_get::<String, _>("user_id")? == job.user_id
        && row.try_get::<String, _>("outbox_user")? == job.user_id
        && row.try_get::<String, _>("outbox_kind")? == job.kind.as_str()
        && proof_kind == job.kind
        && email == job.email
        && row.try_get::<String, _>("token_digest")? == job.token_digest.as_str()
        && row.try_get::<String, _>("payload_ciphertext")? == job.payload_ciphertext
        && row.try_get::<i64, _>("expires_at")? == job.expires_at
        && row.try_get::<i64, _>("outbox_expires")? == job.expires_at;
    let expected: String = row.try_get("credential_fingerprint")?;
    let facts = CurrentAccountMailProof {
        kind: proof_kind,
        created_at: row.try_get("created_at")?,
        expires_at: row
            .try_get::<i64, _>("expires_at")?
            .min(row.try_get("outbox_expires")?),
        consumed: row.try_get::<Option<i64>, _>("consumed_at")?.is_some(),
        identity_current: user
            .as_ref()
            .is_some_and(|u| u.user_id == job.user_id && u.can_authenticate(false)),
        credential_current: user.as_ref().is_some_and(|u| {
            hex::encode(Sha256::digest(u.password_hash.as_str().as_bytes())) == expected
        }),
        recipient_current: stable
            && user
                .as_ref()
                .is_some_and(|u| EmailAddress::new(&u.email).is_ok_and(|email| email == job.email)),
        email_verified: user.as_ref().is_some_and(|u| u.email_verified),
    };
    Ok(account_mail_proof_available(&facts, now))
}
impl AccountMailRepository for PostgresAccountMail {
    fn claim(&self, lease: OpaqueSecret, _now: i64) -> AccountFuture<'_, AccountMailClaim> {
        Box::pin(async move {
            finish(self.unit.run(options(),|c|{let lease=lease.clone();let clock=self.clock.clone();Box::pin(async move {
   write_locks(c).await?;let now=clock.now().get();recover(c,now).await?;
   let row=sqlx::query("UPDATE native_account_mail_outbox SET state='running',attempts=attempts+1,lease_token=$1,lease_until=$2,dispatch_started_at=NULL,dispatch_lease_digest=NULL,updated_at=$3 WHERE id=(SELECT id FROM native_account_mail_outbox WHERE state='queued' AND attempts<6 AND next_attempt_at<=$3 ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
    .bind(lease.expose()).bind(now.checked_add(ACCOUNT_MAIL_LEASE_MS).ok_or_else(account_mail_unavailable)?).bind(now).fetch_optional(&mut *c).await?;
   let Some(row)=row else{return Ok(AccountMailClaim::Idle);};
   let id:String=row.try_get("id")?;let proof_id:String=row.try_get("proof_id")?;let count:i64=row.try_get("attempts")?;
   let proof=sqlx::query("SELECT email,token_digest FROM native_account_proofs WHERE id=$1").bind(&proof_id).fetch_one(&mut *c).await?;
   let job=AccountMailAttempt{id:id.clone(),proof_id,user_id:row.try_get("user_id")?,kind:kind(&row.try_get::<String,_>("kind")?)?,email:EmailAddress::new(&proof.try_get::<String,_>("email")?)?,token_digest:SecretDigest::new(proof.try_get("token_digest")?)?,payload_ciphertext:row.try_get("payload_ciphertext")?,attempts:count,lease,lease_until:row.try_get("lease_until")?,expires_at:row.try_get("expires_at")?};
   event(c,&id,count,"claimed",None,now).await?;
   if !current(c,&job,now).await? {status(c,(&id,count),"cancelled","cancelled",Some(AccountMailFailure::ProofUnavailable.code()),now,now).await?;return Ok(AccountMailClaim::Cancelled);}
   Ok(AccountMailClaim::Ready(job))
  })}).await)
        })
    }
    fn begin_dispatch<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        profile: &'a AccountMailProviderProfile,
        fingerprint: &'a str,
        _now: i64,
    ) -> AccountFuture<'a, bool> {
        let profile_id = profile.profile_id.clone();
        let provider_kind = profile.kind;
        let fingerprint = fingerprint.to_owned();
        Box::pin(async move {
            finish(self.unit.run(options(),|c|{let job=job.clone();let clock=self.clock.clone();let profile_id=profile_id.clone();let fingerprint=fingerprint.clone();Box::pin(async move {
   write_locks(c).await?;let now=clock.now().get();
   if !current(c,&job,now).await? {cancel_if_owned(c,&job,now).await?;return Ok(false);}
   let row=sqlx::query("SELECT provider_kind,provider_profile_id,message_fingerprint FROM native_account_mail_outbox WHERE id=$1").bind(&job.id).fetch_one(&mut *c).await?;
   if row.try_get::<Option<String>,_>("message_fingerprint")?.is_some_and(|old|old!=fingerprint)
    ||row.try_get::<Option<String>,_>("provider_kind")?.is_some_and(|old|old!=provider_kind.as_str())
    ||row.try_get::<Option<String>,_>("provider_profile_id")?.is_some_and(|old|old!=profile_id) {
     status(c,(&job.id,job.attempts),"failed","failed",Some(AccountMailFailure::MessageChanged.code()),now,now).await?;return Ok(false);
   }
   sqlx::query("UPDATE native_account_mail_outbox SET provider_kind=$2,provider_profile_id=$3,message_fingerprint=$4,dispatch_started_at=$5,idempotency_started_at=COALESCE(idempotency_started_at,$5),dispatch_lease_digest=$6,updated_at=$5 WHERE id=$1")
    .bind(&job.id).bind(provider_kind.as_str()).bind(profile_id).bind(fingerprint).bind(now).bind(lease_digest(&job)).execute(&mut *c).await?;
   event(c,&job.id,job.attempts,"dispatch_started",None,now).await?;Ok(true)
  })}).await)
        })
    }
    fn dispatch_current<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        message: AccountEmailMessage,
        provider: Arc<dyn AccountMailProvider>,
    ) -> AccountFuture<'a, AccountMailDispatch> {
        Box::pin(async move {
            let invoked = Arc::new(AtomicBool::new(false));
            let marker = invoked.clone();
            let job = job.clone();
            let clock = self.clock.clone();
            let result = PgRequestLease::new(self.pool.clone())
                .run(move |c| {
                    Box::pin(async move {
                        read_locks(c).await?;
                        let now = clock.now().get();
                        if !current(c, &job, now).await? {
                            return Ok::<_, PgTransactionError<BusinessError>>(
                                AccountMailDispatch::Cancelled,
                            );
                        }
                        let binding=sqlx::query("SELECT provider_kind,provider_profile_id,message_fingerprint,dispatch_lease_digest FROM native_account_mail_outbox WHERE id=$1").bind(&job.id).fetch_one(&mut *c).await?;
                        if binding.try_get::<Option<String>,_>("message_fingerprint")?.as_deref()!=Some(&provider.profile().fingerprint(&message)?) || binding.try_get::<Option<String>,_>("provider_kind")?.as_deref()!=Some(provider.profile().kind.as_str()) || binding.try_get::<Option<String>,_>("provider_profile_id")?.as_deref()!=Some(&provider.profile().profile_id) || binding.try_get::<Option<String>,_>("dispatch_lease_digest")?.as_deref()!=Some(&lease_digest(&job)) {
                            return Ok(AccountMailDispatch::Completed(AccountMailSendOutcome::Rejected(AccountMailFailure::MessageChanged)));
                        }
                        let remaining = job
                            .expires_at
                            .min(job.lease_until)
                            .saturating_sub(now)
                            .min(ACCOUNT_MAIL_PROVIDER_TIMEOUT_MS);
                        if remaining <= 1000 {
                            return Ok(AccountMailDispatch::Cancelled);
                        }
                        marker.store(true, Ordering::SeqCst);
                        let outcome = tokio::time::timeout(
                            Duration::from_millis(
                                u64::try_from(remaining - 500)
                                    .map_err(|_| account_mail_unavailable())?,
                            ),
                            provider.send(&message),
                        )
                        .await
                        .unwrap_or(AccountMailSendOutcome::Uncertain(
                            AccountMailFailure::DeliveryUncertain,
                        ));
                        Ok(AccountMailDispatch::Completed(outcome))
                    })
                })
                .await;
            Ok(match result {
                Ok(value) => value,
                Err(_) => AccountMailDispatch::Completed(if invoked.load(Ordering::SeqCst) {
                    AccountMailSendOutcome::Uncertain(AccountMailFailure::DeliveryUncertain)
                } else {
                    AccountMailSendOutcome::Retryable(AccountMailFailure::ProviderUnavailable)
                }),
            })
        })
    }
    fn finish<'a>(
        &'a self,
        job: &'a AccountMailAttempt,
        outcome: AccountMailDispatch,
        _now: i64,
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            let outcome = Arc::new(outcome);
            finish(self.unit.run(options(),|c|{let job=job.clone();let clock=self.clock.clone();let outcome=outcome.clone();Box::pin(async move {
   write_locks(c).await?;let now=clock.now().get();
   let row=sqlx::query("SELECT state,attempts,lease_token,provider_kind,idempotency_started_at,dispatch_lease_digest,failure_code FROM native_account_mail_outbox WHERE id=$1").bind(&job.id).fetch_one(&mut *c).await?;
   if row.try_get::<i64,_>("attempts")?!=job.attempts {return Ok(());}
   let state:String=row.try_get("state")?;
   if row.try_get::<Option<String>,_>("lease_token")?.as_deref()!=Some(job.lease.expose()) || state!="running" {
       // A reset may cancel the proof after provider acceptance but before ACK.
       // Preserve cancellation and record only the observed transport fact.
       if row.try_get::<Option<String>,_>("dispatch_lease_digest")?.as_deref()==Some(&lease_digest(&job)) && state=="cancelled"
           && let AccountMailDispatch::Completed(AccountMailSendOutcome::Accepted{provider_id})=outcome.as_ref()
           && !provider_id.trim().is_empty() && provider_id.len()<=512 && !provider_id.chars().any(|c|c.is_control()) {
           sqlx::query("UPDATE native_account_mail_outbox SET provider_id=$2,sent_at=$3 WHERE id=$1 AND state='cancelled'").bind(&job.id).bind(provider_id).bind(now).execute(&mut *c).await?;
           event(c,&job.id,job.attempts,"accepted",Some("PROOF_CANCELLED_AFTER_DISPATCH"),now).await?;
       }
       return Ok(());
   }
   match outcome.as_ref() {
    AccountMailDispatch::Cancelled=>status(c,(&job.id,job.attempts),"cancelled","cancelled",Some(AccountMailFailure::ProofUnavailable.code()),now,now).await?,
    AccountMailDispatch::Completed(AccountMailSendOutcome::Accepted{provider_id}) if !provider_id.trim().is_empty()&&provider_id.len()<=512&&!provider_id.chars().any(|c|c.is_control())=>{
     sqlx::query("UPDATE native_account_mail_outbox SET state='sent',provider_id=$2,sent_at=$3,lease_token=NULL,lease_until=NULL,failure_code=NULL,updated_at=$3 WHERE id=$1").bind(&job.id).bind(provider_id).bind(now).execute(&mut *c).await?;
     event(c,&job.id,job.attempts,"accepted",None,now).await?;
    },
    AccountMailDispatch::Completed(value)=>{
     let (reason,unknown,retry)=match value {AccountMailSendOutcome::Retryable(reason)=>(*reason,false,true),AccountMailSendOutcome::Rejected(reason)=>(*reason,false,false),AccountMailSendOutcome::Uncertain(reason)=>(*reason,true,true),AccountMailSendOutcome::Accepted{..}=>(AccountMailFailure::DeliveryUncertain,true,true)};
     let resend=row.try_get::<Option<String>,_>("provider_kind")?.as_deref()==Some("resend");
     let safe_unknown=resend&&row.try_get::<Option<i64>,_>("idempotency_started_at")?.is_some_and(|first|account_mail_can_retry_unknown(AccountMailProviderKind::Resend,first,job.expires_at,now));
     let next=if retry&&(!unknown||safe_unknown)&&now<job.expires_at {account_mail_next_attempt(job.attempts,now).filter(|next|*next<job.expires_at)}else{None};
     status(c,(&job.id,job.attempts),if next.is_some(){"queued"}else{"failed"},if unknown{"uncertain"}else if next.is_some(){"retry_scheduled"}else{"failed"},Some(reason.code()),next.unwrap_or(now),now).await?;
    }
   }
   Ok(())
  })}).await)
        })
    }
}
