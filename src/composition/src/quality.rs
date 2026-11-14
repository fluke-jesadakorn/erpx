//! Native Quality read and cancellation assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! Quality workspace, detail and cancellation facts.
use crate::business_http::BusinessScopeSlugs;
use crate::quality_commands::{
    QualityCommandPreparation, QualityOwnerCommand, execute_quality_command,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest},
};
use erpx_server_quality::public::quality_commands::{
    HoldStatus, InspectionResult, QualityAlertAcknowledgeCommand,
    QualityAlertAcknowledgementReceipt, QualityDispositionCommand, QualityDispositionReceipt,
    QualityHoldCommand, QualityHoldReceipt, QualityInspectionCommand, QualityInspectionReceipt,
    QualityReleaseCommand, QualityReleaseReceipt, QualitySpecActionCommand, QualitySpecCommand,
    QualitySpecReceipt, SpecStatus, normalize_quality_alert_acknowledge,
    normalize_quality_cancellation, normalize_quality_disposition, normalize_quality_inspection,
    normalize_quality_release, normalize_quality_spec_action, normalize_quality_spec_save,
};
use erpx_server_quality::public::quality_http::{
    PgQualityRead, QualityHttpBackend, QualityHttpCommandRequest, QualityHttpFuture,
    QualityHttpHistoryReply, QualityHttpHistoryRequest, QualityHttpHoldCreateRequest,
    QualityHttpReadReply, QualityHttpReadRequest, QualityHttpSaveSpecRequest, QualityHttpTarget,
    parse_alert_acknowledge, parse_cancellation, parse_disposition, parse_hold_create,
    parse_inspection, parse_quality_history_query, parse_release, parse_spec_action,
    parse_spec_save, quality_router, read_quality_cancellation_history_page,
    read_quality_detail_core,
};
use erpx_server_quality::public::quality_read::{
    QualityActor, QualityCancellationCommand, QualityCancellationReceipt,
    quality_detail_core_with_same_person, quality_detail_with_same_person,
    quality_workspace_with_same_person,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId,
};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn cancellation_access_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_ACCESS_CHANGED",
        "Choose a current authorized Quality assignment.",
    )
}

fn hold_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found.",
    )
}

const QUALITY_HISTORY_PAGE_SIZE: usize = 100;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_UNAVAILABLE",
        "The Quality workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct QualitySources;

impl HoldingWorkSourcePreparation for QualitySources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Commercial purchase authority is fenced before any owner lock so
            // the same serializable unit can prove the current History right.
            lock_commercial_authority_sources(c)
                .await
                .map_err(business)?;
            erpx_server_quality::public::quality_http::lock_quality_cancellation_writes(c)
                .await
                .map_err(business)?;
            erpx_server_quality::public::quality_http::lock_quality_read_sources(c)
                .await
                .map_err(business)?;
            Ok(())
        })
    }
}

#[derive(Clone)]
pub struct QualityConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl QualityConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn quality_actor(current: &AuthorizedWorkContext) -> Result<QualityActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(QualityActor {
        user_id: current.user_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
    })
}

