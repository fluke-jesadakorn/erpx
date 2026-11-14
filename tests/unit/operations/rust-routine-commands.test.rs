//! Native routine template/schedule application use cases over in-memory owner
//! ports. The PostgreSQL adapter has its own companion test.
use erpx_server_operations::public::routine_commands::{
    RoutineCommandRepository, RoutineCommandSources, RoutineFuture, RoutineReceipt,
    RoutineScheduleRow, RoutineTemplateRow, SaveRoutineScheduleCommand,
    SaveRoutineTemplateCommand, SaveSchedulePlan, SaveTemplatePlan, ScheduleStateCommand,
    ScheduleStatePlan, StoredRoutineReceipt, change_schedule_state, end_schedule,
    next_occurrence, save_schedule, save_template,
};
use erpx_server_operations::public::staff_tasks::{
    ProjectFact, RouteFact, StaffFact, StaffTaskActor, TaskScope,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const NOW: i64 = 1_788_000_000_000;

#[derive(Default)]
struct State {
    templates: HashMap<String, RoutineTemplateRow>,
    schedules: HashMap<String, RoutineScheduleRow>,
    receipts: HashMap<String, StoredRoutineReceipt>,
    captures: Vec<String>,
    writes: usize,
}

#[derive(Default)]
struct Repo {
    state: Mutex<State>,
}

impl Repo {
    fn store(&self, key: &str, plan: &RoutineReceipt, operation: &str, record: &str, fingerprint: &str) {
        let mut state = self.state.lock().unwrap();
        state.writes += 1;
        state.receipts.insert(
            key.into(),
            StoredRoutineReceipt {
                operation: operation.into(),
                record_id: record.into(),
                request_fingerprint: fingerprint.into(),
                result_json: Some(serde_json::to_string(plan).unwrap()),
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
                actor_id: "creator".into(),
            },
        );
    }
}

impl RoutineCommandRepository for Repo {
    type Error = BusinessError;

    fn connection(&mut self) -> Option<&mut sqlx::PgConnection> {
        None
    }
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> RoutineFuture<'a, Option<StoredRoutineReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().receipts.get(key).cloned()) })
    }
    fn template<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineTemplateRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .unwrap()
                .templates
                .get(id)
                .filter(|row| row.company_id == company_id)
                .cloned())
        })
    }
    fn schedule<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> RoutineFuture<'a, Option<RoutineScheduleRow>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .unwrap()
                .schedules
                .get(id)
                .filter(|row| row.company_id == company_id)
                .cloned())
        })
    }
    fn save_template<'a>(
        &'a mut self,
        plan: SaveTemplatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            {
                let mut state = self.state.lock().unwrap();
                let row = RoutineTemplateRow {
                    id: plan.record_id.clone(),
                    company_id: plan.company_id.clone(),
                    branch_id: plan.branch_id.clone(),
                    department_id: plan.department_id.clone(),
                    title: plan.title.clone(),
                    description: plan.description.clone(),
                    creator_user_id: state
                        .templates
                        .get(&plan.record_id)
                        .map(|row| row.creator_user_id.clone())
                        .unwrap_or_else(|| plan.actor_id.clone()),
                    assignee_user_id: plan.assignee_user_id.clone(),
                    project_id: plan.project_id.clone(),
                    start_approvers_json: plan.start_approvers_json.clone(),
                    completion_approvers_json: plan.completion_approvers_json.clone(),
                    published: plan.published,
                    version: plan.version,
                    created_at: plan.now,
                    updated_at: plan.now,
                };
                state.templates.insert(plan.record_id.clone(), row);
            }
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
            );
            Ok(plan.receipt)
        })
    }
    fn save_schedule<'a>(
        &'a mut self,
        plan: SaveSchedulePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            {
                let mut state = self.state.lock().unwrap();
                let row = RoutineScheduleRow {
                    id: plan.record_id.clone(),
                    template_id: plan.template_id.clone(),
                    company_id: plan.company_id.clone(),
                    frequency: plan.frequency.clone(),
                    timezone: plan.timezone.clone(),
                    occurrence_time: plan.occurrence_time.clone(),
                    start_date: plan.start_date.clone(),
                    end_date: plan.end_date.clone(),
                    weekday: plan.weekday,
                    month_day: plan.month_day,
                    due_offset_hours: plan.due_offset_hours,
                    state: "paused".into(),
                    next_occurrence_at: plan.next_occurrence_at,
                    paused_at: Some(plan.now),
                    last_generated_at: None,
                    last_error: None,
                    version: plan.version,
                    created_at: plan.now,
                    updated_at: plan.now,
                };
                state.schedules.insert(plan.record_id.clone(), row);
                if let Some(capture) = plan.capture.as_ref() {
                    state.captures.push(format!("{}:{}", capture.kind, plan.record_id));
                }
            }
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
            );
            Ok(plan.receipt)
        })
    }
    fn change_schedule_state<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            {
                let mut state = self.state.lock().unwrap();
                let row = state.schedules.get_mut(&plan.record_id).unwrap();
                row.state = plan.state.clone();
                row.version = plan.version;
                row.paused_at = if plan.state == "active" { None } else { Some(plan.now) };
                if plan.state == "ended" {
                    row.next_occurrence_at = None;
                }
                if let Some(capture) = plan.capture.as_ref() {
                    state.captures.push(format!("{}:{}", capture.kind, plan.record_id));
                }
            }
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
            );
            Ok(plan.receipt)
        })
    }
    fn end_schedule<'a>(
        &'a mut self,
        plan: ScheduleStatePlan,
    ) -> RoutineFuture<'a, RoutineReceipt, Self::Error> {
        Box::pin(async move {
            {
                let mut state = self.state.lock().unwrap();
                let row = state.schedules.get_mut(&plan.record_id).unwrap();
                row.state = "ended".into();
                row.version = plan.version;
                row.paused_at = None;
                row.next_occurrence_at = None;
            }
            self.store(
                plan.key.as_str(),
                &plan.receipt,
                &plan.operation,
                &plan.record_id,
                &plan.request_fingerprint,
            );
            Ok(plan.receipt)
        })
    }
}

