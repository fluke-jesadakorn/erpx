//! Native scheduled routine occurrence pass over real migrated PostgreSQL.
//! The test unit uses the same owner adapter, planner and native routine task
//! write that composition binds behind the captured authorization/purchase unit.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::routine_occurrences::{
    OccurrenceOutcome, OccurrencePolicy, OccurrenceSourceFacts, RoutineOccurrenceFuture,
    RoutineOccurrenceUnit, due_schedules, plan_occurrence, source as occurrence_source,
    stage_occurrence,
};
use erpx_server_operations::public::routine_commands::{
    RoutineScheduleRow, next_occurrence,
};
use erpx_server_operations::public::staff_tasks::{
    PgStaffTaskSources, PgStaffTasks, ProjectFact, RouteFact, RoutineTaskWrite, SaveTaskCommand,
    SaveTaskMode, StaffFact, StaffTaskActor, StaffTaskFactDelegates, StaffTaskFuture, TaskScope,
    save_task_mode,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

const NOW: i64 = 1_788_000_000_000;

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool, template: &str, schedule: &str, at: i64) {
    sqlx::query(
        "INSERT INTO routine_templates(id,company_id,branch_id,department_id,title,description,\
         creator_user_id,assignee_user_id,start_approvers_json,completion_approvers_json,\
         published,version,created_at,updated_at) \
         VALUES($1,'company','branch','department','Scheduled checks','Run the checks',\
         'creator','assignee','[]','[]',1,3,$2,$2)",
    )
    .bind(template)
    .bind(NOW)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,\
         occurrence_time,start_date,due_offset_hours,state,next_occurrence_at,version,\
         created_at,updated_at) \
         VALUES($1,$2,'company','daily','Asia/Bangkok','09:00','2026-01-01',24,'active',$3,2,$4,$4)",
    )
    .bind(schedule)
    .bind(template)
    .bind(at)
    .bind(NOW)
    .execute(pool.raw())
    .await
    .unwrap();
}

struct TestSources;

impl StaffTaskFactDelegates for TestSources {
    fn active_staff<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _user_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<StaffFact>, BusinessError> {
        Box::pin(async {
            Ok(Some(StaffFact {
                branch_id: Some("branch".into()),
                department_id: Some("department".into()),
            }))
        })
    }

    fn project<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _company_id: &'a str,
        _project_id: &'a str,
    ) -> StaffTaskFuture<'a, Option<ProjectFact>, BusinessError> {
        Box::pin(async {
            Ok(Some(ProjectFact {
                owner_user_id: "owner".into(),
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
        _approvers: &'a [String],
        _service_task: bool,
    ) -> StaffTaskFuture<'a, RouteFact, BusinessError> {
        Box::pin(async { Ok(RouteFact { blocked_reason: None }) })
    }

    fn open_approval<'a>(
        &'a mut self,
        _unit: Option<&'a mut sqlx::PgConnection>,
        _key: &'a str,
        _task: &'a erpx_server_operations::public::staff_tasks::TaskRow,
        _phase: &'a str,
        _route: &'a RouteFact,
        _version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

struct DisabledUnit;

impl RoutineOccurrenceUnit for DisabledUnit {
    fn run<'a>(
        &'a mut self,
        connection: &'a mut sqlx::PgConnection,
        selected: &'a RoutineScheduleRow,
        at: i64,
        now: i64,
    ) -> RoutineOccurrenceFuture<'a, Option<OccurrenceOutcome>> {
        Box::pin(async move {
            let Some(source) = occurrence_source(connection, &selected.company_id, &selected.id, at)
                .await?
            else {
                return Ok(None);
            };
            let original = &source.schedule;
            if original.state != "active"
                || original.version != selected.version
                || original.next_occurrence_at != Some(at)
            {
                return Ok(None);
            }
            let template = source.template.as_ref().ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "TEMPLATE_UNAVAILABLE",
                    "The published routine template is unavailable.",
                )
            })?;
            if !template.is_published() {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unavailable,
                    "TEMPLATE_UNAVAILABLE",
                    "The published routine template is unavailable.",
                )
                .into());
            }
            let facts = OccurrenceSourceFacts {
                pause_exists: source.pause_exists,
                pause_ended_at: source.pause_ended_at,
                prior: source.prior,
                start_blocked: None,
                completion_blocked: None,
                project_available: true,
            };
            let plan = plan_occurrence(template, original, &facts, at)?;
            let generated = !plan.state.is_empty();
            if generated {
                let actor = StaffTaskActor {
                    user_id: template.creator_user_id.clone(),
                    assignment_id: String::new(),
                    company_id: template.company_id.clone(),
                    branch_id: template.branch_id.clone(),
                    department_id: template.department_id.clone(),
                    permissions: Vec::new(),
                    modules: Vec::new(),
                    expires_at: i64::MAX,
                };
                let input = SaveTaskCommand {
                    title: template.title.clone(),
                    description: template.description.clone(),
                    assignee_user_id: template.assignee_user_id.clone(),
                    project_id: template.project_id.clone(),
                    start_approvers: plan.start_approvers.clone(),
                    completion_approvers: plan.completion_approvers.clone(),
                    due_at: Some(
                        erpx_server_operations::public::routine_occurrences::due_at_iso(plan.due_at)
                            .unwrap(),
                    ),
                    expected_version: 1,
                    reason: String::new(),
                };
                let mut repository = PgStaffTasks::new(&mut *connection);
                let mut sources = PgStaffTaskSources::new(TestSources);
                save_task_mode(
                    &actor,
                    &mut repository,
                    &mut sources,
                    None,
                    &input,
                    &plan.key,
                    false,
                    now,
                    SaveTaskMode::Routine(RoutineTaskWrite {
                        template_id: template.id.clone(),
                        template_version: template.version,
                        branch_id: template.branch_id.clone(),
                        department_id: template.department_id.clone(),
                        state: plan.state.clone(),
                        blocked_reason: plan.blocked_reason.clone(),
                        notification: plan.notification.clone(),
                        creator_notification: plan.creator_notification.clone(),
                        open_start_approval: plan.open_start_approval,
                        request_fingerprint: format!("routine.occurrence\n{}\n", plan.task_id),
                        creator_user_id: template.creator_user_id.clone(),
                        record_id: Some(plan.task_id.clone()),
                        schedule_id: Some(original.id.clone()),
                        occurrence_at: Some(at),
                        due_at_millis: Some(plan.due_at),
                        require_write: false,
                        require_current_staff: false,
                        write_ledger: false,
                    }),
                )
                .await?;
            }
            stage_occurrence(
                connection,
                None,
                &original.id,
                &original.company_id,
                original.version,
                &template.id,
                at,
                plan.next,
                generated,
                &plan.task_id,
                &plan.state,
                template.branch_id.as_deref(),
                template.department_id.as_deref(),
                now,
                &[],
            )
            .await?;
            Ok(Some(OccurrenceOutcome {
                next: plan.next,
                generated,
            }))
        })
    }
}

