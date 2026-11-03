//! Owner-composed Field Collaboration read; sources are inspected and retained
//! in the same selected Holding transaction, never through internal HTTP.
use crate::field_collaboration_command::FieldCollaborationTarget;
use crate::field_service_records_source::PreparedRecordsSource;
use crate::field_service_records_source::{
    FieldCollaborationSources, FieldRecordsSourcePort, prepare_assignee,
};
use crate::field_service_reservation_read::{FieldServiceRenderedContext, context};
use crate::field_service_source_record::SourceRecordContext;
use erpx_server_collaboration::public::collaboration_work::read_field_work_rows;
use erpx_server_identity_access::public::{
    DeviceProof,
    collaboration_candidates::read_collaboration_candidates,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_people_payroll::public::field_service_workspace::read_service_workspace_people;
use erpx_server_records::public::source_record::{
    SourceKind, SourceRecord, SourceReference, prepare_source,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldCollaborationWorkspace {
    pub relations: Vec<Value>,
    pub cards: Vec<Value>,
    pub people: Vec<Value>,
    pub source_types: Vec<&'static str>,
    pub field_only_relations: bool,
    pub card_source_types: Vec<&'static str>,
    pub work: Vec<erpx_server_collaboration::public::collaboration_work::ActionWork>,
    pub can_write: bool,
    pub can_review: bool,
}
#[derive(Serialize)]
pub struct FieldCollaborationReadResponse {
    pub data: FieldCollaborationWorkspace,
    pub context: FieldServiceRenderedContext,
}
pub(crate) async fn visible<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    kind: SourceKind,
    id: &str,
) -> Result<Option<(SourceRecord, PreparedRecordsSource)>, Error> {
    let mut port = FieldRecordsSourcePort {
        connection: c,
        context: SourceRecordContext {
            actor: current,
            holding: &target.holding,
            company: &target.company,
            catalogue: &target.catalogue,
            clock: target.clock.as_ref(),
        },
    };
    match prepare_source(
        &mut port,
        target.company.as_str(),
        &SourceReference {
            kind,
            id: id.into(),
            expected_version: None,
        },
    )
    .await
    {
        Ok(prepared) => Ok(Some(prepared.into_parts())),
        Err(PgTransactionError::Business(e))
            if matches!(e.kind, FailureKind::Forbidden | FailureKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str().ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_WORKSPACE_INVALID",
            "The Collaboration workspace is unavailable.",
        )
        .into()
    })
}
async fn read<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
) -> Result<FieldCollaborationReadResponse, Error> {
    current.require_permission("records.read")?;
    if current.holding_id() != &target.holding
        || current.selection().workspace().company_id != target.company
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COLLABORATION_SCOPE_REQUIRED",
            "Use the current Company.",
        )
        .into());
    }
    let scope = current.selection().workspace();
    let rows = read_field_work_rows(
        c,
        &target.company,
        scope.branch_id.as_ref().map(|v| v.as_str()),
        scope.department_id.as_ref().map(|v| v.as_str()),
    )
    .await?;
    let mut guards = vec![];
    let mut reply_budget = crate::field_service_reservation_read::ReplyBudget(0);
    let mut relations = vec![];
    let mut cards = vec![];
    for row in &rows.relations {
        let left = visible(
            c,
            current,
            target,
            SourceKind::parse(text(row, "source_type")?)?,
            text(row, "source_id")?,
        )
        .await?;
        let right = visible(
            c,
            current,
            target,
            SourceKind::parse(text(row, "target_type")?)?,
            text(row, "target_id")?,
        )
        .await?;
        if let (Some((_, left)), Some((_, right))) = (left, right) {
            guards.extend([left, right]);
            charge(row, &mut reply_budget)?;
            relations.push(row.clone());
        }
    }
    let mut assignees = vec![];
    for row in &rows.cards {
        if let Some((source, guard)) = visible(
            c,
            current,
            target,
            SourceKind::parse(text(row, "source_type")?)?,
            text(row, "source_id")?,
        )
        .await?
        {
            let ctx = SourceRecordContext {
                actor: current,
                holding: &target.holding,
                company: &target.company,
                catalogue: &target.catalogue,
                clock: target.clock.as_ref(),
            };
            let assignee = prepare_assignee(
                c,
                &ctx,
                text(row, "owner_id")?,
                row["branch_id"].as_str().unwrap_or(""),
                row["department_id"].as_str(),
            )
            .await?;
            let mut card = row.clone();
            card["owner_eligible"] = json!(if assignee.eligible() { 1 } else { 0 });
            card["source_title"] = json!(source.title);
            card["source_href"] = json!(format!("/field-service/{}", component(&source.id)));
            assignees.push(assignee);
            guards.push(guard);
            charge(&card, &mut reply_budget)?;
            cards.push(card);
        }
    }
    let people = read_service_workspace_people(
        c,
        &target.company,
        cards
            .iter()
            .map(|r| text(r, "owner_id").map(str::to_owned))
            .collect::<Result<_, _>>()?,
        true,
    )
    .await?;
    let candidates = read_collaboration_candidates(
        c,
        &target.company,
        people
            .data()
            .iter()
            .filter(|p| p.eligible)
            .map(|p| p.user_id.clone())
            .collect(),
        target.clock.now(),
    )
    .await?;
    for card in &mut cards {
        if let Some(person) = people
            .data()
            .iter()
            .find(|p| Some(p.user_id.as_str()) == card["owner_id"].as_str())
        {
            card["owner_name"] = json!(person.name);
        }
    }
    let work = cards
        .iter()
        .map(|row| {
            let row = serde_json::from_value::<
                erpx_server_collaboration::public::collaboration_work::ActionWorkInput,
            >(row.clone())
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "COLLABORATION_WORKSPACE_INVALID",
                    "The Collaboration work source is unavailable.",
                )
            })?;
            Ok(
                erpx_server_collaboration::public::collaboration_work::action_work(
                    &row,
                    current.user_id().as_str(),
                    current.has_permission("records.review"),
                ),
            )
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let data = FieldCollaborationWorkspace {
        work,
        relations,
        cards,
        people: people
            .data()
            .iter()
            .filter(|p| p.eligible && candidates.users().contains(&p.user_id))
            .map(|p| json!({"id":p.user_id,"name":p.name}))
            .collect(),
        source_types: vec!["field_service_order", "support_ticket"],
        field_only_relations: true,
        card_source_types: vec!["field_service_order"],
        can_write: current.has_permission("records.write"),
        can_review: current.has_permission("records.review"),
    };
    let ctx = SourceRecordContext {
        actor: current,
        holding: &target.holding,
        company: &target.company,
        catalogue: &target.catalogue,
        clock: target.clock.as_ref(),
    };
    for guard in guards {
        guard.require_current(c, &ctx).await?;
    }
    for assignee in assignees {
        assignee.require_current(c, target.clock.as_ref()).await?;
    }
    people.require_current(c).await?;
    candidates.require_current(c, target.clock.now()).await?;
    rows.require_current(c).await?;
    require_selected_actor_current(c, current, target.clock.now().get()).await?;
    let response = FieldCollaborationReadResponse {
        data,
        context: context(current)?,
    };
    serde_json::to_writer(
        &mut crate::field_service_reservation_read::ReplyBudget(0),
        &response,
    )
    .map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_WORKSPACE_LIMIT",
            "The Collaboration workspace exceeds this bounded read.",
        )
    })?;
    Ok(response)
}
pub async fn read_selected_field_collaboration<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
) -> Result<FieldCollaborationReadResponse, Error>
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
                if w.company_id != expected {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "COLLABORATION_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                w.mutation = false;
                Ok(w)
            },
            WorkAuthorizationRequest {
                permission: "records.read".into(),
                module: Some("collaboration".into()),
                destination: "collaboration".into(),
                api: true,
            },
            FieldCollaborationSources,
            move |c, current| {
                let target = target.clone();
                Box::pin(async move { read(c, current, &target).await })
            },
        )
        .await
}

pub(crate) fn component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char)
        } else {
            out.push_str(&format!("%{byte:02X}"))
        }
    }
    out
}

fn charge(
    value: &Value,
    budget: &mut crate::field_service_reservation_read::ReplyBudget,
) -> Result<(), Error> {
    serde_json::to_writer(budget, value).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "COLLABORATION_WORKSPACE_LIMIT",
            "The Collaboration workspace exceeds this bounded read.",
        )
        .into()
    })
}
