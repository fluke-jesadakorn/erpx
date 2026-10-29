//! Composition of genuine Stock Count Flow participant facts. Each projection
//! remains a current same-unit owner witness; no request supplies role grants.
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    service_count_flow_participants::{
        current_journal_flow_principal, service_count_flow_active_members,
        service_count_flow_reviewers,
    },
    service_initial_count_authority::CurrentServiceCountAuthority,
};
use erpx_server_organization::public::journal_flow_department::journal_flow_team_name;
use erpx_server_people_payroll::public::service_count_flow_people::{
    current_journal_flow_people, read_service_count_member_names,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::stock_count_flow::{
    StockCountFlowBasis, StockCountFlowRecord, StockCountFlowView, StockCountParticipant,
    StockCountStep, StockCountStepFacts, read_stock_count_flow_basis, resolve_stock_count_flow,
};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_PRIMARY_REQUIRED",
        "The current Count Flow participant source changed.",
    )
    .into()
}
#[derive(Clone, PartialEq, Eq)]
struct Participants {
    facts: [StockCountStepFacts; 3],
    teams: BTreeMap<String, Option<String>>,
}
async fn participants(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    record: &StockCountFlowRecord,
    basis: &StockCountFlowBasis,
    now: i64,
) -> Result<Participants, Error> {
    let company = &current.selection().workspace().company_id;
    let staff = current_journal_flow_people(c, company).await?;
    if staff.len() > 500 {
        return Err(changed());
    }
    let members = service_count_flow_active_members(c, company, now).await?;
    let names = read_service_count_member_names(c, company, &members).await?;
    let mut known = BTreeMap::new();
    let mut teams = BTreeMap::new();
    for p in &staff {
        let name = journal_flow_team_name(c, company, p.department_id.as_deref()).await?;
        known.insert(
            p.user_id.clone(),
            StockCountParticipant {
                user_id: p.user_id.clone(),
                name: p.name.clone(),
                team_name: name,
            },
        );
        teams.insert(p.user_id.clone(), p.department_id.clone());
    }
    let member_names: BTreeMap<String, String> = names.into_iter().collect();
    for id in members {
        known
            .entry(id.clone())
            .or_insert_with(|| StockCountParticipant {
                user_id: id.clone(),
                name: member_names.get(&id).cloned().unwrap_or_else(|| id.clone()),
                team_name: "Company".into(),
            });
        teams.entry(id).or_insert(None);
    }
    let staff_ids: Vec<String> = staff.iter().map(|p| p.user_id.clone()).collect();
    let raw = service_count_flow_reviewers(
        c,
        company,
        &staff_ids,
        &record.creator,
        &record.preparer,
        now,
    )
    .await?;
    let mut reviewers = Vec::new();
    let mut seen = BTreeSet::new();
    for id in raw {
        let canonical = current_journal_flow_principal(c, company, &id).await?;
        if seen.insert(canonical.clone())
            && let Some(person) = known.get(&canonical)
        {
            reviewers.push(person.clone());
        }
    }
    let owner_id = current_journal_flow_principal(c, company, &record.creator).await?;
    let draft = known
        .get(&owner_id)
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let preferred = [
        Some(record.creator.as_str()),
        None,
        record.approved_by.as_deref(),
    ];
    let mut selected: Vec<Option<String>> = Vec::new();
    for (i, step) in [
        StockCountStep::Draft,
        StockCountStep::Submitted,
        StockCountStep::Approved,
    ]
    .into_iter()
    .enumerate()
    {
        let v = basis.selected_user(step, preferred[i])?;
        selected.push(match v {
            Some(v) => Some(current_journal_flow_principal(c, company, &v).await?),
            None => None,
        });
    }
    let draft_role = [
        "workflow.warehouse_manager",
        "workflow.operations_manager",
        "workflow.holding_owner",
    ]
    .iter()
    .any(|p| current.has_permission(p));
    let review_role = ["workflow.finance_approver", "workflow.holding_owner"]
        .iter()
        .any(|p| current.has_permission(p));
    Ok(Participants {
        facts: [
            StockCountStepFacts {
                step: StockCountStep::Draft,
                candidates: draft,
                selected: selected[0].clone(),
                actor_role_allowed: draft_role,
            },
            StockCountStepFacts {
                step: StockCountStep::Submitted,
                candidates: reviewers.clone(),
                selected: selected[1].clone(),
                actor_role_allowed: review_role,
            },
            StockCountStepFacts {
                step: StockCountStep::Approved,
                candidates: reviewers,
                selected: selected[2].clone(),
                actor_role_allowed: review_role,
            },
        ],
        teams,
    })
}
pub struct CurrentServiceCountFlow {
    pub basis: StockCountFlowBasis,
    pub view: StockCountFlowView,
    participants: Participants,
}
impl CurrentServiceCountFlow {
    /// Recheck external participant facts after an expected Flow route/event
    /// mutation. The Workflow owner's planned witness checks that mutation;
    /// only its selection may legitimately differ from this earlier view.
    pub async fn require_participants_after<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        clock: &C,
    ) -> Result<(), Error> {
        let record = &self.view.record;
        let basis =
            read_stock_count_flow_basis(c, &record.company_id, &record.count_id, &record.branch_id)
                .await?;
        let after = participants(c, current, record, &basis, clock.now().get()).await?;
        if after.teams != self.participants.teams
            || after
                .facts
                .iter()
                .zip(&self.participants.facts)
                .any(|(a, b)| {
                    a.step != b.step
                        || a.candidates != b.candidates
                        || a.actor_role_allowed != b.actor_role_allowed
                })
        {
            return Err(changed());
        }
        basis.require_current(c).await?;
        Ok(())
    }

    pub async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        clock: &C,
    ) -> Result<(), Error> {
        self.basis.require_current(c).await?;
        if participants(
            c,
            current,
            &self.view.record,
            &self.basis,
            clock.now().get(),
        )
        .await?
            != self.participants
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_count_flow<C: Clock>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    authority: &CurrentServiceCountAuthority<'_>,
    clock: &C,
    record: StockCountFlowRecord,
    purchase_allowed: bool,
) -> Result<CurrentServiceCountFlow, Error> {
    let basis =
        read_stock_count_flow_basis(c, &record.company_id, &record.count_id, &record.branch_id)
            .await?;
    let facts = participants(c, current, &record, &basis, clock.now().get()).await?;
    let view = resolve_stock_count_flow(
        record,
        basis.version(),
        basis.defaults_revision(),
        facts.facts.clone(),
        current.user_id().as_str(),
        authority.company_admin(),
        purchase_allowed,
    );
    Ok(CurrentServiceCountFlow {
        basis,
        view,
        participants: facts,
    })
}
impl CurrentServiceCountFlow {
    pub fn wire(&self) -> Result<serde_json::Value, Error> {
        let events: Vec<serde_json::Value> =
            serde_json::from_str(self.basis.events_json()).map_err(|_| changed())?;
        let history:Vec<serde_json::Value>=events.iter().map(|e|serde_json::json!({"action":e["action"],"actorName":e["actor_name"],"at":e["created_at"],"reason":e["reason"],"step":e["step_key"]})).collect();
        let person = |p: &StockCountParticipant| serde_json::json!({"userId":p.user_id,"name":p.name,"teamId":self.participants.teams.get(&p.user_id).cloned().flatten(),"teamName":p.team_name});
        let steps:Vec<serde_json::Value>=self.view.steps.iter().map(|s|{
   let (action,role,roles)=match s.step{StockCountStep::Draft=>("Prepare and submit warehouse document","warehouse_manager",vec!["warehouse_manager","operations_manager","holding_owner"]),StockCountStep::Submitted=>("Approve stock count for GL review","finance_approver",vec!["finance_approver","holding_owner"]),StockCountStep::Approved=>("Review Debit / Credit and post stock adjustment","finance_approver",vec!["finance_approver","holding_owner"])};
   let status=if self.view.current==Some(s.step){"current"}else if events.iter().any(|e|e["step_key"].as_str()==Some(s.step.key())&&!matches!(e["action"].as_str(),Some("assign"|"configure"))){"recorded"}else{"upcoming"};
   serde_json::json!({"key":s.step.key(),"action":action,"role":role,"allowedRoles":roles,"teamName":s.team_name,"primary":s.decision.primary.as_ref().map(person),"candidates":s.candidates.iter().map(&person).collect::<Vec<_>>(),"status":status,"blocker":s.decision.blocker})
  }).collect();
        let current = self.view.current.map(|s| steps[s.index()].clone());
        let next = self
            .view
            .current
            .and_then(|s| steps.get(s.index() + 1))
            .cloned();
        Ok(
            serde_json::json!({"kind":"stock_count","recordId":self.view.record.count_id,"recordVersion":self.view.record.version,"version":self.view.flow_version,"defaultsRevision":self.view.defaults_revision,"state":self.view.record.status,"steps":steps,"current":current,"next":next,"canAct":self.view.can_act,"canManage":self.view.can_manage,"blocker":self.view.blocker,"history":history}),
        )
    }
}
