//! Native Projects handoff-view read use case.
//!
//! The repository owns the bounded owner reads: the current visible project
//! (for owner and scope facts) and the stored handoff rows. Both are executed
//! by the Projects Postgres adapter; no TypeScript prepares or executes SQL and
//! there is no fallback projection. Composition supplies the actor's
//! purchase-qualified cost capability and the same IAM person-equivalence
//! predicate the retained read policy used.

use crate::domain::project_handoffs::{
    HandoffRow, HandoffViewer, project_handoff_view, snapshot_invalid,
};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type HandoffViewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait HandoffViewRepository: Send + Sync {
    fn handoffs<'a>(
        &'a self,
        company: &'a str,
        project: &'a str,
    ) -> HandoffViewFuture<'a, Vec<HandoffRow>>;

    fn project_owner<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
        project: &'a str,
    ) -> HandoffViewFuture<'a, Option<String>>;
}

pub struct HandoffViewActor {
    /// Current, purchase-qualified cost-history capability.
    pub can_read_costs: bool,
    /// IAM person equivalence for the actor, bound by composition from the
    /// same current-authority port used by the retained owner reads.
    pub same_person: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

/// Return the complete `ProjectHandoffView[]` payload with parity to the
/// retained owner read. Each snapshot is projected under the current viewer;
/// a malformed stored snapshot fails closed instead of leaking unfiltered rows.
pub async fn read_handoff_views<R: HandoffViewRepository>(
    actor: &HandoffViewActor,
    repository: &R,
    company: &str,
    user: &str,
    project: &str,
) -> Result<Vec<Value>, BusinessError> {
    let owner = repository.project_owner(company, user, project).await?;
    let is_owner = owner.as_deref().is_some_and(|id| (actor.same_person)(id));
    let viewer = HandoffViewer {
        is_owner,
        can_read_costs: actor.can_read_costs,
        same_person: actor.same_person.clone(),
    };
    let rows = repository.handoffs(company, project).await?;
    rows.iter()
        .map(|row| {
            let snapshot: Value =
                serde_json::from_str(&row.snapshot_json).map_err(|_| snapshot_invalid())?;
            project_handoff_view(row, snapshot, &viewer)
        })
        .collect()
}