fn next(rule: &RoutineScheduleRow, after: i64) -> i64 {
    let rule = erpx_server_operations::public::routine_commands::RoutineRule {
        frequency: erpx_server_operations::public::routine_commands::RoutineFrequency::Daily,
        timezone: rule.timezone.clone(),
        occurrence_time: rule.occurrence_time.clone(),
        start_date: rule.start_date.clone(),
        end_date: rule.end_date.clone(),
        weekday: rule.weekday,
        month_day: rule.month_day,
    };
    next_occurrence(&rule, after).unwrap().unwrap()
}

#[tokio::test]
async fn generates_bounded_cursor_ordered_tasks_with_creator_schedule_and_due_offset() {
    fixture::with_database(3, |holding| async move {
        migrate(&holding).await;
        seed(&holding, "template", "schedule", NOW).await;
        let policy = OccurrencePolicy::default();
        let first = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW + 5 * 86_400_000,
            2,
            policy.clone(),
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(first.generated, 2);
        assert_eq!(first.examined, 2);
        let rows = sqlx::query(
            "SELECT id,creator_user_id,assignee_user_id,due_at,schedule_id,occurrence_at,state,\
             template_id,template_version FROM staff_tasks ORDER BY occurrence_at",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<String, _>("id"), format!("task-schedule-{NOW}"));
        assert_eq!(rows[0].get::<String, _>("creator_user_id"), "creator");
        assert_eq!(rows[0].get::<String, _>("assignee_user_id"), "assignee");
        assert_eq!(rows[0].get::<i64, _>("due_at"), NOW + 86_400_000);
        assert_eq!(rows[0].get::<String, _>("schedule_id"), "schedule");
        assert_eq!(rows[0].get::<i64, _>("occurrence_at"), NOW);
        assert_eq!(rows[0].get::<String, _>("state"), "ready");
        assert_eq!(rows[0].get::<String, _>("template_id"), "template");
        assert_eq!(rows[0].get::<i64, _>("template_version"), 3);
        let cursor = sqlx::query_scalar::<_, i64>(
            "SELECT next_occurrence_at FROM routine_schedules WHERE id='schedule'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(cursor, next(&seed_schedule(&holding).await, NOW + 86_400_000));

        let second = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW + 5 * 86_400_000,
            20,
            policy,
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(second.generated, 4);
        assert_eq!(second.examined, 4);
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_tasks")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        assert_eq!(count, 6);
        let audit = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE action='routine.occurrence'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(audit, 6);
        let outbox = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox_events WHERE event_type='task.generated'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 6);
    })
    .await;
}

