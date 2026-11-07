//! Native Benefits enrollment workspace HTTP assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! Benefits workspace, People worker/name and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_benefits::public::workspace::{
    BenefitsActor, BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture, BenefitsPeoplePort,
    BenefitsWorker, PgBenefitsWorkspaceRead, benefits_workspace,
};
use erpx_server_benefits::public::workspace_http::{
    BenefitsWorkspaceHttpBackend, BenefitsWorkspaceHttpFuture, BenefitsWorkspaceHttpReadRequest,
    BenefitsWorkspaceHttpResponse,
};
use erpx_server_benefits::public::command::{
    BenefitsCommandActor, PgBenefitsCommand,
    execute_benefits_command,
};
use erpx_server_benefits::public::command_http::{
    BenefitsCommandHttpBackend, BenefitsCommandHttpFuture, BenefitsCommandHttpRequest,
    BenefitsCommandHttpResponse,
};
use erpx_server_people_payroll::public::benefit_person::{
    CurrentBenefitPerson, inspect_benefit_person, inspect_benefit_person_names, lock_benefit_people_sources,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_shared::public::{CommandKey, TimestampMillis};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BENEFITS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_UNAVAILABLE",
        "The Benefits workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct BenefitsSources;

impl HoldingWorkSourcePreparation for BenefitsSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await.map_err(business)?;
            lock_benefit_people_sources(c).await.map_err(business)?;
            Ok(())
        })
    }
}

#[derive(Clone, Copy)]
struct BenefitsCommandSources;

impl HoldingWorkSourcePreparation for BenefitsCommandSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await.map_err(business)?;
            lock_benefit_people_sources(c).await.map_err(business)?;
            Ok(())
        })
    }
}

async fn benefits_people(
    connection: &mut PgConnection,
    company: &CompanyId,
    user_id: &str,
    extra_person_ids: &[String],
) -> Result<(Option<BenefitsWorker>, Vec<(String, String)>, CurrentBenefitPerson), Error> {
    let person = inspect_benefit_person(connection, company.as_str(), user_id)
        .await
        .map_err(business)?;
    let worker = person.selected().map(|employee| BenefitsWorker {
        person_id: employee.person_id.clone(),
        engagement_id: employee.engagement_id.clone(),
    });
    let mut person_ids = extra_person_ids.to_vec();
    if let Some(worker) = &worker {
        if !person_ids.iter().any(|id| id == &worker.person_id) {
            person_ids.push(worker.person_id.clone());
        }
    }
    let names = if person_ids.is_empty() {
        Vec::new()
    } else {
        inspect_benefit_person_names(connection, company.as_str(), &person_ids)
            .await
            .map_err(business)?
    };
    Ok((worker, names, person))
}

async fn benefits_entitlement(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    now: TimestampMillis,
) -> Result<(BenefitsEntitlement, PurchaseWitness), Error> {
    let catalogue = current_package_catalogue()?;
    let mut reader = PgCompanyPurchaseReader::new(
        holding.clone(),
        company.clone(),
        connection,
        &catalogue,
    );
    let request = |purpose| PurchaseRequest {
        capabilities: vec!["people.benefits".into()],
        purpose,
        mode: PackageEnforcementMode::Enforced,
    };
    let history = reader
        .inspect(&request(CapabilityPurpose::History), now)
        .await
        .map_err(business)?;
    let new_work = reader
        .inspect(&request(CapabilityPurpose::NewWork), now)
        .await
        .map_err(business)?;
    let settle = reader
        .inspect(&request(CapabilityPurpose::SettleExisting), now)
        .await
        .map_err(business)?;
    let allowed = |inspection: &erpx_server_commercial::public::purchases::PurchaseInspection| {
        inspection
            .access
            .iter()
            .find(|access| access.capability == "people.benefits")
            .is_some_and(|access| access.allowed_by_mode)
    };
    Ok((
        BenefitsEntitlement {
            enforced: true,
            history_allowed: allowed(&history),
            new_work: allowed(&new_work),
            settle_existing: allowed(&settle),
            revision: history.revision.clone(),
        },
        history.witness.clone(),
    ))
}

struct BenefitsPeopleSnapshot {
    worker: Option<BenefitsWorker>,
    names: Vec<(String, String)>,
}

impl BenefitsPeoplePort for BenefitsPeopleSnapshot {
    type Error = BusinessError;

    fn worker<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _user_id: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        Box::pin(async move { Ok(self.worker.clone()) })
    }

    fn employee_names<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        person_ids: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .names
                .iter()
                .filter(|(id, _)| person_ids.iter().any(|wanted| wanted == id))
                .cloned()
                .collect())
        })
    }
}

struct BenefitsEntitlementSnapshot(BenefitsEntitlement);

