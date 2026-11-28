//! Control-only private session lease. Normal host-specific authentication must
//! establish the supplied proof before this recheck; no business grant is made.
use crate::adapters::portal_session::verify_portal_session;
use crate::domain::{
    account_auth::SESSION_LIFETIME_MS,
    portal_session::{PortalSessionProof, PortalSessionReadRequest},
};
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Postgres, Row, Transaction};
type Error = PgTransactionError<BusinessError>;
fn revoked() -> Error {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "SESSION_REVOKED",
        "Sign in again to open this private document.",
    )
    .into()
}
fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SESSION_READ_TRANSACTION_REQUIRED",
        "Private reads require the native current-session owner.",
    )
    .into()
}
fn db(error: sqlx::Error) -> Error {
    if matches!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03" | "40P01")
    ) {
        BusinessError::new(
            FailureKind::Conflict,
            "SESSION_AUTHORITY_BUSY",
            "Your sign-in is changing. Reload and try again.",
        )
        .into()
    } else {
        error.into()
    }
}
#[derive(Debug)]
struct Binding {
    audience: String,
    environment: String,
    parent: Option<String>,
    authenticated: i64,
    expires: i64,
}
fn current(b: &Binding, audience: &str, now: i64) -> bool {
    b.audience == audience
        && b.environment == "development"
        && b.authenticated <= now
        && b.expires > now
        && b.authenticated
            .checked_add(SESSION_LIFETIME_MS)
            .is_some_and(|limit| b.expires <= limit)
}
async fn binding(
    c: &mut sqlx::PgConnection,
    id: &str,
    lock: bool,
) -> Result<Option<Binding>, Error> {
    let sql = if lock {
        "SELECT audience,environment,parent_session_id,authenticated_at,absolute_expires_at FROM sso_session_bindings WHERE session_id=$1 FOR SHARE"
    } else {
        "SELECT audience,environment,parent_session_id,authenticated_at,absolute_expires_at FROM sso_session_bindings WHERE session_id=$1"
    };
    let row = sqlx::query(sql)
        .bind(id)
        .fetch_optional(c)
        .await
        .map_err(db)?;
    row.map(|r| {
        Ok(Binding {
            audience: r.try_get("audience").map_err(db)?,
            environment: r.try_get("environment").map_err(db)?,
            parent: r.try_get("parent_session_id").map_err(db)?,
            authenticated: r.try_get("authenticated_at").map_err(db)?,
            expires: r.try_get("absolute_expires_at").map_err(db)?,
        })
    })
    .transpose()
}
async fn selected(c: &mut sqlx::PgConnection, id: &str) -> Result<bool, Error> {
    let row:Option<(Option<String>,)>=sqlx::query_as("SELECT b.active_session_id FROM account_device_accounts a JOIN account_device_browsers b ON b.id=a.browser_id WHERE a.session_id=$1").bind(id).fetch_optional(c).await.map_err(db)?;
    Ok(row.is_none_or(|(active,)| active.as_deref() == Some(id)))
}
/// Owns the Control transaction until the caller finishes the Holding unit.
pub struct PortalPrivateSessionLease {
    tx: Transaction<'static, Postgres>,
    expected: PortalSessionProof,
    audience: Option<String>,
}
impl PortalPrivateSessionLease {
    /// `expected` must be the actual normal-authenticated session for this host.
    pub async fn begin(
        pool: &DatabasePool,
        expected: PortalSessionProof,
        audience: Option<String>,
        now: i64,
    ) -> Result<Self, Error> {
        let mut tx = pool.raw().begin().await.map_err(|_| unavailable())?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("SET LOCAL lock_timeout='5s'")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("LOCK TABLE session IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("LOCK TABLE sso_session_bindings IN SHARE MODE")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let mut lease = Self {
            tx,
            expected,
            audience,
        };
        lease.require_current(now).await?;
        Ok(lease)
    }
    /// Recheck while holding the same Control lease; caller supplies fresh time.
    pub async fn require_current(&mut self, now: i64) -> Result<(), Error> {
        let c = &mut *self.tx;
        let id = &self.expected.session_id;
        let first = binding(&mut *c, id, false).await?;
        if self.audience.is_none() != first.is_none() {
            return Err(revoked());
        }
        let parent = first.as_ref().and_then(|b| b.parent.clone());
        for row_id in parent.iter().chain(std::iter::once(id)) {
            sqlx::query("SELECT id FROM session WHERE id=$1 FOR SHARE")
                .bind(row_id)
                .fetch_optional(&mut *c)
                .await
                .map_err(db)?;
        }
        let session:Option<(String,i64)>=sqlx::query_as("SELECT \"userId\",(EXTRACT(EPOCH FROM \"expiresAt\"::timestamptz)*1000)::bigint FROM session WHERE id=$1").bind(id).fetch_optional(&mut *c).await.map_err(db)?;
        if !session.is_some_and(|(user, expires)| user == self.expected.user_id && expires > now) {
            return Err(revoked());
        }
        if let Some(audience) = &self.audience {
            let child = binding(&mut *c, id, true).await?.ok_or_else(revoked)?;
            if child.parent != parent || !current(&child, audience, now) {
                return Err(revoked());
            }
            if audience == "account" {
                if parent.is_some() || !selected(&mut *c, id).await? {
                    return Err(revoked());
                }
            } else {
                let parent_id = parent.as_deref().ok_or_else(revoked)?;
                let pb = binding(&mut *c, parent_id, true)
                    .await?
                    .ok_or_else(revoked)?;
                let ps:Option<(String,i64)>=sqlx::query_as("SELECT \"userId\",(EXTRACT(EPOCH FROM \"expiresAt\"::timestamptz)*1000)::bigint FROM session WHERE id=$1").bind(parent_id).fetch_optional(&mut *c).await.map_err(db)?;
                if pb.parent.is_some()
                    || !current(&pb, "account", now)
                    || pb.authenticated != child.authenticated
                    || pb.expires < child.expires
                    || !ps.is_some_and(|(u, e)| u == self.expected.user_id && e > now)
                    || !selected(&mut *c, parent_id).await?
                {
                    return Err(revoked());
                }
            }
        }
        let user: Option<(String, i64)> =
            sqlx::query_as("SELECT email,\"emailVerified\" FROM \"user\" WHERE id=$1")
                .bind(&self.expected.user_id)
                .fetch_optional(c)
                .await
                .map_err(db)?;
        if !user.is_some_and(|(email, verified)| {
            verified != 0 && email.to_lowercase() == self.expected.email.to_lowercase()
        }) {
            return Err(revoked());
        }
        Ok(())
    }
    pub async fn commit(self) -> Result<(), Error> {
        self.tx.commit().await.map_err(db)
    }
    pub async fn rollback(self) -> Result<(), Error> {
        self.tx.rollback().await.map_err(db)
    }
}