pub struct QualityService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: QualityConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> QualityService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: QualityConfiguration,
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
            permission: "quality.read".into(),
            module: Some("quality".into()),
            destination: "quality".into(),
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
    ) -> Result<QualityHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let actor = quality_actor(current)?;
                        let mut repository = PgQualityRead { connection };
                        let data =
                            quality_workspace_with_same_person(&actor, &mut repository, |user| {
                                erpx_server_shared::public::UserId::new(user.to_owned())
                                    .is_ok_and(|id| current.same_person_user(&id))
                            })
                            .await?;
                        let _ = clock;
                        Ok(QualityHttpReadReply::Workspace(Box::new(data)))
                    })
                },
            )
            .await
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<QualityHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let actor = quality_actor(current)?;
                        let mut repository = PgQualityRead { connection };
                        let data =
                            quality_detail_with_same_person(&actor, &id, &mut repository, |user| {
                                erpx_server_shared::public::UserId::new(user.to_owned())
                                    .is_ok_and(|id| current.same_person_user(&id))
                            })
                            .await?;
                        Ok(QualityHttpReadReply::Detail(Box::new(data)))
                    })
                },
            )
            .await
    }

    /// Detail source facts that deliberately exclude cancellation history. The
    /// caller must fetch the complete history through the paged contract rather
    /// than treating a bounded detail array as the whole history.
    async fn detail_core<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<erpx_server_quality::public::quality_read::QualityDetailCore, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let now = clock.now();
                        let witness = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["supply.quality".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let actor = quality_actor(current)?;
                        let Some(hold_source) = read_quality_detail_core(
                            connection,
                            &actor.company_id,
                            actor.branch_id.as_ref(),
                            &id,
                        )
                        .await?
                        else {
                            return Err(hold_not_found().into());
                        };
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(quality_detail_core_with_same_person(
                            &actor,
                            hold_source.data().clone(),
                            &|user| {
                                erpx_server_shared::public::UserId::new(user.to_owned())
                                    .is_ok_and(|id| current.same_person_user(&id))
                            },
                        )?)
                    })
                },
            )
            .await
    }

    /// One complete-history page in the same serializable Quality unit as the
    /// read. The cursor is parsed only after Account/IAM authorization and is a
    /// comparison input, never authority.
    async fn history<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
        raw_query: Option<String>,
    ) -> Result<QualityHttpHistoryReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                QualitySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let raw_query = raw_query.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let cursor = parse_quality_history_query(raw_query.as_deref())?;
                        let now = clock.now();
                        let witness = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["supply.quality".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let actor = quality_actor(current)?;
                        let page = read_quality_cancellation_history_page(
                            connection,
                            &actor.company_id,
                            actor.branch_id.as_ref(),
                            &id,
                            cursor.before,
                            cursor.head,
                            cursor.revision.as_deref(),
                            QUALITY_HISTORY_PAGE_SIZE,
                        )
                        .await?;
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(QualityHttpHistoryReply {
                            items: page.rows().to_vec(),
                            next_before: page.next_before(),
                            head_version: page.head_version(),
                            history_revision: page.history_revision().to_owned(),
                        })
                    })
                },
            )
            .await
    }

    fn command_access(permission: &'static str) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: permission.into(),
            module: Some("quality".into()),
            destination: "quality".into(),
            api: true,
        }
    }

    pub async fn save_spec(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualitySpecCommand,
    ) -> Result<QualitySpecReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_spec_save(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.release"),
                QualityCommandPreparation::new(false),
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
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::SaveSpec(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        Ok(spec_receipt(&outcome)?)
                    })
                },
            )
            .await
    }

    pub async fn transition_spec(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualitySpecActionCommand,
    ) -> Result<QualitySpecReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_spec_action(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.release"),
                QualityCommandPreparation::new(false),
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
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::TransitionSpec(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        Ok(spec_receipt(&outcome)?)
                    })
                },
            )
            .await
    }

    pub async fn hold_create(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualityHoldCommand,
    ) -> Result<QualityHoldReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.hold"),
                QualityCommandPreparation::new(false),
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
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::HoldCreate(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        hold_receipt(&outcome).map_err(Into::into)
                    })
                },
            )
            .await
    }

    pub async fn inspect(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualityInspectionCommand,
    ) -> Result<QualityInspectionReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_inspection(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.inspect"),
                QualityCommandPreparation::new(true),
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
                        let inspection_key = command.key.as_str().to_owned();
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::Inspect(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        Ok(inspection_receipt(&inspection_key, &outcome)?)
                    })
                },
            )
            .await
    }

    pub async fn acknowledge(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualityAlertAcknowledgeCommand,
    ) -> Result<QualityAlertAcknowledgementReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_alert_acknowledge(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::access(),
                QualityCommandPreparation::new(false),
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
                        if !current.has_permission("quality.release")
                            && !current.has_permission("quality.inspect")
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "QUALITY_ALERT_PERMISSION_REQUIRED",
                                "A quality inspector or manager must acknowledge this escalation.",
                            )
                            .into());
                        }
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::Acknowledge(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        alert_acknowledgement_receipt(&outcome)
                    })
                },
            )
            .await
    }

    pub async fn release(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualityReleaseCommand,
    ) -> Result<QualityReleaseReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_release(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.release"),
                QualityCommandPreparation::release(),
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
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::Release(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        release_receipt(&outcome)
                    })
                },
            )
            .await
    }

    pub async fn scrap(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: QualityDispositionCommand,
    ) -> Result<QualityDispositionReceipt, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let command = normalize_quality_disposition(command)?;
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::command_access("quality.release"),
                QualityCommandPreparation::scrap(),
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
                        let mut identifiers = QualityIds;
                        let outcome = execute_quality_command(
                            connection,
                            current,
                            &QualityOwnerCommand::Scrap(command),
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        disposition_receipt(&outcome)
                    })
                },
            )
            .await
    }

    pub async fn cancel<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: QualityCancellationCommand,
    ) -> Result<QualityCancellationReceipt, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        {
            let command = normalize_quality_cancellation(&command)?;
            self.authorizer
                .execute_with_sources_deferred(
                    host,
                    proof,
                    self.input(input),
                    Self::access(),
                    QualityCommandPreparation::cancellation(),
                    move |connection, current| {
                        let holding = holding.clone();
                        let company = company.clone();
                        let command = command.clone();
                        let clock = clock.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope().into());
                            }
                            current.require_permission("quality.read")?;
                            if current
                                .require_permission(command.action.permission())
                                .is_err()
                            {
                                return Err(cancellation_access_changed().into());
                            }
                            let mut identifiers = QualityIds;
                            let outcome = execute_quality_command(
                                connection,
                                current,
                                &QualityOwnerCommand::Cancel(command),
                                PackageEnforcementMode::Enforced,
                                clock.as_ref(),
                                clock.as_ref(),
                                &mut identifiers,
                            )
                            .await?;
                            cancellation_receipt(&outcome).map_err(Into::into)
                        })
                    },
                )
                .await
        }
    }
}