impl BenefitsEntitlementPort for BenefitsEntitlementSnapshot {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

#[derive(Clone)]
pub struct BenefitsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl BenefitsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn benefits_work_context(current: &AuthorizedWorkContext) -> String {
    use sha2::{Digest, Sha256};
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let mut hash = Sha256::new();
    for value in [
        current.user_id().as_str(),
        current.holding_id().as_str(),
        workspace.company_id.as_str(),
        workspace.branch_id.as_ref().map(|v| v.as_str()).unwrap_or(""),
        workspace.department_id.as_ref().map(|v| v.as_str()).unwrap_or(""),
        assignment.id.as_str(),
        assignment.role.as_str(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(selected.revision().to_be_bytes());
    hash.update(assignment.version.to_be_bytes());
    format!("{:x}", hash.finalize())
}

fn benefits_actor(current: &AuthorizedWorkContext) -> Result<BenefitsActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(BenefitsActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        work_context: benefits_work_context(current),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

pub struct BenefitsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: BenefitsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> BenefitsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: BenefitsConfiguration,
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
            permission: "records.read".into(),
            module: Some("benefits".into()),
            destination: "benefits".into(),
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
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        personal: bool,
    ) -> Result<BenefitsWorkspaceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let personal = personal;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                BenefitsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let personal = personal;
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = benefits_actor(current)?;
                        let now = clock.now();
                        let extra_person_ids = if !personal
                            && actor
                                .permissions
                                .iter()
                                .any(|permission| permission == "benefits.review")
                        {
                            let mut reviewer_read = PgBenefitsWorkspaceRead::new(connection);
                            reviewer_read
                                .election_person_ids(&company, 1001)
                                .await
                                .map_err(business)?
                        } else {
                            Vec::new()
                        };
                        let (worker, names, person) = benefits_people(
                            connection,
                            &company,
                            current.user_id().as_str(),
                            &extra_person_ids,
                        )
                        .await?;
                        let (entitlement, witness) =
                            benefits_entitlement(connection, &holding, &company, now).await?;
                        let mut repository = PgBenefitsWorkspaceRead::new(connection);
                        let mut people = BenefitsPeopleSnapshot { worker, names };
                        let mut entitlement = BenefitsEntitlementSnapshot(entitlement);
                        let workspace = benefits_workspace(
                            &actor,
                            personal,
                            &mut repository,
                            &mut people,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        drop(repository);
                        person.require_current(connection).await.map_err(business)?;
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?;
                        let data = serde_json::to_value(workspace).map_err(|_| unavailable())?;
                        Ok(BenefitsWorkspaceHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        body: &Value,
        key: &CommandKey,
    ) -> Result<BenefitsCommandHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let body = body.clone();
        let key = key.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("benefits".into()),
                    destination: "benefits".into(),
                    api: true,
                },
                BenefitsCommandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let body = body.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let now = clock.now();
                        let (worker, names, person) = benefits_people(
                            connection,
                            &company,
                            current.user_id().as_str(),
                            &[],
                        )
                        .await?;
                        let (entitlement, witness) =
                            benefits_entitlement(connection, &holding, &company, now).await?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let actor = BenefitsCommandActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: assignment.role.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            work_context: benefits_work_context(current),
                            expires_at: current.expires_at(),
                        };
                        let mut repository = PgBenefitsCommand::new(connection, &actor);
                        let mut people = BenefitsPeopleSnapshot { worker, names };
                        let mut entitlement_port =
                            BenefitsEntitlementSnapshot(entitlement);
                        let result = execute_benefits_command(
                            &actor,
                            &body,
                            &key,
                            &mut repository,
                            &mut people,
                            &mut entitlement_port,
                            now,
                        )
                        .await?;
                        drop(repository);
                        person.require_current(connection).await.map_err(business)?;
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?;
                        Ok(BenefitsCommandHttpResponse {
                            data: serde_json::json!({
                                "id": result.id,
                                "status": result.status,
                                "version": result.version,
                            }),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct BenefitsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<BenefitsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> BenefitsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<BenefitsService<C>>,
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
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> BenefitsCommandHttpBackend for BenefitsBackend<C> {
    fn command<'a>(
        &'a self,
        request: BenefitsCommandHttpRequest<'a>,
    ) -> BenefitsCommandHttpFuture<'a, BenefitsCommandHttpResponse> {
        Box::pin(async move {
            let (proof, mut selection) = self.context(request.headers)?;
            let revision = erpx_server_identity_access::public::account_inbound_http::unique_request_header(
                request.headers,
                "x-erpx-context-revision",
            )?
            .ok_or_else(|| BusinessError::invalid("BENEFITS_CONTEXT_REQUIRED", "Use the current work revision."))?
            .parse()
            .map_err(|_| BusinessError::invalid("BENEFITS_CONTEXT_REQUIRED", "Use the current work revision."))?;
            let assignment = erpx_server_identity_access::public::account_inbound_http::unique_request_header(
                request.headers,
                "x-erpx-context-assignment",
            )?
            .ok_or_else(|| BusinessError::invalid("BENEFITS_CONTEXT_REQUIRED", "Choose the current assignment."))?
            .to_owned();
            let assignment_version = erpx_server_identity_access::public::account_inbound_http::unique_request_header(
                request.headers,
                "x-erpx-context-assignment-version",
            )?
            .ok_or_else(|| BusinessError::invalid("BENEFITS_CONTEXT_REQUIRED", "Use the current assignment version."))?
            .parse()
            .map_err(|_| BusinessError::invalid("BENEFITS_CONTEXT_REQUIRED", "Use the current assignment version."))?;
            selection.mutation = true;
            selection.expected = Some(
                erpx_server_identity_access::public::work_context::ExpectedWorkSelection {
                    revision,
                    assignment_id: erpx_server_shared::public::AssignmentId::new(assignment)?,
                    assignment_version,
                },
            );
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.body,
                    request.key,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> BenefitsWorkspaceHttpBackend for BenefitsBackend<C> {
    fn read<'a>(
        &'a self,
        request: BenefitsWorkspaceHttpReadRequest<'a>,
    ) -> BenefitsWorkspaceHttpFuture<'a, BenefitsWorkspaceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let personal = request.personal;
            self.service
                .workspace(request.host, proof, move || Ok(selection), personal)
                .await
                .map_err(business)
        })
    }
}
