//! Native Organization roster read.
//!
//! This is the read-only counterpart of the retained TypeScript
//! `organizationRoster` family (`adapters/organization-roster-read.ts`): the
//! same Company-scoped people and position rows, read on the caller's
//! transaction. The retained owner-scope role gate
//! (`assert_organization_access`) runs before any row is read; row shape and
//! ordering match the retained family so its consumers can move to this port
//! without behavior drift.
use crate::domain::access::{
    AccessPerson, AccessPosition, OrganizationAccessActor, assert_organization_access,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId};
use std::future::Future;
use std::pin::Pin;

pub type RosterFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait OrganizationRosterRepository: Send {
    type Error: From<BusinessError> + Send;

    fn roster_people<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> RosterFuture<'a, Vec<AccessPerson>, Self::Error>;

    fn roster_positions<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> RosterFuture<'a, Vec<AccessPosition>, Self::Error>;
}

/// The retained `organizationRoster` pair: non-ended employee people with
/// their level projection, and Company positions with level numbers.
#[derive(Clone, Debug, Default)]
pub struct OrganizationRosterRead {
    pub people: Vec<AccessPerson>,
    pub positions: Vec<AccessPosition>,
}

/// Read the roster only when the retained permission facts allow it. Every
/// source read stays inside the caller's transaction; the caller keeps its
/// branch/department projection window, exactly like the retained family.
pub async fn read_organization_roster<R: OrganizationRosterRepository>(
    repository: &mut R,
    actor: &OrganizationAccessActor,
) -> Result<OrganizationRosterRead, R::Error> {
    assert_organization_access(actor)?;
    let company = CompanyId::new(actor.company_id.clone())?;
    let people = repository.roster_people(&company).await?;
    let positions = repository.roster_positions(&company).await?;
    Ok(OrganizationRosterRead { people, positions })
}
