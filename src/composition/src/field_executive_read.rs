//! Current Field reports and evidence. Owning facts, Records, IAM and labels
//! remain inside the selected native Holding unit; this is not an HTTP guard.
use crate::{
    field_collaboration_command::FieldCollaborationTarget, field_collaboration_read::visible,
    field_service_records_source::FieldCollaborationSources,
    field_service_source_record::SourceRecordContext,
};
use erpx_server_field_service::public::executive::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_organization::public::field_service_workspace::read_service_report_node_names;
use erpx_server_people_payroll::public::service_count_flow_people::read_service_count_member_names;
use erpx_server_records::public::source_record::SourceKind;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
type Error = PgTransactionError<BusinessError>;
#[derive(Serialize)]
pub struct FieldExecutiveReadResponse {
    pub data: Value,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
pub(crate) struct PreparedFieldExecutiveRead {
    pub response: FieldExecutiveReadResponse,
    guard: Option<FieldExecutiveReadGuard>,
}
struct FieldExecutiveReadGuard {
    candidates: CurrentFieldExecutiveCandidates,
    sources: Vec<crate::field_service_records_source::PreparedRecordsSource>,
    nodes: Vec<String>,
    people: Vec<String>,
    node_names: Vec<(String, String)>,
    person_names: Vec<(String, String)>,
}
impl PreparedFieldExecutiveRead {
    pub(crate) async fn require_current<C: Clock + Send + Sync>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        target: &FieldCollaborationTarget<C>,
    ) -> Result<(), Error> {
        if let Some(g) = &self.guard {
            let ctx = SourceRecordContext {
                purchase_locks: None,
                actor: current,
                holding: &target.holding,
                company: &target.company,
                catalogue: &target.catalogue,
                clock: target.clock.as_ref(),
            };
            for source in &g.sources {
                source.require_current(c, &ctx).await?;
            }
            g.candidates.require_current(c).await?;
            if read_service_report_node_names(c, &target.company, &g.nodes).await? != g.node_names
                || read_service_count_member_names(c, &target.company, &g.people).await?
                    != g.person_names
            {
                return Err(invalid().into());
            }
        }
        require_selected_actor_current(c, current, target.clock.now().get()).await?;
        Ok(())
    }
}
fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_REPORT_SOURCE_INVALID",
        "The current Field report is unavailable.",
    )
}
pub(crate) async fn prepare_read<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    query: FieldExecutiveQuery,
    record_id: Option<&str>,
) -> Result<PreparedFieldExecutiveRead, Error> {
    if current.holding_id() != &target.holding
        || current.selection().workspace().company_id != target.company
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "FIELD_REPORT_SCOPE_REQUIRED",
            "Use the current Holding and Company.",
        )
        .into());
    }
    query.validate()?;
    if query.access_only {
        use erpx_server_commercial::public::purchases::{
            CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
        };
        let mut reader = PgCompanyPurchaseReader::new(
            target.holding.clone(),
            target.company.clone(),
            c,
            &target.catalogue,
        );
        let purchase = reader
            .inspect(
                &PurchaseRequest {
                    capabilities: vec!["service.orders".into()],
                    purpose: CapabilityPurpose::History,
                    mode: PackageEnforcementMode::Enforced,
                },
                target.clock.now(),
            )
            .await?;
        let allowed = purchase.require_allowed().is_ok();
        let current_purchase = reader
            .revalidate(&purchase.witness, target.clock.now())
            .await?;
        if current_purchase.require_allowed().is_ok() != allowed {
            return Err(invalid().into());
        }
        require_selected_actor_current(c, current, target.clock.now().get()).await?;
        return Ok(PreparedFieldExecutiveRead {
            response: FieldExecutiveReadResponse {
                data: json!({"available":allowed}),
                context: crate::field_service_reservation_read::context(current)?,
            },
            guard: None,
        });
    }
    let scope = current.selection().workspace();
    let candidates = read_field_evidence_candidates(
        c,
        &target.company,
        scope.branch_id.as_ref().map(|v| v.as_str()),
        scope.department_id.as_ref().map(|v| v.as_str()),
        query.clone(),
        record_id,
    )
    .await?;
    let mut records = Vec::new();
    let mut guards = Vec::new();
    for row in candidates.data() {
        if let Some((source, guard)) =
            visible(c, current, target, SourceKind::FieldServiceOrder, &row.id).await?
        {
            if source.version != row.version || source.status != row.status {
                return Err(invalid().into());
            }
            records.push(row);
            guards.push(guard);
        }
    }
    let nodes: Vec<String> = match query.breakdown {
        FieldBreakdown::Branch => records
            .iter()
            .map(|r| r.branch_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        FieldBreakdown::Department => records
            .iter()
            .filter_map(|r| r.department_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
    let people: Vec<String> = if query.breakdown == FieldBreakdown::Technician {
        records
            .iter()
            .map(|r| r.technician_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    if people.len() > 500 {
        return Err(invalid().into());
    }
    let node_names = read_service_report_node_names(c, &target.company, &nodes).await?;
    let person_names = read_service_count_member_names(c, &target.company, &people).await?;
    let node_map: BTreeMap<_, _> = node_names.iter().cloned().collect();
    let mut person_map = BTreeMap::new();
    for (id, name) in &person_names {
        person_map.entry(id.clone()).or_insert(name.clone());
    }
    let labels = records
        .iter()
        .map(|row| match query.breakdown {
            FieldBreakdown::Branch => node_map
                .get(&row.branch_id)
                .cloned()
                .unwrap_or("Branch".into()),
            FieldBreakdown::Department => row
                .department_id
                .as_ref()
                .and_then(|id| node_map.get(id))
                .cloned()
                .unwrap_or("Unassigned".into()),
            FieldBreakdown::Technician => person_map
                .get(&row.technician_id)
                .cloned()
                .unwrap_or("Unassigned".into()),
            FieldBreakdown::Month => row.month.clone().unwrap_or_default(),
            FieldBreakdown::Status => row.status.clone(),
            FieldBreakdown::Default => match query.metric {
                FieldMetric::Open => row.status.clone(),
                FieldMetric::Billing => row.month.clone().unwrap_or_default(),
                FieldMetric::Completion => "Field visits".into(),
            },
        })
        .collect::<Vec<_>>();
    let data = if let Some(limit) = query.evidence_limit {
        let rows=records.iter().take(limit+1).map(|r|json!({"id":r.id,"scheduled_at":r.scheduled_at,"visit_completed_at":r.visit_completed_at,"status":r.status,"version":r.version})).collect::<Vec<_>>();
        json!({"evidence":rows})
    } else {
        let raw = aggregate(
            query.metric,
            labels
                .iter()
                .zip(&records)
                .map(|(label, row)| (label.as_str(), *row)),
        );
        let raw = order_field_metric_rows(c, raw).await?;
        let mut ordered_sources = labels.iter().zip(&records).collect::<Vec<_>>();
        ordered_sources.sort_by_key(|(_, row)| row.source_order);
        let sources=ordered_sources.into_iter().map(|(label,r)|json!({"label":label,"id":r.id,"facts":{"id":r.id,"status":r.status,"version":r.version,"created_at":r.created_at,"updated_at":r.updated_at},"owner_kind":"field_service","owner_id":r.id,"owner_args":null})).collect::<Vec<_>>();
        json!({"raw":raw,"sources":sources})
    };
    let ctx = SourceRecordContext {
        purchase_locks: None,
        actor: current,
        holding: &target.holding,
        company: &target.company,
        catalogue: &target.catalogue,
        clock: target.clock.as_ref(),
    };
    for guard in &guards {
        guard.require_current(c, &ctx).await?;
    }
    candidates.require_current(c).await?;
    if read_service_report_node_names(c, &target.company, &nodes).await? != node_names
        || read_service_count_member_names(c, &target.company, &people).await? != person_names
    {
        return Err(invalid().into());
    }
    require_selected_actor_current(c, current, target.clock.now().get()).await?;
    let response = FieldExecutiveReadResponse {
        data,
        context: crate::field_service_reservation_read::context(current)?,
    };
    serde_json::to_writer(
        &mut crate::field_service_reservation_read::ReplyBudget(0),
        &response,
    )
    .map_err(|_| invalid())?;
    Ok(PreparedFieldExecutiveRead {
        response,
        guard: Some(FieldExecutiveReadGuard {
            candidates,
            sources: guards,
            nodes,
            people,
            node_names,
            person_names,
        }),
    })
}
pub async fn read_selected_field_executive<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    query: FieldExecutiveQuery,
) -> Result<FieldExecutiveReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let expected = target.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut w = workspace()?;
                w.mutation = false;
                if w.company_id != expected {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "FIELD_REPORT_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(w)
            },
            WorkAuthorizationRequest {
                permission: "records.read".into(),
                module: Some("field_service".into()),
                destination: "executive/field-service".into(),
                api: true,
            },
            FieldCollaborationSources,
            move |c, current| {
                let target = target.clone();
                let query = query.clone();
                Box::pin(async move {
                    Ok(prepare_read(c, current, &target, query, None)
                        .await?
                        .response)
                })
            },
        )
        .await
}
