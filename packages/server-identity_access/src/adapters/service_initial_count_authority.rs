//! Selected Account/Holding Count authority, retained and rechecked on the same
//! unit. Purpose is explicit; a read or independent reviewer never acquires an
//! inventory.prepare requirement.
use crate::{
    adapters::{
        permission_catalog::permission_catalog,
        postgres_effective_authority::current_authority_facts,
        selected_command_authority::selected_unit, work_admission::work_module_catalog,
    },
    application::{
        command_metadata::project, effective_authority::authorize_effective_work,
        selected_command_authority::same_selected_command,
    },
    domain::{
        command_metadata::CommandIamProjection,
        effective_authority::{AuthorizedWorkContext, WorkAuthorizationRequest},
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, UserId};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUEUED_AUTHORITY_CHANGED",
        "The original selected Work authority changed.",
    )
    .into()
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceCountAuthorityPurpose {
    Read,
    Prepare,
    Review,
}
impl ServiceCountAuthorityPurpose {
    fn permission(self) -> &'static str {
        match self {
            Self::Read => "inventory.read",
            Self::Prepare => "inventory.prepare",
            Self::Review => "inventory.approve",
        }
    }
}
pub struct CurrentServiceCountAuthority<'a> {
    context: &'a AuthorizedWorkContext,
    purpose: ServiceCountAuthorityPurpose,
    unit: Unit,
    projection: CommandIamProjection,
    company_admin: bool,
}
impl CurrentServiceCountAuthority<'_> {
    pub fn iam(&self) -> &CommandIamProjection {
        &self.projection
    }
    pub fn same_person(&self, user: &UserId) -> bool {
        self.context.user_id() == user || self.context.same_person_user(user)
    }
    pub fn company_admin(&self) -> bool {
        self.company_admin
    }
    pub async fn require_current(
        &self,
        c: &mut PgConnection,
        clock: &impl Clock,
    ) -> Result<(), Error> {
        let projection = inspect(c, self.context, self.purpose, clock).await?;
        if selected_unit(c).await? != self.unit
            || projection != self.projection
            || company_admin(c, self.context, clock.now().get()).await? != self.company_admin
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn capture_service_count_authority<'a>(
    c: &mut PgConnection,
    context: &'a AuthorizedWorkContext,
    purpose: ServiceCountAuthorityPurpose,
    clock: &impl Clock,
) -> Result<CurrentServiceCountAuthority<'a>, Error> {
    let unit = selected_unit(c).await?;
    if context.selected_unit.as_ref() != Some(&unit) {
        return Err(changed());
    }
    let projection = inspect(c, context, purpose, clock).await?;
    let company_admin = company_admin(c, context, clock.now().get()).await?;
    Ok(CurrentServiceCountAuthority {
        company_admin,
        context,
        purpose,
        unit,
        projection,
    })
}
async fn inspect(
    c: &mut PgConnection,
    context: &AuthorizedWorkContext,
    purpose: ServiceCountAuthorityPurpose,
    clock: &impl Clock,
) -> Result<CommandIamProjection, Error> {
    if context.selected_unit.as_ref() != Some(&selected_unit(c).await?) {
        return Err(changed());
    }
    let workspace = context.selection().workspace();
    if workspace.department_id.is_some()
        || (purpose == ServiceCountAuthorityPurpose::Review && workspace.branch_id.is_some())
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INVENTORY_SCOPE_REQUIRED",
            "Use a current Company or Branch Stock assignment.",
        )
        .into());
    }
    context.require_permission("inventory.read")?;
    context.require_permission(purpose.permission())?;
    let now = clock.now().get();
    context.require_current_time(now)?;
    let request = context.selected_request.as_ref().ok_or_else(changed)?;
    let access = WorkAuthorizationRequest {
        permission: purpose.permission().into(),
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
    current.require_permission(purpose.permission())?;
    Ok(project(
        &current,
        Some(current.selection().assignment().version),
    )?)
}

async fn company_admin(
    c: &mut PgConnection,
    context: &AuthorizedWorkContext,
    now: i64,
) -> Result<bool, Error> {
    let selection = context.selection();
    let assignment = selection.assignment();
    let company = &selection.workspace().company_id;
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND iam_role_allows(role,company_id,'organization.manage') AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=$3 AND valid_until>$3)").bind(assignment.id.as_str()).bind(company.as_str()).bind(now).fetch_one(c).await?)
}
