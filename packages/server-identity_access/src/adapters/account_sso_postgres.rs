use crate::{
    adapters::{
        account_postgres::{TxResult, finish, options},
        account_sessions::{lock_session_reads, lock_session_writes, read_session, session_row},
    },
    application::{account_ports::*, account_sso::*},
    domain::{
        account_auth::{AccountSession, RequestAudience},
        account_sso::*,
    },
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgUnitOfWork};
use erpx_server_shared::public::Clock;
use sqlx::{PgConnection, Row, postgres::PgRow};
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub struct PostgresAccountSso {
    unit: PgUnitOfWork,
    policy: SsoPolicy,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl PostgresAccountSso {
    pub fn from_control_database(
        pool: DatabasePool,
        policy: SsoPolicy,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool),
            policy,
            clock,
        }
    }
}
fn equal(left: &SsoDigest, right: &SsoDigest) -> bool {
    bool::from(left.as_str().as_bytes().ct_eq(right.as_str().as_bytes()))
}
async fn lock_sso(connection: &mut PgConnection) -> TxResult<()> {
    sqlx::query(
        "LOCK TABLE sso_browser_transactions,sso_authorization_grants IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}
fn browser_row(row: PgRow) -> TxResult<BrowserSsoTransaction> {
    Ok(BrowserSsoTransaction {
        id: SsoTransactionId::new(row.try_get("id")?)?,
        browser_hash: SsoDigest::new(row.try_get("browser_hash")?)?,
        state_hash: SsoDigest::new(row.try_get("state_hash")?)?,
        challenge: SsoDigest::new(row.try_get("challenge")?)?,
        audience: row.try_get("audience")?,
        environment: row.try_get("environment")?,
        callback: row.try_get("callback")?,
        return_path: SsoReturnPath::stored(row.try_get("return_path")?)?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        consumed_at: row.try_get("consumed_at")?,
    })
}
async fn browser(
    connection: &mut PgConnection,
    id: &SsoTransactionId,
) -> TxResult<BrowserSsoTransaction> {
    browser_row(
        sqlx::query("SELECT * FROM sso_browser_transactions WHERE id=$1 FOR UPDATE")
            .bind(id.as_str())
            .fetch_optional(connection)
            .await?
            .ok_or_else(rejected_sso)?,
    )
}
fn grant_row(row: PgRow) -> TxResult<SsoAuthorizationGrant> {
    Ok(SsoAuthorizationGrant {
        code_hash: SsoDigest::new(row.try_get("code_hash")?)?,
        transaction_id: SsoTransactionId::new(row.try_get("transaction_id")?)?,
        parent_session_id: row.try_get("parent_session_id")?,
        audience: row.try_get("audience")?,
        environment: row.try_get("environment")?,
        callback: row.try_get("callback")?,
        state_hash: SsoDigest::new(row.try_get("state_hash")?)?,
        challenge: SsoDigest::new(row.try_get("challenge")?)?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        consumed_at: row.try_get("consumed_at")?,
    })
}
/// A grant-bound parent lookup, never a manufactured cookie proof or permission-bearing actor.
async fn selected_parent(
    connection: &mut PgConnection,
    id: &str,
    environment: &str,
    now: i64,
) -> TxResult<AccountSession> {
    let row=sqlx::query(r#"SELECT s.id session_id,s."userId" user_id,u.name,u.email,u."emailVerified" email_verified,u.image,b.authenticated_at,
 LEAST(b.absolute_expires_at,(EXTRACT(EPOCH FROM s."expiresAt"::timestamptz)*1000)::bigint) expires_at
 FROM session s JOIN sso_session_bindings b ON b.session_id=s.id JOIN "user" u ON u.id=s."userId"
 JOIN account_device_accounts a ON a.session_id=s.id JOIN account_device_browsers d ON d.id=a.browser_id AND d.active_session_id=s.id
 LEFT JOIN principals p ON p.user_id=u.id WHERE s.id=$1 AND b.audience='account' AND b.environment=$2 AND b.parent_session_id IS NULL
 AND COALESCE(p.status,'active')='active' AND b.authenticated_at<=$3 AND b.absolute_expires_at>$3 AND b.absolute_expires_at<=b.authenticated_at+43200000
 AND s."expiresAt"::timestamptz>to_timestamp($3::double precision/1000)"#).bind(id).bind(environment).bind(now).fetch_optional(connection).await?.ok_or_else(rejected_sso)?;
    Ok(session_row(&row)?)
}
async fn authorize(
    connection: &mut PgConnection,
    input: AuthorizeSso,
    policy: &SsoPolicy,
    clock: &(dyn Clock + Send + Sync),
) -> TxResult<Option<String>> {
    lock_session_reads(connection).await?;
    lock_sso(connection).await?;
    let instant = clock.now().get();
    if input.now > instant {
        return Err(rejected_sso().into());
    }
    let transaction = browser(connection, &input.transaction_id).await?;
    transaction.require_current(policy, instant)?;
    if !equal(&transaction.state_hash, &input.state_hash)
        || !equal(&transaction.challenge, &input.challenge)
    {
        return Err(rejected_sso().into());
    }
    let Some(parent) = read_session(
        connection,
        &input.proof,
        &RequestAudience::Account,
        &policy.account().environment,
        instant,
    )
    .await?
    else {
        return Ok(None);
    };
    let expires = grant_expiry(instant, &transaction, &parent)?;
    sqlx::query("INSERT INTO sso_authorization_grants(code_hash,transaction_id,parent_session_id,audience,environment,callback,state_hash,challenge,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(input.code_hash.as_str()).bind(transaction.id.as_str()).bind(parent.id).bind(&transaction.audience).bind(&transaction.environment).bind(&transaction.callback).bind(transaction.state_hash.as_str()).bind(transaction.challenge.as_str()).bind(instant).bind(expires).execute(connection).await?;
    transaction.require_current(policy, clock.now().get())?;
    if clock.now().get() >= expires {
        return Err(rejected_sso().into());
    }
    Ok(Some(transaction.callback))
}
async fn consume(
    connection: &mut PgConnection,
    input: ConsumeSso,
    policy: &SsoPolicy,
    clock: &(dyn Clock + Send + Sync),
) -> TxResult<SsoConsumption> {
    lock_session_writes(connection).await?;
    lock_sso(connection).await?;
    let instant = clock.now().get();
    if input.now > instant {
        return Err(rejected_sso().into());
    }
    let browser = browser(connection, &input.transaction_id).await?;
    let grant = grant_row(
        sqlx::query("SELECT * FROM sso_authorization_grants WHERE code_hash=$1 FOR UPDATE")
            .bind(input.code_hash.as_str())
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(rejected_sso)?,
    )?;
    grant.require_current(&browser, policy, instant)?;
    if browser.audience != input.audience
        || !equal(&grant.code_hash, &input.code_hash)
        || !equal(&browser.browser_hash, &input.browser_hash)
        || !equal(&browser.state_hash, &input.state_hash)
        || !equal(&grant.state_hash, &browser.state_hash)
        || !equal(&browser.challenge, &input.verifier_challenge)
        || !equal(&grant.challenge, &browser.challenge)
    {
        return Err(rejected_sso().into());
    }
    let parent = selected_parent(
        connection,
        &grant.parent_session_id,
        &policy.account().environment,
        instant,
    )
    .await?;
    let expires = parent.expires_at;
    sqlx::query(r#"INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt") VALUES($1,$2,$3,
 to_char(to_timestamp($4::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
 to_char(to_timestamp($5::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
 to_char(to_timestamp($6::double precision/1000) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))"#).bind(&input.session_id).bind(input.session_digest.as_str()).bind(&parent.user.id).bind(parent.authenticated_at).bind(instant).bind(expires).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES($1,$2,$3,$4,$5,$6)").bind(&input.session_id).bind(&input.audience).bind(&policy.account().environment).bind(&parent.id).bind(parent.authenticated_at).bind(expires).execute(&mut *connection).await?;
    let browser_count = sqlx::query(
        "UPDATE sso_browser_transactions SET consumed_at=$2 WHERE id=$1 AND consumed_at IS NULL",
    )
    .bind(browser.id.as_str())
    .bind(instant)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    let grant_count=sqlx::query("UPDATE sso_authorization_grants SET consumed_at=$2 WHERE code_hash=$1 AND consumed_at IS NULL").bind(grant.code_hash.as_str()).bind(instant).execute(connection).await?.rows_affected();
    if browser_count != 1 || grant_count != 1 {
        return Err(rejected_sso().into());
    }
    let final_now = clock.now().get();
    grant.require_current(&browser, policy, final_now)?;
    if parent.authenticated_at > final_now || parent.expires_at <= final_now {
        return Err(rejected_sso().into());
    }
    Ok(SsoConsumption {
        session_id: input.session_id,
        expires_at: expires,
        return_path: browser.return_path,
    })
}
impl AccountSsoRepository for PostgresAccountSso {
    fn start(&self, input: BrowserSsoTransaction) -> AccountFuture<'_, ()> {
        Box::pin(async move {
            finish(self.unit.run(options(),|connection|{let input=input.clone();let policy=self.policy.clone();let clock=self.clock.clone();Box::pin(async move{lock_sso(connection).await?;input.require_current(&policy,clock.now().get())?;let created=input.created_at;let expires=input.expires_at;sqlx::query("INSERT INTO sso_browser_transactions(id,browser_hash,state_hash,challenge,audience,environment,callback,return_path,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(input.id.as_str()).bind(input.browser_hash.as_str()).bind(input.state_hash.as_str()).bind(input.challenge.as_str()).bind(input.audience).bind(input.environment).bind(input.callback).bind(input.return_path.as_str()).bind(input.created_at).bind(input.expires_at).execute(connection).await?;if !(created..expires).contains(&clock.now().get()){return Err(rejected_sso().into());}Ok(())})}).await)
        })
    }
    fn authorize(&self, input: AuthorizeSso) -> AccountFuture<'_, Option<String>> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let policy = self.policy.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            authorize(connection, input, &policy, clock.as_ref()).await
                        })
                    })
                    .await,
            )
        })
    }
    fn consume(&self, input: ConsumeSso) -> AccountFuture<'_, SsoConsumption> {
        Box::pin(async move {
            finish(
                self.unit
                    .run(options(), |connection| {
                        let input = input.clone();
                        let policy = self.policy.clone();
                        let clock = self.clock.clone();
                        Box::pin(async move {
                            consume(connection, input, &policy, clock.as_ref()).await
                        })
                    })
                    .await,
            )
        })
    }
}
