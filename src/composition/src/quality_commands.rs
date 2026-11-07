//! Native Quality specification, inspection and cancellation command composition.
//!
//! The selected Account/Holding context, current IAM permission, Commercial
//! purchase admission, Commerce master witness, owner effects and Workflow
//! receipt/audit/outbox share the same serializable Holding unit.
use erpx_server_collaboration::public::quality_notification::{
    PgQualityNotificationSource, QualityNotificationFacts, RetainedQualityNotifications,
    lock_quality_notifications, stage_quality_notifications,
};
use erpx_server_commerce::public::quality_masters::{
    CurrentQualityMasters, QualityMasterRequest, inspect_quality_masters, lock_quality_masters,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, HoldingWorkSourcePreparation,
};
use erpx_server_identity_access::public::quality_recipients::{
    CurrentQualityRecipients, QualityInspectionRecipientRequest, QualityInspectionRecipientStatus,
    QualityRecipientRequest, QualityRecipientStatus, SelectedQualityRecipients,
    inspect_quality_inspection_recipients, inspect_quality_recipients_for_selected,
    lock_quality_recipient_sources,
};
use erpx_server_quality::public::quality_commands::{
    HoldStatus, PreparedQualityCancellation, PreparedQualityInspection, PreparedQualitySpecSave,
    PreparedQualitySpecTransition, QualityCancellationCommand, QualityInspectionCommand,
    QualitySpecAction, QualitySpecActionCommand, QualitySpecCommand, RetainedQualityCancellation,
    lock_quality_command_sources, prepare_quality_cancellation, prepare_quality_inspection,
    prepare_quality_spec_save, prepare_quality_spec_transition,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_COMMAND_CHANGED",
        "The Quality command changed. Reload and review before submitting.",
    )
}

fn company_scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SPEC_COMPANY_SCOPE",
        "Use a Company Quality manager assignment to configure inspection specifications.",
    )
}

fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Use a Company or Branch quality assignment.",
    )
}

fn product_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRODUCT_INVALID",
        "Choose an active product in this company.",
    )
}

/// Utility-only locks acquired before the first Holding snapshot.
#[derive(Clone, Copy)]
pub struct QualityCommandPreparation {
    notifications: bool,
    evidence: bool,
    cancellation: bool,
}

impl QualityCommandPreparation {
    pub const fn new(notifications: bool) -> Self {
        Self {
            notifications,
            evidence: notifications,
            cancellation: false,
        }
    }

    pub const fn cancellation() -> Self {
        Self {
            notifications: true,
            evidence: false,
            cancellation: true,
        }
    }
}

impl HoldingWorkSourcePreparation for QualityCommandPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_quality_masters(c).await?;
            if self.cancellation {
                erpx_server_quality::public::quality_http::lock_quality_cancellation_writes(c)
                    .await?;
            }
            lock_quality_command_sources(c).await?;
            if self.notifications {
                if self.evidence {
                    erpx_server_files::public::quality_inspection_evidence::lock_quality_inspection_evidence_sources(c)
                        .await?;
                }
                let mut recipients = lock_quality_recipient_sources(c).await?;
                lock_quality_notifications(recipients.connection()).await?;
                recipients.bind().await?;
            }
            Ok(())
        })
    }
}

#[derive(Clone)]
pub enum QualityOwnerCommand {
    SaveSpec(QualitySpecCommand),
    TransitionSpec(QualitySpecActionCommand),
    Inspect(QualityInspectionCommand),
    Cancel(QualityCancellationCommand),
}

impl QualityOwnerCommand {
    fn key(&self) -> &CommandKey {
        match self {
            Self::SaveSpec(command) => &command.key,
            Self::TransitionSpec(command) => &command.key,
            Self::Inspect(command) => &command.key,
            Self::Cancel(command) => &command.command_key,
        }
    }

    fn operation(&self) -> String {
        match self {
            Self::SaveSpec(command) => command.operation().to_owned(),
            Self::TransitionSpec(command) => command.operation(),
            Self::Inspect(command) => command.operation().to_owned(),
            Self::Cancel(command) => command.operation(),
        }
    }

    fn record_id(&self) -> Result<RecordId, BusinessError> {
        match self {
            Self::SaveSpec(command) => command.record_id(),
            Self::TransitionSpec(command) => Ok(command.id.clone()),
            Self::Inspect(command) => Ok(command.hold_id.clone()),
            Self::Cancel(command) => Ok(command.hold_id.clone()),
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::SaveSpec(_) | Self::TransitionSpec(_) => "quality.release",
            Self::Inspect(_) => "quality.inspect",
            Self::Cancel(command) => command.action.permission(),
        }
    }

