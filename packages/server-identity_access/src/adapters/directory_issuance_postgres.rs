//! Provider evidence is consumed by ordinary Account issuance, never a fake credential.
use super::{
    account_group_recovery::{current_revision, rotate_key},
    account_postgres::{
        BrowserEnrollment, IssuedAccountSession, browser, ensure_browser, issue_account_session,
        options, revision,
    },
    account_sessions::lock_session_writes,
    directory_postgres::{PostgresDirectory, RetainedDirectoryIdentity, Tx, finish, unavailable},
};
use crate::{
    application::{
        account_auth::{PresentedSecrets, SignedIn},
        account_ports::{AccountFuture, CompletedSignIn, DeviceProof},
        directory::*,
    },
    domain::{
        account_auth::{SecretDigest, SecretPurpose, stale_device},
        directory::DirectoryConnectionDescriptor,
    },
};
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
struct DirectorySignInCommand {
    subject: VerifiedDirectorySubject,
    descriptor: DirectoryConnectionDescriptor,
    proof: DeviceProof,
    expected_revision: Option<i64>,
    candidate: SecretDigest,
    session_id: String,
    session_digest: SecretDigest,
    handle: String,
    nonce: SecretDigest,
}
impl PostgresDirectory {
    async fn complete_directory_sign_in(
        &self,
        control: &mut PgConnection,
        input: DirectorySignInCommand,
    ) -> Tx<(CompletedSignIn, bool, RetainedDirectoryIdentity)> {
        lock_session_writes(control).await?;
        Self::lock_routes(control).await?;
        if input.descriptor.holding_id.is_none() {
            super::directory_mapping_postgres::lock_sources(control, true).await?;
        }
        let now = self.clock.now().get();
        input.subject.require_current_credential(now)?;
        let prepared = ensure_browser(
            control,
            &input.proof,
            &input.candidate,
            &self.environment,
            input.expected_revision,
            now,
            BrowserEnrollment::FreshAuthenticated,
        )
        .await?;
        let state = browser(control, &prepared.id)
            .await?
            .ok_or_else(stale_device)?;
        revision(&state, input.expected_revision, now)?;
        let original = current_revision(control, &prepared.id).await?;
        let until = input
            .subject
            .expires_at()
            .min(prepared.recovery_expires_at.unwrap_or(i64::MAX));
        let changed = sqlx::query(RESERVE_BROWSER)
            .bind(prepared.id.as_str())
            .bind(input.nonce.as_str())
            .bind(until)
            .bind(now)
            .bind(original)
            .execute(&mut *control)
            .await?
            .rows_affected();
        if changed != 1 {
            return Err(stale_device().into());
        }
        let mut lease = self
            .retain_identity(control, &input.descriptor, &input.subject)
            .await?;
        let eligible: bool = sqlx::query_scalar(CURRENT_ACCOUNT)
            .bind(&lease.facts.user_id)
            .fetch_one(&mut *control)
            .await?;
        if !eligible {
            return Err(unavailable().into());
        }
        let now = self.clock.now().get();
        input.subject.require_current_credential(now)?;
        lease.require_current(control, now).await?;
        let pending: bool = sqlx::query_scalar(CURRENT_RESERVATION)
            .bind(prepared.id.as_str())
            .bind(original)
            .bind(input.nonce.as_str())
            .bind(until)
            .bind(now)
            .fetch_one(&mut *control)
            .await?;
        if !pending {
            return Err(stale_device().into());
        }
        let created = prepared.created || prepared.recovery_expires_at.is_some();
        if prepared.recovery_expires_at.is_some() {
            rotate_key(control, &prepared.id, &prepared.cookie, now).await?;
        }
        let completed = issue_account_session(
            control,
            IssuedAccountSession {
                proof: input.proof,
                browser: prepared.id,
                cookie_digest: prepared.cookie,
                session_id: input.session_id,
                session_digest: input.session_digest,
                handle: input.handle,
                user_id: lease.facts.user_id.clone(),
                now,
            },
            &self.environment,
            self.invalidation.as_ref(),
            self.clock.as_ref(),
            &self.registry,
        )
        .await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *control)
            .await?;
        let final_now = self.clock.now().get();
        input.subject.require_current_credential(final_now)?;
        lease.require_current(control, final_now).await?;
        if final_now >= until || completed.session.expires_at <= final_now {
            return Err(stale_device().into());
        }
        // This owned value must survive the outer Control COMMIT. Holding's
        // read transaction and exact profile generation drop only afterwards.
        Ok((completed, created, lease))
    }
}
impl DirectoryAttemptCounter for PostgresDirectory {
    fn attempt<'a>(&'a self, connection_id: &'a str, username: &'a str) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            // Exact B0 opaque key: category + ':' + lowercase(connection + ':' + user).
            let identity = format!("{connection_id}:{username}").to_lowercase();
            let key = hex::encode(Sha256::digest(format!("directory:{identity}").as_bytes()));
            let now = self.clock.now().get();
            let cutoff = now.checked_sub(300000).ok_or_else(unavailable)?;
            let result = PgUnitOfWork::new(self.control.clone())
                .run(TransactionOptions::default(), |c| {
                    let key = key.clone();
                    Box::pin(async move {
                        let count: i32 = sqlx::query_scalar(COUNT_DIRECTORY_ATTEMPT)
                            .bind(key)
                            .bind(now)
                            .bind(cutoff)
                            .fetch_one(c)
                            .await?;
                        Ok(count)
                    })
                })
                .await;
            // Denial is OUTSIDE the committed counter unit. Invalid credentials
            // and rejected ninth/later attempts remain durably throttled.
            if finish(result)? > 8 {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "IAM_RATE_LIMIT",
                    "Too many attempts. Wait five minutes before trying again.",
                ));
            }
            Ok(())
        })
    }
}
impl DirectoryAccountIssuance for PostgresDirectory {
    fn issue<'a>(
        &'a self,
        subject: VerifiedDirectorySubject,
        descriptor: DirectoryConnectionDescriptor,
        presented: &'a PresentedSecrets,
        expected_revision: Option<i64>,
    ) -> DirectoryIssueFuture<'a> {
        Box::pin(async move {
            let result = async {
                subject.require_current_credential(self.clock.now().get())?;
                let browser_secret = self.secrets.issue()?;
                let session_secret = self.secrets.issue()?;
                let handle = self.secrets.identifier()?;
                let input = DirectorySignInCommand {
                    subject,
                    descriptor,
                    proof: presented.proof(self.secrets.as_ref()),
                    expected_revision,
                    candidate: self
                        .secrets
                        .digest(SecretPurpose::DeviceBrowser, &browser_secret),
                    session_id: self.secrets.identifier()?,
                    session_digest: self.secrets.digest(SecretPurpose::Session, &session_secret),
                    handle: handle.clone(),
                    nonce: self
                        .secrets
                        .digest(SecretPurpose::Reservation, &self.secrets.issue()?),
                };
                let this = self.clone();
                let result = PgUnitOfWork::new(self.control.clone())
                    .run_once(options(), move |control| {
                        Box::pin(
                            async move { this.complete_directory_sign_in(control, input).await },
                        )
                    })
                    .await;
                let (completed, created, _lease) = finish(result)?;
                Ok(SignedIn {
                    result: completed,
                    browser_secret: created.then_some(browser_secret),
                    session_secret,
                    handle,
                })
            }
            .await;
            result.map_err(|error: BusinessError| {
                if error.code.starts_with("DIRECTORY_") {
                    DirectoryAdmissionError::IdentityDenied
                } else {
                    DirectoryAdmissionError::AccountCommand(error)
                }
            })
        })
    }
}
const COUNT_DIRECTORY_ATTEMPT: &str = "INSERT INTO iam_throttles(key,window_start,attempts) VALUES($1,$2,1) ON CONFLICT(key) DO UPDATE SET window_start=CASE WHEN iam_throttles.window_start<$3 THEN excluded.window_start ELSE iam_throttles.window_start END,attempts=CASE WHEN iam_throttles.window_start<$3 THEN 1 ELSE LEAST(iam_throttles.attempts,2147483646)+1 END RETURNING attempts";
const CURRENT_ACCOUNT: &str = "SELECT EXISTS(SELECT 1 FROM \"user\" u LEFT JOIN principals p ON p.user_id=u.id WHERE u.id=$1 AND COALESCE(p.status,'active')='active')";
const RESERVE_BROWSER: &str = "UPDATE account_device_browsers SET pending_nonce=$2,pending_until=$3,updated_at=$4 WHERE id=$1 AND revision=$5";
const CURRENT_RESERVATION: &str = "SELECT EXISTS(SELECT 1 FROM account_device_browsers WHERE id=$1 AND revision=$2 AND pending_nonce=$3 AND pending_until=$4 AND pending_until>$5)";
