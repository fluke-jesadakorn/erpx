//! Native manual routine run over real migrated PostgreSQL. The owner adapter
//! and application commit one serializable unit: occurrence row, command
//! receipt/audit/outbox, assignee/approver notifications and the start approval.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::routine_commands::{
    PgRoutineCommands, RoutineCommandRepository, RunRoutineTemplateCommand, run_template,
};
use erpx_server_operations::public::staff_tasks::{
    PgStaffTaskSources, PgStaffTasks, ProjectFact, RouteFact, StaffFact, StaffTaskFactDelegates,
    StaffTaskFuture, TaskScope, notify_staff_task_approval,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "TEST_PERSISTENCE_FAILED",
            "The test persistence operation failed.",
        ),
    }
}

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

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

fn actor() -> erpx_server_operations::public::staff_tasks::StaffTaskActor {
    erpx_server_operations::public::staff_tasks::StaffTaskActor {
        user_id: "runner".into(),
        assignment_id: "assignment-runner".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}

async fn seed_template(
    pool: &DatabasePool,
    id: &str,
    start: &[&str],
    completion: &[&str],
    published: i64,
    project_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO routine_templates(id,company_id,branch_id,department_id,title,description,\
         creator_user_id,assignee_user_id,project_id,start_approvers_json,completion_approvers_json,\
         published,version,created_at,updated_at) VALUES($1,'company','branch','department',$2,\
         'Run once','creator','assignee',$3,$4,$5,$6,3,$7,$7)",
    )
    .bind(id)
    .bind(format!("Routine {id}"))
    .bind(project_id)
    .bind(serde_json::to_string(start).unwrap())
    .bind(serde_json::to_string(completion).unwrap())
    .bind(published)
    .bind(NOW)
    .execute(pool.raw())
    .await
    .unwrap();
}

#[derive(Clone, Copy)]
struct Sources {
    now: i64,
    blocked: Option<&'static str>,
    project_missing: bool,
}

