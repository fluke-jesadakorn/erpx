//! Bind an RFQ receipt comparison to the current Identity-selected actor.
//! This does not authorize replay or perform purchase admission.
use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandReceipt, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseInspection, PurchaseRequest, PurchaseWriteSourceLocks,
};
use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, CommandIamMetadata,
};
use erpx_server_identity_access::public::work_context::WorkScopeKind;
use erpx_server_procurement::public::{
    rfq_transition::RfqTransitionAction, rfq_transition_normalization::NormalizedRfqTransition,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use erpx_server_workflow::public::{
    commands::CommandField,
    rfq_command_receipt::{CurrentRfqReceipt, RfqReceiptMatch},
    source_ordered_json::{ScopedWorkAuthority, SourceOrderedFingerprintActor},
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, PartialEq, Eq)]
struct ActorImage {
    holding: String,
    user: String,
    session: String,
    company: String,
    branch: Option<String>,
    department: Option<String>,
    assignment: String,
    assignment_user: String,
    assignment_company: String,
    role: String,
    assignment_scope: WorkScopeKind,
    assignment_scope_node: Option<String>,
    assignment_valid_from: i64,
    assignment_valid_until: i64,
    assignment_revoked: bool,
    assignment_modules: Vec<String>,
    revision: i64,
    assignment_version: i64,
    expires_at: i64,
    modules: Vec<String>,
    iam: CommandIamMetadata,
}
fn actor_image(current: &AuthorizedWorkContext) -> ActorImage {
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    ActorImage {
        holding: current.holding_id().as_str().into(),
        user: current.user_id().as_str().into(),
        session: current.session_id().as_str().into(),
        company: w.company_id.as_str().into(),
        branch: w.branch_id.as_ref().map(|x| x.as_str().into()),
        department: w.department_id.as_ref().map(|x| x.as_str().into()),
        assignment: a.id.as_str().into(),
        assignment_user: a.user_id.as_str().into(),
        assignment_company: a.company_id.as_str().into(),
        role: a.role.clone(),
        assignment_scope: a.scope_kind,
        assignment_scope_node: a.scope_node_id.as_ref().map(|x| x.as_str().into()),
        assignment_valid_from: a.valid_from,
        assignment_valid_until: a.valid_until,
        assignment_revoked: a.revoked,
        assignment_modules: a.modules.clone(),
        revision: s.revision(),
        assignment_version: a.version,
        expires_at: current.expires_at(),
        modules: current.modules().to_vec(),
        iam: current.command_iam_metadata().clone(),
    }
}
fn require_actor(expected: &ActorImage, current: &AuthorizedWorkContext) -> Result<(), Error> {
    if *expected != actor_image(current) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PROCUREMENT_ACCESS_CHANGED",
            "Choose a current authorized purchasing work role.",
        )
        .into());
    }
    Ok(())
}
/// Pending read-phase decision; it is not the original target/source guard.
pub(crate) struct PendingRfqPurchase {
    actor: ActorImage,
    boundary: PurchaseCommandBoundary,
    read: PurchaseInspection,
}
pub(crate) struct ActiveRfqPurchase {
    actor: ActorImage,
    boundary: PurchaseCommandBoundary,
}

