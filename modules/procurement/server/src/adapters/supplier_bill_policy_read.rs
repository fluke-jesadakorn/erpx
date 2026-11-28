//! PostgreSQL read adapter for the native Procurement supplier-invoice policy.
//!
//! This is the only layer that executes SQL for this operation. It reads the
//! Procurement-owned policy header and its immutable event history on the
//! caller-owned Holding connection, joining the current HR person display name
//! exactly as the retained `sourceImage`/`orderedRead` literals did. No IAM
//! authority, commercial entitlement or command write happens here.
use crate::application::supplier_bill_policy_read::{
    invoice_policy, PolicyReadFuture, PolicyReadRepository,
};
use crate::domain::supplier_bill_policy_read::{
    InvoicePolicyView, PolicyEventRow, PolicyHeadRow, PolicyPurchase, PolicyReadActor,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_BILL_POLICY_UNAVAILABLE",
        "The Procurement invoice policy is unavailable.",
    )
    .into()
}

pub struct PgSupplierBillPolicyRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgSupplierBillPolicyRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    /// Current Company matching policy with the active HR person name. The
    /// `merged_into_id IS NULL` predicate mirrors the retained people projection
    /// so a merged identity never supplies a display name.
    pub async fn head(
        &mut self,
        company: &CompanyId,
    ) -> Result<Option<PolicyHeadRow>, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(
            "SELECT p.matching_mode,p.version,p.updated_by,p.updated_at,\
             COALESCE(person.legal_name,p.updated_by) updated_by_name \
             FROM procurement_invoice_policies p \
             LEFT JOIN hr_people person ON person.company_id=p.company_id \
               AND person.user_id=p.updated_by AND person.merged_into_id IS NULL \
             WHERE p.company_id=$1",
        )
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(PolicyHeadRow {
                matching_mode: row.try_get("matching_mode")?,
                version: row.try_get("version")?,
                updated_by: row.try_get("updated_by")?,
                updated_at: row.try_get("updated_at")?,
                updated_by_name: row.try_get("updated_by_name")?,
            })
        })
        .transpose()
    }

    /// Immutable policy events, newest version first, with the actor display
    /// name. Events are read only when the caller may manage the policy; the
    /// application gate runs before this method is called.
    pub async fn events(
        &mut self,
        company: &CompanyId,
    ) -> Result<Vec<PolicyEventRow>, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let rows = sqlx::query(
            "SELECT e.id,e.company_id,e.version,e.previous_mode,e.matching_mode,\
             e.actor_id,e.reason,e.created_at,\
             COALESCE(person.legal_name,e.actor_id) actor_name \
             FROM procurement_invoice_policy_events e \
             LEFT JOIN hr_people person ON person.company_id=e.company_id \
               AND person.user_id=e.actor_id AND person.merged_into_id IS NULL \
             WHERE e.company_id=$1 ORDER BY e.version DESC",
        )
        .bind(company.as_str())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PolicyEventRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    version: row.try_get("version")?,
                    previous_mode: row.try_get("previous_mode")?,
                    matching_mode: row.try_get("matching_mode")?,
                    actor_id: row.try_get("actor_id")?,
                    reason: row.try_get("reason")?,
                    created_at: row.try_get("created_at")?,
                    actor_name: row.try_get("actor_name")?,
                })
            })
            .collect()
    }
}

impl PolicyReadRepository for PgSupplierBillPolicyRead<'_> {
    type Error = Error;

    fn head<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PolicyReadFuture<'a, Option<PolicyHeadRow>, Self::Error> {
        Box::pin(async move { PgSupplierBillPolicyRead::head(self, company).await })
    }

    fn events<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PolicyReadFuture<'a, Vec<PolicyEventRow>, Self::Error> {
        Box::pin(async move { PgSupplierBillPolicyRead::events(self, company).await })
    }
}

/// Runs the retained policy read on the caller-owned Holding connection.
/// Composition supplies the Account-derived actor, the Company Finance
/// authority decision, the commercial purchased-package overlay and `now`;
/// this helper never opens a transaction and never grants authority.
pub async fn invoice_policy_in_transaction(
    connection: &mut PgConnection,
    actor: &PolicyReadActor,
    company_authority: bool,
    purchase: Option<PolicyPurchase>,
    now: erpx_platform_shared_server::public::TimestampMillis,
) -> Result<InvoicePolicyView, Error> {
    let mut repository = PgSupplierBillPolicyRead::new(connection);
    invoice_policy(actor, &mut repository, company_authority, purchase, now).await
}