    fn company_scope(&self) -> bool {
        matches!(self, Self::SaveSpec(_) | Self::TransitionSpec(_))
    }

    fn purpose(&self) -> CapabilityPurpose {
        match self {
            Self::SaveSpec(_) => CapabilityPurpose::NewWork,
            Self::TransitionSpec(command) => match command.action {
                QualitySpecAction::Activate => CapabilityPurpose::NewWork,
                QualitySpecAction::Cancel => CapabilityPurpose::SettleExisting,
            },
            Self::Inspect(_) => CapabilityPurpose::SettleExisting,
            Self::Cancel(_) => CapabilityPurpose::SettleExisting,
        }
    }

    fn input_json(&self) -> Result<CommandJson, BusinessError> {
        match self {
            Self::SaveSpec(command) => CommandJson::from_serializable(&SpecSaveJson {
                product_id: command.input.product_id.as_str(),
                name: &command.input.name,
                expected_version: command.input.expected_version,
                metrics: command
                    .input
                    .metrics
                    .iter()
                    .map(|metric| SpecMetricJson {
                        code: &metric.code,
                        label: &metric.label,
                        unit: &metric.unit,
                        minimum: metric.minimum.as_deref().unwrap_or(""),
                        maximum: metric.maximum.as_deref().unwrap_or(""),
                    })
                    .collect(),
            }),
            Self::TransitionSpec(command) => CommandJson::from_serializable(&SpecActionJson {
                expected_version: command.expected_version,
                action: command.action.as_str(),
                reason: &command.reason,
            }),
            Self::Cancel(command) => CommandJson::from_serializable(&CancellationJson {
                expected_version: command.expected_version,
                action: command.action.as_str(),
                reason: &command.reason,
            }),
            Self::Inspect(command) => CommandJson::from_serializable(&InspectionJson {
                expected_version: command.expected_version,
                result: command.result.map(|result| result.as_str()),
                measurements: command
                    .measurements
                    .iter()
                    .map(|measurement| InspectionMeasurementJson {
                        metric_id: &measurement.metric_id,
                        value: &measurement.value,
                    })
                    .collect(),
                attachment_ids: &command.attachment_ids,
                attachment_set_ids: &command.attachment_set_ids,
                evidence: &command.evidence,
            }),
        }
    }

