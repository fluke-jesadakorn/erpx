//! PostgreSQL adapter for the native CRM account create slice.
//!
//! Only the CRM owner's `crm_accounts` table is written here. The caller owns
//! the retained serializable Holding transaction. No purchase check, no
//! notification and no Workflow ledger write happens in this adapter; those
//! stay in the TS runtime (`purchases.ts`, `executeCommand`) until the
//! coordinator wires the Rust ledger + `sales.crm` enforcement path.
use crate::application::accounts::{AccountsFuture, AccountCreateRepository};
use crate::domain::accounts::AccountCreatePlan;
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