struct Sources {
    owner: bool,
    staff: HashMap<String, StaffFact>,
    projects: HashSet<String>,
    blocked: Option<&'static str>,
}

impl Default for Sources {
    fn default() -> Self {
        let mut staff = HashMap::new();
        staff.insert(
            "creator".into(),
            StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            },
        );
        staff.insert(
            "worker".into(),
            StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            },
        );
        staff.insert(
            "approver".into(),
            StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            },
        );
        Self {
            owner: false,
            staff,
            projects: HashSet::new(),
            blocked: None,
        }
    }
}

impl RoutineCommandSources for Sources {
    type Error = BusinessError;
    fn company_owner<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> RoutineFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.owner) })
    }
    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        user_id: &'a str,
    ) -> RoutineFuture<'a, Option<StaffFact>, Self::Error> {
        Box::pin(async move { Ok(self.staff.get(user_id).cloned()) })
    }
    fn project<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        project_id: &'a str,
    ) -> RoutineFuture<'a, Option<ProjectFact>, Self::Error> {
        Box::pin(async move {
            Ok(self.projects.contains(project_id).then(|| ProjectFact {
                owner_user_id: "creator".into(),
                title: "Project".into(),
            }))
        })
    }
    fn task_route<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _scope: &'a TaskScope,
        _creator_user_id: &'a str,
        _assignee_user_id: &'a str,
        _phase: &'a str,
        approvers: &'a [String],
        _service_task: bool,
    ) -> RoutineFuture<'a, RouteFact, Self::Error> {
        let blocked = if approvers.is_empty() {
            None
        } else {
            self.blocked.map(str::to_owned)
        };
        Box::pin(async move { Ok(RouteFact { blocked_reason: blocked }) })
    }
}

