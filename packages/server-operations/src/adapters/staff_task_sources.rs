//! Concrete composition-ready [`StaffTaskSources`] for native Staff Work.
//!
//! Operations owns the task, delegation and notification rows. Every other fact
//! needed to authorize or present a task belongs to another owner and must be
//! supplied by composition under the caller's retained serializable Holding
//! unit. [`StaffTaskFactDelegates`] is that seam: each method has a fail-closed
//! default with its own named error code, so a source composition has not
//! mounted refuses the command instead of fabricating a fact or copying another
//! owner's SQL.
//!
//! Two facts are genuinely owner-local and are resolved here without a delegate:
//!
//! - an ordinary task (no start/completion approvers) has no Workflow approval,
//!   so its approval view is `not_required`; and
//! - an empty approval route has nothing to validate, so it is not blocked.
//!
//! Reviewed tasks and non-empty routes always go to the Workflow delegate.
use crate::application::staff_tasks::{
    PostFact, ProjectFact, RouteFact, StaffFact, StaffTaskFuture, StaffTaskSources,
};
use crate::domain::staff_tasks::{
    ApprovalView, AttachmentRow, DecisionFact, StaffTaskActor, TaskRow, TaskScope,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

fn source_required(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

/// Cross-owner Staff Work facts and effects.
///
/// Composition implements this trait on one per-request adapter that holds the
/// retained PostgreSQL unit plus the owning Identity, People, Projects,
/// Collaboration, Files, Workflow and PMS ports. Every method defaults to the
/// named fail-closed refusal; composition overrides only the facts it can serve
/// from the current owner.
pub trait StaffTaskFactDelegates: Send {
    /// People: the user is current active staff for the Company, with their
    /// current branch/department placement.
    fn active_staff<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, BusinessError> {
        let _ = (company_id, user_id);
        Box::pin(async {
            Err(source_required(
                "TASK_PEOPLE_ACTIVE_STAFF_SOURCE_REQUIRED",
                "Staff Work requires the current People active-staff port, which composition has not mounted.",
            ))
        })
    }

    /// Projects: the project identity and accountable owner for the Company.
    fn project<'a>(
        &'a mut self,
        company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, BusinessError> {
        let _ = (company_id, project_id);
        Box::pin(async {
            Err(source_required(
                "TASK_PROJECT_SOURCE_REQUIRED",
                "Staff Work requires the current Projects port, which composition has not mounted.",
            ))
        })
    }

    /// Identity: the current IAM mapping says the actor is this user.
    fn same_person<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        let _ = (actor, user_id);
        Box::pin(async {
            Err(source_required(
                "TASK_IDENTITY_SAME_PERSON_SOURCE_REQUIRED",
                "Staff Work requires the current Identity same-person port, which composition has not mounted.",
            ))
        })
    }

    /// Identity: the current IAM mapping says two users are the same person.
    fn same_person_between<'a>(
        &'a mut self,
        company_id: &'a str,
        user_a: &'a str,
        user_b: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        let _ = (company_id, user_a, user_b);
        Box::pin(async {
            Err(source_required(
                "TASK_IDENTITY_SAME_PERSON_BETWEEN_SOURCE_REQUIRED",
                "Staff Work requires the current Identity person-mapping port, which composition has not mounted.",
            ))
        })
    }

    /// Collaboration: the post is readable by the actor.
    fn readable_post<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        post_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, BusinessError> {
        let _ = (actor, post_id);
        Box::pin(async {
            Err(source_required(
                "TASK_COLLABORATION_POST_SOURCE_REQUIRED",
                "Staff Work requires the current Collaboration post-read port, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: validate the ordered approval route for the task scope.
    fn task_route<'a>(
        &'a mut self,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, BusinessError> {
        let _ = (
            scope,
            creator_user_id,
            assignee_user_id,
            phase,
            approvers,
            service_task,
        );
        Box::pin(async {
            Err(source_required(
                "TASK_WORKFLOW_ROUTE_SOURCE_REQUIRED",
                "Staff Work requires the current Workflow route port, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: the current approval instance for the task and whether this
    /// actor may decide it.
    fn approval_view<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, BusinessError> {
        let _ = (actor, task);
        Box::pin(async {
            Err(source_required(
                "TASK_WORKFLOW_APPROVAL_VIEW_SOURCE_REQUIRED",
                "Staff Work requires the current Workflow approval view, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: open the approval instance for the next task version.
    fn open_approval<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        phase: &'a str,
        route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        let _ = (key, task, phase, route, version);
        Box::pin(async {
            Err(source_required(
                "TASK_WORKFLOW_OPEN_APPROVAL_SOURCE_REQUIRED",
                "Staff Work requires the current Workflow open-approval port, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: record one approval or rejection decision.
    fn prepare_decision<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        approve: bool,
        note: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, BusinessError> {
        let _ = (key, task, approve, note, expected_version);
        Box::pin(async {
            Err(source_required(
                "TASK_WORKFLOW_DECISION_SOURCE_REQUIRED",
                "Staff Work requires the current Workflow decision port, which composition has not mounted.",
            ))
        })
    }

    /// Workflow: close the open approval when the task is cancelled.
    fn close_approvals<'a>(
        &'a mut self,
        key: &'a str,
        task_id: &'a str,
        reason: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        let _ = (key, task_id, reason, expected_version);
        Box::pin(async {
            Err(source_required(
                "TASK_WORKFLOW_CLOSE_APPROVAL_SOURCE_REQUIRED",
                "Staff Work requires the current Workflow close-approval port, which composition has not mounted.",
            ))
        })
    }

    /// Files: current task evidence attachments.
    fn attachments<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, BusinessError> {
        let _ = (actor, task_id);
        Box::pin(async {
            Err(source_required(
                "TASK_FILES_ATTACHMENT_SOURCE_REQUIRED",
                "Staff Work requires the current Files attachment port, which composition has not mounted.",
            ))
        })
    }

    /// People: the current display name for a user.
    fn person_name<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, BusinessError> {
        let _ = (company_id, user_id);
        Box::pin(async {
            Err(source_required(
                "TASK_PEOPLE_NAME_SOURCE_REQUIRED",
                "Staff Work requires the current People display-name port, which composition has not mounted.",
            ))
        })
    }

    /// PMS: the Operations feature is enabled for the Company.
    fn feature_enabled<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, BusinessError> {
        let _ = company_id;
        Box::pin(async {
            Err(source_required(
                "TASK_PMS_FEATURE_SOURCE_REQUIRED",
                "Staff Work requires the current PMS feature-availability port, which composition has not mounted.",
            ))
        })
    }
}

/// The default delegate: every cross-owner fact is absent, so every fact that a
/// command needs refuses with its named code. Ordinary task presentation still
/// works because those facts are owner-local in [`PgStaffTaskSources`].
#[derive(Clone, Copy, Debug, Default)]
pub struct FailClosedStaffTaskFactDelegates;

impl StaffTaskFactDelegates for FailClosedStaffTaskFactDelegates {}

/// Concrete composition-facing [`StaffTaskSources`].
///
/// It performs no cross-owner SQL: composition supplies current owner facts
/// through [`StaffTaskFactDelegates`]. The only local decisions are the two
/// owner-local facts documented at the module root.
///
/// The application holds the repository and the sources as two simultaneous
/// `&mut` ports, so a delegate cannot borrow the repository's
/// `&mut PgConnection`. Composition should back the delegate with its owner
/// ports over a separate pool connection or a prepared read snapshot; the
/// Staff Work command still writes through the repository's retained unit.
pub struct PgStaffTaskSources<D: StaffTaskFactDelegates> {
    delegates: D,
}

impl<D: StaffTaskFactDelegates> PgStaffTaskSources<D> {
    pub fn new(delegates: D) -> Self {
        Self { delegates }
    }

    pub fn delegates(&mut self) -> &mut D {
        &mut self.delegates
    }
}

impl PgStaffTaskSources<FailClosedStaffTaskFactDelegates> {
    /// Sources with no cross-owner delegate mounted.
    pub fn fail_closed() -> Self {
        Self::new(FailClosedStaffTaskFactDelegates)
    }
}

fn ordinary(task: &TaskRow) -> bool {
    task.start_approvers().is_ok_and(|value| value.is_empty())
        && task
            .completion_approvers()
            .is_ok_and(|value| value.is_empty())
}

impl<D: StaffTaskFactDelegates> StaffTaskSources for PgStaffTaskSources<D> {
    type Error = BusinessError;

    fn active_staff<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, Self::Error> {
        self.delegates.active_staff(company_id, user_id)
    }

    fn project<'a>(
        &'a mut self,
        company_id: &'a str,
        project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, Self::Error> {
        self.delegates.project(company_id, project_id)
    }

    fn same_person<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        self.delegates.same_person(actor, user_id)
    }

    fn same_person_between<'a>(
        &'a mut self,
        company_id: &'a str,
        user_a: &'a str,
        user_b: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        self.delegates
            .same_person_between(company_id, user_a, user_b)
    }

    fn readable_post<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        post_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<PostFact>, Self::Error> {
        self.delegates.readable_post(actor, post_id)
    }

    fn task_route<'a>(
        &'a mut self,
        scope: &'a TaskScope,
        creator_user_id: &'a str,
        assignee_user_id: &'a str,
        phase: &'a str,
        approvers: &'a [String],
        service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, Self::Error> {
        if approvers.is_empty() {
            return Box::pin(async move {
                Ok(RouteFact {
                    blocked_reason: None,
                })
            });
        }
        self.delegates.task_route(
            scope,
            creator_user_id,
            assignee_user_id,
            phase,
            approvers,
            service_task,
        )
    }

    fn approval_view<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task: &'a TaskRow,
    ) -> StaffTaskFuture<'a, ApprovalView, Self::Error> {
        if ordinary(task) {
            return Box::pin(async move {
                Ok(ApprovalView {
                    state: "not_required".into(),
                    phase: None,
                    can_decide: false,
                    blocked_reason: None,
                    steps: serde_json::json!([]),
                })
            });
        }
        self.delegates.approval_view(actor, task)
    }

    fn open_approval<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        phase: &'a str,
        route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        self.delegates
            .open_approval(key, task, phase, route, version)
    }

    fn prepare_decision<'a>(
        &'a mut self,
        key: &'a str,
        task: &'a TaskRow,
        approve: bool,
        note: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, DecisionFact, Self::Error> {
        self.delegates
            .prepare_decision(key, task, approve, note, expected_version)
    }

    fn close_approvals<'a>(
        &'a mut self,
        key: &'a str,
        task_id: &'a str,
        reason: &'a str,
        expected_version: i64,
    ) -> StaffTaskFuture<'a, (), Self::Error> {
        self.delegates
            .close_approvals(key, task_id, reason, expected_version)
    }

    fn attachments<'a>(
        &'a mut self,
        actor: &'a StaffTaskActor,
        task_id: &'a str,
    ) -> StaffTaskFuture<'a, Vec<AttachmentRow>, Self::Error> {
        self.delegates.attachments(actor, task_id)
    }

    fn person_name<'a>(
        &'a mut self,
        company_id: &'a str,
        user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<String>, Self::Error> {
        self.delegates.person_name(company_id, user_id)
    }

    fn feature_enabled<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> StaffTaskFuture<'a, bool, Self::Error> {
        self.delegates.feature_enabled(company_id)
    }
}
