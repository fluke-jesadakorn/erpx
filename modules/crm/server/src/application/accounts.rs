//! Native CRM account create / Finance-link use cases.
//!
//! The application layer validates trimmed plans via the domain rules and
//! delegates persistence to the adapter ports. It performs no SQL, no purchase
//! check (that stays in TS `purchases.ts` / `withCRMPurchasedCommand`) and no
//! Workflow ledger write (receipts still come from TS `executeCommand`). The
//! Finance customer witness and reviewer authority are composition inputs.
use crate::domain::accounts::{
    AccountCreateInput, AccountCreatePlan, AccountLinkFinanceActor, AccountLinkFinanceInput,
    AccountLinkFinancePlan, ContactCreateInput, ContactCreatePlan, GuestLinkInput, GuestLinkPlan,
    GuestSharingInput, GuestSharingPlan, validate_account_create, validate_account_link_finance,
    validate_contact_create, validate_guest_link, validate_guest_sharing,
};
use erpx_platform_shared_server::public::BusinessError;
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
    command_key: &str,
    input: &AccountCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<AccountCreatePlan, R::Error>
where
    R: AccountCreateRepository,
{
    if actor_user_id.is_empty() || company_id.is_empty() || command_key.is_empty() || now < 0 {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_INPUT_INVALID",
            "Use a bounded CRM account name of 2 to 160 characters.",
        )
        .into());
    }
    let plan = validate_account_create(input)?;
    // The route layer passes the command key; the record id (`account-{key}`)
    // matches the TS writer exactly. The repository writes only `crm_accounts`.
    let record_id = crate::domain::accounts::record_id_for_key(command_key);
    repository
        .insert_account(company_id, &record_id, actor_user_id, &plan, now)
        .await?;
    Ok(plan)
}

pub trait AccountLinkFinanceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn link_finance<'a>(
        &'a mut self,
        actor: &'a AccountLinkFinanceActor,
        plan: &'a AccountLinkFinancePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error>;
}

/// Validate and apply the owner-local Finance customer link. Composition must
/// first authorize `commerce.approve` and retain the Finance customer witness.
pub async fn link_account_finance<R>(
    actor: &AccountLinkFinanceActor,
    input: &AccountLinkFinanceInput,
    repository: &mut R,
    now: i64,
) -> Result<AccountLinkFinancePlan, R::Error>
where
    R: AccountLinkFinanceRepository,
{
    if actor.user_id.is_empty() || actor.company_id.is_empty() || now < 0 {
        return Err(BusinessError::invalid(
            "CRM_ACCOUNT_INPUT_INVALID",
            "Use a bounded Finance customer link with review evidence.",
        )
        .into());
    }
    let plan = validate_account_link_finance(actor, input)?;
    repository.link_finance(actor, &plan, now).await?;
    Ok(plan)
}

/// Owner-local persistence for the remaining identity actions. Only CRM-owned
/// `crm_contacts`, `crm_guest_links` and `crm_guest_sharing_events` rows are
/// written; the PMS source witness, package capability and Workflow receipt
/// remain composition responsibilities.
pub trait IdentityFamilyRepository: Send {
    type Error: From<BusinessError> + Send;

    fn insert_contact<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a ContactCreatePlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error>;

    fn link_guest<'a>(
        &'a mut self,
        company_id: &'a str,
        link_id: &'a str,
        sharing_event_id: &'a str,
        created_by: &'a str,
        plan: &'a GuestLinkPlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error>;

    fn change_guest_sharing<'a>(
        &'a mut self,
        company_id: &'a str,
        sharing_event_id: &'a str,
        created_by: &'a str,
        plan: &'a GuestSharingPlan,
        now: i64,
    ) -> AccountsFuture<'a, (), Self::Error>;
}

fn bounded_identity_actor(actor_user_id: &str, company_id: &str, command_key: &str, now: i64) -> Result<(), BusinessError> {
    if actor_user_id.is_empty() || company_id.is_empty() || command_key.is_empty() || now < 0 {
        return Err(identity_actor_invalid());
    }
    Ok(())
}

fn identity_actor_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_IDENTITY_INPUT_INVALID",
        "Use a bounded CRM identity command.",
    )
}

pub async fn create_contact<R>(
    actor_user_id: &str,
    company_id: &str,
    command_key: &str,
    input: &ContactCreateInput,
    repository: &mut R,
    now: i64,
) -> Result<ContactCreatePlan, R::Error>
where
    R: IdentityFamilyRepository,
{
    bounded_identity_actor(actor_user_id, company_id, command_key, now)?;
    let plan = validate_contact_create(input)?;
    let record_id = crate::domain::accounts::contact_record_id_for_key(command_key);
    repository
        .insert_contact(company_id, &record_id, actor_user_id, &plan, now)
        .await?;
    Ok(plan)
}

pub async fn link_guest<R>(
    actor_user_id: &str,
    company_id: &str,
    command_key: &str,
    input: &GuestLinkInput,
    repository: &mut R,
    now: i64,
) -> Result<GuestLinkPlan, R::Error>
where
    R: IdentityFamilyRepository,
{
    bounded_identity_actor(actor_user_id, company_id, command_key, now)?;
    let plan = validate_guest_link(input)?;
    let link_id = crate::domain::accounts::guest_link_record_id_for_key(command_key);
    let sharing_event_id = crate::domain::accounts::sharing_event_id_for_key(command_key);
    repository
        .link_guest(company_id, &link_id, &sharing_event_id, actor_user_id, &plan, now)
        .await?;
    Ok(plan)
}

pub async fn change_guest_sharing<R>(
    actor_user_id: &str,
    company_id: &str,
    command_key: &str,
    input: &GuestSharingInput,
    repository: &mut R,
    now: i64,
) -> Result<GuestSharingPlan, R::Error>
where
    R: IdentityFamilyRepository,
{
    bounded_identity_actor(actor_user_id, company_id, command_key, now)?;
    let plan = validate_guest_sharing(input)?;
    let sharing_event_id = crate::domain::accounts::sharing_event_id_for_key(command_key);
    repository
        .change_guest_sharing(company_id, &sharing_event_id, actor_user_id, &plan, now)
        .await?;
    Ok(plan)
}