fn actor() -> StaffTaskActor {
    StaffTaskActor {
        user_id: "creator".into(),
        assignment_id: "assignment-creator".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}

fn template_command() -> SaveRoutineTemplateCommand {
    SaveRoutineTemplateCommand {
        title: "  Daily delivery check  ".into(),
        description: "  Review the receiving log  ".into(),
        assignee_user_id: "worker".into(),
        project_id: None,
        start_approvers: Vec::new(),
        completion_approvers: vec!["approver".into()],
        published: true,
        expected_version: 1,
    }
}

fn schedule_command() -> SaveRoutineScheduleCommand {
    serde_json::from_value(serde_json::json!({
        "frequency": "daily",
        "startDate": "2026-01-01",
        "occurrenceTime": "09:00",
        "timezone": "Asia/Bangkok",
        "expectedVersion": 1
    }))
    .unwrap()
}

async fn create_template(repo: &mut Repo, sources: &mut Sources, key: &str) -> RoutineReceipt {
    save_template(
        &actor(),
        repo,
        sources,
        None,
        &template_command(),
        None,
        key,
        NOW,
    )
    .await
    .unwrap()
}

async fn create_schedule(
    repo: &mut Repo,
    sources: &mut Sources,
    template_id: &str,
    key: &str,
) -> RoutineReceipt {
    save_schedule(
        &actor(),
        repo,
        sources,
        template_id,
        None,
        &schedule_command(),
        key,
        None,
        NOW,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn creates_a_trimmed_published_template_once() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let key = "routine-create-key";
    let receipt = create_template(&mut repo, &mut sources, key).await;
    assert_eq!(
        receipt,
        RoutineReceipt::template(format!("routine-{key}"), 1)
    );
    let state = repo.state.lock().unwrap();
    let row = state.templates.values().next().unwrap();
    assert_eq!(row.title, "Daily delivery check");
    assert_eq!(row.description, "Review the receiving log");
    assert_eq!(row.published, 1);
    assert_eq!(row.version, 1);
    assert_eq!(state.writes, 1);
    drop(state);
    // An exact retry returns the stored receipt without a second write.
    let replay = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        None,
        &template_command(),
        None,
        key,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(replay, receipt);
    assert_eq!(repo.state.lock().unwrap().writes, 1);
}

#[tokio::test]
async fn rejects_a_reused_key_with_a_different_request() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    create_template(&mut repo, &mut sources, "routine-key-one").await;
    let error = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        None,
        &SaveRoutineTemplateCommand {
            title: "Different routine".into(),
            ..template_command()
        },
        None,
        "routine-key-one",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn updates_only_with_the_current_version() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-one").await;
    let updated = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            title: "Revised delivery check".into(),
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-update-one",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(updated, RoutineReceipt::template(created.id.clone(), 2));
    let stale = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            title: "Stale delivery check".into(),
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-update-two",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.kind, FailureKind::Conflict);
    assert_eq!(
        repo.state
            .lock()
            .unwrap()
            .templates
            .get(&created.id)
            .unwrap()
            .version,
        2
    );
}

#[tokio::test]
async fn keeps_template_editing_with_the_creator_or_company_owner() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-two").await;
    let stranger = StaffTaskActor {
        user_id: "worker".into(),
        ..actor()
    };
    let denied = save_template(
        &stranger,
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            title: "Not mine".into(),
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-update-three",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.code, "TEMPLATE_OWNER_REQUIRED");
    sources.owner = true;
    save_template(
        &stranger,
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            title: "Owner revised".into(),
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-update-four",
        NOW,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn creates_a_paused_schedule_with_a_local_next_occurrence() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-three").await;
    let schedule = create_schedule(&mut repo, &mut sources, &created.id, "schedule-create-one").await;
    assert_eq!(
        schedule,
        RoutineReceipt::schedule("schedule-schedule-create-one", "paused", 1)
    );
    let rule = schedule_command().rule();
    let next = next_occurrence(&rule, NOW - 1).unwrap().unwrap();
    let state = repo.state.lock().unwrap();
    let row = state.schedules.get(&schedule.id).unwrap();
    assert_eq!(row.state, "paused");
    assert_eq!(row.next_occurrence_at, Some(next));
    assert!(next > NOW - 1 && next < NOW + 2 * 86_400_000);
    // Bangkok has no daylight saving: the following daily occurrence is one day
    // later unless the monthly clamp applies.
    assert_eq!(
        next_occurrence(&rule, next).unwrap(),
        Some(next + 86_400_000)
    );
}

#[tokio::test]
async fn updates_only_paused_schedules_and_resumes_a_published_source() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-four").await;
    let schedule =
        create_schedule(&mut repo, &mut sources, &created.id, "schedule-create-two").await;
    save_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &created.id,
        Some(&schedule.id),
        &SaveRoutineScheduleCommand {
            due_offset_hours: 48,
            expected_version: 1,
            ..schedule_command()
        },
        "schedule-update-one",
        None,
        NOW,
    )
    .await
    .unwrap();
    let resumed = change_schedule_state(
        &actor(),
        &mut repo,
        &mut sources,
        &schedule.id,
        &ScheduleStateCommand {
            expected_version: 2,
        },
        true,
        "schedule-resume-one",
        None,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        resumed,
        RoutineReceipt::schedule(schedule.id.clone(), "active", 3)
    );
    let active_update = save_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &created.id,
        Some(&schedule.id),
        &schedule_command(),
        "schedule-update-two",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(active_update.kind, FailureKind::Conflict);
    let paused = change_schedule_state(
        &actor(),
        &mut repo,
        &mut sources,
        &schedule.id,
        &ScheduleStateCommand {
            expected_version: 3,
        },
        false,
        "schedule-pause-one",
        None,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        paused,
        RoutineReceipt::schedule(schedule.id.clone(), "paused", 4)
    );
}

