//! Finance's exact party predicate for a Commerce order save. This is a
//! transaction-bound source decision, not an authority or purchase grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String, String, String, String, i64);

/// Acquire before the unit's first protected snapshot. A lock conflict must
/// retry the complete unit; composition owns the global lock order.
pub async fn lock_commerce_order_party_source(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_PARTY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.finance_parties IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCE_PARTY_SERIALIZABLE_REQUIRED").into());
    }
    let (held, rls, oid): (bool, bool, i64) = sqlx::query_as("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.relation=c.oid AND l.granted AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')),row_security_active(c.oid),c.oid::bigint FROM pg_class c WHERE c.oid='public.finance_parties'::regclass").fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("COMMERCE_PARTY_LOCK_REQUIRED").into());
    }
    if rls {
        return Err(PersistenceError::new("COMMERCE_PARTY_RLS_UNSUPPORTED").into());
    }
    let (database, pid, xid, started, role, session, path): (String, i32, String, String, String, String, String) = sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text,current_user::text,session_user::text,current_setting('search_path')").fetch_one(c).await?;
    Ok((database, pid, xid, started, role, session, path, oid))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommerceOrderPartyKind {
    Customer,
    Supplier,
}
impl CommerceOrderPartyKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Supplier => "supplier",
        }
    }
}

async fn decision(
    c: &mut PgConnection,
    company: &CompanyId,
    party_id: &str,
    kind: CommerceOrderPartyKind,
    currency: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.finance_parties WHERE id=$1 AND company_id=$2 AND kind=$3 AND status='active' AND currency=$4 AND person_id IS NULL)")
        .bind(party_id).bind(company.as_str()).bind(kind.as_str()).bind(currency).fetch_one(c).await?)
}

pub struct CurrentCommerceOrderPartyGuard {
    unit: Unit,
    company: CompanyId,
    party_id: String,
    kind: CommerceOrderPartyKind,
    currency: String,
    matched: bool,
}

/// Evaluate at the legacy party-guard phase, after Money. The selected company,
/// order kind and currency must come from authenticated owner sources.
pub async fn observe_commerce_order_party_guard(
    c: &mut PgConnection,
    company: &CompanyId,
    party_id: &str,
    kind: CommerceOrderPartyKind,
    currency: &str,
) -> Result<CurrentCommerceOrderPartyGuard, Error> {
    // Current native Commerce transport caps the complete JSON body at 8 KiB.
    // The currency is trusted setup data; bound it independently as well.
    if party_id.len() > 8 * 1024 || currency.len() > 8 * 1024 {
        return Err(PersistenceError::new("COMMERCE_PARTY_SOURCE_INPUT_TOO_LARGE").into());
    }
    let identity = unit(&mut *c).await?;
    let matched = decision(&mut *c, company, party_id, kind, currency).await?;
    if unit(c).await? != identity {
        return Err(changed().into());
    }
    Ok(CurrentCommerceOrderPartyGuard {
        unit: identity,
        company: company.clone(),
        party_id: party_id.to_owned(),
        kind,
        currency: currency.to_owned(),
        matched,
    })
}
impl CurrentCommerceOrderPartyGuard {
    pub fn matched(&self) -> bool {
        self.matched
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(&mut *c).await? != self.unit
            || decision(
                &mut *c,
                &self.company,
                &self.party_id,
                self.kind,
                &self.currency,
            )
            .await?
                != self.matched
            || unit(c).await? != self.unit
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_PARTY_SOURCE_CHANGED",
        "The Finance party source changed.",
    )
}