async fn seed_schedule(pool: &DatabasePool) -> RoutineScheduleRow {
    let image: String =
        sqlx::query_scalar("SELECT to_jsonb(s)::text FROM routine_schedules s WHERE s.id='schedule'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
    serde_json::from_str(&image).unwrap()
}

#[tokio::test]
async fn skips_pauses_and_existing_occurrences_while_advancing_the_cursor() {
    fixture::with_database(3, |holding| async move {
        migrate(&holding).await;
        seed(&holding, "template", "schedule", NOW).await;
        sqlx::query(
            "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at,ended_at) \
             VALUES('pause','schedule',$1,$2)",
        )
        .bind(NOW + 86_400_000)
        .bind(NOW + 3 * 86_400_000)
        .execute(holding.raw())
        .await
        .unwrap();
        let pass = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW + 5 * 86_400_000,
            20,
            OccurrencePolicy::default(),
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(pass.generated, 4);
        assert_eq!(pass.examined, 5);
        let occurrences = sqlx::query_scalar::<_, i64>(
            "SELECT occurrence_at FROM staff_tasks ORDER BY occurrence_at",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            occurrences,
            vec![
                NOW,
                NOW + 55_200_000,
                NOW + 314_400_000,
                NOW + 400_800_000
            ]
        );
        // Re-selecting the first instant proves prior-occurrence deduplication:
        // the cursor advances without a second task.
        sqlx::query("UPDATE routine_schedules SET next_occurrence_at=$1 WHERE id='schedule'")
            .bind(NOW)
            .execute(holding.raw())
            .await
            .unwrap();
        let pass = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW + 5 * 86_400_000,
            20,
            OccurrencePolicy::default(),
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(pass.generated, 0);
        assert_eq!(pass.examined, 5);
        let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_tasks")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        assert_eq!(count, 4);
    })
    .await;
}

#[tokio::test]
async fn records_source_errors_and_respects_the_production_rollout_gate() {
    fixture::with_database(3, |holding| async move {
        migrate(&holding).await;
        seed(&holding, "template", "schedule", NOW).await;
        sqlx::query("UPDATE routine_templates SET published=0 WHERE id='template'")
            .execute(holding.raw())
            .await
            .unwrap();
        let pass = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW,
            1,
            OccurrencePolicy::default(),
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(pass.generated, 0);
        assert_eq!(pass.examined, 1);
        let error = sqlx::query_scalar::<_, Option<String>>(
            "SELECT last_error FROM routine_schedules WHERE id='schedule'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(error.as_deref(), Some("The published routine template is unavailable."));

        sqlx::query("UPDATE routine_templates SET published=1 WHERE id='template'")
            .execute(holding.raw())
            .await
            .unwrap();
        sqlx::query("UPDATE routine_schedules SET last_error=NULL WHERE id='schedule'")
            .execute(holding.raw())
            .await
            .unwrap();
        let policy = OccurrencePolicy {
            production: true,
            scope_sql: None,
        };
        let pass = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW,
            1,
            policy.clone(),
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(pass.generated, 0);
        assert_eq!(pass.examined, 0);
        sqlx::query(
            "INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,\
             reason,changed_by,changed_at) VALUES('company','operations','pilot',1,'test','test',$1)",
        )
        .bind(NOW)
        .execute(holding.raw())
        .await
        .unwrap();
        let pass = erpx_server_operations::public::routine_occurrences::generate_due_occurrences(
            holding.clone(),
            NOW,
            1,
            policy,
            DisabledUnit,
        )
        .await
        .unwrap();
        assert_eq!(pass.generated, 1);
        assert_eq!(pass.examined, 1);
    })
    .await;
}

#[tokio::test]
async fn stages_the_captured_authorization_cursor_journal_and_contiguous_step() {
    fixture::with_database(3, |holding| async move {
        migrate(&holding).await;
        seed(&holding, "template", "schedule", NOW).await;
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) \
             VALUES('company','company','company','Company','company')",
        )
        .execute(holding.raw())
        .await
        .unwrap();
        let source_hash: String = sqlx::query_scalar(
            "SELECT encode(sha256(convert_to((SELECT jsonb_build_object(\
             'template',to_jsonb(t),'schedule',to_jsonb(s)-ARRAY['next_occurrence_at',\
             'last_generated_at','last_error','updated_at','paused_at','state'])::text \
             FROM routine_templates t JOIN routine_schedules s \
             ON s.template_id=t.id AND s.company_id=t.company_id \
             WHERE t.id='template' AND s.id='schedule'),'UTF8')),'hex')",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) \
             VALUES('auth','routine.schedule.resume','runner','hash','schedule','{}',$1,'{}')",
        )
        .bind(NOW)
        .execute(holding.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO work_routine_execution_authorizations(command_id,schedule_id,template_id,\
             company_id,kind,actor_id,assignment_id,authority_json,source_hash,template_version,\
             schedule_version,initial_occurrence_at,created_at) \
             VALUES('auth','schedule','template','company','resume','runner','assignment',\
             '{\"companyId\":\"company\",\"userId\":\"runner\",\"assignmentId\":\"assignment\",\
             \"role\":\"employee\",\"assignmentVersion\":1}',$1,3,2,$2,$3)",
        )
        .bind(&source_hash)
        .bind(NOW)
        .bind(NOW)
        .execute(holding.raw())
        .await
        .unwrap();
        let mut tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let authorization =
            erpx_server_operations::public::routine_occurrences::require_authorization(
                &mut tx, "schedule", 2,
            )
            .await
            .unwrap();
        let actor = authorization.actor().unwrap();
        let template = sqlx::query_scalar::<_, String>(
            "SELECT to_jsonb(t)::text FROM routine_templates t WHERE t.id='template'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let template: erpx_server_operations::public::routine_commands::RoutineTemplateRow =
            serde_json::from_str(&template).unwrap();
        let schedule = seed_schedule(&holding).await;
        erpx_server_operations::public::routine_occurrences::require_authorization_source(
            &mut tx,
            &authorization,
            &template,
            &schedule,
            &actor,
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,\
             creator_user_id,assignee_user_id,due_at,start_approvers_json,completion_approvers_json,\
             state,template_id,template_version,schedule_id,occurrence_at,created_at,updated_at) \
             VALUES('task-schedule-'||$1,'company','branch','department','Scheduled checks',\
             'Run the checks','creator','assignee',$2,'[]','[]','ready','template',3,'schedule',$1,$3,$3)",
        )
        .bind(NOW)
        .bind(NOW + 86_400_000)
        .bind(NOW)
        .execute(&mut *tx)
        .await
        .unwrap();
        erpx_server_operations::public::routine_occurrences::stage_occurrence(
            &mut tx,
            Some(&authorization),
            "schedule",
            "company",
            2,
            "template",
            NOW,
            Some(NOW + 55_200_000),
            true,
            &format!("task-schedule-{NOW}"),
            "ready",
            Some("branch"),
            Some("department"),
            NOW,
            &[],
        )
        .await
        .unwrap();
        erpx_server_operations::public::routine_occurrences::require_lineage(
            &mut tx,
            &authorization,
            Some(NOW + 55_200_000),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let step = sqlx::query(
            "SELECT authorization_command_id,occurrence_at,next_occurrence_at,task_id,generated,\
             source_hash,effects_hash FROM work_routine_execution_steps WHERE schedule_id='schedule'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(step.get::<String, _>("authorization_command_id"), "auth");
        assert_eq!(step.get::<i64, _>("occurrence_at"), NOW);
        assert_eq!(
            step.get::<i64, _>("next_occurrence_at"),
            NOW + 55_200_000
        );
        assert_eq!(
            step.get::<String, _>("task_id"),
            format!("task-schedule-{NOW}")
        );
        assert!(step.get::<bool, _>("generated"));
        assert_eq!(step.get::<String, _>("source_hash"), source_hash);
        assert_eq!(step.get::<String, _>("effects_hash").len(), 64);
        let audit = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE action='routine.occurrence' \
             AND record_id='task-schedule-'||$1",
        )
        .bind(NOW)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(audit, 1);
        let outbox = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox_events WHERE event_type='task.generated' \
             AND aggregate_id='task-schedule-'||$1",
        )
        .bind(NOW)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(outbox, 1);
        sqlx::query("UPDATE routine_templates SET description='Changed' WHERE id='template'")
            .execute(holding.raw())
            .await
            .unwrap();
        let mut tx = holding.raw().begin().await.unwrap();
        let schedule = seed_schedule(&holding).await;
        let error = erpx_server_operations::public::routine_occurrences::require_authorization_source(
            &mut tx,
            &authorization,
            &template,
            &schedule,
            &actor,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            PgTransactionError::Business(error) if error.code == "WORK_ROUTINE_SOURCE_CHANGED"
        ));
    })
    .await;
}

#[tokio::test]
async fn due_schedule_selection_is_cursor_ordered_and_bounded() {
    fixture::with_database(3, |holding| async move {
        migrate(&holding).await;
        seed(&holding, "template-a", "schedule-a", NOW + 2).await;
        seed(&holding, "template-b", "schedule-b", NOW).await;
        seed(&holding, "template-c", "schedule-c", NOW + 1).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let schedules = due_schedules(
            &mut *connection,
            NOW + 2,
            2,
            &OccurrencePolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            schedules.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["schedule-b", "schedule-c"]
        );
    })
    .await;
}