/// Token-authenticated lease. Unlike the recheck-only lease, its constructor
/// itself verifies the presented token through the Identity owner.
pub struct AuthenticatedPortalLease {
    lease: PortalPrivateSessionLease,
}

/// A current identity borrowed from the live authenticated lease. Private
/// fields prevent callers from constructing a view from arbitrary user facts.
pub struct VerifiedPortalIdentity<'a> {
    proof: &'a PortalSessionProof,
}
impl VerifiedPortalIdentity<'_> {
    pub fn session_id(&self) -> &str {
        &self.proof.session_id
    }
    pub fn user_id(&self) -> &str {
        &self.proof.user_id
    }
    pub fn email(&self) -> &str {
        &self.proof.email
    }
    pub fn email_verified(&self) -> bool {
        self.proof.email_verified
    }
    pub fn authenticated_at(&self) -> i64 {
        self.proof.authenticated_at
    }
    pub fn expires_at(&self) -> i64 {
        self.proof.expires_at
    }
}
impl AuthenticatedPortalLease {
    /// Verify the opaque token before acquiring the Control fence, matching the
    /// normal-auth-before-lease ordering. Never retains the token.
    pub async fn authenticate(
        pool: &DatabasePool,
        request: &PortalSessionReadRequest<'_>,
        now: i64,
    ) -> Result<Self, Error> {
        let mut c = pool.raw().acquire().await.map_err(|_| unavailable())?;
        let proof = verify_portal_session(&mut c, request, now)
            .await?
            .ok_or_else(revoked)?;
        drop(c);
        let lease =
            PortalPrivateSessionLease::begin(pool, proof, request.audience.map(str::to_owned), now)
                .await?;
        Ok(Self { lease })
    }
    /// The borrowed view cannot outlive the held lease or overlap its mutation.
    pub async fn verified_identity(
        &mut self,
        now: i64,
    ) -> Result<VerifiedPortalIdentity<'_>, Error> {
        self.lease.require_current(now).await?;
        Ok(VerifiedPortalIdentity {
            proof: &self.lease.expected,
        })
    }
    pub async fn require_current(&mut self, now: i64) -> Result<(), Error> {
        self.lease.require_current(now).await
    }
    pub async fn commit(self) -> Result<(), Error> {
        self.lease.commit().await
    }
    pub async fn rollback(self) -> Result<(), Error> {
        self.lease.rollback().await
    }
}
