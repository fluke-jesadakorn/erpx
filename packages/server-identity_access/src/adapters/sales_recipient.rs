//! Identity half of the retained Sales recipient predicate. People employment
//! facts and the actual owner source are independently retained by composition.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub struct SalesRecipientRequest {
    pub company_id: CompanyId,
    pub user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
}

pub struct CurrentSalesRecipient {
    request: SalesRecipientRequest,
    eligible: bool,
    unit: Unit,
}

/// Opaque proof that this transaction acquired the source locks before this
/// port's read. Composition must call the lock function before any snapshot.
pub struct SalesRecipientSourceLocks {
    unit: Unit,
}

/// Holds the original connection while composition prepares its other sources.
/// Caller must enter before the first transaction snapshot and perform only
/// source-locking utility operations until `bind`.
pub struct SalesRecipientLockPreparation<'a> {
    connection: &'a mut PgConnection,
}
impl SalesRecipientLockPreparation<'_> {
    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }
    pub async fn bind(self) -> Result<SalesRecipientSourceLocks, Error> {
        Ok(SalesRecipientSourceLocks {
            unit: unit(self.connection).await?,
        })
    }
}

impl CurrentSalesRecipient {
    pub fn is_eligible(&self) -> bool {
        self.eligible
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || read(c, &self.request).await? != self.eligible {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IAM_SALES_RECIPIENT_CHANGED",
                "The current Sales recipient assignment changed.",
            )
            .into());
        }
        Ok(())
    }
}

/// Call before the first snapshot in the owning transaction.
///
/// This locks every Identity, organization, People and policy source used by the
/// recipient predicate. Composition may acquire additional owner locks through
/// the returned preparation's `connection()` before `bind()`; `bind()` must
/// occur before any snapshot or read.
pub async fn lock_sales_recipient_sources(
    c: &mut PgConnection,
) -> Result<SalesRecipientLockPreparation<'_>, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_TRANSACTION_REQUIRED").into());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(c).await?;
    Ok(SalesRecipientLockPreparation { connection: c })
}

pub async fn inspect_sales_recipient(
    c: &mut PgConnection,
    locks: &SalesRecipientSourceLocks,
    request: SalesRecipientRequest,
) -> Result<CurrentSalesRecipient, Error> {
    let unit = unit(c).await?;
    if unit != locks.unit {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_LOCKS_REQUIRED").into());
    }
    let eligible = read(c, &request).await?;
    Ok(CurrentSalesRecipient {
        request,
        eligible,
        unit,
    })
}

async fn read(c: &mut PgConnection, r: &SalesRecipientRequest) -> Result<bool, Error> {
    // Both permissions must belong to the same active scoped assignment, as
    // in Sales' existing recipient predicate. Use the actual database clock.
    Ok(sqlx::query_scalar::<_, bool>("SELECT true FROM assignments a WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) OR (a.scope_kind='department' AND a.scope_node_id=$4)) ORDER BY a.id LIMIT 1 FOR SHARE OF a")
        .bind(r.company_id.as_str()).bind(&r.user_id).bind(&r.branch_id).bind(r.department_id.as_deref())
        .fetch_optional(c).await?.unwrap_or(false))
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("IAM_SALES_RECIPIENT_TRANSACTION_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
