use super::account_holding_bindings::ConfiguredHoldingRegistry;
use super::account_mfa_crypto::AccountMfaKeyring;
use super::account_mfa_postgres::{current, require_fresh_mfa_in_transaction};
use super::account_postgres::{TxResult, finish, options};
use crate::{
    application::{
        account_ports::{AccountFuture, DeviceProof},
        recovery::{ActiveRow, RecoveryCommands, RecoveryTarget},
    },
    domain::{
        account_auth::RequestAudience,
        recovery::{
            RecoveryInput, RecoveryReceipt, expires_at, idempotency_conflict, not_active,
            recovery_unavailable,
        },
    },
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::sync::Arc;

/// One Holding or the Control database. Every command is a single serializable
/// unit: idempotency receipt, temporary session, audit and stored result.
pub struct PgRecoveryCommands {
    unit: PgUnitOfWork,
    environment: String,
    keys: Arc<AccountMfaKeyring>,
    holdings: Arc<ConfiguredHoldingRegistry>,
}

impl PgRecoveryCommands {
    pub fn new(
        pool: DatabasePool,
        environment: String,
        keys: Arc<AccountMfaKeyring>,
        holdings: Arc<ConfiguredHoldingRegistry>,
    ) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool),
            environment,
            keys,
            holdings,
        }
    }
}

fn digest(
    company_id: &str,
    holding_slug: Option<&str>,
    actor: &str,
    session: &str,
    operation: &str,
    input: &RecoveryInput,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        serde_json::to_vec(&json!({
            "actor": actor,
            "session": session,
            "company": company_id,
            "holding": holding_slug,
            "operation": operation,
            "input": input,
        }))
        .unwrap_or_default(),
    );
    format!("{:x}", hasher.finalize())
}

async fn store_receipt(
    connection: &mut PgConnection,
    key: &str,
    target: &RecoveryTarget,
    actor: &str,
    request_hash: &str,
    result: &Value,
    now: i64,
) -> TxResult<()> {
    sqlx::query("INSERT INTO iam_receipts VALUES($1,$2,$3,$4,$5,$6)")
        .bind(key)
        .bind(&target.company_id)
        .bind(actor)
        .bind(request_hash)
        .bind(result)
        .bind(now)
        .execute(connection)
        .await?;
    Ok(())
}

async fn record_audit(
    connection: &mut PgConnection,
    target: &RecoveryTarget,
    actor: &str,
    session: &str,
    operation: &str,
    key: &str,
    detail: Value,
    now: i64,
) -> TxResult<()> {
    sqlx::query("INSERT INTO iam_audit VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(format!("emergency:{operation}:{key}"))
        .bind(&target.company_id)
        .bind(actor)
        .bind("recovery")
        .bind(operation)
        .bind(session)
        .bind(detail)
        .bind(now)
        .execute(connection)
        .await?;
    Ok(())
}

async fn replay(
    connection: &mut PgConnection,
    key: &str,
    target: &RecoveryTarget,
    actor: &str,
    request_hash: &str,
) -> TxResult<Option<Value>> {
    let row = sqlx::query(
        "SELECT company_id,actor_id,request_hash,result FROM iam_receipts WHERE id=$1 FOR UPDATE",
    )
    .bind(key)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let receipt = RecoveryReceipt {
        company_id: row.try_get("company_id")?,
        actor_id: row.try_get("actor_id")?,
        request_hash: row.try_get("request_hash")?,
        result: row.try_get("result")?,
    };
    if receipt.company_id != target.company_id
        || receipt.actor_id != actor
        || receipt.request_hash != request_hash
    {
        return Err(idempotency_conflict().into());
    }
    Ok(Some(receipt.result))
}

async fn designated(
    connection: &mut PgConnection,
    target: &RecoveryTarget,
    user_id: &str,
    now: i64,
) -> TxResult<Option<i64>> {
    let member: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM iam_members WHERE company_id=$1 AND user_id=$2 AND status='active' AND login_policy='emergency' FOR SHARE",
    )
    .bind(&target.company_id)
    .bind(user_id)
    .fetch_optional(&mut *connection)
    .await?;
    if member.is_none() {
        return Ok(None);
    }
    let grant: Option<i64> = if target.holding_slug.is_some() {
        sqlx::query_scalar(
            "SELECT valid_until FROM assignments WHERE user_id=$1 AND role='holding_owner' AND iam_assignment_active(id,$2) AND company_id=$3 ORDER BY valid_until DESC LIMIT 1 FOR SHARE",
        )
        .bind(user_id)
        .bind(now)
        .bind(&target.company_id)
        .fetch_optional(&mut *connection)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT valid_until FROM platform_assignments WHERE user_id=$1 AND role='platform_admin' AND iam_assignment_active(id,$2) ORDER BY valid_until DESC LIMIT 1 FOR SHARE",
        )
        .bind(user_id)
        .bind(now)
        .fetch_optional(&mut *connection)
        .await?
    };
    Ok(grant)
}

