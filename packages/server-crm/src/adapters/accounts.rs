//! PostgreSQL adapter for the native CRM account create and Finance-link
//! owner effects.
//!
//! Only the CRM owner's `crm_accounts` table is written here. The caller owns
//! the retained serializable Holding transaction. No purchase check, Finance
//! customer witness, notification or Workflow ledger write happens in this
//! adapter; those stay in the TS runtime (`purchases.ts`, `executeCommand`)
//! until the coordinator wires the Rust ledger + `sales.crm` enforcement path.
use crate::application::accounts::{
    AccountCreateRepository, AccountLinkFinanceRepository, AccountsFuture,
};
use crate::domain::accounts::{
    AccountCreatePlan, AccountLinkFinanceActor, AccountLinkFinanceInput, AccountLinkFinancePlan,
    validate_account_link_finance,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_ACCOUNT_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_ACCOUNT_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCrmAccountCreate<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmAccountCreate<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn insert(
        &mut self,
        company_id: &str,
        record_id: &str,
        created_by: &str,
        plan: &AccountCreatePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        if company_id.is_empty() || record_id.is_empty() || created_by.is_empty() {
            return Err(BusinessError::invalid(
                "CRM_ACCOUNT_INPUT_INVALID",
                "Use a bounded CRM account name of 2 to 160 characters.",
            )
            .into());
        }
        let inserted = sqlx::query_scalar::<_, i64>(
            "INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING version",
        )
        .bind(record_id)
        .bind(company_id)
        .bind(&plan.kind)
        .bind(&plan.name)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .fetch_optional(&mut *self.connection)
        .await?;
        if inserted.is_none() {
            return Err(PersistenceError::new("CRM_ACCOUNT_INSERT_FAILED").into());
        }
        Ok(())
    }

    pub async fn load(
        &mut self,
        company_id: &str,
        record_id: &str,
    ) -> Result<Option<AccountCreatePlan>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query("SELECT kind,name FROM crm_accounts WHERE id=$1 AND company_id=$2")
            .bind(record_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            use sqlx::Row;
            Ok(AccountCreatePlan {
                kind: row.try_get("kind")?,
                name: row.try_get("name")?,
            })
        })
        .transpose()
    }
}

impl PgCrmAccountCreate<'_> {
    fn _unused(_: FailureKind) {}

    /// Read-only link preparation for the Workflow `prepare` phase. It checks
    /// schema, version, independent-reviewer and selected branch/department
    /// scope without writing. The Finance customer witness stays external.
    pub async fn prepare_link_finance(
        &mut self,
        actor: &AccountLinkFinanceActor,
        input: &AccountLinkFinanceInput,
    ) -> Result<AccountLinkFinancePlan, Error> {
        let plan = validate_account_link_finance(actor, input)?;
        serializable(&mut *self.connection).await?;
        let available: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM public.crm_accounts a WHERE a.id=$1 AND a.company_id=$2 AND a.version=$3 AND a.finance_party_id IS NULL AND a.created_by<>$4 AND ($5::text IS NULL OR EXISTS(SELECT 1 FROM public.crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=$5 AND ($6::text IS NULL OR l.department_id=$6))))",
        )
        .bind(&plan.account_id)
        .bind(&actor.company_id)
        .bind(plan.expected_version)
        .bind(&actor.user_id)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .fetch_one(&mut *self.connection)
        .await?;
        if !available {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CONFLICT",
                "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
            )
            .into());
        }
        Ok(plan)
    }

    /// Link an independent reviewer's Finance customer to an unlinked CRM
    /// account. The caller must supply the selected branch/department and a
    /// current Finance customer witness; this update is not that witness.
    pub async fn apply_link_finance(
        &mut self,
        actor: &AccountLinkFinanceActor,
        plan: &AccountLinkFinancePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        let updated = sqlx::query(
            "UPDATE public.crm_accounts a SET finance_party_id=$1,review_evidence=$2,reviewed_by=$3,version=version+1,updated_at=$4 WHERE a.id=$5 AND a.company_id=$6 AND a.version=$7 AND a.finance_party_id IS NULL AND a.created_by<>$3 AND ($8::text IS NULL OR EXISTS(SELECT 1 FROM public.crm_leads l WHERE l.account_id=a.id AND l.company_id=a.company_id AND l.branch_id=$8 AND ($9::text IS NULL OR l.department_id=$9)))",
        )
        .bind(&plan.customer_id)
        .bind(&plan.evidence)
        .bind(&actor.user_id)
        .bind(now)
        .bind(&plan.account_id)
        .bind(&actor.company_id)
        .bind(plan.expected_version)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "CONFLICT",
                "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
            )
            .into());
        }
        Ok(())
    }
}

impl AccountCreateRepository for PgCrmAccountCreate<'_> {
    type Error = Error;

    fn insert_account<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a AccountCreatePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmAccountCreate::insert(self, company_id, record_id, created_by, plan, now).await
        })
    }
}

impl AccountLinkFinanceRepository for PgCrmAccountCreate<'_> {
    type Error = Error;

    fn link_finance<'a>(
        &'a mut self,
        actor: &'a AccountLinkFinanceActor,
        plan: &'a AccountLinkFinancePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error> {
        Box::pin(async move { PgCrmAccountCreate::apply_link_finance(self, actor, plan, now).await })
    }
}
