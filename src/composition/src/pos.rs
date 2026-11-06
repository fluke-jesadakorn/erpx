//! Native POS composition HTTP mount. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the POS cash
//! movement command facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_pos::public::cash_movement::{
    CashMovementActor, CreateCashMovement, create_cash_movement, PgCashMovementCommand,
};
use erpx_server_pos::public::cash_movement_http::{
    CashMovementHttpBackend, CashMovementHttpFuture, CashMovementHttpRequest,
    CashMovementHttpResponse, CashMovementHttpTarget,
};
use erpx_server_pos::public::commands::{
    PgPosCommandRepository, PosActor, PosCommand, PosCommandInput, PosCommandRepository,
    PosCatalogProduct as PosCatalogSnapshotProduct, PosCatalogSnapshot, PosFinancePort, PosFuture,
    PosHospitalityPort, PosJournalRequest, PosOrganizationPort, PosResult, execute_pos_command,
};
use erpx_server_pos::public::commands_http::{
    PosCommandHttpBackend, PosCommandHttpFuture, PosCommandHttpRequest, PosCommandHttpResponse,
    PosCommandHttpTarget,
};
use erpx_server_commerce::public::pos_catalog::{lock_pos_catalog, read_pos_catalog_product};
use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::pos_journal::{
    PosJournalLine as FinancePosJournalLine, PosJournalPosting, post_pos_journal,
};
use erpx_server_finance::public::pos_sources::{
    finance_party_exists, lock_pos_finance_sources, read_open_period_for_date,
};
use erpx_server_organization::public::pos_issuer::{
    lock_pos_issuer_source, read_company_display_name,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "POS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_UNAVAILABLE",
        "The POS workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PosSources;

impl HoldingWorkSourcePreparation for PosSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Copy)]
struct PosCommandSources;

impl HoldingWorkSourcePreparation for PosCommandSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_pos_finance_sources(c).await.map_err(business)?;
            lock_pos_issuer_source(c).await.map_err(business)?;
            lock_pos_catalog(c).await.map_err(business)?;
            Ok(())
        })
    }
}

struct PosFinanceQueue {
    business_date: String,
    period_open: bool,
    party_exists: bool,
    journals: Vec<PosJournalRequest>,
}

impl PosFinancePort for PosFinanceQueue {
    fn open_period<'a>(&'a mut self, _: &'a CompanyId, date: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async move {
            if date == self.business_date && self.period_open {
                Ok(())
            } else {
                Err(BusinessError::new(
                    FailureKind::Conflict,
                    "POS_PERIOD_CLOSED",
                    "Open the Finance period for this business date.",
                ))
            }
        })
    }

    /// Reserve the deterministic Finance journal identity from the POS posting
    /// source key. The actual owner write runs on the same serializable
    /// transaction after the POS command has inserted its posting receipt.
    fn post_journal<'a>(
        &'a mut self,
        request: &'a PosJournalRequest,
    ) -> PosFuture<'a, Option<String>> {
        Box::pin(async move {
            // The Finance owner resolves the open period for the journal's
            // own business date inside the same transaction; this port only
            // reserves the deterministic journal identity and exact draft.
            let journal_id = format!("journal-{}", request.source_key);
            self.journals.push(request.clone());
            Ok(Some(journal_id))
        })
    }

    fn party_exists<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(self.party_exists) })
    }
}

struct PosHospitalitySnapshot;

impl PosHospitalityPort for PosHospitalitySnapshot {
    fn hotel_day_open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_HOSPITALITY_PORT_REQUIRED",
                "The PMS hospitality port is not mounted.",
            ))
        })
    }

    fn room_charge_exists<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_HOSPITALITY_PORT_REQUIRED",
                "The PMS hospitality port is not mounted.",
            ))
        })
    }
}

struct PosOrganizationSnapshot(String);

