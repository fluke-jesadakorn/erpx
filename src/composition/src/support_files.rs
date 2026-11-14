//! Native Support evidence attachment intake composition.
//!
//! Files owns the draft set, reservation, quarantined bytes and pending scan.
//! Support owns the ticket status/version/responsibility and the purchased
//! `support.evidence` capability. Both are checked in the same SERIALIZABLE unit
//! that writes the Files ledger; the scan runs through the existing native
//! ClamAV worker and never falls back to the retained TypeScript writer.
use crate::business_http::BusinessScopeSlugs;
use crate::files::FilesConfiguration;
use crate::files_scan_native::NativeFilesScanWorker;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, lock_commercial_authority_sources,
};
use erpx_server_files::public::field_visit_upload::LocalFieldQuarantineWriter;
use erpx_server_files::public::support_attachment_http::{
    SupportAttachmentCompleteHttpRequest, SupportAttachmentCreateHttpRequest,
    SupportAttachmentHttpBackend, SupportAttachmentHttpFuture, SupportAttachmentHttpResponse,
    SupportAttachmentReserveHttpRequest, SupportAttachmentScanHttpRequest,
};
use erpx_server_files::public::support_attachment_intake::{
    SupportAttachmentActor, SupportAttachmentAuthority, SupportAttachmentFileRequest,
    SupportAttachmentIntakeAvailability, SupportAttachmentSetRequest, SupportAttachmentStage,
    SupportAttachmentTarget, SupportAttachmentTicketFacts, complete_support_attachment_upload,
    create_support_attachment_set, reserve_support_attachment_file,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis,
};
use erpx_server_support::public::support_attachment::{
    SUPPORT_EVIDENCE_CAPABILITY, SupportAttachmentActor as SupportActor,
    authorize_support_attachment,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;
use tokio::sync::Mutex;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FILES_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_SUPPORT_UNAVAILABLE",
        "The Support file workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Same envelope the attachment read client validates. Keep the keys exactly
/// aligned with `packages/server-files/src/public/native-support-attachments.ts`.
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

fn files_actor(current: &AuthorizedWorkContext, company: &CompanyId) -> SupportAttachmentActor {
    let workspace = current.selection().workspace();
    SupportAttachmentActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: company.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
    }
}

fn support_actor(current: &AuthorizedWorkContext, now: TimestampMillis) -> Result<SupportActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_EVIDENCE_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    Ok(SupportActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
        can_read: current.has_permission("support.read") || current.has_permission("records.read"),
        can_work: current.has_permission("support.work"),
        can_manage: current.has_permission("support.manage"),
        same_person_ids: Vec::new(),
    })
}

/// Files authority port implemented by the current Account context, the Support
/// native guard and the Commercial purchase reader — all on the caller's unit.
struct SupportAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    catalogue: &'a CommercialCatalogue,
    clock: &'a C,
}

impl<C: Clock + Send + Sync> SupportAttachmentAuthority for SupportAuthority<'_, C> {
    fn authorize<'a>(
        &'a self,
        c: &'a mut PgConnection,
        target: &'a SupportAttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, SupportAttachmentTicketFacts, BusinessError> {
        Box::pin(async move {
            let actor = support_actor(self.current, self.clock.now())?;
            let stage = match target.stage {
                SupportAttachmentStage::Read => SupportAttachmentStage::Read,
                SupportAttachmentStage::Write => SupportAttachmentStage::Write,
            };
            let ticket = authorize_support_attachment(
                c,
                &actor,
                target.ticket_id,
                target.purpose_key,
                target.expected_target_version,
                stage,
            )
            .await?;
            Ok(SupportAttachmentTicketFacts {
                company_id: ticket.company_id,
                branch_id: ticket.branch_id,
                department_id: ticket.department_id,
                version: ticket.version,
            })
        })
    }

    fn require_purchase<'a>(
        &'a self,
        c: &'a mut PgConnection,
        company_id: &'a str,
        _purpose_key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let company = CompanyId::new(company_id)?;
            let mut reader = PgCompanyPurchaseReader::new(
                self.current.holding_id().clone(),
                company,
                c,
                self.catalogue,
            );
            reader
                .inspect(
                    &PurchaseRequest {
                        capabilities: vec![SUPPORT_EVIDENCE_CAPABILITY.into()],
                        purpose: CapabilityPurpose::SettleExisting,
                        mode: PackageEnforcementMode::Enforced,
                    },
                    self.clock.now(),
                )
                .await?
                .require_allowed()?;
            Ok(())
        })
    }
}

