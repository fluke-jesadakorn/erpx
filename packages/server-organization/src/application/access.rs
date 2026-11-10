//! Organization access projection use case.
//!
//! The repository supplies current sources and owns the transaction; this
//! layer keeps the permission gate, the branch/department window and the
//! bounded projection. It never opens a transaction or exports more rows than
//! the same transaction authorizes.
use crate::domain::access::{
    assert_organization_access, project_organization_access, AccessLevel, AccessNode, AccessPerson,
    AccessPosition, OrganizationAccessActor, OrganizationAccessData, OrganizationAccessSnapshot,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId};
use std::future::Future;
use std::pin::Pin;

pub type AccessFuture<'a, T, E = BusinessError> =
    Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait OrganizationAccessRepository: Send {
    type Error: From<BusinessError> + Send;

    fn access_roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> AccessFuture<'a, (Vec<AccessPerson>, Vec<AccessPosition>), Self::Error>;

    fn access_nodes<'a>(
        &'a mut self,
        actor: &'a OrganizationAccessActor,
    ) -> AccessFuture<'a, Vec<AccessNode>, Self::Error>;

    fn access_levels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> AccessFuture<'a, Vec<AccessLevel>, Self::Error>;
}

/// `new Date(ms).toISOString().slice(0, 10)` as used by the retained owner.
pub fn utc_date(timestamp_ms: i64) -> String {
    let days = timestamp_ms.div_euclid(86_400_000);
    civil_date(days)
}

fn civil_date(days_since_epoch: i64) -> String {
    // Howard Hinnant's civil_from_days, valid for the bounded epoch range.
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Read the bounded projection only when the current permission facts allow
/// it. Every source read stays inside the caller's transaction.
pub async fn read_organization_access<R: OrganizationAccessRepository>(
    repository: &mut R,
    actor: &OrganizationAccessActor,
    clock: &(dyn Clock + Sync),
) -> Result<OrganizationAccessData, R::Error> {
    assert_organization_access(actor)?;
    let company = CompanyId::new(actor.company_id.clone())?;
    let (people, positions) = repository.access_roster(&company).await?;
    let nodes = repository.access_nodes(actor).await?;
    let levels = repository.access_levels(&company).await?;
    Ok(project_organization_access(
        actor,
        OrganizationAccessSnapshot {
            people,
            positions,
            nodes,
            levels,
        },
        &utc_date(clock.now().get()),
    ))
}
