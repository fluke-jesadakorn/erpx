//! Native Benefits enrollment workspace read use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. Package history and People worker/name facts arrive through
//! public owner ports supplied by composition; this owner never reads another
//! owner's tables. The application enforces the current actor policy, current
//! package history entitlement, worker scope and every bounded
//! plan/window/dependent/enrollment row before presentation.
use crate::domain::workspace::{
    BenefitDependentRow, BenefitElectionRow, BenefitPlanRow, BenefitWindowRow, BenefitsActor,
    BenefitsEntitlement, BenefitsWorker, BenefitsWorkspace, DEPENDENT_LIMIT, ELECTION_LIMIT,
    PLAN_LIMIT, WINDOW_LIMIT, apply_employee_names, validate_workspace,
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
