//! Composition of owner-sourced current reviewer facts with Workflow's
//! retained route and event. No owner is permitted to borrow another's SQL.
use erpx_server_finance::public::journal_review::{JournalReviewCommand, ReviewJournal};
use erpx_server_identity_access::public::{
    effective_authority::AuthorizedWorkContext,
    journal_flow_participants::{
        current_journal_flow_principal, eligible_journal_flow_participant,
    },
};
use erpx_server_organization::public::journal_flow_department::journal_flow_team_name;
use erpx_server_people_payroll::public::journal_flow_people::current_journal_flow_people;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use erpx_server_workflow::public::journal_post_flow::{
    JournalFlowCandidate, JournalPostFlowRequest, JournalPostFlowWitness, read_journal_flow_basis,
};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_PRIMARY_REQUIRED",
        "The current journal reviewer route changed.",
    )
    .into()
}
async fn candidates(
    c: &mut PgConnection,
    company: &CompanyId,
    creator: &str,
) -> Result<Vec<JournalFlowCandidate>, Error> {
    let mut candidates = Vec::new();
    for person in current_journal_flow_people(c, company).await? {
        if eligible_journal_flow_participant(c, company, &person.user_id, creator).await? {
            candidates.push(JournalFlowCandidate {
                user_id: person.user_id,
                name: person.name,
                team_name: journal_flow_team_name(c, company, person.department_id.as_deref())
                    .await?,
            });
        }
    }
    Ok(candidates)
}
pub(super) struct JournalPostFlow {
    witness: JournalPostFlowWitness,
    company: CompanyId,
    creator: String,
    people: Vec<JournalFlowCandidate>,
    preferred: Option<String>,
    mapped: Option<String>,
}
impl JournalPostFlow {
    pub async fn prepare(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        journal: &ReviewJournal,
        command: &JournalReviewCommand,
        now: i64,
    ) -> Result<Self, Error> {
        if !matches!(journal.source_type.as_str(), "manual" | "opening_import") {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "JOURNAL_FLOW_SOURCE_UNSUPPORTED",
                "This journal source is not available for native posting.",
            )
            .into());
        }
        let company = current.selection().workspace().company_id.clone();
        let people = candidates(c, &company, &journal.created_by).await?;
        let basis = read_journal_flow_basis(c, &company, &journal.id).await?;
        let preferred = basis.preferred_primary()?;
        let mapped = match &preferred {
            Some(id) => Some(current_journal_flow_principal(c, &company, id).await?),
            None => None,
        };
        let actor = current.user_id().as_str();
        let name = people
            .iter()
            .find(|p| p.user_id == actor)
            .map(|p| p.name.as_str())
            .unwrap_or(actor);
        let permitted = current.has_permission("workflow.finance_approver")
            || current.has_permission("workflow.holding_owner");
        let event = uuid::Uuid::new_v4().to_string();
        let witness = JournalPostFlowWitness::prepare(
            basis,
            JournalPostFlowRequest {
                candidates: &people,
                mapped_preferred: mapped.as_deref(),
                actor,
                actor_name: name,
                actor_can_participate: permitted,
                command: command.key.as_str(),
                event: &event,
                version: journal.version + 1,
                now,
            },
        )?;
        Ok(Self {
            witness,
            company,
            creator: journal.created_by.clone(),
            people,
            preferred,
            mapped,
        })
    }
    pub async fn before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.witness.apply_before(c).await
    }
    pub async fn after(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.witness.apply_after(c).await
    }
    pub async fn retain(&self, c: &mut PgConnection) -> Result<(), Error> {
        if candidates(c, &self.company, &self.creator).await? != self.people {
            return Err(changed());
        }
        let mapped = match &self.preferred {
            Some(id) => Some(current_journal_flow_principal(c, &self.company, id).await?),
            None => None,
        };
        if mapped != self.mapped {
            return Err(changed());
        }
        self.witness.retain(c).await
    }
}
