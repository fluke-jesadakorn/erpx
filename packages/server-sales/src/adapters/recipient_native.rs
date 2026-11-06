//! Sales-owned recipient reference composed from Identity authority facts and
//! People workforce facts.
//!
//! Lock semantics:
//! - `prepare_sales_recipient_sources` delegates to Identity's
//!   `lock_sales_recipient_sources`. It must run first in a SERIALIZABLE
//!   transaction; Identity locks assignments, roles, organization sources,
//!   People authority sources (`hr_people`, `hr_engagements`, ...), and policy
//!   sources before the first snapshot.
//! - The returned preparation exposes `connection()` so composition can acquire
//!   any additional owner locks (for example CRM leads) before `bind()`. `bind()`
//!   captures the immutable transaction/unit token; no read or snapshot may
//!   happen before it.
//! - `inspect_sales_recipient` re-verifies that same unit token, reads the
//!   Identity predicate, then reads People using a timestamp taken from
//!   `current_timestamp` (transaction start), preserving the TypeScript
//!   `current_timestamp` engagement-date rule even if the unit crosses UTC
//!   midnight.
//!
//! People workforce_reference reuse:
//! - Sales reuses `erpx_server_people_payroll::public::workforce_reference::
//!   inspect_workforce_reference_raw` instead of defining its own employment
//!   SQL. Only the required eligibility predicate is transferred; no contact,
//!   salary, or profile data leaves People.
//! - Because Identity's source lock already covers `hr_people` and
//!   `hr_engagements`, Sales does not call a separate People lock function.
//! - `CurrentSalesRecipient::require_current` re-checks both the Identity unit/
//!   eligibility and the People transaction/eligibility using the stored
//!   transaction timestamp.
//!
//! This is a scoped recipient reference, not a selected actor or command grant.
//! The business owner retains its actual source and command authorization.
use erpx_server_identity_access::public::sales_recipient::{
    CurrentSalesRecipient as IdentityRecipient, SalesRecipientLockPreparation,
    SalesRecipientRequest as IdentityRequest, SalesRecipientSourceLocks,
    inspect_sales_recipient as inspect_identity, lock_sales_recipient_sources,
};
use erpx_server_people_payroll::public::workforce_reference::{
    CurrentWorkforceReference, inspect_workforce_reference_raw,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

pub struct SalesRecipientRequest {
    pub company_id: CompanyId,
    pub user_id: String,
    pub branch_id: String,
    pub department_id: Option<String>,
}

pub struct CurrentSalesRecipient {
    identity: IdentityRecipient,
    people: CurrentWorkforceReference,
    transaction_time: TimestampMillis,
}

impl CurrentSalesRecipient {
    pub fn is_eligible(&self) -> bool {
        self.identity.is_eligible() && self.people.is_eligible()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.identity.require_current(&mut *c).await?;
        self.people.require_current(c, self.transaction_time).await
    }
}

/// Call before any snapshot. Identity preparation locks its own sources and
/// People sources; caller acquires all other owner locks via `connection()`
/// before binding. Keep the bound token and actual source in this same unit.
pub async fn prepare_sales_recipient_sources(
    c: &mut PgConnection,
) -> Result<SalesRecipientLockPreparation<'_>, Error> {
    lock_sales_recipient_sources(c).await
}

pub async fn inspect_sales_recipient(
    c: &mut PgConnection,
    locks: &SalesRecipientSourceLocks,
    request: SalesRecipientRequest,
) -> Result<CurrentSalesRecipient, Error> {
    let identity = inspect_identity(
        &mut *c,
        locks,
        IdentityRequest {
            company_id: request.company_id.clone(),
            user_id: request.user_id.clone(),
            branch_id: request.branch_id,
            department_id: request.department_id,
        },
    )
    .await?;
    // TypeScript uses PostgreSQL current_timestamp for the engagement date;
    // it is fixed at transaction start even if the unit crosses UTC midnight.
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM current_timestamp)*1000)::bigint")
            .fetch_one(&mut *c)
            .await?;
    let transaction_time = TimestampMillis::new(now);
    let people =
        inspect_workforce_reference_raw(c, &request.company_id, &request.user_id, transaction_time)
            .await?;
    Ok(CurrentSalesRecipient {
        identity,
        people,
        transaction_time,
    })
}
