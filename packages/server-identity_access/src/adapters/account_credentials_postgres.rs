use crate::{
    adapters::{
        account_postgres::{
            IssuedAccountSession, TxResult, browser, credential, ensure_browser, expire_family,
            finish, issue_account_session, options, revision,
        },
        account_sessions::{lock_session_reads, lock_session_writes, read_session},
    },
    application::{account_credentials::*, account_ports::*},
    domain::{account_auth::*, account_credentials::*},
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork};
use erpx_server_shared::public::{BusinessError, Clock};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

pub struct PostgresAccountCredentials {
    unit: PgUnitOfWork,
    environment: String,
    invitations: Arc<dyn InvitationCredentialAuthority>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountCredentials {
    pub fn from_control_database(
        pool: DatabasePool,
        environment: String,
        invitations: Arc<dyn InvitationCredentialAuthority>,
        clock: Arc<dyn Clock + Send + Sync>,
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
            invitations,
            clock,
        })
    }
}
async fn locks(connection: &mut PgConnection) -> TxResult<()> {
    lock_session_writes(connection).await?;
    sqlx::query("LOCK TABLE account,native_account_proofs,native_account_mail_outbox,native_account_credential_events,account_invitations,account_invitation_claims,holding_bindings IN SHARE ROW EXCLUSIVE MODE").execute(connection).await?;
    Ok(())
}
fn fingerprint(hash: &PasswordHash) -> String {
    hex::encode(Sha256::digest(hash.as_str().as_bytes()))
}
async fn audit(
    connection: &mut PgConnection,
    event_id: &str,
    user_id: &str,
    action: &str,
    proof_id: Option<&str>,
    now: i64,
) -> TxResult<()> {
    sqlx::query("INSERT INTO native_account_credential_events(id,user_id,action,proof_id,created_at) VALUES($1,$2,$3,$4,$5)").bind(event_id).bind(user_id).bind(action).bind(proof_id).bind(now).execute(connection).await?;
    Ok(())
}
async fn cancel_proofs(
    connection: &mut PgConnection,
    user_id: &str,
    kind: Option<&str>,
    now: i64,
) -> TxResult<()> {
    sqlx::query("UPDATE native_account_mail_outbox o SET state='cancelled',lease_token=NULL,lease_until=NULL,updated_at=$3 FROM native_account_proofs p WHERE p.id=o.proof_id AND p.user_id=$1 AND ($2::text IS NULL OR p.kind=$2) AND o.state IN ('queued','running','failed')").bind(user_id).bind(kind).bind(now).execute(&mut *connection).await?;
    sqlx::query("UPDATE native_account_proofs SET consumed_at=$3 WHERE user_id=$1 AND ($2::text IS NULL OR kind=$2) AND consumed_at IS NULL").bind(user_id).bind(kind).bind(now).execute(connection).await?;
    Ok(())
}
async fn queue(connection: &mut PgConnection, input: PreparedCredentialMail) -> TxResult<()> {
    let Some(current) = credential(connection, &input.email)
        .await?
        .filter(|value| value.can_authenticate(false))
    else {
        return Ok(());
    };
    if input.kind == CredentialProofKind::EmailVerification && current.email_verified {
        return Ok(());
    }
    let recent:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_account_proofs WHERE user_id=$1 AND kind=$2 AND created_at>$3)").bind(&current.user_id).bind(input.kind.as_str()).bind(input.now-60_000).fetch_one(&mut *connection).await?;
    if recent {
        return Ok(());
    }
    cancel_proofs(
        connection,
        &current.user_id,
        Some(input.kind.as_str()),
        input.now,
    )
    .await?;
    let expires = input
        .now
        .checked_add(CREDENTIAL_PROOF_LIFETIME_MS)
        .ok_or_else(invalid_proof)?;
    sqlx::query("INSERT INTO native_account_proofs(id,user_id,kind,token_digest,email,credential_fingerprint,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&input.proof_id).bind(&current.user_id).bind(input.kind.as_str()).bind(input.token_digest.as_str()).bind(&current.email).bind(fingerprint(&current.password_hash)).bind(input.now).bind(expires).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO native_account_mail_outbox(id,proof_id,user_id,kind,payload_ciphertext,next_attempt_at,expires_at,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$6,$6)").bind(input.outbox_id).bind(&input.proof_id).bind(&current.user_id).bind(input.kind.as_str()).bind(input.ciphertext).bind(input.now).bind(expires).execute(&mut *connection).await?;
    audit(
        connection,
        &input.event_id,
        &current.user_id,
        match input.kind {
            CredentialProofKind::EmailVerification => "verification_requested",
            CredentialProofKind::PasswordReset => "reset_requested",
        },
        Some(&input.proof_id),
        input.now,
    )
    .await
}
async fn proof(
    connection: &mut PgConnection,
    kind: CredentialProofKind,
    digest: &SecretDigest,
    now: i64,
) -> TxResult<(String, CredentialSnapshot, i64)> {
    let row=sqlx::query("SELECT id,user_id,email,credential_fingerprint,expires_at FROM native_account_proofs WHERE token_digest=$1 AND kind=$2 AND consumed_at IS NULL AND created_at<=$3 AND expires_at>$3").bind(digest.as_str()).bind(kind.as_str()).bind(now).fetch_optional(&mut *connection).await?.ok_or_else(invalid_proof)?;
    let email = EmailAddress::new(&row.try_get::<String, _>("email")?)?;
    let current = credential(connection, &email)
        .await?
        .filter(|value| value.can_authenticate(false))
        .ok_or_else(invalid_proof)?;
    if current.user_id != row.try_get::<String, _>("user_id")?
        || fingerprint(&current.password_hash)
            != row.try_get::<String, _>("credential_fingerprint")?
    {
        return Err(invalid_proof().into());
    }
    Ok((row.try_get("id")?, current, row.try_get("expires_at")?))
}
async fn revoke_all(connection: &mut PgConnection, user_id: &str, now: i64) -> TxResult<()> {
    let sessions = sqlx::query_scalar::<_, String>("SELECT id FROM session WHERE \"userId\"=$1")
        .bind(user_id)
        .fetch_all(&mut *connection)
        .await?;
    let browsers: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM account_device_browsers WHERE active_session_id=ANY($1::text[])",
    )
    .bind(&sessions)
    .fetch_all(&mut *connection)
    .await?;
    for browser in browsers {
        let changed=sqlx::query("UPDATE account_device_browsers SET active_session_id=NULL,revision=revision+1,pending_nonce=NULL,pending_until=NULL,updated_at=$2 WHERE id=$1 AND revision<$3").bind(browser).bind(now).bind(MAX_DEVICE_REVISION).execute(&mut *connection).await?.rows_affected();
        if changed != 1 {
            return Err(stale_device().into());
        }
    }
    for session in sessions {
        expire_family(connection, &session).await?;
    }
    Ok(())
}
async fn replace_password(
    connection: &mut PgConnection,
    current: &CredentialSnapshot,
    new: &PasswordHash,
    now: i64,
) -> TxResult<()> {
    let changed=sqlx::query("UPDATE account SET password=$3,\"updatedAt\"=$4 WHERE \"userId\"=$1 AND \"accountId\"=$1 AND \"providerId\"='credential' AND issuer='local:credential' AND password=$2").bind(&current.user_id).bind(current.password_hash.as_str()).bind(new.as_str()).bind(now).execute(&mut *connection).await?.rows_affected();
    if changed != 1 {
        return Err(invalid_proof().into());
    }
    revoke_all(connection, &current.user_id, now).await?;
    cancel_proofs(connection, &current.user_id, None, now).await
}
async fn authenticated_credential(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> TxResult<CredentialSnapshot> {
    let session = read_session(
        connection,
        proof,
        &RequestAudience::Account,
        environment,
        now,
    )
    .await?
    .ok_or_else(denied)?;
    let email = EmailAddress::new(&session.user.email)?;
    credential(connection, &email)
        .await?
        .filter(|value| value.user_id == session.user.id && value.can_authenticate(true))
        .ok_or_else(|| denied().into())
}

impl AccountCredentialRepository for PostgresAccountCredentials {
    fn inspect_password_reset(&self, token: SecretDigest, _now: i64) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let token = token.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            lock_session_reads(connection).await?;
                            sqlx::query("LOCK TABLE account,native_account_proofs IN SHARE MODE")
                                .execute(&mut *connection)
                                .await?;
                            proof(
                                connection,
                                CredentialProofKind::PasswordReset,
                                &token,
                                clock.now().get(),
                            )
                            .await?;
                            Ok(())
                        })
                    })
                    .await,
            )
        })
    }
    fn request_mail(&self, input: PreparedCredentialMail) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let mut input = input.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            locks(connection).await?;
                            input.now = clock.now().get();
                            queue(connection, input).await
                        })
                    })
                    .await,
            )
        })
    }
    fn reset_password(&self, input: ResetCredential) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            locks(connection).await?;
                            let now = clock.now().get();
                            let (id, current, expires) = proof(
                                connection,
                                CredentialProofKind::PasswordReset,
                                &input.token_digest,
                                now,
                            )
                            .await?;
                            replace_password(connection, &current, &input.password_hash, now)
                                .await?;
                            audit(
                                connection,
                                &input.event_id,
                                &current.user_id,
                                "password_reset",
                                Some(&id),
                                now,
                            )
                            .await?;
                            if clock.now().get() >= expires {
                                return Err(invalid_proof().into());
                            }
                            Ok(())
                        })
                    })
                    .await,
            )
        })
    }
    fn verify_email(&self, input: VerifyCredential) -> AccountFuture<'_, VerifiedCredential> {
        Box::pin(async move {
            finish(self.unit.run(options(),|connection|{let input=input.clone();let environment=self.environment.clone();let clock=self.clock.clone();Box::pin(async move{
  locks(connection).await?;
                            let now=clock.now().get();let(id,current,expires)=proof(connection,CredentialProofKind::EmailVerification,&input.token_digest,now).await?;
  let(browser_id,created_browser)=ensure_browser(connection,&input.proof,&input.candidate_browser,now).await?;let state=browser(connection,&browser_id).await?.ok_or_else(invalid_proof)?;revision(&state,input.expected_revision,now)?;
  sqlx::query("UPDATE \"user\" SET \"emailVerified\"=1,\"updatedAt\"=$2 WHERE id=$1").bind(&current.user_id).bind(now).execute(&mut *connection).await?;cancel_proofs(connection,&current.user_id,Some(CredentialProofKind::EmailVerification.as_str()),now).await?;
  let completed=issue_account_session(connection,IssuedAccountSession{proof:input.proof,browser:browser_id,session_id:input.session_id,session_digest:input.session_digest,handle:input.handle,user_id:current.user_id.clone(),now},&environment).await?;
  audit(connection,&input.event_id,&current.user_id,"email_verified",Some(&id),now).await?;if clock.now().get()>=expires{return Err(invalid_proof().into());}Ok(VerifiedCredential{completed,created_browser})
 })}).await)
        })
    }
    fn current_credential(
        &self,
        proof: DeviceProof,
        _now: i64,
    ) -> AccountFuture<'_, CredentialSnapshot> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let proof = proof.clone();
                        let clock = self.clock.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            lock_session_reads(connection).await?;
                            sqlx::query("LOCK TABLE account IN SHARE MODE")
                                .execute(&mut *connection)
                                .await?;
                            authenticated_credential(
                                connection,
                                &proof,
                                &environment,
                                clock.now().get(),
                            )
                            .await
                        })
                    })
                    .await,
            )
        })
    }
    fn change_password(&self, input: ChangeCredential) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let clock = self.clock.clone();
                        let environment = self.environment.clone();
                        Box::pin(async move {
                            locks(connection).await?;
                            let now = clock.now().get();
                            let current = authenticated_credential(
                                connection,
                                &input.proof,
                                &environment,
                                now,
                            )
                            .await?;
                            if current.password_hash != input.previous {
                                return Err(denied().into());
                            }
                            replace_password(connection, &current, &input.password_hash, now)
                                .await?;
                            audit(
                                connection,
                                &input.event_id,
                                &current.user_id,
                                "password_changed",
                                None,
                                now,
                            )
                            .await
                        })
                    })
                    .await,
            )
        })
    }
    fn sign_up(&self, input: SignUpCredential) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            let result=self.unit.run(options(),|connection|{let mut input=input.clone();let invitations=self.invitations.clone();let clock=self.clock.clone();Box::pin(async move{
   locks(connection).await?;input.mail.now=clock.now().get();
   let row=sqlx::query("SELECT id,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,department_id,created_by,assignment_id,expires_at,created_at FROM account_invitations WHERE token_hash=$1 AND status='pending' AND expires_at>$2").bind(input.invitation_digest.as_str()).bind(input.mail.now).fetch_optional(&mut *connection).await?.ok_or_else(signup_unavailable)?;
   let id:String=row.try_get("id")?;let email=EmailAddress::new(&row.try_get::<String,_>("email")?)?;if email!=input.email{return Err(signup_unavailable().into());}
   let holding_id:String=row.try_get("holding_id")?;let company_id:String=row.try_get("company_id")?;
   let person_id:String=row.try_get("person_id")?;let name:String=row.try_get("name")?;
   let created_by:String=row.try_get("created_by")?;
   let authorizer_control_active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" u LEFT JOIN principals p ON p.user_id=u.id WHERE u.id=$1 AND COALESCE(p.status,'active')='active')").bind(&created_by).fetch_one(&mut *connection).await?;
   let holding_binding_active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND status='active')").bind(&holding_id).fetch_one(&mut *connection).await?;
   if !authorizer_control_active||!holding_binding_active{return Err(signup_unavailable().into());}
   let query=InvitationCredentialQuery{invitation_id:id.clone(),holding_id,company_id,person_id,name:name.clone(),role_payload:row.try_get("role")?,scope_kind:row.try_get("scope_kind")?,branch_id:row.try_get("branch_id")?,department_id:row.try_get("department_id")?,created_by,authorizer_control_active,holding_binding_active,assignment_id:row.try_get("assignment_id")?,expires_at:row.try_get("expires_at")?,created_at:row.try_get("created_at")?,email:email.clone(),now:input.mail.now};
   let mut lease=invitations.inspect(&query).await?;let facts=lease.facts();validate_invitation_credential(facts)?;if facts.invitation_id!=id||facts.holding_id!=query.holding_id||facts.company_id!=query.company_id||facts.person_id!=query.person_id||facts.email!=email||facts.now<input.mail.now{return Err(signup_unavailable().into());}input.mail.now=facts.now;
   let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM \"user\" WHERE lower(email)=$1)").bind(email.as_str()).fetch_one(&mut *connection).await?;if exists{return Err(signup_unavailable().into());}
   let user_id=format!("usr_invite_{id}");
   sqlx::query("INSERT INTO account_invitation_claims(invitation_id,eligible,claimed_at) VALUES($1,1,$2)").bind(&id).bind(input.mail.now).execute(&mut *connection).await?;
   sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES($1,$2,$3,0,$4,$4)").bind(&user_id).bind(&name).bind(email.as_str()).bind(input.mail.now).execute(&mut *connection).await?;
   sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") VALUES($1,$2,'credential','local:credential',$2,$3,$4,$4)").bind(input.credential_id).bind(&user_id).bind(input.password_hash.as_str()).bind(input.mail.now).execute(&mut *connection).await?;
   sqlx::query("INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES($1,$2,$3,'active',$4)").bind(input.principal_id).bind(&user_id).bind(name).bind(input.mail.now).execute(&mut *connection).await?;
   sqlx::query("UPDATE account_invitations SET status='accepted',accepted_user_id=$2 WHERE id=$1 AND status='pending'").bind(&id).bind(&user_id).execute(&mut *connection).await?;
   queue(connection,input.mail.clone()).await?;audit(connection,&format!("signup-{}",input.mail.event_id),&user_id,"invitation_credential_created",Some(&input.mail.proof_id),input.mail.now).await?;
   lease.recheck(clock.now().get()).await?;
   Ok(((),lease))
  })}).await;
            // The owner lease remains alive through the successful Control commit, including retries.
            finish(result).map(|(output, _lease)| output)
        })
    }
}
