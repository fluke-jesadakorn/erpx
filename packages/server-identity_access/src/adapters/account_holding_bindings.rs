//! Identity-owned configured binding and current Control source adapter. Static configuration is not a live binding
//! proof, a session, an actor or permission to enter a Holding.
use crate::domain::account_auth::{AccountPolicy, RequestAudience};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection, Row};
use std::collections::{BTreeMap, BTreeSet};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfiguredAccountHolding {
    pub holding_id: HoldingId,
    pub slug: String,
    pub write_binding: String,
    pub read_binding: String,
    pub files_binding: String,
}
#[derive(Clone)]
pub struct ConfiguredHoldingRegistry {
    entries: BTreeMap<String, ConfiguredAccountHolding>,
}
impl ConfiguredHoldingRegistry {
    pub fn new(
        policy: &AccountPolicy,
        configured: Vec<ConfiguredAccountHolding>,
    ) -> Result<Self, BusinessError> {
        policy.validate()?;
        if configured.len() != policy.holdings.len() || configured.len() > 64 {
            return Err(invalid());
        }
        let mut ids = BTreeMap::new();
        let mut slugs = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for entry in configured {
            if entry.slug.is_empty()
                || entry.slug.len() > 64
                || !entry
                    .slug
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !slugs.insert(entry.slug.clone())
                || ids.contains_key(&entry.holding_id)
            {
                return Err(invalid());
            }
            for key in [
                &entry.write_binding,
                &entry.read_binding,
                &entry.files_binding,
            ] {
                if key.is_empty()
                    || key.len() > 128
                    || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    || !bindings.insert(key.clone())
                {
                    return Err(invalid());
                }
            }
            ids.insert(entry.holding_id.clone(), entry);
        }
        let mut entries = BTreeMap::new();
        for target in &policy.holdings {
            let parts = target.audience.split(':').collect::<Vec<_>>();
            let id = HoldingId::new(parts[2]).map_err(|_| invalid())?;
            let entry = ids.remove(&id).ok_or_else(invalid)?;
            if entries.insert(target.audience.clone(), entry).is_some() {
                return Err(invalid());
            }
        }
        if !ids.is_empty() {
            return Err(invalid());
        }
        Ok(Self { entries })
    }
    pub fn audience_for_holding(
        &self,
        holding: &HoldingId,
    ) -> Result<RequestAudience, BusinessError> {
        self.entries
            .iter()
            .find(|(_, entry)| &entry.holding_id == holding)
            .map(|(audience, _)| RequestAudience::Holding {
                audience: audience.clone(),
            })
            .ok_or_else(unavailable)
    }
    pub fn configured(&self) -> impl Iterator<Item = &ConfiguredAccountHolding> {
        self.entries.values()
    }
    pub fn expected_for_audience(
        &self,
        audience: &RequestAudience,
    ) -> Result<&ConfiguredAccountHolding, BusinessError> {
        let RequestAudience::Holding { audience } = audience else {
            return Err(unavailable());
        };
        self.entries.get(audience).ok_or_else(unavailable)
    }
    /// Acquire the complete session-first utility bundle before calling this.
    /// The returned source is retained on this SAME Control transaction only.
    pub async fn read_current(
        &self,
        c: &mut PgConnection,
        audience: &RequestAudience,
    ) -> Result<CurrentHoldingBinding, Error> {
        require_transaction(c).await?;
        let expected = self.expected_for_audience(audience)?.clone();
        let actual = read(c, &expected.holding_id)
            .await?
            .ok_or_else(unavailable)?;
        if actual != expected {
            return Err(unavailable().into());
        }
        Ok(CurrentHoldingBinding {
            expected,
            transaction: transaction(c).await?,
        })
    }
    /// Startup only: exact configured coverage, not a request-lifetime authority.
    /// Caller starts its own short Control unit and locks sources BEFORE reads.
    pub async fn verify_startup(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_transaction(c).await?;
        let rows=sqlx::query("SELECT CASE WHEN octet_length(holding_id)<=200 THEN holding_id END id FROM holding_bindings WHERE status='active' ORDER BY holding_id LIMIT 65 FOR SHARE")
   .fetch_all(&mut*c).await?;
        if rows.len() != self.entries.len() {
            return Err(unavailable().into());
        }
        let mut observed = BTreeSet::new();
        for row in rows {
            let id: String = row
                .try_get::<Option<String>, _>("id")?
                .ok_or_else(unavailable)?;
            observed.insert(HoldingId::new(id).map_err(|_| unavailable())?);
        }
        let expected = self
            .entries
            .values()
            .map(|v| v.holding_id.clone())
            .collect::<BTreeSet<_>>();
        if observed != expected {
            return Err(unavailable().into());
        }
        for audience in self.entries.keys() {
            self.read_current(
                c,
                &RequestAudience::Holding {
                    audience: audience.clone(),
                },
            )
            .await?;
        }
        Ok(())
    }
}
#[derive(Eq, PartialEq)]
struct Transaction {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}
/// No public constructor, Clone, Serialize or Deserialize. No actor is minted.
pub struct CurrentHoldingBinding {
    expected: ConfiguredAccountHolding,
    transaction: Transaction,
}
impl CurrentHoldingBinding {
    pub fn holding_id(&self) -> &HoldingId {
        &self.expected.holding_id
    }
    pub fn write_binding(&self) -> &str {
        &self.expected.write_binding
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_transaction(c).await?;
        if transaction(c).await? != self.transaction
            || read(c, &self.expected.holding_id).await?.as_ref() != Some(&self.expected)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Utility only. Session-bearing callers acquire session/device source fences
/// first, then this lock, BEFORE ANY data SELECT fixes a Control snapshot.
/// Queue-only callers have no Account credential/Argon work in this lease.
pub async fn lock_holding_binding_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ACCOUNT_CONTROL_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE holding_bindings IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn require_transaction(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ACCOUNT_CONTROL_TRANSACTION_REQUIRED").into());
    }
    let level: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if !matches!(level.as_str(), "read committed" | "serializable") {
        return Err(PersistenceError::new("ACCOUNT_CONTROL_ISOLATION_REQUIRED").into());
    }
    Ok(())
}
async fn read(
    c: &mut PgConnection,
    id: &HoldingId,
) -> Result<Option<ConfiguredAccountHolding>, Error> {
    // Do not put active/expected-binding predicates in WHERE. A known row updated
    // after an older SERIALIZABLE snapshot must reach its FOR SHARE tuple fence
    // and raise40001; it must not silently return the old active version.
    let row=sqlx::query("SELECT status,CASE WHEN octet_length(slug)<=64 THEN slug END slug,CASE WHEN octet_length(write_binding)<=128 THEN write_binding END write_binding,CASE WHEN octet_length(read_binding)<=128 THEN read_binding END read_binding,CASE WHEN octet_length(files_binding)<=128 THEN files_binding END files_binding FROM holding_bindings WHERE holding_id=$1 FOR SHARE")
  .bind(id.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if row.try_get::<&str, _>("status")? != "active" {
        return Ok(None);
    }
    let get = |name| {
        row.try_get::<Option<String>, _>(name)
            .map_err(Error::from)?
            .ok_or_else(|| PgTransactionError::Business(unavailable()))
    };
    Ok(Some(ConfiguredAccountHolding {
        holding_id: id.clone(),
        slug: get("slug")?,
        write_binding: get("write_binding")?,
        read_binding: get("read_binding")?,
        files_binding: get("files_binding")?,
    }))
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    let r=sqlx::query("SELECT current_database() d,pg_backend_pid() p,pg_current_xact_id()::text x,pg_postmaster_start_time()::text s").fetch_one(c).await?;
    Ok(Transaction {
        database: r.try_get("d")?,
        backend: r.try_get("p")?,
        xid: r.try_get("x")?,
        postmaster: r.try_get("s")?,
    })
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "INVALID_ACCOUNT_HOLDING_REGISTRY",
        "Configure exact distinct Holding identities, slugs and binding keys.",
    )
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "HOLDING_BINDING_UNAVAILABLE",
        "Not found.",
    )
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "HOLDING_BINDING_CHANGED",
        "The current Holding binding changed.",
    )
    .into()
}
