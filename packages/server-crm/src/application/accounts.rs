//! Native CRM account create use case.
//!
//! The application layer validates the trimmed plan via the domain rules and
//! delegates persistence to the adapter port. It performs no SQL, no purchase
//! check (that stays in TS `purchases.ts` / `withCRMPurchasedCommand`) and no
//! Workflow ledger write (receipts still come from TS `executeCommand`).
use crate::domain::accounts::{AccountCreateInput, AccountCreatePlan, validate_account_create};
use erpx_server_shared::public::BusinessError;
use std::future::Future;

pub type AccountsFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Owner-local account row. Only `crm_accounts` is written by this slice.
pub trait AccountCreateRepository: Send {
    type Error: From<BusinessError> + Send;

    fn insert_account<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a AccountCreatePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error>;
}

pub async fn create_account<R>(
    actor_user_id: &str,
    company_id: &str,
    input: &AccountCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<AccountCreatePlan, R::Error>
where
    R: AccountCreateRepository,
{
    if actor_user_id.is_empty() || now < 0 {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_INPUT_INVALID",
            "Use a bounded CRM account name of 2 to 160 characters.",
        )
        .into());
    }
    let plan = validate_account_create(input)?;
    // The route layer passes the command key; the record id (`account-{key}`)
    // matches the TS writer exactly. The repository writes only `crm_accounts`.
    let record_id = crate::domain::accounts::record_id_for_key("__application__");
    repository
        .insert_account(company_id, &record_id, actor_user_id, &plan, now)
        .await?;
    Ok(plan)
}