impl PosOrganizationPort for PosOrganizationSnapshot {
    fn issuer_name<'a>(&'a mut self, _: &'a CompanyId) -> PosFuture<'a, String> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

#[derive(Clone)]
pub struct PosConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PosConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn pos_actor(current: &AuthorizedWorkContext) -> Result<CashMovementActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CashMovementActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
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

pub struct PosService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PosConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PosService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PosConfiguration,
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
            permission: "pos.sell".into(),
            module: Some("pos".into()),
            destination: "pos".into(),
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

    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CreateCashMovement,
    ) -> Result<CashMovementHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let command = command.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PosSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pos.sell")?;
                        let actor = pos_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgCashMovementCommand::new(connection);
                        let receipt = create_cash_movement(&actor, &command, &mut repository, now)
                            .await
                            .map_err(|error| match error {
                                PgTransactionError::Business(error) => error,
                                PgTransactionError::Persistence(_) => unavailable(),
                            })?;
                        Ok(CashMovementHttpResponse {
                            data: receipt.to_wire(),
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
        command: PosCommand,
    ) -> Result<PosCommandHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "pos.sell".into(),
                    module: Some("pos".into()),
                    destination: "pos".into(),
                    api: true,
                },
                PosCommandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pos.sell")?;
                        let now = clock.now();
                        let business_date = {
                            let mut repository = PgPosCommandRepository::new(connection);
                            repository.business_date(now).await.map_err(business)?
                        };
                        let period_open = read_open_period_for_date(
                            connection,
                            company.as_str(),
                            &business_date,
                        )
                        .await
                        .map_err(business)?
                        .is_some();
                        let party = match &command.input {
                            erpx_server_pos::public::commands::PosCommandInput::SaleCreate(input) => {
                                match &input.party_id {
                                    Some(party) => finance_party_exists(
                                        connection,
                                        company.as_str(),
                                        party,
                                    )
                                    .await
                                    .map_err(business)?,
                                    None => true,
                                }
                            }
                            _ => true,
                        };
                        let issuer = read_company_display_name(connection, company.as_str())
                            .await
                            .map_err(business)?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let actor = PosActor {
                            user_id: current.user_id().clone(),
                            holding_id: current.holding_id().clone(),
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            assignment_id: assignment.id.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        let catalog = match &command.input {
                            PosCommandInput::SaleAdd(input) => {
                                let mut snapshot = PgPosCommandRepository::new(connection);
                                let sale = snapshot
                                    .sale(&company, &input.id)
                                    .await
                                    .map_err(business)?;
                                match sale {
                                    Some(sale) => read_pos_catalog_product(
                                        connection,
                                        &company,
                                        &sale.register_id,
                                        &input.product_id,
                                    )
                                    .await
                                    .map_err(business)?
                                    .map(|product| PosCatalogSnapshot {
                                        register_id: sale.register_id,
                                        product: PosCatalogSnapshotProduct {
                                            id: product.id,
                                            name: product.name,
                                            price_minor: product.price_minor,
                                            tax_bps: product.tax_bps,
                                            tax_inclusive: product.tax_inclusive,
                                            kind: product.kind,
                                            station: product.station,
                                            recipe_json: product.recipe_json,
                                            modifiers_json: product.modifiers_json,
                                            catalog_version: product.catalog_version,
                                        },
                                    }),
                                    None => None,
                                }
                            }
                            _ => None,
                        };
                        let mut repository = PgPosCommandRepository::new(connection);
                        let mut finance = PosFinanceQueue {
                            business_date,
                            period_open,
                            party_exists: party,
                            journals: Vec::new(),
                        };
                        let mut hospitality = PosHospitalitySnapshot;
                        let mut organization = PosOrganizationSnapshot(issuer);
                        let result: PosResult = execute_pos_command(
                            &actor,
                            &command,
                            &mut repository,
                            catalog.as_ref(),
                            &mut finance,
                            &mut hospitality,
                            &mut organization,
                            now,
                        )
                        .await?;
                        drop(repository);
                        let finance_actor = FinanceActor {
                            user_id: actor.user_id.clone(),
                            holding_id: actor.holding_id.clone(),
                            company_id: actor.company_id.clone(),
                            assignment_id: actor.assignment_id.clone(),
                        };
                        for request in finance.journals {
                            let posting = PosJournalPosting {
                                journal_id: format!("journal-{}", request.source_key),
                                actor: finance_actor.clone(),
                                chart_id: request.chart_id,
                                currency: request.currency,
                                business_date: request.business_date,
                                memo: format!("POS {}", request.source_key),
                                source_key: request.source_key,
                                source_id: request.source_id,
                                lines: request
                                    .lines
                                    .into_iter()
                                    .map(|line| FinancePosJournalLine {
                                        account_id: line.account_id,
                                        debit_minor: line.debit,
                                        credit_minor: line.credit,
                                    })
                                    .collect(),
                            };
                            post_pos_journal(connection, clock.as_ref(), &posting)
                                .await
                                .map_err(business)?;
                        }
                        Ok(PosCommandHttpResponse {
                            data: result.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PosBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PosService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PosBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PosService<C>>,
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

impl<C: Clock + Send + Sync + 'static> PosBackend<C> {
    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let (proof, mut selection) = self.context(headers)?;
        let revision = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(|| BusinessError::invalid("POS_CONTEXT_REQUIRED", "Use the current work revision."))?
            .parse()
            .map_err(|_| BusinessError::invalid("POS_CONTEXT_REQUIRED", "Use the current work revision."))?;
        let assignment = unique_request_header(headers, "x-erpx-context-assignment")?
            .ok_or_else(|| BusinessError::invalid("POS_CONTEXT_REQUIRED", "Choose the current assignment."))?
            .to_owned();
        let assignment_version = unique_request_header(headers, "x-erpx-context-assignment-version")?
            .ok_or_else(|| BusinessError::invalid("POS_CONTEXT_REQUIRED", "Use the current assignment version."))?
            .parse()
            .map_err(|_| BusinessError::invalid("POS_CONTEXT_REQUIRED", "Use the current assignment version."))?;
        selection.mutation = true;
        selection.expected = Some(ExpectedWorkSelection {
            revision,
            assignment_id: AssignmentId::new(assignment)?,
            assignment_version,
        });
        Ok((proof, selection))
    }
}

impl<C: Clock + Send + Sync + 'static> PosCommandHttpBackend for PosBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PosCommandHttpRequest<'a>,
    ) -> PosCommandHttpFuture<'a, PosCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> CashMovementHttpBackend for PosBackend<C> {
    fn create<'a>(
        &'a self,
        request: CashMovementHttpRequest<'a>,
    ) -> CashMovementHttpFuture<'a, CashMovementHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let key = CommandKey::new(request.key)?;
            let shift_id = RecordId::new(request.shift_id)?;
            let command = CreateCashMovement {
                key,
                shift_id,
                expected_version: request.expected_version,
                amount_minor: request.amount_minor,
                reason: request.reason.to_owned(),
            };
            self.service
                .create(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CashMovementHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_pos::public::cash_movement_http::cash_movement_router(targets)
}