#[tokio::test]
async fn refuses_to_resume_an_unpublished_source() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-five").await;
    let schedule =
        create_schedule(&mut repo, &mut sources, &created.id, "schedule-create-three").await;
    save_template(
        &actor(),
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            published: false,
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-unpublish-one",
        NOW,
    )
    .await
    .unwrap();
    let error = change_schedule_state(
        &actor(),
        &mut repo,
        &mut sources,
        &schedule.id,
        &ScheduleStateCommand {
            expected_version: 1,
        },
        true,
        "schedule-resume-two",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TEMPLATE_UNPUBLISHED");
}

#[tokio::test]
async fn ends_a_schedule_once_and_records_the_capture_kind() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-six").await;
    let schedule =
        create_schedule(&mut repo, &mut sources, &created.id, "schedule-create-four").await;
    let capture = erpx_server_operations::public::routine_commands::RoutineCapture {
        kind: "create",
        authority: erpx_server_operations::public::routine_commands::RoutineAuthority {
            actor_json: "{\"userId\":\"creator\"}".into(),
            role: "owner".into(),
        },
    };
    save_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &created.id,
        None,
        &schedule_command(),
        "schedule-create-five",
        Some(capture),
        NOW,
    )
    .await
    .unwrap();
    let ended = end_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &schedule.id,
        &ScheduleStateCommand {
            expected_version: 1,
        },
        "schedule-end-one",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(
        ended,
        RoutineReceipt::schedule(schedule.id.clone(), "ended", 2)
    );
    let again = end_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &schedule.id,
        &ScheduleStateCommand {
            expected_version: 2,
        },
        "schedule-end-two",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(again.code, "SCHEDULE_ENDED");
    assert!(
        repo.state
            .lock()
            .unwrap()
            .captures
            .contains(&format!("create:schedule-schedule-create-five"))
    );
}

#[tokio::test]
async fn requires_the_selected_operations_write_scope() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let read_only = StaffTaskActor {
        permissions: vec!["records.read".into()],
        ..actor()
    };
    let denied = save_template(
        &read_only,
        &mut repo,
        &mut sources,
        None,
        &template_command(),
        None,
        "routine-create-seven",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(denied.kind, FailureKind::Forbidden);
    let expired = StaffTaskActor {
        expires_at: NOW - 1,
        ..actor()
    };
    let changed = save_template(
        &expired,
        &mut repo,
        &mut sources,
        None,
        &template_command(),
        None,
        "routine-create-eight",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(changed.code, "WORK_CONTEXT_CHANGED");
    let no_module = StaffTaskActor {
        modules: vec!["collaboration".into()],
        ..actor()
    };
    let forbidden = save_template(
        &no_module,
        &mut repo,
        &mut sources,
        None,
        &template_command(),
        None,
        "routine-create-nine",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(forbidden.code, "MODULE_FORBIDDEN");
}

#[tokio::test]
async fn validates_fields_timezone_dates_and_routes() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let mut input = template_command();
    input.start_approvers.clear();
    input.completion_approvers.clear();
    let no_approver = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        None,
        &input,
        None,
        "routine-create-ten",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(no_approver.kind, FailureKind::InvalidInput);

    let created = create_template(&mut repo, &mut sources, "routine-create-eleven").await;
    let bad_timezone = save_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &created.id,
        None,
        &SaveRoutineScheduleCommand {
            timezone: "Mars/Olympus".into(),
            ..schedule_command()
        },
        "schedule-create-six",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(bad_timezone.code, "ROUTINE_FIELDS_INVALID");
    let bad_date = save_schedule(
        &actor(),
        &mut repo,
        &mut sources,
        &created.id,
        None,
        &SaveRoutineScheduleCommand {
            start_date: "2026-02-30".into(),
            ..schedule_command()
        },
        "schedule-create-seven",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(bad_date.code, "ROUTINE_FIELDS_INVALID");

    sources.blocked = Some("The approval route is unavailable.");
    let route = save_template(
        &actor(),
        &mut repo,
        &mut sources,
        Some(&created.id),
        &SaveRoutineTemplateCommand {
            title: "Blocked route".into(),
            expected_version: 1,
            ..template_command()
        },
        None,
        "routine-update-five",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(route.code, "ROUTINE_ROUTE_INVALID");
}

#[tokio::test]
async fn scopes_reads_and_commands_to_the_selected_company() {
    let mut repo = Repo::default();
    let mut sources = Sources::default();
    let created = create_template(&mut repo, &mut sources, "routine-create-twelve").await;
    let other = StaffTaskActor {
        company_id: "other-company".into(),
        ..actor()
    };
    let missing = save_schedule(
        &other,
        &mut repo,
        &mut sources,
        &created.id,
        None,
        &schedule_command(),
        "schedule-create-eight",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, "TEMPLATE_NOT_FOUND");
}