/// The outer coordinator must acquire all owner utility locks before taking any
/// snapshot; a lock witness proves presence and unit, not acquisition timing.
pub(crate) async fn prepare_rfq_purchase(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    mode: PackageEnforcementMode,
    input: &NormalizedRfqTransition,
    receipt: Option<&CurrentRfqReceipt>,
    locks: Option<&PurchaseWriteSourceLocks>,
    catalogue: &CommercialCatalogue,
    boundary_key: &str,
    now: TimestampMillis,
) -> Result<Option<PendingRfqPurchase>, Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let locks = locks.ok_or_else(|| {
        erpx_server_runtime::public::postgres::PersistenceError::new("RFQ_PURCHASE_LOCKS_REQUIRED")
    })?;
    locks.require_current(&mut *c).await?;
    let inspection = inspect_rfq_purchase(&mut *c, current, mode, input, receipt, catalogue, now)
        .await?
        .ok_or_else(|| {
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "RFQ_PURCHASE_INSPECTION_REQUIRED",
            )
        })?;
    let purpose = inspection
        .access
        .first()
        .ok_or_else(|| {
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "RFQ_PURCHASE_INSPECTION_REQUIRED",
            )
        })?
        .purpose;
    let scope = PurchaseCommandScope {
        company_id: current.selection().workspace().company_id.clone(),
        capability: "supply.purchasing".into(),
        purpose,
    };
    let boundary = PurchaseCommandBoundary::new(mode, boundary_key, scope)?.ok_or_else(|| {
        erpx_server_runtime::public::postgres::PersistenceError::new(
            "RFQ_PURCHASE_BOUNDARY_REQUIRED",
        )
    })?;
    locks.require_current(c).await?;
    Ok(Some(PendingRfqPurchase {
        actor: actor_image(current),
        boundary,
        read: inspection,
    }))
}
impl PendingRfqPurchase {
    /// Caller emits the retained shadow read observation before starting the
    /// boundary. This value is scoped decision data, never an authority token.
    pub(crate) fn read_observation(&self) -> &PurchaseInspection {
        &self.read
    }
    /// Call only after the original RFQ target and current scoped permission
    /// checks. Neither this stage nor its actor snapshot proves those checks.
    pub(crate) async fn begin(
        mut self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        locks: &PurchaseWriteSourceLocks,
        catalogue: &CommercialCatalogue,
    ) -> Result<ActiveRfqPurchase, Error> {
        require_actor(&self.actor, current)?;
        locks.require_current(&mut *c).await?;
        self.boundary
            .begin(&mut *c, current.holding_id(), catalogue)
            .await?;
        locks.require_current(c).await?;
        Ok(ActiveRfqPurchase {
            actor: self.actor,
            boundary: self.boundary,
        })
    }
}
impl ActiveRfqPurchase {
    /// Caller revalidates the real RFQ, Records, Identity and Workflow effects
    /// before this final guard and must consume returned shadow telemetry.
    pub(crate) async fn complete(
        mut self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        locks: &PurchaseWriteSourceLocks,
        catalogue: &CommercialCatalogue,
    ) -> Result<PurchaseCommandReceipt, Error> {
        require_actor(&self.actor, current)?;
        locks.require_current(&mut *c).await?;
        let observation = self
            .boundary
            .complete(&mut *c, current.holding_id(), catalogue)
            .await?;
        locks.require_current(c).await?;
        Ok(observation)
    }
}

fn operation(action: RfqTransitionAction) -> &'static str {
    match action {
        RfqTransitionAction::Submit => "procurement.rfq.submit",
        RfqTransitionAction::Approve => "procurement.rfq.approve",
        RfqTransitionAction::Return => "procurement.rfq.return",
        RfqTransitionAction::Cancel => "procurement.rfq.cancel",
    }
}

/// Disabled mode must call this without first inspecting the receipt. Enabled
/// modes classify history by stored operation/actor/company before hash replay,
/// matching the retained purchase wrapper. This is not a purchase decision.
pub(crate) async fn rfq_purchase_purpose(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    mode: PackageEnforcementMode,
    input: &NormalizedRfqTransition,
    receipt: Option<&CurrentRfqReceipt>,
) -> Result<Option<CapabilityPurpose>, Error> {
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let receipt = receipt.ok_or_else(|| {
        erpx_server_runtime::public::postgres::PersistenceError::new(
            "RFQ_PURCHASE_RECEIPT_REQUIRED",
        )
    })?;
    let history = receipt
        .selects_purchase_history(
            c,
            operation(input.action()),
            current.user_id().as_str(),
            current.selection().workspace().company_id.as_str(),
        )
        .await?;
    Ok(Some(if history {
        CapabilityPurpose::History
    } else if input.action() == RfqTransitionAction::Cancel {
        CapabilityPurpose::SettleExisting
    } else {
        CapabilityPurpose::NewWork
    }))
}

/// Read the actual Commercial decision after the owner has taken all source
/// locks in the reviewed pre-snapshot order. This is only the TS read-phase
/// gate: the purchase boundary, shadow telemetry and post-effects checks must
/// still run before any command can commit.
pub(crate) async fn inspect_rfq_purchase(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    mode: PackageEnforcementMode,
    input: &NormalizedRfqTransition,
    receipt: Option<&CurrentRfqReceipt>,
    catalogue: &CommercialCatalogue,
    now: TimestampMillis,
) -> Result<Option<PurchaseInspection>, Error> {
    let Some(purpose) = rfq_purchase_purpose(c, current, mode, input, receipt).await? else {
        return Ok(None);
    };
    let mut reader = PgCompanyPurchaseReader::new(
        current.holding_id().clone(),
        current.selection().workspace().company_id.clone(),
        c,
        catalogue,
    );
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["supply.purchasing".into()],
                purpose,
                mode,
            },
            now,
        )
        .await?;
    if let Some(denied) = inspection
        .access
        .iter()
        .find(|access| !access.allowed_by_mode)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            denied.decision.code.as_code(),
            "This package does not permit the requested work.",
        )
        .into());
    }
    Ok(Some(inspection))
}

