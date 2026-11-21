//! Unmounted Records detail preparation. Later quotation gates and the full
//! authority/source transaction are deliberately not satisfied by this type.
use crate::procurement_rfq_access::{
    RfqInitialRequestAccess, RfqRequestDetailEvent, RfqRequestTeamHandler,
    initial_rfq_request_access, rfq_request_detail_events, validate_rfq_request_service_scope,
};
use crate::procurement_rfq_quantity::{RfqItemMapError, RfqMappedItems, rfq_quotation_items_json};
use erpx_server_files::public::rfq_request_attachments::{
    RfqRequestAttachment, read_rfq_request_attachments,
};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_identity_access::public::rfq_request_membership::require_rfq_request_actor_unit;
use erpx_server_procurement::public::rfq_request_allocations::read_rfq_request_allocations;
use erpx_server_procurement::public::rfq_target_source::CurrentRfqTarget;
use erpx_server_records::public::rfq_request_source::{
    CurrentRfqQuotationRow, RfqRequestChild, observe_rfq_quotation_row, read_rfq_request_children,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use erpx_server_support::public::rfq_request_access::rfq_request_team_name;
use erpx_server_workflow::public::source_ordered_json::SourceOrderedJson;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;

/// None of these outcomes grants request access or authorizes a command.
pub(crate) enum RfqDetailPreparation<'a> {
    NotFound,
    JsonSyntax,
    PropertyTypeError,
    RuntimeCoercionUnproven,
    DatabaseNumberOutOfRange,
    NumericMaterializationPending,
    DepartmentPolicyRequired,
    ApprovalRouteRequired,
    ChildAccessRequired,
    QuotationChecksRequired(RfqRequestDetailFacts<'a>),
}
pub(crate) struct RfqRequestDetailFacts<'a> {
    current: &'a AuthorizedWorkContext,
    target: &'a CurrentRfqTarget,
    source: &'a CurrentRfqQuotationRow,
    items_json: String,
    owner: bool,
    handlers: Vec<RfqRequestTeamHandler>,
    events: Vec<RfqRequestDetailEvent>,
    team_name: Option<String>,
    children: Vec<RfqRequestChild>,
    attachments: Vec<RfqRequestAttachment>,
}
impl RfqRequestDetailFacts<'_> {
    pub(crate) fn items_json(&self) -> &str {
        &self.items_json
    }
    pub(crate) fn owner(&self) -> bool {
        self.owner
    }
    pub(crate) fn handlers(&self) -> &[RfqRequestTeamHandler] {
        &self.handlers
    }
    pub(crate) fn events(&self) -> &[RfqRequestDetailEvent] {
        &self.events
    }
    pub(crate) fn team_name(&self) -> Option<&str> {
        self.team_name.as_deref()
    }
    pub(crate) fn children(&self) -> &[RfqRequestChild] {
        &self.children
    }
    pub(crate) fn attachments(&self) -> &[RfqRequestAttachment] {
        &self.attachments
    }
}

pub(crate) enum RfqQuotationGate {
    NotEligible,
    AllocationRequired,
}

/// This is still an observation, not permission to commit or a route result.
/// The returned mapping must be retained alongside the real source witnesses;
/// its serialized JSON must not be parsed back for in-memory decisions.
pub(crate) enum RfqQuotationItemsStage<'a> {
    NotEligible,
    MappingError(RfqItemMapError),
    Ready(RfqPreparedItems<'a>),
}
pub(crate) struct RfqPreparedItems<'a> {
    detail: &'a RfqRequestDetailFacts<'a>,
    mapped: RfqMappedItems,
}
impl RfqPreparedItems<'_> {
    pub(crate) fn mapped(&self) -> &RfqMappedItems {
        &self.mapped
    }
    /// Re-read the actual allocation query and compare its JS-observable item
    /// result. Team/People/Organization and full command fences remain caller
    /// responsibilities; this witness alone never grants authority.
    pub(crate) async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_rfq_request_actor_unit(&mut *c, self.detail.current).await?;
        self.detail.source.require_current(&mut *c).await?;
        self.detail.target.require_current(&mut *c).await?;
        let rows = read_rfq_request_allocations(&mut *c, self.detail.source.id()).await?;
        let next = rfq_quotation_items_json(&self.detail.items_json, &rows)
            .map_err(|_| PersistenceError::new("RFQ_QUOTATION_ITEMS_CHANGED"))?;
        if !self.mapped.equivalent(&next) {
            return Err(PersistenceError::new("RFQ_QUOTATION_ITEMS_CHANGED").into());
        }
        self.detail.source.require_current(&mut *c).await?;
        self.detail.target.require_current(c).await
    }
}

