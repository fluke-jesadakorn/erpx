//! Native Organization position-save assembly. The selected coordinator owns
//! Account/Holding authority; this module projects the current actor, runs the
//! owner unit and sequences the People roster-exit phases with the
//! Collaboration notices inside the same serializable transaction.
use crate::{
    business_http::BusinessScopeSlugs,
    organization_nodes::{OrganizationSources, organization_actor},
};
use erpx_server_collaboration::public::roster_position_notifications::{
    write_roster_position_exit_request_notices, write_roster_position_exit_week_notices,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_organization::public::organization_nodes::OrganizationFuture;
use erpx_server_organization::public::position::{
    OrganizationPositionHttpBackend, OrganizationPositionHttpFuture,
    OrganizationPositionRepository, PgOrganizationPosition, PositionGraphGuard, PositionInput,
    PositionRoster, RosterExitPort, SaveOrganizationPositionHttpRequest, save_position,
};
use erpx_server_people_payroll::public::roster_position_exit::{
    roster_position_exit_plan, stage_roster_position_exit_invalidate_requests,
    stage_roster_position_exit_lock_and_history, stage_roster_position_exit_release_and_history,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_shared::public::{TimestampMillis, UserId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_UNAVAILABLE",
        "The organization command is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// One owner unit: the Position repository and the cross-owner roster-exit port
/// share the caller's serializable connection.
struct PositionUnit<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PositionUnit<'a> {
    fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

impl OrganizationPositionRepository for PositionUnit<'_> {
    type Error = Error;

    fn replay<'c>(
        &'c mut self,
        actor: &'c erpx_server_organization::public::organization_nodes::OrganizationCommandActor,
        key: &'c str,
        operation: &'c str,
        record_id: &'c str,
        input: &'c PositionInput,
    ) -> OrganizationFuture<'c, Option<Value>, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .replay(actor, key, operation, record_id, input)
                .await
        })
    }

    fn roster<'c>(
        &'c mut self,
        company: &'c CompanyId,
    ) -> OrganizationFuture<'c, PositionRoster, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .roster(company)
                .await
        })
    }

    fn branch_active<'c>(
        &'c mut self,
        company: &'c CompanyId,
        branch: &'c str,
    ) -> OrganizationFuture<'c, bool, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .branch_active(company, branch)
                .await
        })
    }

    fn department_active<'c>(
        &'c mut self,
        company: &'c CompanyId,
        department: &'c str,
        branch: Option<&'c str>,
    ) -> OrganizationFuture<'c, bool, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .department_active(company, department, branch)
                .await
        })
    }

    fn position_level<'c>(
        &'c mut self,
        company: &'c CompanyId,
        level: &'c str,
    ) -> OrganizationFuture<'c, Option<i64>, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .position_level(company, level)
                .await
        })
    }

    fn graph_guard<'c>(
        &'c mut self,
        company: &'c CompanyId,
        key: &'c str,
    ) -> OrganizationFuture<'c, PositionGraphGuard, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .graph_guard(company, key)
                .await
        })
    }

    fn exiting_engagements<'c>(
        &'c mut self,
        company: &'c CompanyId,
        position: &'c str,
    ) -> OrganizationFuture<'c, Vec<String>, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .exiting_engagements(company, position)
                .await
        })
    }

    fn persist<'c>(
        &'c mut self,
        actor: &'c erpx_server_organization::public::organization_nodes::OrganizationCommandActor,
        id: Option<&'c str>,
        input: &'c PositionInput,
        key: &'c str,
        guard: &'c PositionGraphGuard,
        roster_required: bool,
        exiting: &'c [String],
        now: i64,
    ) -> OrganizationFuture<'c, Value, Self::Error> {
        Box::pin(async move {
            PgOrganizationPosition::new(&mut *self.connection)
                .persist(actor, id, input, key, guard, roster_required, exiting, now)
                .await
        })
    }
}

impl RosterExitPort for PositionUnit<'_> {
    fn execute<'c>(
        &'c mut self,
        actor: &'c erpx_server_organization::public::organization_nodes::OrganizationCommandActor,
        engagement_ids: &'c [String],
        key: &'c str,
        reason: &'c str,
        now: i64,
    ) -> OrganizationFuture<'c, (), BusinessError> {
        Box::pin(async move {
            let actor_user = UserId::new(actor.command.user_id.as_str().to_owned())?;
            let plan = roster_position_exit_plan(
                &actor.command.company_id,
                actor_user.as_str(),
                engagement_ids,
                key,
                reason,
                TimestampMillis::new(now),
            )?;
            let Some(plan) = plan else { return Ok(()) };
            stage_roster_position_exit_lock_and_history(&mut *self.connection, &plan)
                .await
                .map_err(business)?;
            write_roster_position_exit_week_notices(
                &mut *self.connection,
                &plan.company_id,
                &plan.engagement_ids,
                now,
            )
            .await
            .map_err(business)?;
            stage_roster_position_exit_release_and_history(&mut *self.connection, &plan)
                .await
                .map_err(business)?;
            write_roster_position_exit_request_notices(
                &mut *self.connection,
                &plan.company_id,
                &plan.engagement_ids,
                now,
            )
            .await
            .map_err(business)?;
            stage_roster_position_exit_invalidate_requests(&mut *self.connection, &plan)
                .await
                .map_err(business)?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct OrganizationPositionConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OrganizationPositionConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct OrganizationPositionService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: OrganizationPositionConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationPositionService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: OrganizationPositionConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "organization.manage".into(),
            module: None,
            destination: "people/organization".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_position<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: &str,
        id: Option<String>,
        position: PositionInput,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let key = key.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let id = id.clone();
                    let position = position.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("organization.manage")?;
                        let actor = organization_actor(current)?;
                        let mut unit = PositionUnit::new(connection);
                        save_position(
                            &mut unit,
                            actor,
                            &key,
                            id.as_deref(),
                            position,
                            clock.as_ref(),
                        )
                        .await
                        .map_err(Error::from)
                    })
                },
            )
            .await
    }
}

pub struct OrganizationPositionBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<OrganizationPositionService<C>>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationPositionBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<OrganizationPositionService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, Option<String>, Option<String>), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((inbound.into_proof(), branch, department))
    }

    fn request(
        &self,
        proof: DeviceProof,
        branch: Option<String>,
        department: Option<String>,
    ) -> (DeviceProof, HoldingWorkspaceRequest) {
        let company = self.service.company().clone();
        (
            proof,
            HoldingWorkspaceRequest {
                company_id: company,
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: true,
            },
        )
    }
}

impl<C: Clock + Send + Sync + 'static> OrganizationPositionHttpBackend
    for OrganizationPositionBackend<C>
{
    fn save_position<'a>(
        &'a self,
        request: SaveOrganizationPositionHttpRequest<'a>,
    ) -> OrganizationPositionHttpFuture<'a> {
        Box::pin(async move {
            let (proof, branch, department) = self.context(request.headers)?;
            let (proof, selection) = self.request(proof, branch, department);
            let id = request.position_id.map(str::to_owned);
            let position = request.input.clone();
            self.service
                .save_position(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key,
                    id,
                    position,
                )
                .await
                .map_err(business)
        })
    }
}