#[derive(Clone, Copy)]
struct SupportFilesSources;

impl HoldingWorkSourcePreparation for SupportFilesSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            erpx_server_manage_erpx::public::lock_beta_policy_sources(c).await?;
            Ok(())
        })
    }
}

pub struct SupportFilesService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FilesConfiguration,
    clock: Arc<C>,
    catalogue: Arc<CommercialCatalogue>,
    writer: Arc<LocalFieldQuarantineWriter>,
    availability: SupportAttachmentIntakeAvailability,
    workers: Arc<Mutex<Vec<NativeFilesScanWorker<C>>>>,
}

impl<C: Clock + Send + Sync + 'static> SupportFilesService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FilesConfiguration,
        clock: Arc<C>,
        catalogue: Arc<CommercialCatalogue>,
        writer: Arc<LocalFieldQuarantineWriter>,
        availability: SupportAttachmentIntakeAvailability,
        workers: Arc<Mutex<Vec<NativeFilesScanWorker<C>>>>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            catalogue,
            writer,
            availability,
            workers,
        }
    }

    fn command_input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company().clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        }
    }

    fn read_input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company().clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    fn command_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: WorkPermissionRequirement::AnyOf(vec![
                "support.manage".into(),
                "support.work".into(),
            ]),
            module: Some("support".into()),
            destination: "support.evidence".into(),
            api: true,
        }
    }

    fn read_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: WorkPermissionRequirement::AnyOf(vec![
                "support.read".into(),
                "records.read".into(),
            ]),
            module: Some("support".into()),
            destination: "support.evidence".into(),
            api: true,
        }
    }

    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        request: SupportAttachmentSetRequest,
        key: CommandKey,
    ) -> Result<SupportAttachmentHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        let availability = self.availability;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::command_access(),
                SupportFilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    let key = key.clone();
                    let request = request.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let now = clock.now();
                        let actor = files_actor(current, &company);
                        let authority = SupportAuthority {
                            current,
                            catalogue: &catalogue,
                            clock: clock.as_ref(),
                        };
                        let set = create_support_attachment_set(
                            connection,
                            &authority,
                            &actor,
                            &request,
                            key.as_str(),
                            availability,
                            now.get(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(SupportAttachmentHttpResponse {
                            data: serde_json::to_value(set).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn reserve<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        set_id: String,
        request: SupportAttachmentFileRequest,
        key: CommandKey,
    ) -> Result<SupportAttachmentHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let holding_slug = self.configuration.holding_slug().to_owned();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        let availability = self.availability;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::command_access(),
                SupportFilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    let set_id = set_id.clone();
                    let key = key.clone();
                    let request = request.clone();
                    let holding_slug = holding_slug.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let now = clock.now();
                        let actor = files_actor(current, &company);
                        let authority = SupportAuthority {
                            current,
                            catalogue: &catalogue,
                            clock: clock.as_ref(),
                        };
                        let storage_limit =
                            erpx_server_manage_erpx::public::company_storage_limit(
                                connection, &company,
                            )
                            .await?;
                        let reservation = reserve_support_attachment_file(
                            connection,
                            &authority,
                            &actor,
                            &holding_slug,
                            &set_id,
                            &request,
                            key.as_str(),
                            availability,
                            storage_limit,
                            now.get(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(SupportAttachmentHttpResponse {
                            data: serde_json::to_value(reservation).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn complete<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        upload_id: String,
        content_type: String,
        bytes: Vec<u8>,
    ) -> Result<SupportAttachmentHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        let writer = self.writer.clone();
        let availability = self.availability;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                Self::command_access(),
                SupportFilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    let writer = writer.clone();
                    let upload_id = upload_id.clone();
                    let content_type = content_type.clone();
                    let bytes = bytes.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let now = clock.now();
                        let actor = files_actor(current, &company);
                        let authority = SupportAuthority {
                            current,
                            catalogue: &catalogue,
                            clock: clock.as_ref(),
                        };
                        let completion = complete_support_attachment_upload(
                            connection,
                            writer.as_ref(),
                            &authority,
                            &actor,
                            &upload_id,
                            &content_type,
                            bytes,
                            availability,
                            now.get(),
                        )
                        .await
                        .map_err(business)?;
                        Ok(SupportAttachmentHttpResponse {
                            data: serde_json::to_value(completion).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    /// Resolve the completed upload to its source, re-authorize the current
    /// Support case, run the native scan worker and return the bounded state.
    pub async fn scan<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        upload_id: String,
    ) -> Result<SupportAttachmentHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        let upload_id = upload_id.clone();
        let source = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.read_input(input),
                Self::read_access(),
                SupportFilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    let upload_id = upload_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let now = clock.now();
                        let row: Option<(String, String, String, i64, String)> = sqlx::query_as(
                            "SELECT u.owner_id,u.purpose,a.id,s.expected_target_version,s.component_kind \
                             FROM upload_intents u JOIN attachments a ON a.upload_id=u.id \
                             JOIN attachment_sets s ON s.id=u.attachment_set_id \
                             WHERE u.id=$1 AND u.company_id=$2 AND u.created_at<=$3",
                        )
                        .bind(&upload_id)
                        .bind(company.as_str())
                        .bind(now.get())
                        .fetch_optional(&mut *connection)
                        .await?;
                        let (ticket, purpose, source, expected, _kind) =
                            row.ok_or_else(not_found)?;
                        let actor = support_actor(current, now)?;
                        let _ticket = authorize_support_attachment(
                            connection,
                            &actor,
                            &ticket,
                            &purpose,
                            Some(expected),
                            SupportAttachmentStage::Read,
                        )
                        .await?;
                        Ok((source, context(current)))
                    })
                },
            )
            .await?;
        let (source, reply_context) = source;
        let record = RecordId::new(source.as_str())?;
        let mut scanned = false;
        let workers = self.workers.lock().await;
        for worker in workers.iter() {
            match worker.run_next(Some(&record)).await {
                Ok(erpx_server_files::public::attachment_scan::AttachmentScanRunOutcome::NoWork) => continue,
                Ok(_) => {
                    scanned = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        if !scanned {
            return Err(unavailable().into());
        }
        // Fresh bounded read of the settled source.
        let company = self.configuration.company().clone();
        let state = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.read_input(move || {
                    Ok(HoldingWorkspaceRequest {
                        company_id: company,
                        branch_slug: None,
                        department_slug: None,
                        expected: None,
                        mutation: false,
                    })
                }),
                Self::read_access(),
                SupportFilesSources,
                {
                    let source = source.clone();
                    move |connection, _current| {
                        let source = source.clone();
                        Box::pin(async move {
                            let state: Option<String> = sqlx::query_scalar(
                                "SELECT source_state FROM attachments WHERE id=$1",
                            )
                            .bind(&source)
                            .fetch_optional(&mut *connection)
                            .await?;
                            Ok(state.unwrap_or_else(|| "quarantined".into()))
                        })
                    }
                },
            )
            .await?;
        Ok(SupportAttachmentHttpResponse {
            data: serde_json::json!({"sourceId": source, "state": state}),
            context: reply_context,
        })
    }
}

pub struct SupportFilesBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SupportFilesService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SupportFilesBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SupportFilesService<C>>,
    ) -> Self {
        Self { policy, scopes, service }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(
            inbound.audience(),
            erpx_server_identity_access::public::RequestAudience::Holding { .. }
        ) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.configuration.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> SupportAttachmentHttpBackend
    for SupportFilesBackend<C>
{
    fn create<'a>(
        &'a self,
        request: SupportAttachmentCreateHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .create(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.input.clone(),
                    CommandKey::new(request.key)?,
                )
                .await
                .map_err(business)
        })
    }

    fn reserve<'a>(
        &'a self,
        request: SupportAttachmentReserveHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .reserve(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.set_id.to_owned(),
                    request.input.clone(),
                    CommandKey::new(request.key)?,
                )
                .await
                .map_err(business)
        })
    }

    fn complete<'a>(
        &'a self,
        request: SupportAttachmentCompleteHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .complete(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.upload_id.to_owned(),
                    request.content_type.to_owned(),
                    request.bytes.to_vec(),
                )
                .await
                .map_err(business)
        })
    }

    fn scan<'a>(
        &'a self,
        request: SupportAttachmentScanHttpRequest<'a>,
    ) -> SupportAttachmentHttpFuture<'a, SupportAttachmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .scan(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.upload_id.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}