/// Run the actual Procurement allocation read only after quotation gate and
/// scope validation; then apply full item spread and typed-unit mapping.
pub(crate) async fn prepare_rfq_quotation_items<'a>(
    c: &mut PgConnection,
    detail: &'a RfqRequestDetailFacts<'a>,
) -> Result<RfqQuotationItemsStage<'a>, Error> {
    match prepare_rfq_quotation_gate(&mut *c, detail).await? {
        RfqQuotationGate::NotEligible => return Ok(RfqQuotationItemsStage::NotEligible),
        RfqQuotationGate::AllocationRequired => {}
    }
    let rows = read_rfq_request_allocations(&mut *c, detail.source.id()).await?;
    let mapped = match rfq_quotation_items_json(&detail.items_json, &rows) {
        Ok(value) => value,
        Err(error) => return Ok(RfqQuotationItemsStage::MappingError(error)),
    };
    require_rfq_request_actor_unit(&mut *c, detail.current).await?;
    detail.source.require_current(&mut *c).await?;
    detail.target.require_current(c).await?;
    Ok(RfqQuotationItemsStage::Ready(RfqPreparedItems {
        detail,
        mapped,
    }))
}

/// Execute only after a complete detail result at the source's original
/// quotation gate. Beta is the trusted admission-time Identity snapshot. A
/// successful gate still requires allocation, item mapping and same-unit
/// source/actor/post-effect fences before it may affect any command.
pub(crate) async fn prepare_rfq_quotation_gate(
    c: &mut PgConnection,
    detail: &RfqRequestDetailFacts<'_>,
) -> Result<RfqQuotationGate, Error> {
    let current = detail.current;
    let source = detail.source;
    require_rfq_request_actor_unit(&mut *c, current).await?;
    detail.target.require_current(&mut *c).await?;
    source.require_current(&mut *c).await?;
    let row: serde_json::Value = serde_json::from_str(
        source
            .image_json()
            .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?,
    )
    .map_err(|_| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    if current.beta() || row.get("kind").and_then(|v| v.as_str()) != Some("purchase") {
        return Ok(RfqQuotationGate::NotEligible);
    }
    let approval = row
        .get("approval_json")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    let parsed = SourceOrderedJson::parse(approval)
        .map_err(|_| PersistenceError::new("RFQ_APPROVAL_ALREADY_VALIDATED"))?;
    let approved: Vec<u16> = "approved".encode_utf16().collect();
    if parsed.root().property("decision").and_then(|v| v.utf16()) != Some(approved.as_slice()) {
        return Ok(RfqQuotationGate::NotEligible);
    }
    let member = detail
        .handlers
        .iter()
        .any(|handler| handler.user_id == current.user_id().as_str());
    let handler = member
        && row
            .get("assignee_user_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| current.same_person_user_raw(id));
    if !handler || row.get("state").and_then(|v| v.as_str()) != Some("in_progress") {
        return Ok(RfqQuotationGate::NotEligible);
    }
    validate_rfq_request_service_scope(&mut *c, current, source).await?;
    source.require_current(&mut *c).await?;
    detail.target.require_current(c).await?;
    Ok(RfqQuotationGate::AllocationRequired)
}

/// Enabled-mode preparation with a same-unit selected actor, RFQ target and
/// Records row. Before command use, the caller must retain/revalidate all
/// relevant owner sources, purchase admission and post-effect fences. Disabled
/// mode requires its own original non-wrapper path and cannot call this seam.
pub(crate) async fn prepare_rfq_request_detail_enabled<'a>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    target: &'a CurrentRfqTarget,
    source: &'a CurrentRfqQuotationRow,
    policy_now: i64,
) -> Result<RfqDetailPreparation<'a>, Error> {
    use RfqDetailPreparation::*;
    require_rfq_request_actor_unit(&mut *c, current).await?;
    target.require_current(&mut *c).await?;
    let target_image: serde_json::Value = serde_json::from_str(&target.facts().image_json)
        .map_err(|_| PersistenceError::new("RFQ_TARGET_IMAGE_INVALID"))?;
    if !target.facts().within_selected_scope(current)
        || source.company() != &current.selection().workspace().company_id
        || target_image
            .get("record")
            .and_then(|v| v.get("request_id"))
            .and_then(|v| v.as_str())
            != Some(source.id())
    {
        return Err(PersistenceError::new("RFQ_REQUEST_TARGET_MISMATCH").into());
    }
    use crate::procurement_rfq_materialization::{RfqMaterialization, rfq_records_materialization};
    match rfq_records_materialization(&mut *c, source.id(), source.company().as_str(), true).await?
    {
        None => return Ok(NotFound),
        Some(RfqMaterialization::NumberOutOfRange) => return Ok(DatabaseNumberOutOfRange),
        Some(RfqMaterialization::UnsupportedNumeric) => return Ok(NumericMaterializationPending),
        Some(RfqMaterialization::Safe) => {}
    }
    let (owner, handlers) = match initial_rfq_request_access(&mut *c, current, source, policy_now)
        .await?
    {
        RfqInitialRequestAccess::Missing | RfqInitialRequestAccess::NotFound => {
            return Ok(NotFound);
        }
        RfqInitialRequestAccess::JsonSyntax => return Ok(JsonSyntax),
        RfqInitialRequestAccess::PropertyTypeError => return Ok(PropertyTypeError),
        RfqInitialRequestAccess::RuntimeCoercionUnproven => return Ok(RuntimeCoercionUnproven),
        RfqInitialRequestAccess::DepartmentPolicyRequired => return Ok(DepartmentPolicyRequired),
        RfqInitialRequestAccess::ApprovalRouteRequired { .. } => return Ok(ApprovalRouteRequired),
        RfqInitialRequestAccess::DetailRequired { owner, handlers } => (owner, handlers),
    };
    let row: serde_json::Value = serde_json::from_str(
        source
            .image_json()
            .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?,
    )
    .map_err(|_| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    let company = current.selection().workspace().company_id.as_str();
    let events = rfq_request_detail_events(&mut *c, company, source.id()).await?;
    let team_name = match row
        .get("team_id")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        Some(team) => rfq_request_team_name(&mut *c, company, team).await?,
        None => None,
    };
    let candidates = read_rfq_request_children(&mut *c, source.company(), source.id()).await?;
    let mut children = Vec::new();
    for child in candidates {
        let witness = observe_rfq_quotation_row(&mut *c, source.company(), &child.id).await?;
        match rfq_records_materialization(&mut *c, &child.id, source.company().as_str(), true)
            .await?
        {
            None => continue,
            Some(RfqMaterialization::NumberOutOfRange) => return Ok(DatabaseNumberOutOfRange),
            Some(RfqMaterialization::UnsupportedNumeric) => {
                return Ok(NumericMaterializationPending);
            }
            Some(RfqMaterialization::Safe) => {}
        }
        match initial_rfq_request_access(&mut *c, current, &witness, policy_now).await? {
            RfqInitialRequestAccess::Missing | RfqInitialRequestAccess::NotFound => {}
            RfqInitialRequestAccess::DetailRequired { .. } => children.push(child),
            RfqInitialRequestAccess::JsonSyntax => return Ok(JsonSyntax),
            RfqInitialRequestAccess::PropertyTypeError => return Ok(PropertyTypeError),
            RfqInitialRequestAccess::RuntimeCoercionUnproven => return Ok(RuntimeCoercionUnproven),
            RfqInitialRequestAccess::DepartmentPolicyRequired
            | RfqInitialRequestAccess::ApprovalRouteRequired { .. } => {
                return Ok(ChildAccessRequired);
            }
        }
    }
    let items_json = row
        .get("items_json")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PersistenceError::new("RFQ_REQUEST_IMAGE_INVALID"))?;
    if SourceOrderedJson::parse(items_json).is_err() {
        return Ok(JsonSyntax);
    }
    let attachments = read_rfq_request_attachments(&mut *c, source.id()).await?;
    source.require_current(&mut *c).await?;
    target.require_current(c).await?;
    Ok(QuotationChecksRequired(RfqRequestDetailFacts {
        current,
        target,
        source,
        items_json: items_json.into(),
        owner,
        handlers,
        events,
        team_name,
        children,
        attachments,
    }))
}