/// Caller retains the actual Identity authorizer transaction, revalidates it
/// and performs the original purchase decision before treating a match as replay.
pub(crate) async fn compare_rfq_receipt<'a>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    receipt: &'a CurrentRfqReceipt,
    record_id: &str,
    input: &NormalizedRfqTransition,
) -> Result<RfqReceiptMatch<'a>, Error> {
    receipt.require_current(c).await?;
    let selected = current.selection();
    let assignment = selected.assignment();
    let workspace = selected.workspace();
    let actor = SourceOrderedFingerprintActor {
        user_id: current.user_id().as_str(),
        assignment_id: assignment.id.as_str(),
        role: &assignment.role,
        company_id: workspace.company_id.as_str(),
        branch: workspace
            .branch_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str()))
            .unwrap_or(CommandField::Null),
        department: workspace
            .department_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(ScopedWorkAuthority {
            session_id: current.session_id().as_str(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
    };
    Ok(receipt.compare(&actor, operation(input.action()), record_id, input.json())?)
}

/// Fresh-only observation. A matching historical receipt still needs replay
/// authorization and is never converted to this witness. This is not a
/// purchase, request approval or command-writer grant.
pub(crate) struct FreshRfqReplayWitness<'a> {
    actor: &'a AuthorizedWorkContext,
    receipt: &'a CurrentRfqReceipt,
    input: &'a NormalizedRfqTransition,
    key: String,
    record_id: String,
}
impl FreshRfqReplayWitness<'_> {
    pub(crate) async fn require_for(
        &self,
        c: &mut PgConnection,
        actor: &AuthorizedWorkContext,
        input: &NormalizedRfqTransition,
        key: &str,
        record_id: &str,
    ) -> Result<bool, Error> {
        if !std::ptr::eq(actor, self.actor)
            || !std::ptr::eq(input, self.input)
            || key != self.key
            || record_id != self.record_id
            || !self.receipt.matches_key(key)
        {
            return Ok(false);
        }
        erpx_server_identity_access::public::rfq_request_membership::require_rfq_request_actor_unit(&mut *c,actor).await?;
        Ok(matches!(
            compare_rfq_receipt(c, actor, self.receipt, record_id, input).await?,
            RfqReceiptMatch::Missing
        ))
    }
}
pub(crate) async fn fresh_rfq_replay_witness<'a>(
    c: &mut PgConnection,
    actor: &'a AuthorizedWorkContext,
    receipt: &'a CurrentRfqReceipt,
    input: &'a NormalizedRfqTransition,
    key: &str,
    record_id: &str,
) -> Result<Option<FreshRfqReplayWitness<'a>>, Error> {
    let witness = FreshRfqReplayWitness {
        actor,
        receipt,
        input,
        key: key.into(),
        record_id: record_id.into(),
    };
    if witness.require_for(c, actor, input, key, record_id).await? {
        Ok(Some(witness))
    } else {
        Ok(None)
    }
}

/// Combines only the historical assertion and fresh receipt observations. It
/// remains non-authorizing and cannot be passed to an owner writer.
pub(crate) async fn rfq_observations_match(
    c: &mut PgConnection,
    fresh: &FreshRfqReplayWitness<'_>,
    assertion: &crate::procurement_rfq_assertions::RfqAssertionWitness<'_>,
    actor: &AuthorizedWorkContext,
    target: &erpx_server_procurement::public::rfq_target_source::CurrentRfqTarget,
    input: &NormalizedRfqTransition,
    key: &str,
) -> bool {
    matches!(
        fresh
            .require_for(&mut *c, actor, input, key, &target.facts().id)
            .await,
        Ok(true)
    ) && assertion
        .require_for(c, actor, target, input, key)
        .await
        .is_ok()
}