    fn request(&self) -> Result<WorkflowRequest, BusinessError> {
        Ok(WorkflowRequest {
            key: self.key().as_str().to_owned(),
            operation: self.operation(),
            record_id: self.record_id()?.as_str().to_owned(),
            input: self.input_json()?,
        })
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SpecMetricJson<'a> {
    code: &'a str,
    label: &'a str,
    unit: &'a str,
    minimum: &'a str,
    maximum: &'a str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SpecSaveJson<'a> {
    product_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_version: Option<i64>,
    metrics: Vec<SpecMetricJson<'a>>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SpecActionJson<'a> {
    expected_version: i64,
    action: &'a str,
    reason: &'a str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionMeasurementJson<'a> {
    metric_id: &'a str,
    value: &'a str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CancellationJson<'a> {
    expected_version: i64,
    action: &'a str,
    reason: &'a str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectionJson<'a> {
    expected_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a str>,
    measurements: Vec<InspectionMeasurementJson<'a>>,
    attachment_ids: &'a [String],
    attachment_set_ids: &'a [String],
    evidence: &'a str,
}

struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a QualityOwnerCommand,
    mode: PackageEnforcementMode,
    clock: &'a G,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}

impl<G: Clock + Sync> Authority<'_, G> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission(self.command.permission())?;
        let expected = self.command.request()?;
        let workspace = self.current.selection().workspace();
        if !self
            .current
            .modules()
            .iter()
            .any(|module| module == "quality")
            || self.clock.now().get() >= self.current.expires_at()
            || request.key != expected.key
            || request.operation != expected.operation
            || request.record_id != expected.record_id
            || request.input.as_str() != expected.input.as_str()
        {
            return Err(changed());
        }
        if self.command.company_scope() {
            if workspace.branch_id.is_some() || workspace.department_id.is_some() {
                return Err(company_scope());
            }
        } else if workspace.department_id.is_some() {
            return Err(scope_required());
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}

impl<G: Clock + Sync> PgCommandAuthority for Authority<'_, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(request)?)
    }

    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(request)? != actor {
            return Err(changed().into());
        }
        let now = self.clock.now();
        let mut purchases = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        let purpose = match disposition {
            CommandDisposition::Execute => self.command.purpose(),
            CommandDisposition::Replay => CapabilityPurpose::History,
        };
        if let Some(witness) = &self.purchase {
            purchases
                .revalidate(witness, now)
                .await?
                .require_allowed()?;
        } else {
            self.purchase = Some(
                purchases
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["supply.quality".into()],
                            purpose,
                            mode: self.mode,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

enum Prepared {
    Save {
        prepared: PreparedQualitySpecSave,
        master: CurrentQualityMasters,
    },
    Transition(PreparedQualitySpecTransition),
    Inspection(PreparedQualityInspection),
    Cancel(PreparedQualityCancellation),
}

enum Written<'a> {
    Save {
        prepared: PreparedQualitySpecSave,
        master: CurrentQualityMasters,
    },
    Transition(PreparedQualitySpecTransition),
    Inspection {
        prepared: PreparedQualityInspection,
        recipients: CurrentQualityRecipients,
        retained: RetainedQualityNotifications,
    },
    Cancel {
        prepared: RetainedQualityCancellation,
        recipients: SelectedQualityRecipients<'a>,
        retained: RetainedQualityNotifications,
    },
}

/// Collaboration-owned notice source joined to the actual saved Quality
/// after-image and current Identity recipient witness.
struct InspectionNotificationSource<'a> {
    prepared: &'a PreparedQualityInspection,
    recipients: &'a CurrentQualityRecipients,
    facts: QualityNotificationFacts,
}

impl PgQualityNotificationSource for InspectionNotificationSource<'_> {
    fn facts(&self) -> &QualityNotificationFacts {
        &self.facts
    }

    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.prepared.require_after(c).await?;
        self.recipients.require_current(c).await?;
        let hold = self.prepared.hold();
        if self.facts.company_id != hold.company_id
            || self.facts.hold_id != hold.id
            || self.facts.version != hold.version
            || self.facts.updated_at != hold.updated_at
            || self.facts.recipients != self.recipients.recipients()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

/// Collaboration-owned notice source joined to the saved cancellation
/// after-image and the current Identity recipient witness.
struct CancellationNotificationSource<'a> {
    prepared: &'a RetainedQualityCancellation,
    recipients: &'a SelectedQualityRecipients<'a>,
    now: i64,
    facts: QualityNotificationFacts,
}

impl PgQualityNotificationSource for CancellationNotificationSource<'_> {
    fn facts(&self) -> &QualityNotificationFacts {
        &self.facts
    }

    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.prepared.require_current(c).await?;
        self.recipients.require_current(c, self.now).await?;
        let hold = self.prepared.hold();
        if self.facts.company_id != hold.company_id
            || self.facts.hold_id != hold.id
            || self.facts.version != hold.version
            || self.facts.updated_at != hold.updated_at
            || self.facts.recipients != self.recipients.recipients()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

fn cancellation_notification_facts(
    command: &QualityCancellationCommand,
    prepared: &RetainedQualityCancellation,
    recipients: &SelectedQualityRecipients<'_>,
) -> QualityNotificationFacts {
    let hold = prepared.hold();
    QualityNotificationFacts {
        command_key: command.command_key.as_str().to_owned(),
        company_id: hold.company_id.clone(),
        hold_id: hold.id.clone(),
        version: hold.version,
        updated_at: hold.updated_at,
        recipients: recipients.recipients().to_vec(),
    }
}

fn cancellation_recipient_request(
    prepared: &RetainedQualityCancellation,
) -> Result<QualityRecipientRequest, BusinessError> {
    let hold = prepared.hold();
    let status = match hold.status {
        HoldStatus::CancelPending => QualityRecipientStatus::CancellationPending,
        HoldStatus::Cancelled => QualityRecipientStatus::Cancelled,
        HoldStatus::Held => QualityRecipientStatus::Held,
        _ => return Err(changed()),
    };
    Ok(QualityRecipientRequest {
        company_id: CompanyId::new(hold.company_id.clone())?,
        branch_id: RecordId::new(hold.branch_id.clone())?,
        creator: hold.created_by.clone(),
        cancellation_requester: hold.cancel_requested_by.clone(),
        status,
    })
}

fn inspection_notification_facts(
    command: &QualityInspectionCommand,
    prepared: &PreparedQualityInspection,
    recipients: &CurrentQualityRecipients,
) -> QualityNotificationFacts {
    let hold = prepared.hold();
    QualityNotificationFacts {
        command_key: command.key.as_str().to_owned(),
        company_id: hold.company_id.clone(),
        hold_id: hold.id.clone(),
        version: hold.version,
        updated_at: hold.updated_at,
        recipients: recipients.recipients().to_vec(),
    }
}

struct Effects<'a, W> {
    current: &'a AuthorizedWorkContext,
    command: &'a QualityOwnerCommand,
    write_clock: &'a W,
    prepared: Option<Prepared>,
    written: Option<Written<'a>>,
}

#[derive(serde::Serialize)]
struct SpecResult<'a> {
    id: &'a str,
    status: &'a str,
    version: i64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CancellationResult<'a> {
    id: &'a str,
    hold_id: &'a str,
    cancellation_id: &'a str,
    status: &'a str,
    version: i64,
}
#[derive(serde::Serialize)]
struct InspectionResultJson<'a> {
    id: &'a str,
    status: &'a str,
    result: &'a str,
    version: i64,
}

impl<W: Clock + Sync> PgOwnerCommandEffects for Effects<'_, W> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let preparation = match self.command {
            QualityOwnerCommand::Inspect(_) => QualityCommandPreparation::new(true),
            QualityOwnerCommand::Cancel(_) => QualityCommandPreparation::cancellation(),
            _ => QualityCommandPreparation::new(false),
        };
        preparation.prepare(c).await
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let company = self.current.selection().workspace().company_id.clone();
        let now = self.write_clock.now().get();
        let actor = self.current.user_id().clone();
        match self.command {
            QualityOwnerCommand::SaveSpec(command) => {
                let master = inspect_quality_masters(
                    c,
                    QualityMasterRequest::Product {
                        company: company.clone(),
                        product_id: command.input.product_id.as_str().to_owned(),
                    },
                )
                .await?;
                if master.facts().is_none() {
                    return Err(product_invalid().into());
                }
                let prepared = prepare_quality_spec_save(c, command, &company, &actor, now).await?;
                let (id, status, version) = {
                    let receipt = prepared.receipt();
                    (receipt.id.clone(), receipt.status.as_str(), receipt.version)
                };
                let result = CommandJson::from_serializable(&SpecResult {
                    id: &id,
                    status,
                    version,
                })?;
                self.prepared = Some(Prepared::Save { prepared, master });
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            QualityOwnerCommand::TransitionSpec(command) => {
                let prepared =
                    prepare_quality_spec_transition(c, command, &company, &actor, now).await?;
                let (id, status, version) = {
                    let receipt = prepared.receipt();
                    (receipt.id.clone(), receipt.status.as_str(), receipt.version)
                };
                let result = CommandJson::from_serializable(&SpecResult {
                    id: &id,
                    status,
                    version,
                })?;
                self.prepared = Some(Prepared::Transition(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            QualityOwnerCommand::Cancel(command) => {
                let branch = self.current.selection().workspace().branch_id.clone();
                let prepared = prepare_quality_cancellation(
                    c,
                    command,
                    &company,
                    branch.as_ref(),
                    &actor,
                    now,
                )
                .await?;
                let (id, hold_id, cancellation_id, status, version) = {
                    let receipt = prepared.receipt();
                    (
                        receipt.id.clone(),
                        receipt.hold_id.clone(),
                        receipt.cancellation_id.clone(),
                        receipt.status.as_str(),
                        receipt.version,
                    )
                };
                let result = CommandJson::from_serializable(&CancellationResult {
                    id: &id,
                    hold_id: &hold_id,
                    cancellation_id: &cancellation_id,
                    status,
                    version,
                })?;
                self.prepared = Some(Prepared::Cancel(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            QualityOwnerCommand::Inspect(command) => {
                let branch = self.current.selection().workspace().branch_id.clone();
                let prepared =
                    prepare_quality_inspection(c, command, &company, branch.as_ref(), &actor, now)
                        .await?;
                let (id, status, result_value, version) = {
                    let receipt = prepared.receipt();
                    (
                        receipt.id.clone(),
                        receipt.status.as_str(),
                        receipt.result.as_str(),
                        receipt.version,
                    )
                };
                let result = CommandJson::from_serializable(&InspectionResultJson {
                    id: &id,
                    status,
                    result: result_value,
                    version,
                })?;
                self.prepared = Some(Prepared::Inspection(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
        }
    }

    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Save { prepared, master } => {
                prepared.apply(c).await?;
                master.require_current(c).await?;
                self.written = Some(Written::Save { prepared, master });
            }
            Prepared::Transition(prepared) => {
                prepared.apply(c).await?;
                self.written = Some(Written::Transition(prepared));
            }
            Prepared::Cancel(prepared) => {
                let prepared = prepared.apply(c).await?;
                let QualityOwnerCommand::Cancel(command) = self.command else {
                    return Err(changed().into());
                };
                let request = cancellation_recipient_request(&prepared)?;
                let now = self.write_clock.now().get();
                let recipients =
                    inspect_quality_recipients_for_selected(c, self.current, request, now).await?;
                let facts = cancellation_notification_facts(command, &prepared, &recipients);
                let source = CancellationNotificationSource {
                    prepared: &prepared,
                    recipients: &recipients,
                    now,
                    facts,
                };
                let retained = stage_quality_notifications(c, &source).await?;
                self.written = Some(Written::Cancel {
                    prepared,
                    recipients,
                    retained,
                });
            }
            Prepared::Inspection(prepared) => {
                prepared.apply(c).await?;
                let QualityOwnerCommand::Inspect(command) = self.command else {
                    return Err(changed().into());
                };
                let hold = prepared.hold();
                let recipient_status = match hold.status {
                    HoldStatus::Held => QualityInspectionRecipientStatus::Held,
                    HoldStatus::AwaitingRelease => {
                        QualityInspectionRecipientStatus::AwaitingRelease
                    }
                    _ => return Err(changed().into()),
                };
                let mut preparation = lock_quality_recipient_sources(c).await?;
                let locks = preparation.bind().await?;
                let recipients = inspect_quality_inspection_recipients(
                    c,
                    &locks,
                    QualityInspectionRecipientRequest {
                        company_id: CompanyId::new(hold.company_id.clone())?,
                        branch_id: RecordId::new(hold.branch_id.clone())?,
                        creator: hold.created_by.clone(),
                        last_inspector: hold.last_inspector.clone(),
                        status: recipient_status,
                    },
                )
                .await?;
                let facts = inspection_notification_facts(command, &prepared, &recipients);
                let source = InspectionNotificationSource {
                    prepared: &prepared,
                    recipients: &recipients,
                    facts,
                };
                let retained = stage_quality_notifications(c, &source).await?;
                self.written = Some(Written::Inspection {
                    prepared,
                    recipients,
                    retained,
                });
            }
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        match self.written.as_ref().ok_or_else(changed)? {
            Written::Save { prepared, master } => {
                prepared.require_after(c).await?;
                master.require_current(c).await?;
            }
            Written::Transition(prepared) => prepared.require_after(c).await?,
            Written::Inspection {
                prepared,
                recipients,
                retained,
            } => {
                prepared.require_after(c).await?;
                let QualityOwnerCommand::Inspect(command) = self.command else {
                    return Err(changed().into());
                };
                let source = InspectionNotificationSource {
                    prepared,
                    recipients,
                    facts: inspection_notification_facts(command, prepared, recipients),
                };
                retained.require_current(c, &source).await?;
            }
            Written::Cancel {
                prepared,
                recipients,
                retained,
            } => {
                prepared.require_current(c).await?;
                let QualityOwnerCommand::Cancel(command) = self.command else {
                    return Err(changed().into());
                };
                let now = self.write_clock.now().get();
                let source = CancellationNotificationSource {
                    prepared,
                    recipients,
                    now,
                    facts: cancellation_notification_facts(command, prepared, recipients),
                };
                retained.require_current(c, &source).await?;
            }
        }
        Ok(())
    }
}

/// Execute one native Quality owner command in the caller's retained unit.
pub async fn execute_quality_command<W: Clock + Sync, G: Clock + Sync, I: IdentifierSource>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    command: &QualityOwnerCommand,
    mode: PackageEnforcementMode,
    write_clock: &W,
    guard_clock: &G,
    identifiers: &mut I,
) -> Result<CommandOutcome, Error> {
    let request = command.request()?;
    let mut authority = Authority {
        current,
        command,
        mode,
        clock: guard_clock,
        catalogue: current_package_catalogue()?,
        purchase: None,
    };
    let mut effects = Effects {
        current,
        command,
        write_clock,
        prepared: None,
        written: None,
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write_clock,
        identifiers,
    )
    .execute(&request, &mut effects)
    .await
}
