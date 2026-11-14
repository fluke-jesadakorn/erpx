//! Cookie recovery uses independently proven native parents, never a stored
//! digest as a bearer or a union of unrelated browser memberships.
use crate::{
    adapters::{account_postgres::TxResult, account_sessions::proven_accounts},
    application::account_ports::DeviceProof,
    domain::account_auth::{
        DeviceAccount, DeviceAccounts, MAXIMUM_SAVED_ACCOUNTS, SecretDigest, stale_device,
    },
};
use sqlx::{PgConnection, Row};
use std::collections::BTreeSet;

pub(crate) async fn browser_key(
    connection: &mut PgConnection,
    digest: Option<&SecretDigest>,
) -> Result<Option<SecretDigest>, sqlx::Error> {
    let Some(digest) = digest else {
        return Ok(None);
    };
    let id: Option<String> = sqlx::query_scalar("SELECT browser_id FROM native_account_device_group_keys WHERE cookie_digest=$1 AND revoked_at IS NULL")
        .bind(digest.as_str()).fetch_optional(connection).await?;
    id.map(|id| {
        SecretDigest::new(id)
            .map_err(|_| sqlx::Error::Protocol("Invalid native browser key binding".into()))
    })
    .transpose()
}

pub(crate) struct RecoverableBrowser {
    pub id: SecretDigest,
    /// The actual proof that selected this group must still be live at ACK.
    pub proof_expires_at: i64,
}

pub(crate) enum BrowserDiscovery {
    None,
    Recoverable(RecoverableBrowser),
    Ambiguous,
}

pub(crate) async fn browser_discovery(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> TxResult<BrowserDiscovery> {
    let accounts = proven_accounts(connection, proof, environment, now).await?;
    if let Some(active) = accounts.iter().find(|account| {
        account.selected
            && proof
                .active
                .as_ref()
                .is_some_and(|digest| digest.as_str() == account.digest)
    }) {
        return Ok(BrowserDiscovery::Recoverable(RecoverableBrowser {
            id: SecretDigest::new(active.browser_id.clone())?,
            proof_expires_at: active.session.expires_at,
        }));
    }
    let groups = accounts
        .iter()
        .map(|account| account.browser_id.as_str())
        .collect::<BTreeSet<_>>();
    if groups.len() > 1 {
        return Ok(BrowserDiscovery::Ambiguous);
    }
    let Some(id) = groups.first() else {
        return Ok(BrowserDiscovery::None);
    };
    // Any independently proven live member can recover its one existing group.
    let expiry = accounts
        .iter()
        .map(|account| account.session.expires_at)
        .max()
        .ok_or_else(stale_device)?;
    Ok(BrowserDiscovery::Recoverable(RecoverableBrowser {
        id: SecretDigest::new((*id).to_owned())?,
        proof_expires_at: expiry,
    }))
}

pub(crate) async fn recoverable_browser(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> TxResult<Option<RecoverableBrowser>> {
    match browser_discovery(connection, proof, environment, now).await? {
        BrowserDiscovery::None => Ok(None),
        BrowserDiscovery::Recoverable(group) => Ok(Some(group)),
        BrowserDiscovery::Ambiguous => Err(stale_device().into()),
    }
}

pub(crate) async fn create_key(
    connection: &mut PgConnection,
    browser: &SecretDigest,
    cookie: &SecretDigest,
    now: i64,
) -> TxResult<()> {
    sqlx::query("INSERT INTO native_account_device_group_keys(cookie_digest,browser_id,created_at) VALUES($1,$2,$3)")
        .bind(cookie.as_str()).bind(browser.as_str()).bind(now).execute(connection).await?;
    Ok(())
}

/// Caller retains the session-first Control writer and must fence every actual
/// old child across Holdings before ACK/commit. Pending key writes are invisible
/// to other readers and roll back on failure; this helper has no independent commit.
pub(crate) async fn rotate_key(
    connection: &mut PgConnection,
    browser: &SecretDigest,
    cookie: &SecretDigest,
    now: i64,
) -> TxResult<()> {
    let previous = sqlx::query("UPDATE native_account_device_group_keys SET revoked_at=$2 WHERE browser_id=$1 AND revoked_at IS NULL")
        .bind(browser.as_str()).bind(now).execute(&mut *connection).await?.rows_affected();
    if previous != 1 {
        return Err(stale_device().into());
    }
    create_key(connection, browser, cookie, now).await
}

pub(crate) async fn current_revision(
    connection: &mut PgConnection,
    browser: &SecretDigest,
) -> TxResult<i64> {
    let row = sqlx::query("SELECT revision FROM account_device_browsers WHERE id=$1")
        .bind(browser.as_str())
        .fetch_optional(connection)
        .await?
        .ok_or_else(stale_device)?;
    Ok(row.try_get("revision")?)
}

/// Discovery can reveal only presented live proofs and their CAS revision. It
/// does not rotate a key, select an account, issue a session or acknowledge work.
pub(crate) async fn recovery_snapshot(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> TxResult<DeviceAccounts> {
    if browser_key(connection, proof.browser.as_ref())
        .await?
        .is_some()
    {
        return Ok(
            super::account_sessions::device_snapshot(connection, proof, environment, now).await?,
        );
    }
    let Some(group) = recoverable_browser(connection, proof, environment, now).await? else {
        return Ok(DeviceAccounts::empty());
    };
    let revision = current_revision(connection, &group.id).await?;
    let accounts = proven_accounts(connection, proof, environment, now)
        .await?
        .into_iter()
        .filter(|account| account.browser_id == group.id.as_str())
        .map(|account| DeviceAccount {
            handle: account.handle,
            name: account.session.user.name,
            email: account.session.user.email,
            image: account.session.user.image,
            current: account.selected,
        })
        .collect();
    Ok(DeviceAccounts {
        initialized: false,
        revision,
        maximum: MAXIMUM_SAVED_ACCOUNTS,
        accounts,
    })
}
