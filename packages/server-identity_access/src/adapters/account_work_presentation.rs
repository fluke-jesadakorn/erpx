//! Native selected Work data for rendering/navigation only. No command actor is
//! constructed and no copied permission list can authorize a business write.
use super::{
    holding_work_context::HoldingAccountWorkScope,
    postgres_effective_authority::{current_authority_facts, lock_effective_authority_sources},
    work_admission::work_module_catalog,
};
use crate::{
    application::effective_authority::{
        current_iam_metadata, evaluate_current_iam_authority, inspect_selected_work,
        selected_context_read,
    },
    domain::effective_authority::{
        CommandIamMetadata, SelectedContextRead, WorkAuthorizationRequest,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
pub struct AccountWorkPresentation {
    pub context: SelectedContextRead,
    pub iam: CommandIamMetadata,
    pub beta: bool,
}
pub async fn lock_account_work_presentation_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_effective_authority_sources(c).await?;
    erpx_server_organization::public::lock_hierarchy_sources(c).await
}
pub async fn read_account_work_presentation(
    c: &mut PgConnection,
    scope: &HoldingAccountWorkScope,
    destination: &str,
    api: bool,
    now: i64,
) -> Result<AccountWorkPresentation, Error> {
    scope.require_current(c, now).await?;
    let request = scope.request();
    let facts = current_authority_facts(c, request, now).await?;
    // inspect_selected_work uses destination/admission and current IAM but never
    // grants the empty permission. Business commands still use the authorizer.
    let deadline = inspect_selected_work(
        &facts,
        &WorkAuthorizationRequest {
            permission: String::new().into(),
            module: None,
            destination: destination.into(),
            api,
        },
        now,
    )?;
    let evaluated = evaluate_current_iam_authority(&facts.iam, now)?;
    let context = selected_context_read(request, &facts, work_module_catalog()?, now, deadline)?;
    let iam = current_iam_metadata(request, &facts, &evaluated);
    scope.require_current(c, now).await?;
    Ok(AccountWorkPresentation {
        context,
        iam,
        beta: facts.beta,
    })
}
