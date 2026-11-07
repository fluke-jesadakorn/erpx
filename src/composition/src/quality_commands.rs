//! Native Quality specification, inspection and cancellation command composition.
//!
//! The selected Account/Holding context, current IAM permission, Commercial
//! purchase admission, Commerce master witness, owner effects and Workflow
//! receipt/audit/outbox share the same serializable Holding unit.
use erpx_server_collaboration::public::quality_notification::{
    PgQualityNotificationSource, QualityNotificationFacts, RetainedQualityNotifications,
    lock_quality_notifications, stage_quality_notifications,
};
use erpx_server_collaboration::public::service_count_flow_notification::{
    CountFlowNotificationFacts, PgCountFlowNotificationSource, RetainedCountFlowNotification,
    prepare_count_flow_notification,
};
use erpx_server_commerce::public::quality_masters::{
    CurrentQualityMasters, QualityMasterRequest, inspect_quality_masters, lock_quality_masters,
};
use erpx_server_commerce::public::quality_scrap_stock::{
    CurrentQualityScrapStock, inspect_quality_scrap_stock, lock_quality_scrap_stock_sources,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_files::public::quality_disposition_evidence::{
    CurrentQualityDispositionEvidenceSets, inspect_quality_disposition_evidence_sets,
    lock_quality_disposition_evidence_sources,
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
use erpx_server_identity_access::public::quality_scrap_approvers::{
    CurrentQualityScrapApprovers, inspect_quality_scrap_approvers,
    lock_quality_scrap_approver_sources,
};
use erpx_server_inventory::public::quality_scrap_count::{
    PreparedQualityScrapCount, QualityScrapCountLot, QualityScrapCountRequest,
    lock_quality_scrap_count_sources, prepare_quality_scrap_count,
};
use erpx_server_inventory::public::quality_scrap_lots::{
    CurrentQualityScrapLots, inspect_quality_scrap_lots, lock_quality_scrap_lot_sources,
};
use erpx_server_quality::public::quality_commands::{
    HoldStatus, PreparedQualityCancellation, PreparedQualityInspection, PreparedQualityRelease,
    PreparedQualitySpecSave, PreparedQualitySpecTransition, QualityCancellationCommand,
    QualityDispositionCommand, QualityDispositionReceipt, QualityInspectionCommand,
    QualityReleaseCommand, QualityScrapLot, QualityScrapStock,
    QualitySpecAction, QualitySpecActionCommand, QualitySpecCommand, RetainedQualityCancellation,
    lock_quality_command_sources, lock_quality_scrap_writes, plan_quality_scrap,
    prepare_quality_cancellation, prepare_quality_inspection, prepare_quality_release,
    prepare_quality_spec_save, prepare_quality_spec_transition, read_quality_hold_for_command,
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
    stock: bool,
    lots: bool,
    counts: bool,
    disposition_evidence: bool,
}

impl QualityCommandPreparation {
    pub const fn new(notifications: bool) -> Self {
        Self {
            notifications,
            evidence: notifications,
            cancellation: false,
            stock: false,
            lots: false,
            counts: false,
            disposition_evidence: false,
        }
    }

    pub const fn cancellation() -> Self {
        Self {
            notifications: true,
            evidence: false,
            cancellation: true,
            stock: false,
            lots: false,
            counts: false,
            disposition_evidence: false,
        }
    }

    pub const fn release() -> Self {
        Self {
            notifications: true,
            evidence: false,
            cancellation: false,
            stock: false,
            lots: false,
            counts: false,
            disposition_evidence: false,
        }
    }

    /// Stable owner lock order for scrap:
    /// Commercial authority -> Commerce masters/stock -> Quality command and
    /// disposition writes -> Inventory Count rows/lines -> Inventory
    /// lots/balances -> Files evidence -> Identity recipients -> Collaboration
    /// notifications. This follows the existing Count writers, which acquire
    /// the Count tables before lot evidence.
    pub const fn scrap() -> Self {
        Self {
            notifications: true,
            evidence: false,
            cancellation: false,
            stock: true,
            lots: true,
            counts: true,
            disposition_evidence: true,
        }
    }
}

impl HoldingWorkSourcePreparation for QualityCommandPreparation {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            lock_quality_masters(c).await?;
            if self.stock {
                lock_quality_scrap_stock_sources(c).await?;
            }
            if self.cancellation {
                erpx_server_quality::public::quality_http::lock_quality_cancellation_writes(c)
                    .await?;
            }
            lock_quality_command_sources(c).await?;
            if self.counts {
                lock_quality_scrap_count_sources(c).await?;
            }
            if self.lots {
                lock_quality_scrap_lot_sources(c).await?;
            }
            if self.counts {
                lock_quality_scrap_writes(c).await?;
            }
            if self.disposition_evidence {
                lock_quality_disposition_evidence_sources(c).await?;
            }
            if self.notifications {
                if self.evidence {
                    erpx_server_files::public::quality_inspection_evidence::lock_quality_inspection_evidence_sources(c)
                        .await?;
                }
                if self.stock {
                    lock_quality_scrap_approver_sources(c).await?;
                } else {
                    let mut recipients = lock_quality_recipient_sources(c).await?;
                    lock_quality_notifications(recipients.connection()).await?;
                    recipients.bind().await?;
                    return Ok(());
                }
                lock_quality_notifications(c).await?;
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
    Release(QualityReleaseCommand),
    Scrap(QualityDispositionCommand),
}

impl QualityOwnerCommand {
    fn key(&self) -> &CommandKey {
        match self {
            Self::SaveSpec(command) => &command.key,
            Self::TransitionSpec(command) => &command.key,
            Self::Inspect(command) => &command.key,
            Self::Cancel(command) => &command.command_key,
            Self::Release(command) => &command.key,
            Self::Scrap(command) => &command.key,
        }
    }

    fn operation(&self) -> String {
        match self {
            Self::SaveSpec(command) => command.operation().to_owned(),
            Self::TransitionSpec(command) => command.operation(),
            Self::Inspect(command) => command.operation().to_owned(),
            Self::Cancel(command) => command.operation(),
            Self::Release(command) => command.operation().to_owned(),
            Self::Scrap(command) => command.operation().to_owned(),
        }
    }

    fn record_id(&self) -> Result<RecordId, BusinessError> {
        match self {
            Self::SaveSpec(command) => command.record_id(),
            Self::TransitionSpec(command) => Ok(command.id.clone()),
            Self::Inspect(command) => Ok(command.hold_id.clone()),
            Self::Cancel(command) => Ok(command.hold_id.clone()),
            Self::Release(command) => Ok(command.hold_id.clone()),
            Self::Scrap(command) => Ok(command.hold_id.clone()),
        }
    }

    fn permission(&self) -> &'static str {
        match self {
            Self::SaveSpec(_) | Self::TransitionSpec(_) => "quality.release",
            Self::Inspect(_) => "quality.inspect",
            Self::Cancel(command) => command.action.permission(),
            Self::Release(_) | Self::Scrap(_) => "quality.release",
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
            Self::Release(_) | Self::Scrap(_) => CapabilityPurpose::SettleExisting,
        }
    }

    fn capabilities(&self) -> Vec<String> {
        match self {
            Self::Scrap(_) => vec!["supply.quality".into(), "supply.counts".into()],
            _ => vec!["supply.quality".into()],
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
            Self::Release(command) => CommandJson::from_serializable(&ReleaseJson {
                expected_version: command.expected_version,
                reason: &command.reason,
            }),
            Self::Scrap(command) => CommandJson::from_serializable(&ScrapJson {
                expected_version: command.expected_version,
                attachment_set_ids: &command.attachment_set_ids,
                disposition_date: &command.disposition_date,
                quantity: &command.quantity,
                reason: &command.reason,
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
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseJson<'a> {
    expected_version: i64,
    reason: &'a str,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapJson<'a> {
    expected_version: i64,
    attachment_set_ids: &'a [String],
    disposition_date: &'a str,
    quantity: &'a str,
    reason: &'a str,
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
                            capabilities: self.command.capabilities(),
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
    Release(PreparedQualityRelease),
    Scrap(Box<PreparedScrap>),
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
    Release {
        prepared: PreparedQualityRelease,
        recipients: SelectedQualityRecipients<'a>,
        now: i64,
        retained: RetainedQualityNotifications,
    },
    Scrap {
        prepared: Box<PreparedScrap>,
        retained: Vec<RetainedCountFlowNotification>,
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

/// Collaboration-owned notice source joined to the saved release after-image
/// and the current Identity terminal-creator recipient witness.
struct ReleaseNotificationSource<'a> {
    prepared: &'a PreparedQualityRelease,
    recipients: &'a SelectedQualityRecipients<'a>,
    now: i64,
    facts: QualityNotificationFacts,
}

impl PgQualityNotificationSource for ReleaseNotificationSource<'_> {
    fn facts(&self) -> &QualityNotificationFacts {
        &self.facts
    }

    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.prepared.require_after(c).await?;
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

fn release_notification_facts(
    command: &QualityReleaseCommand,
    prepared: &PreparedQualityRelease,
    recipients: &SelectedQualityRecipients<'_>,
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

fn release_recipient_request(
    prepared: &PreparedQualityRelease,
) -> Result<QualityRecipientRequest, BusinessError> {
    let hold = prepared.hold();
    Ok(QualityRecipientRequest {
        company_id: CompanyId::new(hold.company_id.clone())?,
        branch_id: RecordId::new(hold.branch_id.clone())?,
        creator: hold.created_by.clone(),
        cancellation_requester: None,
        status: QualityRecipientStatus::Released,
    })
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


/// All real owner witnesses retained by one scrap preparation. The files
/// evidence witness is optional because a disposition may have no file set.
struct PreparedScrap {
    source_hold: erpx_server_quality::public::quality_read::QualityHold,
    expected_hold: erpx_server_quality::public::quality_read::QualityHold,
    stock: CurrentQualityScrapStock,
    lots: CurrentQualityScrapLots,
    count: PreparedQualityScrapCount,
    evidence: Option<CurrentQualityDispositionEvidenceSets>,
    approvers: CurrentQualityScrapApprovers,
    actor: String,
    key: String,
    receipt: QualityDispositionReceipt,
    now: i64,
}

impl PreparedScrap {
    async fn require_owner_current(
        &self,
        c: &mut PgConnection,
        draft_evidence: bool,
    ) -> Result<(), Error> {
        self.stock.require_current(c).await?;
        self.lots.require_current(c).await?;
        self.approvers.require_current(c).await?;
        if draft_evidence {
            if let Some(evidence) = &self.evidence {
                evidence.require_current(c).await?;
            }
        }
        Ok(())
    }

    async fn require_hold(&self, c: &mut PgConnection, expected: bool) -> Result<(), Error> {
        let company = CompanyId::new(self.expected_hold.company_id.clone())?;
        let branch = RecordId::new(self.expected_hold.branch_id.clone())?;
        let current = read_quality_hold_for_command(c, &company, Some(&branch), &self.expected_hold.id)
            .await?
            .ok_or_else(changed)?;
        let image = if expected { &self.expected_hold } else { &self.source_hold };
        if &current != image {
            return Err(changed().into());
        }
        Ok(())
    }

    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_hold(c, false).await?;
        self.count.require_before(c).await?;
        self.require_owner_current(c, true).await
    }

    async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        self.count.apply(c).await?;
        if let Some(evidence) = &self.evidence {
            evidence
                .apply(c, &self.actor, &self.key, &self.receipt.count_id, self.now)
                .await?;
        }
        let updated = sqlx::query(
            "UPDATE inventory_quality_holds SET status='disposition_pending',disposition_kind='scrap',disposition_reason=$2,disposition_requested_by=$3,disposition_count_id=$4,disposition_quantity_milli=$5,version=$6,updated_at=$7 \
             WHERE id=$1 AND company_id=$8 AND version=$9 AND status='held' AND last_result='fail'",
        )
        .bind(&self.source_hold.id)
        .bind(self.expected_hold.disposition_reason.as_deref())
        .bind(self.expected_hold.disposition_requested_by.as_deref())
        .bind(self.expected_hold.disposition_count_id.as_deref())
        .bind(self.expected_hold.disposition_quantity_milli)
        .bind(self.expected_hold.version)
        .bind(self.now)
        .bind(&self.expected_hold.company_id)
        .bind(self.source_hold.version)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(changed().into());
        }
        sqlx::query(
            "INSERT INTO inventory_quality_disposition_events(id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at) \
             VALUES($1,$2,$3,'request',$4,$5,$6,$7)",
        )
        .bind(format!("quality-disposition-{}", self.key))
        .bind(&self.source_hold.id)
        .bind(&self.receipt.count_id)
        .bind(&self.actor)
        .bind(self.source_hold.version)
        .bind(self.expected_hold.disposition_reason.as_deref())
        .bind(self.now)
        .execute(&mut *c)
        .await?;
        self.require_after(c).await
    }

    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_hold(c, true).await?;
        self.count.require_after(c).await?;
        if let Some(evidence) = &self.evidence {
            evidence
                .require_after(c, &self.actor, &self.key, &self.receipt.count_id, self.now)
                .await?;
        }
        let event: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT action,reviewed_version,count_id FROM inventory_quality_disposition_events WHERE id=$1 AND hold_id=$2",
        )
        .bind(format!("quality-disposition-{}", self.key))
        .bind(&self.source_hold.id)
        .fetch_optional(&mut *c)
        .await?;
        if event
            != Some((
                "request".into(),
                self.source_hold.version,
                self.receipt.count_id.clone(),
            ))
        {
            return Err(changed().into());
        }
        self.require_owner_current(c, false).await
    }
}

struct ScrapNotificationSource {
    facts: CountFlowNotificationFacts,
}

impl PgCountFlowNotificationSource for ScrapNotificationSource {
    fn facts(&self) -> &CountFlowNotificationFacts {
        &self.facts
    }

    async fn require_before(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }

    async fn require_after(&self, _: &mut PgConnection) -> Result<(), Error> {
        Ok(())
    }
}

async fn stage_scrap_notifications(
    c: &mut PgConnection,
    prepared: &PreparedScrap,
) -> Result<Vec<RetainedCountFlowNotification>, Error> {
    let mut retained = Vec::new();
    for recipient in prepared.approvers.recipients() {
        let source = ScrapNotificationSource {
            facts: CountFlowNotificationFacts {
                id: format!("{}-{}", prepared.key, recipient),
                company_id: prepared.expected_hold.company_id.clone(),
                recipient: recipient.clone(),
                count_id: prepared.receipt.count_id.clone(),
                version: 1,
                message: "Quality scrap requires independent Finance review".into(),
                created_at: prepared.now,
            },
        };
        retained.push(
            prepare_count_flow_notification(c, &source)
                .await?
                .persist(c)
                .await?,
        );
    }
    Ok(retained)
}


async fn prepare_scrap(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    command: &QualityDispositionCommand,
    company: &CompanyId,
    actor: &erpx_server_shared::public::UserId,
    now: i64,
) -> Result<PreparedScrap, Error> {
    let branch = current.selection().workspace().branch_id.clone();
    let command = erpx_server_quality::public::quality_commands::normalize_quality_disposition(
        command.clone(),
    )?;
    let source_hold = read_quality_hold_for_command(
        c,
        company,
        branch.as_ref(),
        command.hold_id.as_str(),
    )
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "QUALITY_HOLD_NOT_FOUND",
            "Quality hold not found.",
        )
    })?;
    let self_created = erpx_server_quality::public::quality_commands::quality_same_person(
        c,
        actor.as_str(),
        &source_hold.created_by,
        company.as_str(),
    )
    .await?;
    let self_inspected = match source_hold.last_inspector.as_deref() {
        Some(inspector) => {
            erpx_server_quality::public::quality_commands::quality_same_person(
                c,
                actor.as_str(),
                inspector,
                company.as_str(),
            )
            .await?
        }
        None => false,
    };
    if self_created || self_inspected {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_DISPOSITION_REQUIRED",
            "A third Quality manager must request disposal after the failed inspection.",
        )
        .into());
    }
    if source_hold.status != HoldStatus::Held
        || source_hold.last_result != Some(erpx_server_quality::public::quality_commands::InspectionResult::Fail)
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "FAILED_INSPECTION_REQUIRED",
            "A current failed inspection is required before disposal.",
        )
        .into());
    }
    if source_hold.version != command.expected_version {
        return Err(changed_for("CONFLICT", "The Quality hold changed. Reload and review before submitting.").into());
    }
    let stock = inspect_quality_scrap_stock(
        c,
        company,
        &source_hold.warehouse_id,
        &source_hold.product_id,
    )
    .await?
    .filter(|stock| stock.facts().branch_id == source_hold.branch_id)
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "QUALITY_STOCK_MISSING",
            "The held stock is no longer available.",
        )
    })?;
    let stock_facts = stock.facts();
    let scrap_stock = QualityScrapStock {
        warehouse_id: stock_facts.warehouse_id.clone(),
        product_id: stock_facts.product_id.clone(),
        branch_id: stock_facts.branch_id.clone(),
        quantity_milli: stock_facts.quantity_milli,
        value_minor: stock_facts.value_minor,
        version: stock_facts.version,
        product_version: stock_facts.product_version,
        name: stock_facts.name.clone(),
        unit: stock_facts.unit.clone(),
        tracking_mode: stock_facts.tracking_mode.clone(),
    };
    let lots_witness =
        inspect_quality_scrap_lots(c, company, &source_hold.product_id, &source_hold.warehouse_id)
            .await?;
    let lots = if stock_facts.tracking_mode == "none" {
        Vec::new()
    } else {
        lots_witness
            .facts()
            .iter()
            .map(|lot| QualityScrapLot {
                id: lot.id.clone(),
                tracking_mode: lot.tracking_mode.clone(),
                lot_code: lot.lot_code.clone(),
                serial_number: lot.serial_number.clone(),
                manufactured_date: lot.manufactured_date.clone(),
                expiry_date: lot.expiry_date.clone(),
                quantity_milli: lot.quantity_milli,
                value_minor: lot.value_minor,
                balance_version: lot.balance_version,
            })
            .collect()
    };
    let (plan, expected_hold) = plan_quality_scrap(
        &command,
        &source_hold,
        &scrap_stock,
        &lots,
        actor.as_str(),
        now,
    )?;
    let evidence = if command.attachment_set_ids.is_empty() {
        None
    } else {
        let branch_id = RecordId::new(source_hold.branch_id.clone())?;
        match inspect_quality_disposition_evidence_sets(
            c,
            company,
            &branch_id,
            &source_hold.id,
            command.expected_version,
            &command.attachment_set_ids,
        )
        .await
        {
            Ok(Some(sets)) => Some(sets),
            Ok(None) | Err(PgTransactionError::Business(_)) => {
                return Err(BusinessError::invalid(
                    "QUALITY_ATTACHMENT_INVALID",
                    "Choose ready, unbound disposition evidence uploaded for this hold version.",
                )
                .into());
            }
            Err(error) => return Err(error),
        }
    };
    let approver_locks = erpx_server_identity_access::public::quality_scrap_approvers::lock_quality_scrap_approver_sources(c).await?;
    let approvers =
        inspect_quality_scrap_approvers(c, &approver_locks, company, actor.as_str()).await?;
    let count_request = QualityScrapCountRequest {
        company_id: company.clone(),
        branch_id: source_hold.branch_id.clone(),
        warehouse_id: source_hold.warehouse_id.clone(),
        hold_id: source_hold.id.clone(),
        count_id: plan.count_id.clone(),
        line_id: plan.line_id.clone(),
        number: plan.number.clone(),
        count_date: command.disposition_date.clone(),
        reason: format!("Quality scrap: {}", command.reason),
        net_value_minor: -plan.scrap_value_minor,
        actor_id: actor.as_str().to_owned(),
        now,
        product_id: source_hold.product_id.clone(),
        product_version: stock_facts.product_version,
        description: stock_facts.name.clone(),
        unit: stock_facts.unit.clone(),
        snapshot_quantity_milli: stock_facts.quantity_milli,
        snapshot_value_minor: stock_facts.value_minor,
        snapshot_version: stock_facts.version,
        counted_quantity_milli: plan.counted_quantity_milli,
        counted_value_minor: plan.counted_value_minor,
        lots: plan
            .lots
            .iter()
            .map(|adjustment| QualityScrapCountLot {
                id: adjustment.lot.id.clone(),
                tracking_mode: adjustment.lot.tracking_mode.clone(),
                lot_code: adjustment.lot.lot_code.clone(),
                serial_number: adjustment.lot.serial_number.clone(),
                manufactured_date: adjustment.lot.manufactured_date.clone(),
                expiry_date: adjustment.lot.expiry_date.clone(),
                quantity_milli: adjustment.lot.quantity_milli,
                value_minor: adjustment.lot.value_minor,
                balance_version: adjustment.lot.balance_version,
                counted_quantity_milli: adjustment.lot.quantity_milli - adjustment.reduce_milli,
                counted_value_minor: adjustment.lot.value_minor - adjustment.lot_value_minor,
            })
            .collect(),
    };
    let count = prepare_quality_scrap_count(c, count_request).await?;
    let receipt = QualityDispositionReceipt {
        id: expected_hold.id.clone(),
        status: expected_hold.status,
        version: expected_hold.version,
        count_id: plan.count_id.clone(),
    };
    Ok(PreparedScrap {
        source_hold,
        expected_hold,
        stock,
        lots: lots_witness,
        count,
        evidence,
        approvers,
        actor: actor.as_str().to_owned(),
        key: command.key.as_str().to_owned(),
        receipt,
        now,
    })
}

