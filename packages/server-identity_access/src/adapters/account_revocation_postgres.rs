use crate::{
    application::{
        account_ports::AccountFuture, account_revocation::AccountWorkContextInvalidation,
    },
    domain::account_revocation::{AccountRevocationTarget, RetiredAccountChild, reentry_required},
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, SessionLock, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, HoldingId};
use sqlx::{PgConnection, Row};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub struct AccountHoldingWriter {
    pub write_binding: String,
    pub pool: DatabasePool,
}
/// Only an explicitly configured writer map can satisfy a current Control
/// binding. Missing or changed bindings fail before any Holding is mutated.
pub struct PostgresAccountWorkContextInvalidation {
    holdings: BTreeMap<HoldingId, AccountHoldingWriter>,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountWorkContextInvalidation {
    pub fn new(
        holdings: BTreeMap<HoldingId, AccountHoldingWriter>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Result<Self, BusinessError> {
        let mut bindings = BTreeSet::new();
        if holdings.values().any(|binding| {
            binding.write_binding.is_empty()
                || binding.write_binding.len() > 128
                || !binding
                    .write_binding
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !bindings.insert(binding.write_binding.clone())
        }) {
            return Err(reentry_required());
        }
        Ok(Self { holdings, clock })
    }
}
impl AccountWorkContextInvalidation for PostgresAccountWorkContextInvalidation {
    fn invalidate<'a>(
        &'a self,
        targets: &'a [AccountRevocationTarget],
        children: &'a [RetiredAccountChild],
    ) -> AccountFuture<'a, ()> {
        Box::pin(async move {
            let mut seen = BTreeSet::new();
            for target in targets {
                if !seen.insert(target.holding_id.clone())
                    || self
                        .holdings
                        .get(&target.holding_id)
                        .is_none_or(|entry| entry.write_binding != target.write_binding)
                {
                    return Err(reentry_required());
                }
            }
            // Never acknowledge a child whose owning Holding is absent/suspended.
            if children.iter().any(|child| {
                child.environment != "development" || !seen.contains(&child.holding_id)
            }) {
                return Err(reentry_required());
            }
            for target in targets {
                let entry = self
                    .holdings
                    .get(&target.holding_id)
                    .ok_or_else(reentry_required)?;
                let rows = children.to_vec();
                let clock = self.clock.clone();
                PgUnitOfWork::new(entry.pool.clone()).run(TransactionOptions{session_lock:Some(SessionLock{namespace:"erpx-account-work-invalidation-v1".into(),key:target.holding_id.as_str().into()}),..Default::default()},move |connection|{let rows=rows.clone();let clock=clock.clone();Box::pin(async move{
                // Same order as queued current-authority readers; an in-flight
                // durable owner commit completes before this monotonic fence.
                sqlx::query("LOCK TABLE iam_revoked_sessions,active_work_contexts IN SHARE ROW EXCLUSIVE MODE").execute(&mut *connection).await?;
                let now=clock.now().get();
                for child in &rows {
                    sqlx::query("INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) SELECT session_id,company_id,user_id,$3 FROM active_work_contexts WHERE session_id=$1 AND user_id=$2 ON CONFLICT(session_id,company_id) DO NOTHING").bind(child.session_id.as_str()).bind(child.user_id.as_str()).bind(now).execute(&mut *connection).await?;
                    sqlx::query("DELETE FROM active_work_contexts WHERE session_id=$1 AND user_id=$2").bind(child.session_id.as_str()).bind(child.user_id.as_str()).execute(&mut *connection).await?;
                }
                sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *connection).await?;
                for child in &rows {
                    let remains:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1 AND user_id=$2)").bind(child.session_id.as_str()).bind(child.user_id.as_str()).fetch_one(&mut *connection).await?;
                    if remains{return Err(PgTransactionError::Business(reentry_required()));}
                }
                Ok(())
            })}).await.map_err(|_|reentry_required())?;
            }
            Ok(())
        })
    }
}

/// Caller already holds the Control session-first writer and current binding
/// fence. This function never commits/retries Control or deletes a session.
pub(crate) async fn invalidate_children(
    connection: &mut PgConnection,
    parents: &[String],
    environment: &str,
    invalidator: &dyn AccountWorkContextInvalidation,
) -> Result<(), PgTransactionError<BusinessError>> {
    let rows=sqlx::query("SELECT s.id,s.\"userId\" user_id,p.\"userId\" parent_user,b.audience,b.environment FROM session s JOIN sso_session_bindings b ON b.session_id=s.id JOIN session p ON p.id=b.parent_session_id WHERE b.parent_session_id=ANY($1::text[]) ORDER BY s.id").bind(parents).fetch_all(&mut *connection).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let mut children = Vec::with_capacity(rows.len());
    for row in rows {
        let user: String = row.try_get("user_id")?;
        let env: String = row.try_get("environment")?;
        if user != row.try_get::<String, _>("parent_user")? || env != environment {
            return Err(reentry_required().into());
        }
        children.push(RetiredAccountChild::from_binding(
            row.try_get("id")?,
            user,
            &row.try_get::<String, _>("audience")?,
            &env,
        )?);
    }
    let rows=sqlx::query("SELECT holding_id,write_binding FROM holding_bindings WHERE status='active' ORDER BY holding_id").fetch_all(&mut *connection).await?;
    let targets = rows
        .into_iter()
        .map(|row| {
            Ok(AccountRevocationTarget {
                holding_id: HoldingId::new(row.try_get::<String, _>("holding_id")?)?,
                write_binding: row.try_get("write_binding")?,
            })
        })
        .collect::<Result<Vec<_>, PgTransactionError<BusinessError>>>()?;
    invalidator.invalidate(&targets, &children).await?;
    Ok(())
}