impl RecoveryCommands for PgRecoveryCommands {
    fn activate<'a>(
        &'a self,
        proof: DeviceProof,
        target: &'a RecoveryTarget,
        input: &'a RecoveryInput,
        key: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Value> {
        let environment = self.environment.clone();
        let keys = self.keys.clone();
        let holdings = self.holdings.clone();
        let target = target.clone();
        let input = input.clone();
        let key = key.to_owned();
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), move |connection| {
                        Box::pin(async move {
                            let fresh = require_fresh_mfa_in_transaction(
                                connection,
                                &proof,
                                &RequestAudience::Account,
                                &environment,
                                keys,
                                now,
                                &holdings,
                            )
                            .await?;
                            let user_id = fresh.user_id().to_owned();
                            let session_id = fresh.session_id().to_owned();
                            let request_hash = digest(
                                &target.company_id,
                                target.holding_slug.as_deref(),
                                &user_id,
                                &session_id,
                                "emergency.activate",
                                &input,
                            );
                            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                                .bind(format!("{}:{user_id}:{key}", target.company_id))
                                .execute(&mut *connection)
                                .await?;
                            if let Some(result) = replay(
                                connection,
                                &key,
                                &target,
                                &user_id,
                                &request_hash,
                            )
                            .await?
                            {
                                return Ok(result);
                            }
                            let grant = designated(connection, &target, &user_id, now)
                                .await?
                                .ok_or_else(recovery_unavailable)?;
                            let expires = expires_at(now, grant);
                            sqlx::query("INSERT INTO iam_emergency_sessions VALUES($1,$2,$3,$4,$5) ON CONFLICT(session_id,company_id) DO UPDATE SET expires_at=excluded.expires_at,reason=excluded.reason")
                                .bind(&session_id)
                                .bind(&target.company_id)
                                .bind(&user_id)
                                .bind(expires)
                                .bind(&input.reason)
                                .execute(&mut *connection)
                                .await?;
                            let result = json!({
                                "expiresAt": expires,
                                "message": "Emergency access activated for up to one hour.",
                            });
                            record_audit(
                                connection,
                                &target,
                                &user_id,
                                &session_id,
                                "emergency.activated",
                                &key,
                                json!({"reason": input.reason, "expiresAt": expires}),
                                now,
                            )
                            .await?;
                            store_receipt(
                                connection,
                                &key,
                                &target,
                                &user_id,
                                &request_hash,
                                &result,
                                now,
                            )
                            .await?;
                            Ok(result)
                        })
                    })
                    .await,
            )
        })
    }

    fn revoke<'a>(
        &'a self,
        proof: DeviceProof,
        target: &'a RecoveryTarget,
        input: &'a RecoveryInput,
        key: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Value> {
        let environment = self.environment.clone();
        let holdings = self.holdings.clone();
        let target = target.clone();
        let input = input.clone();
        let key = key.to_owned();
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), move |connection| {
                        Box::pin(async move {
                            let actor = current(
                                connection,
                                &proof,
                                &RequestAudience::Account,
                                &environment,
                                now,
                                &holdings,
                            )
                            .await?;
                            let user_id = actor.user.id;
                            let session_id = actor.id;
                            let request_hash = digest(
                                &target.company_id,
                                target.holding_slug.as_deref(),
                                &user_id,
                                &session_id,
                                "emergency.revoke",
                                &input,
                            );
                            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                                .bind(format!("{}:{user_id}:{key}", target.company_id))
                                .execute(&mut *connection)
                                .await?;
                            if let Some(result) = replay(
                                connection,
                                &key,
                                &target,
                                &user_id,
                                &request_hash,
                            )
                            .await?
                            {
                                return Ok(result);
                            }
                            let deleted = sqlx::query(
                                "DELETE FROM iam_emergency_sessions WHERE session_id=$1 AND user_id=$2 AND company_id=$3 AND expires_at>$4 RETURNING session_id",
                            )
                            .bind(&session_id)
                            .bind(&user_id)
                            .bind(&target.company_id)
                            .bind(now)
                            .fetch_optional(&mut *connection)
                            .await?;
                            if deleted.is_none() {
                                return Err(not_active().into());
                            }
                            let result = json!({
                                "revoked": true,
                                "message": "Temporary emergency access was revoked for this session.",
                            });
                            record_audit(
                                connection,
                                &target,
                                &user_id,
                                &session_id,
                                "emergency.revoked",
                                &key,
                                json!({"reason": input.reason, "revoked": true}),
                                now,
                            )
                            .await?;
                            store_receipt(
                                connection,
                                &key,
                                &target,
                                &user_id,
                                &request_hash,
                                &result,
                                now,
                            )
                            .await?;
                            Ok(result)
                        })
                    })
                    .await,
            )
        })
    }

    fn active<'a>(
        &'a self,
        proof: DeviceProof,
        company_id: &'a str,
        now: i64,
    ) -> AccountFuture<'a, Vec<ActiveRow>> {
        let environment = self.environment.clone();
        let holdings = self.holdings.clone();
        let company_id = company_id.to_owned();
        Box::pin(async move {
            finish(
                self.unit
                    .run_once(options(), move |connection| {
                        Box::pin(async move {
                            let actor = current(
                                connection,
                                &proof,
                                &RequestAudience::Account,
                                &environment,
                                now,
                                &holdings,
                            )
                            .await?;
                            let rows = sqlx::query(
                                "SELECT company_id,reason,expires_at FROM iam_emergency_sessions WHERE session_id=$1 AND user_id=$2 AND expires_at>$3 AND ($4::text = '' OR company_id=$4) ORDER BY company_id",
                            )
                            .bind(&actor.id)
                            .bind(&actor.user.id)
                            .bind(now)
                            .bind(&company_id)
                            .fetch_all(&mut *connection)
                            .await?;
                            let mut access = Vec::with_capacity(rows.len());
                            for row in rows {
                                access.push(ActiveRow {
                                    company_id: row.try_get("company_id")?,
                                    reason: row.try_get("reason")?,
                                    expires_at: row.try_get("expires_at")?,
                                });
                            }
                            Ok(access)
                        })
                    })
                    .await,
            )
        })
    }
}

/// Registered scope list used by `EmergencyAccess::active`.
pub struct PgRecoveryRegistry {
    platform: RecoveryTarget,
    holdings: Vec<RecoveryTarget>,
}

impl PgRecoveryRegistry {
    pub fn new(platform: RecoveryTarget, holdings: Vec<RecoveryTarget>) -> Self {
        Self { platform, holdings }
    }
}

impl crate::application::recovery::RecoveryRegistry for PgRecoveryRegistry {
    fn platform(&self) -> RecoveryTarget {
        self.platform.clone()
    }

    fn holding(&self, slug: &str) -> Option<RecoveryTarget> {
        self.holdings
            .iter()
            .find(|target| target.holding_slug.as_deref() == Some(slug))
            .cloned()
    }

    fn holdings(&self) -> Vec<RecoveryTarget> {
        self.holdings.clone()
    }
}

/// Unused state kept explicit: the Control/Holding pools are owned by the
/// composition and reused; no clone of a connection is retained here.
pub fn target(
    commands: Arc<dyn RecoveryCommands>,
    company_id: String,
    holding_slug: Option<String>,
    display_name: String,
) -> RecoveryTarget {
    RecoveryTarget {
        company_id,
        holding_slug,
        display_name,
        commands,
    }
}