fn changed_for(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
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
struct ReleaseResult<'a> {
    id: &'a str,
    status: &'a str,
    version: i64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapResult<'a> {
    id: &'a str,
    status: &'a str,
    version: i64,
    count_id: &'a str,
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
            QualityOwnerCommand::Release(_) => QualityCommandPreparation::release(),
            QualityOwnerCommand::Scrap(_) => QualityCommandPreparation::scrap(),
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
            QualityOwnerCommand::Release(command) => {
                let branch = self.current.selection().workspace().branch_id.clone();
                let prepared =
                    prepare_quality_release(c, command, &company, branch.as_ref(), &actor, now)
                        .await?;
                let (id, status, version) = {
                    let receipt = prepared.receipt();
                    (receipt.id.clone(), receipt.status.as_str(), receipt.version)
                };
                let result = CommandJson::from_serializable(&ReleaseResult {
                    id: &id,
                    status,
                    version,
                })?;
                self.prepared = Some(Prepared::Release(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            QualityOwnerCommand::Scrap(command) => {
                let prepared =
                    prepare_scrap(c, self.current, command, &company, &actor, now).await?;
                let receipt = &prepared.receipt;
                let result = CommandJson::from_serializable(&ScrapResult {
                    id: &receipt.id,
                    status: receipt.status.as_str(),
                    version: receipt.version,
                    count_id: &receipt.count_id,
                })?;
                let version = receipt.version;
                self.prepared = Some(Prepared::Scrap(Box::new(prepared)));
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
                let preparation = lock_quality_recipient_sources(c).await?;
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
            Prepared::Release(prepared) => {
                prepared.apply(c).await?;
                let QualityOwnerCommand::Release(command) = self.command else {
                    return Err(changed().into());
                };
                let request = release_recipient_request(&prepared)?;
                let now = self.write_clock.now().get();
                let recipients =
                    inspect_quality_recipients_for_selected(c, self.current, request, now).await?;
                let facts = release_notification_facts(command, &prepared, &recipients);
                let source = ReleaseNotificationSource {
                    prepared: &prepared,
                    recipients: &recipients,
                    now,
                    facts,
                };
                let retained = stage_quality_notifications(c, &source).await?;
                self.written = Some(Written::Release {
                    prepared,
                    recipients,
                    now,
                    retained,
                });
            }
            Prepared::Scrap(prepared) => {
                prepared.apply(c).await?;
                let retained = stage_scrap_notifications(c, &prepared).await?;
                self.written = Some(Written::Scrap {
                    prepared,
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
            Written::Release {
                prepared,
                recipients,
                now,
                retained,
            } => {
                prepared.require_after(c).await?;
                let QualityOwnerCommand::Release(command) = self.command else {
                    return Err(changed().into());
                };
                let source = ReleaseNotificationSource {
                    prepared,
                    recipients,
                    now: *now,
                    facts: release_notification_facts(command, prepared, recipients),
                };
                retained.require_current(c, &source).await?;
            }
            Written::Scrap {
                prepared,
                retained,
            } => {
                prepared.require_after(c).await?;
                for notification in retained {
                    notification.require_current(c).await?;
                }
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