struct QualityIds;
impl IdentifierSource for QualityIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

#[derive(serde::Deserialize)]
struct SpecResultJson {
    id: String,
    status: String,
    version: i64,
}

fn spec_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualitySpecReceipt, BusinessError> {
    let value: SpecResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualitySpecReceipt {
        id: value.id,
        status: SpecStatus::parse(&value.status).ok_or_else(changed_result)?,
        version: value.version,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancellationReceiptJson {
    id: String,
    hold_id: String,
    cancellation_id: String,
    status: String,
    version: i64,
}

fn cancellation_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityCancellationReceipt, BusinessError> {
    let value: CancellationReceiptJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityCancellationReceipt {
        id: value.id,
        hold_id: value.hold_id,
        cancellation_id: value.cancellation_id,
        status: HoldStatus::parse(&value.status).ok_or_else(changed_result)?,
        version: value.version,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertAcknowledgeResultJson {
    id: String,
    version: i64,
    acknowledged_by: String,
}

fn alert_acknowledgement_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityAlertAcknowledgementReceipt, Error> {
    let value: AlertAcknowledgeResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityAlertAcknowledgementReceipt {
        id: value.id,
        version: value.version,
        acknowledged_by: value.acknowledged_by,
    })
}

#[derive(serde::Deserialize)]
struct InspectionResultJson {
    id: String,
    status: String,
    result: String,
    version: i64,
}

#[derive(serde::Deserialize)]
struct HoldResultJson {
    id: String,
    status: String,
    version: i64,
}

fn hold_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityHoldReceipt, BusinessError> {
    let value: HoldResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityHoldReceipt {
        id: value.id,
        status: HoldStatus::parse(&value.status).ok_or_else(changed_result)?,
        version: value.version,
    })
}

fn inspection_receipt(
    key: &str,
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityInspectionReceipt, BusinessError> {
    let value: InspectionResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityInspectionReceipt {
        id: value.id,
        inspection_id: format!("inspection-{key}"),
        status: HoldStatus::parse(&value.status).ok_or_else(changed_result)?,
        result: InspectionResult::parse(&value.result).ok_or_else(changed_result)?,
        version: value.version,
    })
}

#[derive(serde::Deserialize)]
struct ReleaseResultJson {
    id: String,
    status: String,
    version: i64,
}

fn release_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityReleaseReceipt, Error> {
    let value: ReleaseResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityReleaseReceipt {
        id: value.id,
        status: HoldStatus::parse(&value.status).ok_or_else(changed_result)?,
        version: value.version,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispositionResultJson {
    id: String,
    status: String,
    version: i64,
    count_id: String,
}

fn disposition_receipt(
    outcome: &erpx_server_workflow::public::commands::CommandOutcome,
) -> Result<QualityDispositionReceipt, Error> {
    let value: DispositionResultJson =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| changed_result())?;
    Ok(QualityDispositionReceipt {
        id: value.id,
        status: HoldStatus::parse(&value.status).ok_or_else(changed_result)?,
        version: value.version,
        count_id: value.count_id,
    })
}

fn changed_result() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_RESULT_INVALID",
        "Refresh the Quality record before continuing.",
    )
}

pub struct QualityBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<QualityService<C>>,
}

