use super::account_group_recovery::browser_key;
use crate::{
    application::account_ports::DeviceProof,
    domain::account_auth::{
        AccountSession, AccountUser, DeviceAccount, DeviceAccounts, MAXIMUM_SAVED_ACCOUNTS,
        RequestAudience,
    },
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{PgConnection, Row, postgres::PgRow};

pub(crate) fn persistence() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_PERSISTENCE_UNAVAILABLE",
        "Account authentication is temporarily unavailable.",
    )
}

/// Acquire before the first authority read; the caller retains its Control transaction.
pub(crate) async fn lock_session_reads(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE session IN SHARE MODE")
        .execute(&mut *connection)
        .await?;
    sqlx::query("LOCK TABLE account_device_browsers,account_device_accounts,native_account_device_group_keys,sso_session_bindings,\"user\",principals IN SHARE MODE").execute(connection).await?;
    Ok(())
}

pub(crate) async fn lock_session_writes(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query("LOCK TABLE session IN EXCLUSIVE MODE")
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "LOCK TABLE account_device_browsers,account_device_accounts,native_account_device_group_keys IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "LOCK TABLE sso_session_bindings,\"user\",principals,holding_bindings IN SHARE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) fn session_row(row: &PgRow) -> Result<AccountSession, sqlx::Error> {
    Ok(AccountSession {
        id: row.try_get("session_id")?,
        user: AccountUser {
            id: row.try_get("user_id")?,
            name: row.try_get("name")?,
            email: row.try_get("email")?,
            email_verified: row.try_get::<i64, _>("email_verified")? == 1,
            image: row.try_get("image")?,
        },
        authenticated_at: row.try_get("authenticated_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

pub(crate) struct LiveAccount {
    pub session: AccountSession,
    pub digest: String,
    pub handle: String,
    pub browser_id: String,
    pub selected: bool,
}

/// A browser grouping cookie is not proof of any saved account. Each row also needs its own secret.
pub(crate) async fn proven_accounts(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> Result<Vec<LiveAccount>, sqlx::Error> {
    let tokens = proof
        .active
        .iter()
        .map(|value| value.as_str().to_owned())
        .chain(
            proof
                .saved
                .iter()
                .map(|(_, value)| value.as_str().to_owned()),
        )
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(r#"SELECT s.id session_id,s."userId" user_id,s.token,a.handle,a.browser_id,u.name,u.email,u."emailVerified" email_verified,u.image,
      COALESCE(device.active_session_id=s.id,false) selected,b.authenticated_at,LEAST(b.absolute_expires_at,(EXTRACT(EPOCH FROM s."expiresAt"::timestamptz)*1000)::bigint) expires_at
      FROM session s JOIN "user" u ON u.id=s."userId" JOIN sso_session_bindings b ON b.session_id=s.id
      JOIN account_device_accounts a ON a.session_id=s.id JOIN account_device_browsers device ON device.id=a.browser_id
      JOIN native_account_device_group_keys keys ON keys.browser_id=a.browser_id AND keys.revoked_at IS NULL
      LEFT JOIN principals p ON p.user_id=u.id
      WHERE s.token=ANY($1::text[]) AND b.audience='account' AND b.environment=$2 AND b.parent_session_id IS NULL
      AND b.authenticated_at<=$3 AND b.absolute_expires_at>$3 AND b.absolute_expires_at<=b.authenticated_at+43200000
      AND s."expiresAt"::timestamptz>to_timestamp($3::double precision/1000) AND COALESCE(p.status,'active')='active'
      ORDER BY u.name,a.handle"#).bind(tokens).bind(environment).bind(now).fetch_all(connection).await?;
    let mut accounts = Vec::new();
    for row in rows {
        let digest: String = row.try_get("token")?;
        let handle: String = row.try_get("handle")?;
        if !proof
            .active
            .as_ref()
            .is_some_and(|value| value.as_str() == digest)
            && !proof.saved.iter().any(|(supplied_handle, value)| {
                supplied_handle == &handle && value.as_str() == digest
            })
        {
            continue;
        }
        accounts.push(LiveAccount {
            session: session_row(&row)?,
            digest,
            handle,
            browser_id: row.try_get("browser_id")?,
            selected: row.try_get("selected")?,
        });
    }
    Ok(accounts)
}

pub(crate) async fn live_accounts(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> Result<Vec<LiveAccount>, sqlx::Error> {
    let Some(id) = browser_key(connection, proof.browser.as_ref()).await? else {
        return Ok(Vec::new());
    };
    Ok(proven_accounts(connection, proof, environment, now)
        .await?
        .into_iter()
        .filter(|account| account.browser_id == id.as_str())
        .collect())
}

pub(crate) async fn device_snapshot(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    environment: &str,
    now: i64,
) -> Result<DeviceAccounts, sqlx::Error> {
    let Some(browser) = browser_key(connection, proof.browser.as_ref()).await? else {
        return Ok(DeviceAccounts::empty());
    };
    let row =
        sqlx::query("SELECT revision,active_session_id FROM account_device_browsers WHERE id=$1")
            .bind(browser.as_str())
            .fetch_optional(&mut *connection)
            .await?;
    let Some(row) = row else {
        return Ok(DeviceAccounts::empty());
    };
    let accounts = live_accounts(connection, proof, environment, now).await?;
    let active: Option<String> = row.try_get("active_session_id")?;
    Ok(DeviceAccounts {
        initialized: true,
        revision: row.try_get("revision")?,
        maximum: MAXIMUM_SAVED_ACCOUNTS,
        accounts: accounts
            .into_iter()
            .map(|account| DeviceAccount {
                handle: account.handle,
                name: account.session.user.name,
                email: account.session.user.email,
                image: account.session.user.image,
                current: active.as_deref() == Some(account.session.id.as_str()),
            })
            .collect(),
    })
}

pub(crate) async fn read_session(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    audience: &RequestAudience,
    environment: &str,
    now: i64,
) -> Result<Option<AccountSession>, sqlx::Error> {
    let Some(active) = &proof.active else {
        return Ok(None);
    };
    match audience {
        RequestAudience::Account => {
            let Some(browser) = browser_key(connection, proof.browser.as_ref()).await? else {
                return Ok(None);
            };
            let selected: Option<String> = sqlx::query_scalar(
                "SELECT active_session_id FROM account_device_browsers WHERE id=$1",
            )
            .bind(browser.as_str())
            .fetch_optional(&mut *connection)
            .await?
            .flatten();
            Ok(live_accounts(connection, proof, environment, now)
                .await?
                .into_iter()
                .find(|account| {
                    account.digest == active.as_str()
                        && selected.as_deref() == Some(account.session.id.as_str())
                })
                .map(|account| account.session))
        }
        RequestAudience::Holding { audience } => {
            let row = sqlx::query(r#"SELECT child.id session_id,child."userId" user_id,u.name,u.email,u."emailVerified" email_verified,u.image,
              cb.authenticated_at,LEAST(cb.absolute_expires_at,pb.absolute_expires_at,(EXTRACT(EPOCH FROM child."expiresAt"::timestamptz)*1000)::bigint,(EXTRACT(EPOCH FROM parent."expiresAt"::timestamptz)*1000)::bigint) expires_at
              FROM session child JOIN sso_session_bindings cb ON cb.session_id=child.id
              JOIN session parent ON parent.id=cb.parent_session_id AND parent."userId"=child."userId"
              JOIN sso_session_bindings pb ON pb.session_id=parent.id
              JOIN account_device_accounts a ON a.session_id=parent.id
              JOIN account_device_browsers device ON device.id=a.browser_id AND device.active_session_id=parent.id
              JOIN "user" u ON u.id=child."userId" LEFT JOIN principals p ON p.user_id=u.id
              WHERE child.token=$1 AND cb.audience=$2 AND cb.environment=$3 AND pb.environment=$3
              AND pb.audience='account' AND pb.parent_session_id IS NULL AND COALESCE(p.status,'active')='active'
              AND child."expiresAt"::timestamptz>to_timestamp($4::double precision/1000) AND parent."expiresAt"::timestamptz>to_timestamp($4::double precision/1000)
              AND pb.authenticated_at<=$4 AND pb.absolute_expires_at>$4 AND pb.absolute_expires_at<=pb.authenticated_at+43200000
              AND cb.authenticated_at<=$4 AND cb.authenticated_at=pb.authenticated_at AND cb.absolute_expires_at>$4
              AND cb.absolute_expires_at<=cb.authenticated_at+43200000 AND cb.absolute_expires_at<=pb.absolute_expires_at"#)
                .bind(active.as_str()).bind(audience).bind(environment).bind(now).fetch_optional(connection).await?;
            row.as_ref().map(session_row).transpose()
        }
    }
}

/// Session proof only. The caller must still resolve current assignments and original owner policies.
/// This borrowed helper never begins, commits, retries or closes the caller's request lease.
pub async fn verify_session_in_transaction(
    connection: &mut PgConnection,
    proof: &DeviceProof,
    audience: &RequestAudience,
    environment: &str,
    now: i64,
) -> Result<Option<AccountSession>, BusinessError> {
    if environment != "development" {
        return Err(BusinessError::invalid(
            "ACCOUNT_ENVIRONMENT_UNAVAILABLE",
            "This Account binding environment is not enabled.",
        ));
    }
    lock_session_reads(connection)
        .await
        .map_err(|_| persistence())?;
    read_session(connection, proof, audience, environment, now)
        .await
        .map_err(|_| persistence())
}
