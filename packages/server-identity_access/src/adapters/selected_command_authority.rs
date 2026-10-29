//! Synchronous selected-browser authority; this does not assert queued work.
use crate::{
    adapters::{
        permission_catalog::permission_catalog,
        postgres_effective_authority::current_authority_facts, work_admission::work_module_catalog,
    },
    application::{
        command_metadata::project,
        effective_authority::authorize_effective_work,
        selected_command_authority::{
            ServicePartsAuthorityPurpose, required_permission, same_selected_command,
        },
    },
    domain::{
        command_metadata::CommandIamProjection,
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub(crate) async fn selected_unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("SELECTED_COMMAND_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// An in-flight selected Identity witness; its projection is not a grant.
pub struct CurrentServicePartsCommandAuthority<'a> {
    context: &'a AuthorizedWorkContext,
    unit: Unit,
    projection: CommandIamProjection,
    purpose: ServicePartsAuthorityPurpose,
}
impl CurrentServicePartsCommandAuthority<'_> {
    pub fn iam(&self) -> &CommandIamProjection {
        &self.projection
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        let projection = inspect(c, self.context, self.purpose, clock).await?;
        if selected_unit(c).await? != self.unit || projection != self.projection {
            return Err(changed());
        }
        Ok(())
    }
}
/// Only a context lent by the real selected Account/Holding coordinator can
/// authorize this synchronous setup. No durable queue claim is created.
pub async fn capture_service_parts_command_authority<'a>(
    c: &mut PgConnection,
    context: &'a AuthorizedWorkContext,
    purpose: ServicePartsAuthorityPurpose,
    clock: &impl Clock,
) -> Result<CurrentServicePartsCommandAuthority<'a>, Error> {
    let unit = selected_unit(c).await?;
    if context.selected_unit.as_ref() != Some(&unit) {
        return Err(changed());
    }
    let projection = inspect(c, context, purpose, clock).await?;
    Ok(CurrentServicePartsCommandAuthority {
        context,
        unit,
        projection,
        purpose,
    })
}
async fn inspect(
    c: &mut PgConnection,
    context: &AuthorizedWorkContext,
    purpose: ServicePartsAuthorityPurpose,
    clock: &impl Clock,
) -> Result<CommandIamProjection, Error> {
    if context.selected_unit.as_ref() != Some(&selected_unit(c).await?) {
        return Err(changed());
    }
    let selection = context.selection();
    let workspace = selection.workspace();
    if workspace.branch_id.is_some() || workspace.department_id.is_some() {
        return Err(changed());
    }
    let permission = required_permission(context, purpose)?;
    let now = clock.now().get();
    context.require_current_time(now)?;
    let request = context.selected_request.as_ref().ok_or_else(changed)?;
    let access = WorkAuthorizationRequest {
        permission: permission.into(),
        module: None,
        destination: "services/field-service".into(),
        api: true,
    };
    let facts = current_authority_facts(c, request, now).await?;
    let mut current = authorize_effective_work(
        request,
        &access,
        facts,
        permission_catalog()?,
        work_module_catalog()?,
        now,
    )?;
    current.expires_at = current.expires_at.min(context.expires_at());
    if !same_selected_command(context, &current)? {
        return Err(changed());
    }
    required_permission(&current, purpose)?;
    Ok(project(
        &current,
        Some(current.selection().assignment().version),
    )?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUEUED_AUTHORITY_CHANGED",
        "The original selected Work authority changed.",
    )
    .into()
}

/// Synchronous selected Stock authority for Service master discovery and writes.
/// This is an in-flight witness, not a delegated or queued permission.
pub struct CurrentServiceStockAuthority<'a> {
    context: &'a AuthorizedWorkContext,
    unit: Unit,
    projection: CommandIamProjection,
}
impl CurrentServiceStockAuthority<'_> {
    pub fn iam(&self) -> &CommandIamProjection {
        &self.projection
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        let projection = inspect_stock(c, self.context, clock).await?;
        if selected_unit(c).await? != self.unit || projection != self.projection {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn capture_service_stock_authority<'a>(
    c: &mut PgConnection,
    context: &'a AuthorizedWorkContext,
    clock: &impl Clock,
) -> Result<CurrentServiceStockAuthority<'a>, Error> {
    let unit = selected_unit(c).await?;
    if context.selected_unit.as_ref() != Some(&unit) {
        return Err(changed());
    }
    let projection = inspect_stock(c, context, clock).await?;
    Ok(CurrentServiceStockAuthority {
        context,
        unit,
        projection,
    })
}
async fn inspect_stock(
    c: &mut PgConnection,
    context: &AuthorizedWorkContext,
    clock: &impl Clock,
) -> Result<CommandIamProjection, Error> {
    if context.selected_unit.as_ref() != Some(&selected_unit(c).await?) {
        return Err(changed());
    }
    if context.selection().workspace().department_id.is_some() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INVENTORY_SCOPE_REQUIRED",
            "Use a current Company or Branch Stock assignment.",
        )
        .into());
    }
    context.require_permission("inventory.read")?;
    context.require_permission("inventory.prepare")?;
    let now = clock.now().get();
    context.require_current_time(now)?;
    let request = context.selected_request.as_ref().ok_or_else(changed)?;
    let access = WorkAuthorizationRequest {
        permission: "inventory.prepare".into(),
        module: None,
        destination: "services/field-service".into(),
        api: true,
    };
    let facts = current_authority_facts(c, request, now).await?;
    let mut current = authorize_effective_work(
        request,
        &access,
        facts,
        permission_catalog()?,
        work_module_catalog()?,
        now,
    )?;
    current.expires_at = current.expires_at.min(context.expires_at());
    if !same_selected_command(context, &current)? {
        return Err(changed());
    }
    current.require_permission("inventory.read")?;
    current.require_permission("inventory.prepare")?;
    Ok(project(
        &current,
        Some(current.selection().assignment().version),
    )?)
}