impl StaffTaskFactDelegates for Sources {
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
        Box::pin(async move {
            Ok((!self.project_missing).then(|| ProjectFact {
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
        Box::pin(async move {
            Ok(RouteFact {
                blocked_reason: self.blocked.map(str::to_owned),
            })
        })
    }

    fn open_approval<'a>(
        &'a mut self,
        unit: Option<&'a mut sqlx::PgConnection>,
        key: &'a str,
        task: &'a erpx_server_operations::public::staff_tasks::TaskRow,
        phase: &'a str,
        _route: &'a RouteFact,
        version: i64,
    ) -> StaffTaskFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let connection = unit.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "TEST_WORKFLOW_SOURCE_REQUIRED",
                    "The test workflow source is unavailable.",
                )
            })?;
            let approvers = match phase {
                "start" => task.start_approvers()?,
                "completion" => task.completion_approvers()?,
                _ => {
                    return Err(BusinessError::invalid(
                        "TASK_APPROVAL_PHASE_INVALID",
                        "Use a start or completion task approval phase.",
                    ));
                }
            };
            // The Workflow owner writes these rows in production; this test
            // fixture pins the exact call contract on the same unit.
            let instance_id = format!("approval-{key}");
            sqlx::query(
                "INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,\
                 record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,\
                 version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,'task',$5,$6,1,$7,$8,$9,0,1,$10,$10)",
            )
            .bind(&instance_id)
            .bind(&task.company_id)
            .bind(task.branch_id.as_deref())
            .bind(task.department_id.as_deref())
            .bind(&task.id)
            .bind(phase)
            .bind(&task.creator_user_id)
            .bind(&task.assignee_user_id)
            .bind(if approvers.is_empty() { "approved" } else { "pending" })
            .bind(self.now)
            .execute(&mut *connection)
            .await
            .map_err(|_| {
                BusinessError::new(
                    FailureKind::Unavailable,
                    "TEST_APPROVAL_SOURCE_UNAVAILABLE",
                    "The test approval source is unavailable.",
                )
            })?;
            for (ordinal, user) in approvers.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,\
                     authority,state,approver_pool_json) VALUES($1,$2,$3,$4,'named','pending',$5)",
                )
                .bind(format!("{instance_id}-{ordinal}"))
                .bind(&instance_id)
                .bind(ordinal as i64)
                .bind(user)
                .bind(serde_json::to_string(&[user]).unwrap())
                .execute(&mut *connection)
                .await
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::Unavailable,
                        "TEST_APPROVAL_SOURCE_UNAVAILABLE",
                        "The test approval source is unavailable.",
                    )
                })?;
            }
            if let Some(first) = approvers.first() {
                notify_staff_task_approval(
                    connection,
                    key,
                    &task.company_id,
                    first,
                    &task.id,
                    version,
                    phase,
                    self.now,
                )
                .await?;
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn creates_replays_and_conflicts_one_manual_routine_run() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed_template(
            &holding,
            "template-start",
            &["start-reviewer"],
            &["completion-reviewer"],
            1,
            None,
        )
        .await;
        let due_at = "2026-12-01T02:00:00Z";
        let first = {
            let mut tx = serializable(&holding).await;
            let template = {
                let mut repo = PgRoutineCommands::new(&mut tx);
                repo.template("company", "template-start").await.unwrap().unwrap()
            };
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: None,
                project_missing: false,
            });
            let receipt = run_template(
                &actor(),
                &mut repo,
                &mut sources,
                Some(&template),
                "template-start",
                &RunRoutineTemplateCommand {
                    due_at: due_at.into(),
                },
                "run-key-start",
                NOW,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            receipt
        };
        assert_eq!(
            first,
            erpx_server_operations::public::staff_tasks::TaskReceipt::new(
                "task-run-key-start",
                "start_approval",
                1
            )
        );
        let row = sqlx::query(
            "SELECT state,creator_user_id,assignee_user_id,due_at,project_id,blocked_reason,\
             template_id,template_version,start_approvers_json,completion_approvers_json \
             FROM staff_tasks WHERE id='task-run-key-start' AND company_id='company'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(row.get::<String, _>("state"), "start_approval");
        assert_eq!(row.get::<String, _>("creator_user_id"), "runner");
        assert_eq!(row.get::<String, _>("assignee_user_id"), "assignee");
        assert_eq!(row.get::<i64, _>("due_at"), 1_796_090_400_000);
        assert_eq!(row.get::<Option<String>, _>("project_id"), None);
        assert_eq!(row.get::<Option<String>, _>("blocked_reason"), None);
        assert_eq!(row.get::<String, _>("template_id"), "template-start");
        assert_eq!(row.get::<i64, _>("template_version"), 3);
        assert_eq!(row.get::<String, _>("start_approvers_json"), r#"["start-reviewer"]"#);
        assert_eq!(
            row.get::<String, _>("completion_approvers_json"),
            r#"["completion-reviewer"]"#
        );
        let receipt = sqlx::query(
            "SELECT operation,record_id,result_json,authority_json \
             FROM command_receipts WHERE command_id='run-key-start'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(receipt.get::<String, _>("operation"), "routine.run");
        assert_eq!(receipt.get::<String, _>("record_id"), "task-run-key-start");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&receipt.get::<String, _>("result_json"))
                .unwrap(),
            serde_json::json!({"id":"task-run-key-start","state":"start_approval","version":1})
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&receipt.get::<String, _>("authority_json"))
                .unwrap()["companyId"],
            "company"
        );
        let audit = sqlx::query(
            "SELECT action,record_type,record_version FROM audit_events \
             WHERE command_id='run-key-start'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(audit.get::<String, _>("action"), "routine.run");
        assert_eq!(audit.get::<String, _>("record_type"), "task");
        assert_eq!(audit.get::<i64, _>("record_version"), 1);
        let outbox = sqlx::query(
            "SELECT event_type,aggregate_type,aggregate_id,aggregate_version FROM outbox_events \
             WHERE command_id='run-key-start'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(outbox.get::<String, _>("event_type"), "routine.run");
        assert_eq!(outbox.get::<String, _>("aggregate_type"), "task");
        assert_eq!(
            outbox.get::<String, _>("aggregate_id"),
            "task-run-key-start"
        );
        assert_eq!(outbox.get::<i64, _>("aggregate_version"), 1);
        let notices = sqlx::query(
            "SELECT user_id,message FROM staff_notifications \
             WHERE record_type='task' AND record_id='task-run-key-start' ORDER BY user_id",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            notices
                .iter()
                .map(|row| (
                    row.get::<String, _>("user_id"),
                    row.get::<String, _>("message")
                ))
                .collect::<Vec<_>>(),
            vec![
                ("assignee".to_owned(), "Routine task assigned to you".to_owned()),
                (
                    "start-reviewer".to_owned(),
                    "Start approval needs your decision".to_owned()
                ),
            ]
        );
        let instance = sqlx::query(
            "SELECT id,state,phase,current_step,version FROM approval_instances \
             WHERE record_type='task' AND record_id='task-run-key-start'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(instance.get::<String, _>("id"), "approval-run-key-start");
        assert_eq!(instance.get::<String, _>("state"), "pending");
        assert_eq!(instance.get::<String, _>("phase"), "start");
        assert_eq!(instance.get::<i64, _>("current_step"), 0);
        let steps = sqlx::query(
            "SELECT ordinal,approver_user_id,authority,state FROM approval_steps \
             WHERE instance_id='approval-run-key-start' ORDER BY ordinal",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].get::<i64, _>("ordinal"), 0);
        assert_eq!(
            steps[0].get::<String, _>("approver_user_id"),
            "start-reviewer"
        );
        assert_eq!(steps[0].get::<String, _>("authority"), "named");
        assert_eq!(steps[0].get::<String, _>("state"), "pending");

        // Same key and request replay the exact receipt without a second row.
        let replay = {
            let mut tx = serializable(&holding).await;
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: None,
                project_missing: false,
            });
            let receipt = run_template(
                &actor(),
                &mut repo,
                &mut sources,
                None,
                "template-start",
                &RunRoutineTemplateCommand {
                    due_at: due_at.into(),
                },
                "run-key-start",
                NOW,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            receipt
        };
        assert_eq!(replay, first);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM staff_tasks")
                .fetch_one(holding.raw())
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM approval_instances WHERE record_type='task'"
            )
            .fetch_one(holding.raw())
            .await
            .unwrap(),
            1
        );

        // A different due instant under the same key is a retained conflict.
        let conflict = {
            let mut tx = serializable(&holding).await;
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: None,
                project_missing: false,
            });
            business(
                run_template(
                    &actor(),
                    &mut repo,
                    &mut sources,
                    None,
                    "template-start",
                    &RunRoutineTemplateCommand {
                        due_at: "2026-12-02T02:00:00Z".into(),
                    },
                    "run-key-start",
                    NOW,
                )
                .await
                .unwrap_err(),
            )
        };
        assert_eq!(conflict.code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn refuses_unpublished_sources_and_keeps_project_or_route_blockers_draft() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        sqlx::query(
            "INSERT INTO staff_projects(id,company_id,title,owner_user_id,version,created_at,updated_at) \
             VALUES('missing-project','company','Unavailable project','owner',1,$1,$1)",
        )
        .bind(NOW)
        .execute(holding.raw())
        .await
        .unwrap();
        seed_template(&holding, "template-unpublished", &[], &[], 0, None).await;
        seed_template(
            &holding,
            "template-project",
            &[],
            &["completion-reviewer"],
            1,
            Some("missing-project"),
        )
        .await;
        seed_template(
            &holding,
            "template-blocked",
            &["start-reviewer"],
            &[],
            1,
            None,
        )
        .await;
        let unpublished = {
            let mut tx = serializable(&holding).await;
            let template = {
                let mut repo = PgRoutineCommands::new(&mut tx);
                repo.template("company", "template-unpublished")
                    .await
                    .unwrap()
                    .unwrap()
            };
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: None,
                project_missing: false,
            });
            let error = business(
                run_template(
                    &actor(),
                    &mut repo,
                    &mut sources,
                    Some(&template),
                    "template-unpublished",
                    &RunRoutineTemplateCommand {
                        due_at: "2026-12-01T02:00:00Z".into(),
                    },
                    "run-key-unpublished",
                    NOW,
                )
                .await
                .unwrap_err(),
            );
            assert_eq!(error.code, "TEMPLATE_UNPUBLISHED");
            tx.rollback().await.unwrap();
            error
        };
        assert_eq!(unpublished.code, "TEMPLATE_UNPUBLISHED");
        let project = {
            let mut tx = serializable(&holding).await;
            let template = {
                let mut repo = PgRoutineCommands::new(&mut tx);
                repo.template("company", "template-project")
                    .await
                    .unwrap()
                    .unwrap()
            };
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: None,
                project_missing: true,
            });
            let receipt = run_template(
                &actor(),
                &mut repo,
                &mut sources,
                Some(&template),
                "template-project",
                &RunRoutineTemplateCommand {
                    due_at: "2026-12-01T02:00:00Z".into(),
                },
                "run-key-project",
                NOW,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            receipt
        };
        assert_eq!(project.state, "draft");
        let blocked = sqlx::query_scalar::<_, String>(
            "SELECT blocked_reason FROM staff_tasks WHERE id='task-run-key-project'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(blocked, "The project is unavailable.");
        let blocked_receipt = {
            let mut tx = serializable(&holding).await;
            let template = {
                let mut repo = PgRoutineCommands::new(&mut tx);
                repo.template("company", "template-blocked")
                    .await
                    .unwrap()
                    .unwrap()
            };
            let mut repo = PgStaffTasks::new(&mut tx);
            let mut sources = PgStaffTaskSources::new(Sources {
                now: NOW,
                blocked: Some("The approval route is unavailable."),
                project_missing: false,
            });
            let receipt = run_template(
                &actor(),
                &mut repo,
                &mut sources,
                Some(&template),
                "template-blocked",
                &RunRoutineTemplateCommand {
                    due_at: "2026-12-01T02:00:00Z".into(),
                },
                "run-key-blocked",
                NOW,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            receipt
        };
        assert_eq!(blocked_receipt.state, "draft");
        let notices = sqlx::query(
            "SELECT user_id,message FROM staff_notifications \
             WHERE record_id='task-run-key-blocked' ORDER BY user_id",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(
            notices
                .iter()
                .map(|row| (
                    row.get::<String, _>("user_id"),
                    row.get::<String, _>("message")
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "assignee".to_owned(),
                    "Routine task blocked: correction required".to_owned()
                ),
                (
                    "runner".to_owned(),
                    "Routine blocked: The approval route is unavailable.".to_owned()
                ),
            ]
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM approval_instances WHERE record_type='task' \
                 AND record_id='task-run-key-blocked'"
            )
            .fetch_one(holding.raw())
            .await
            .unwrap(),
            0
        );
    })
    .await;
}
