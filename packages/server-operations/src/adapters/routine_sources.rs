//! Concrete composition-ready [`RoutineCommandSources`] for native routines.
//!
//! Operations owns the routine template/schedule rows. Every other fact needed
//! to authorize a routine command belongs to another owner and must be supplied
//! by composition under the caller's retained serializable Holding unit.
//! [`RoutineFactDelegates`] is that seam: each method receives the repository's
//! own unit (`Option<&mut sqlx::PgConnection>`) and defaults to a named
//! fail-closed refusal, so a source composition has not mounted refuses the
//! command instead of fabricating a fact or copying another owner's SQL.
use crate::application::routine_commands::{RoutineCommandSources, RoutineFuture};
use crate::application::staff_tasks::{ProjectFact, RouteFact, StaffFact};
use crate::domain::staff_tasks::TaskScope;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::PgConnection;

fn source_required(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

/// Cross-owner routine command facts. Composition implements this trait on one
/// per-request adapter that holds the owning Identity, People, Projects and
/// Workflow ports.
pub trait RoutineFactDelegates: Send {
    /// Identity: the actor holds the current Company holding-owner role.
    fn company_owner<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, bool, BusinessError> {
        let _ = (unit, company_id, user_id);
        Box::pin(async {
            Err(source_required(
                "ROUTINE_IDENTITY_OWNER_SOURCE_REQUIRED",
                "Routines require the current Identity holding-owner port, which composition has not mounted.",
            ))
        })
    }

    /// People: the user is current active staff with placement.
    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, Option<StaffFact>, BusinessError> {
        let _ = (unit, company_id, user_id);
        Box::pin(async {
            Err(source_required(
                "ROUTINE_PEOPLE_ACTIVE_STAFF_SOURCE_REQUIRED",
                "Routines require the current People active-staff port, which composition has not mounted.",
            ))
        })
    }

    /// Projects: the project identity for the Company.
    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> RoutineFuture<'a, Option<ProjectFact>, BusinessError> {
        let _ = (unit, company_id, project_id);
        Box::pin(async {
            Err(source_required(
                "ROUTINE_PROJECT_SOURCE_REQUIRED",
                "Routines require the current Projects port, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: validate the ordered approval route for the routine scope.
    #[allow(clippy::too_many_arguments)]
    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> RoutineFuture<'a, RouteFact, BusinessError> {
        let _ = (
            unit,
            scope,
            creator_user_id,
            assignee_user_id,
            phase,
            approvers,
            service_task,
        );
        Box::pin(async {
            Err(source_required(
                "ROUTINE_WORKFLOW_ROUTE_SOURCE_REQUIRED",
                "Routines require the current Workflow approval-route port, which composition has not mounted.",
            ))
        })
    }
}

/// No cross-owner delegate mounted. Every fact refuses with its named code.
#[derive(Default)]
pub struct FailClosedRoutineFactDelegates;

impl RoutineFactDelegates for FailClosedRoutineFactDelegates {}

pub struct PgRoutineSources<D: RoutineFactDelegates = FailClosedRoutineFactDelegates> {
    delegates: D,
}

impl PgRoutineSources<FailClosedRoutineFactDelegates> {
    pub fn fail_closed() -> Self {
        Self {
            delegates: FailClosedRoutineFactDelegates,
        }
    }
}

impl<D: RoutineFactDelegates> PgRoutineSources<D> {
    pub fn new(delegates: D) -> Self {
        Self { delegates }
    }
    pub fn delegates(&mut self) -> &mut D {
        &mut self.delegates
    }
}

impl<D: RoutineFactDelegates> RoutineCommandSources for PgRoutineSources<D> {
    type Error = BusinessError;

    fn company_owner<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, bool, Self::Error> {
        self.delegates.company_owner(unit, company_id, user_id)
    }

    fn active_staff<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, Option<StaffFact>, Self::Error> {
        self.delegates.active_staff(unit, company_id, user_id)
    }

    fn project<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        company_id: &'a str,
        project_id: &'a str,
    ) -> RoutineFuture<'a, Option<ProjectFact>, Self::Error> {
        self.delegates.project(unit, company_id, project_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn task_route<'a>(
        &'a mut self,
        unit: Option<&'a mut PgConnection>,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> RoutineFuture<'a, RouteFact, Self::Error> {
        if approvers.is_empty() {
            return Box::pin(async move {
                Ok(RouteFact {
                    blocked_reason: None,
                })
            });
        }
        self.delegates.task_route(
            unit,
            scope,
            creator_user_id,
            assignee_user_id,
            phase,
            approvers,
            service_task,
        )
    }
}