impl<C: Clock + Send + Sync + 'static> QualityBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<QualityService<C>>,
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

    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let (proof, mut selection) = self.context(headers)?;
        let field = |name: &'static str| -> Result<&str, BusinessError> {
            let mut values = headers.get_all(name).iter();
            let first = values.next();
            if values.next().is_some() {
                return Err(BusinessError::invalid(
                    "QUALITY_CONTEXT_REQUIRED",
                    "Use one current work context.",
                ));
            }
            first
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "QUALITY_CONTEXT_REQUIRED",
                        "Use the current work revision and assignment.",
                    )
                })
        };
        let revision = field("x-erpx-context-revision")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid("QUALITY_CONTEXT_REQUIRED", "Use the current work revision.")
            })?;
        if revision < 0 {
            return Err(BusinessError::invalid(
                "QUALITY_CONTEXT_REQUIRED",
                "Use the current work revision.",
            ));
        }
        let assignment_id = AssignmentId::new(field("x-erpx-context-assignment")?)?;
        let assignment_version = field("x-erpx-context-assignment-version")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "QUALITY_CONTEXT_REQUIRED",
                    "Use the current assignment version.",
                )
            })?;
        if assignment_version < 1 {
            return Err(BusinessError::invalid(
                "QUALITY_CONTEXT_REQUIRED",
                "Use the current assignment version.",
            ));
        }
        selection.expected = Some(ExpectedWorkSelection {
            revision,
            assignment_id,
            assignment_version,
        });
        selection.mutation = true;
        Ok((proof, selection))
    }
}

impl<C: Clock + Send + Sync + 'static> QualityHttpBackend for QualityBackend<C> {
    fn hold_create<'a>(
        &'a self,
        request: QualityHttpHoldCreateRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHoldReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_hold_create(request.body, request.key.clone())?;
            self.service
                .hold_create(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn read<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            if let Some(id) = request.id {
                self.service
                    .detail(request.host, proof, move || Ok(selection), id.to_owned())
                    .await
                    .map_err(business)
            } else {
                self.service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business)
            }
        })
    }

    fn cancel<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityCancellationReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = parse_cancellation(request.body, request.id, request.key.clone())?;
            self.service
                .cancel(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }

    fn acknowledge<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityAlertAcknowledgementReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_alert_acknowledge(request.body, request.id, request.key.clone())?;
            self.service
                .acknowledge(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn save_spec<'a>(
        &'a self,
        request: QualityHttpSaveSpecRequest<'a>,
    ) -> QualityHttpFuture<'a, QualitySpecReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_spec_save(request.body, request.id, request.key.clone())?;
            self.service
                .save_spec(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn transition_spec<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualitySpecReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_spec_action(request.body, request.id, request.key.clone())?;
            self.service
                .transition_spec(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn inspect<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityInspectionReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_inspection(request.body, request.id, request.key.clone())?;
            self.service
                .inspect(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn release<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityReleaseReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_release(request.body, request.id, request.key.clone())?;
            self.service
                .release(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn disposition<'a>(
        &'a self,
        request: QualityHttpCommandRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityDispositionReceipt> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let command = parse_disposition(request.body, request.id, request.key.clone())?;
            self.service
                .scrap(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }

    fn history<'a>(
        &'a self,
        request: QualityHttpHistoryRequest<'a>,
    ) -> QualityHttpFuture<'a, QualityHttpHistoryReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .history(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.id.to_owned(),
                    request.raw_query.map(str::to_owned),
                )
                .await
                .map_err(business)
        })
    }

    fn detail_core<'a>(
        &'a self,
        request: QualityHttpReadRequest<'a>,
    ) -> QualityHttpFuture<'a, erpx_server_quality::public::quality_read::QualityDetailCore> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let Some(id) = request.id else {
                return Err(not_found());
            };
            self.service
                .detail_core(request.host, proof, move || Ok(selection), id.to_owned())
                .await
                .map_err(business)
        })
    }
}

// Retain the router constructor in feature-specific builds.
#[allow(dead_code)]
fn _router(targets: Vec<QualityHttpTarget>) -> Result<axum::Router, BusinessError> {
    quality_router(targets)
}
