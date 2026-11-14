//! Native Benefits enrollment workspace read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Package history and People worker/name facts arrive through
//! public owner ports supplied by composition; this owner never reads another
//! owner's tables. The application enforces the current actor policy, current
//! package history entitlement, worker scope and every bounded
//! plan/window/dependent/enrollment row before presentation.
use crate::domain::workspace::{
    BenefitDependentRow, BenefitElectionDetail, BenefitElectionDetailSource,
    BenefitElectionEventRow, BenefitElectionRow, BenefitPayrollInputRow, BenefitPlanRow,
    BenefitWindowRow, BenefitsActor, BenefitsEntitlement, BenefitsWorker, BenefitsWorkspace,
    DEPENDENT_LIMIT, DETAIL_HISTORY_LIMIT, ELECTION_LIMIT, PLAN_LIMIT, WINDOW_LIMIT,
    apply_detail_employee_name, apply_employee_names, detail_missing, validate_detail,
    validate_workspace,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::collections::BTreeSet;
use std::future::Future;

pub type BenefitsFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait BenefitsWorkspaceRepository: Send {
    type Error: From<BusinessError> + Send;

    fn plans<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitPlanRow>, Self::Error>;

    fn windows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitWindowRow>, Self::Error>;

    fn dependents<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitDependentRow>, Self::Error>;

    fn elections<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: Option<&'a str>,
        reviewer: bool,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitElectionRow>, Self::Error>;
}

/// Election detail reads for the retained employee/reviewer record page.
/// Implementations read only Benefits rows; People names arrive through
/// [`BenefitsPeoplePort`] in the caller's unit.
pub trait BenefitsDetailRepository: Send {
    type Error: From<BusinessError> + Send;

    fn election_detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        person_id: Option<&'a str>,
        reviewer: bool,
    ) -> BenefitsFuture<'a, Option<BenefitElectionDetailSource>, Self::Error>;

    fn election_history<'a>(
        &'a mut self,
        company: &'a CompanyId,
        election_id: &'a str,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitElectionEventRow>, Self::Error>;

    fn payroll_input<'a>(
        &'a mut self,
        company: &'a CompanyId,
        election_id: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitPayrollInputRow>, Self::Error>;
}

/// Current People/Payroll facts. The worker lookup is the retained active
/// person/engagement resolution; names are the retained legal-name projection.
pub trait BenefitsPeoplePort: Send {
    type Error: From<BusinessError> + Send;

    fn worker<'a>(
        &'a mut self,
        company: &'a CompanyId,
        user_id: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error>;

    fn employee_names<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_ids: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error>;
}

/// Current Commercial package boundary. Disabled enforcement is the retained
/// policy; when enforced the current `people.benefits` history entitlement is
/// required before any worker or owner row is read.
pub trait BenefitsEntitlementPort: Send {
    type Error: Send;

    fn entitlement<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error>;
}

pub async fn benefits_workspace<R, P, E>(
    actor: &BenefitsActor,
    personal: bool,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitsWorkspace, R::Error>
where
    R: BenefitsWorkspaceRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    actor.require_workspace(now)?;
    let entitlement = entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?;
    entitlement.require_history()?;
    let worker = people
        .worker(&actor.company_id, &actor.user_id)
        .await
        .map_err(R::Error::from)?;
    if let Some(worker) = &worker {
        worker.validate()?;
    }
    let reviewer = !personal && actor.can_review();
    let plans = repository.plans(&actor.company_id, PLAN_LIMIT + 1).await?;
    let windows = repository
        .windows(&actor.company_id, WINDOW_LIMIT + 1)
        .await?;
    let dependents = match worker.as_ref() {
        Some(worker) => {
            repository
                .dependents(&actor.company_id, &worker.person_id, DEPENDENT_LIMIT + 1)
                .await?
        }
        None => Vec::new(),
    };
    let mut elections = repository
        .elections(
            &actor.company_id,
            if reviewer {
                None
            } else {
                worker.as_ref().map(|value| value.person_id.as_str())
            },
            reviewer,
            ELECTION_LIMIT + 1,
        )
        .await?;
    let person_ids = elections
        .iter()
        .map(|row| row.person_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !person_ids.is_empty() {
        let names = people
            .employee_names(&actor.company_id, &person_ids)
            .await
            .map_err(R::Error::from)?;
        apply_employee_names(&mut elections, names)?;
    }
    let mut workspace = validate_workspace(
        actor,
        worker.as_ref(),
        reviewer,
        plans,
        windows,
        dependents,
        elections,
    )?;
    workspace.purchase = entitlement.purchase();
    Ok(workspace)
}

/// Read one current Company election detail with its immutable event history
/// and released payroll input. Reviewer scope reads any Company election; an
/// employee reads only their own. Package history admission, current worker
/// facts and the exact People legal-name projection are rechecked in this unit.
pub async fn benefits_election_detail<R, P, E>(
    actor: &BenefitsActor,
    id: &str,
    repository: &mut R,
    people: &mut P,
    entitlement: &mut E,
    now: TimestampMillis,
) -> Result<BenefitElectionDetail, R::Error>
where
    R: BenefitsDetailRepository,
    P: BenefitsPeoplePort,
    E: BenefitsEntitlementPort,
    R::Error: From<P::Error> + From<E::Error>,
{
    actor.require_workspace(now)?;
    entitlement
        .entitlement(actor.company_id.as_str())
        .await
        .map_err(R::Error::from)?
        .require_history()?;
    let worker = people
        .worker(&actor.company_id, &actor.user_id)
        .await
        .map_err(R::Error::from)?;
    if let Some(worker) = &worker {
        worker.validate()?;
    }
    let reviewer = actor.can_review();
    let Some(mut source) = repository
        .election_detail(
            &actor.company_id,
            id,
            worker.as_ref().map(|value| value.person_id.as_str()),
            reviewer,
        )
        .await?
    else {
        return Err(R::Error::from(detail_missing()));
    };
    let person_ids = vec![source.election.person_id.clone()];
    let names = people
        .employee_names(&actor.company_id, &person_ids)
        .await
        .map_err(R::Error::from)?;
    apply_detail_employee_name(&mut source, names)?;
    let history = repository
        .election_history(&actor.company_id, id, DETAIL_HISTORY_LIMIT + 1)
        .await?;
    let payroll = repository.payroll_input(&actor.company_id, id).await?;
    validate_detail(actor, worker.as_ref(), reviewer, source, history, payroll)
        .map_err(R::Error::from)
}
