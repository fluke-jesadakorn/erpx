//! Native preventive-plan create/revise/activate/pause/cancel/generate over the
//! actual Workflow ledger, owner rows and PostgreSQL IANA calendar.
//!
//! The test-local authority stands in for composition's retained Account/IAM/
//! purchase authority; the command unit, CAS, history, occurrence, work
//! creation, notification, receipt and outbox are the real owner effects.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_maintenance::public::preventive_plan::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, HoldingId, IdentifierSource,
    RecordId, TimestampMillis, UserId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, CommandOutcome, PgCommandAuthority,
    WorkflowRequest,
};
use std::path::Path;

const NOW: i64 = 1_800_000_000_000;
const EXPIRES: i64 = NOW + 3_600_000;

struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}

struct Ids {
    prefix: String,
    count: u64,
}
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.count += 1;
        RecordId::new(format!("{}-{}", self.prefix, self.count)).unwrap()
    }
}

struct Authority {
    actor: CommandActor,
}
impl PgCommandAuthority for Authority {
    async fn lock_sources(
        &mut self,
        _: &mut sqlx::PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut sqlx::PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, PgTransactionError<BusinessError>> {
        Ok(self.actor.clone())
    }
    async fn verify_command(
        &mut self,
        _: &mut sqlx::PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if actor != &self.actor {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PREVENTIVE_TEST_AUTHORITY_CHANGED",
                "The test authority changed.",
            )
            .into());
        }
        Ok(())
    }
}

fn actor(user: &str, branch: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new("preventive-assignment").unwrap(),
        role: "custom_maintenance".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: CommandField::Value(branch.into()),
        department_id: CommandField::Null,
        authority: CommandField::Omitted,
        iam: CommandField::Omitted,
        assignment_scope_kind: "branch".into(),
        assignment_scope_node_id: Some(branch.into()),
    }
}

fn receipt(value: serde_json::Value) -> CommandJson {
    CommandJson::parse(&value.to_string()).unwrap()
}

fn definition(owner: &str, first_due_at: i64) -> PreventivePlanDefinition {
    PreventivePlanDefinition {
        asset_id: "asset".into(),
        asset_version: 1,
        title: "Daily safety inspection".into(),
        priority: "normal".into(),
        owner_id: owner.into(),
        interval_days: 1,
        first_due_at,
        time_zone: "America/New_York".into(),
        lead_days: 0,
        expected_version: None,
        reason: "Recurring preventive safety inspection".into(),
    }
}

fn command_parts(definition: &PreventivePlanDefinition) -> serde_json::Value {
    serde_json::json!({
        "assetId": definition.asset_id,
        "assetVersion": definition.asset_version,
        "title": definition.title,
        "priority": definition.priority,
        "ownerId": definition.owner_id,
        "intervalDays": definition.interval_days,
        "firstDueAt": definition.first_due_at,
        "timeZone": definition.time_zone,
        "leadDays": definition.lead_days,
        "reason": definition.reason,
    })
}

fn action_command(
    key: &str,
    plan_id: &str,
    action: MaintenancePreventiveAction,
    expected: i64,
) -> PreventivePlanCommand {
    PreventivePlanCommand::action(
        CommandKey::new(key).unwrap(),
        RecordId::new(plan_id).unwrap(),
        MaintenancePreventiveActionInput {
            expected_version: expected,
            reason: "Reviewed current preventive definition and due work".into(),
            action,
        },
        receipt(serde_json::json!({
            "action": action.action(),
            "expectedVersion": expected,
            "reason": "Reviewed current preventive definition and due work",
        })),
    )
}

async fn run(
    holding: &DatabasePool,
    user: &str,
    branch: &str,
    can_review: bool,
    command: &PreventivePlanCommand,
) -> Result<CommandOutcome, PgTransactionError<BusinessError>> {
    let mut transaction = holding.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let access = PreventivePlanAccess {
        holding_id: HoldingId::new("h001").unwrap(),
        actor_id: user.into(),
        can_review,
        expires_at: EXPIRES,
    };
    let mut authority = Authority {
        actor: actor(user, branch),
    };
    let mut identifiers = Ids {
        prefix: format!("preventive-aux-{}", command.key().as_str()),
        count: 0,
    };
    let outcome = execute_preventive_plan_command(
        CompanyId::new("company").unwrap(),
        &mut transaction,
        &access,
        &mut authority,
        &FixedClock,
        &mut identifiers,
        command,
    )
    .await;
    match outcome {
        Ok(outcome) => {
            transaction.commit().await.unwrap();
            Ok(outcome)
        }
        Err(error) => {
            transaction.rollback().await.unwrap();
            Err(error)
        }
    }
}

fn expect_error(
    result: Result<CommandOutcome, PgTransactionError<BusinessError>>,
) -> PgTransactionError<BusinessError> {
    match result {
        Ok(_) => panic!("expected a refused preventive command"),
        Err(error) => error,
    }
}

fn business(error: &PgTransactionError<BusinessError>) -> &BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected business error, got {other:?}"),
    }
}

async fn setup(holding: &DatabasePool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(holding, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Acme',NULL,'company',NULL,'active'),\
               ('branch','branch','branch','Branch','company','company','branch','active');\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) \
         VALUES('*','custom_maintenance','Maintenance',false,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
         VALUES('*','custom_maintenance',1,'[\"records.read\",\"records.write\",\"records.review\"]','[]','[\"branch\"]','fixture',0);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id) \
         VALUES('requester-assignment','requester','custom_maintenance','[\"maintenance\"]','company',1799999000000,1800003600000,1,'branch','branch'),\
               ('reviewer-assignment','reviewer','custom_maintenance','[\"maintenance\"]','company',1799999000000,1800003600000,1,'branch','branch'),\
               ('owner-assignment','owner','custom_maintenance','[\"maintenance\"]','company',1799999000000,1800003600000,1,'branch','branch');\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) \
         VALUES('owner-person','owner','company','Synthetic Owner','TH','fixture',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
         VALUES('owner-engagement','owner-person','company','employee','salaried','active','2026-01-01','Technician','THB','fixture',0,0);\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) \
         VALUES('company','owner','local:owner','owner-person','active','local');\
         INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) \
         VALUES('asset','company','branch','SYNTHETIC','Synthetic asset','synthetic','THB','fixture',0,0)",
    )
    .execute(holding.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn preventive_plan_command_retains_activation_dst_generation_and_history() {
    fixture::with_database(2, |holding| async move {
        setup(&holding).await;

        // Create: canonical receipt v1 draft, then independent activation.
        let create = definition(
            "owner",
            // 2026-03-07T07:30:59Z -> 02:30 America/New_York.
            1_772_868_659_000,
        );
        let created = PreventivePlanCommand::create(
            CommandKey::new("preventive-create-one").unwrap(),
            create.clone(),
            receipt(command_parts(&create)),
        )
        .unwrap();
        let plan_id = created.id().as_str().to_owned();
        assert_eq!(plan_id, "maintenance-plan-preventive-create-one");
        let outcome = run(&holding, "requester", "branch", false, &created)
            .await
            .unwrap();
        assert_eq!(outcome.result.as_str(), r#"{"id":"maintenance-plan-preventive-create-one","status":"draft","version":1}"#);

        let activate = action_command(
            "preventive-activate-one",
            &plan_id,
            MaintenancePreventiveAction::Activate,
            1,
        );
        let denied = expect_error(
            run(&holding, "requester", "branch", true, &activate).await,
        );
        assert_eq!(business(&denied).code, "INDEPENDENT_PLAN_REVIEW_REQUIRED");
        let activated = run(&holding, "reviewer", "branch", true, &activate)
            .await
            .unwrap();
        assert_eq!(activated.result.as_str(), r#"{"id":"maintenance-plan-preventive-create-one","status":"active","version":2}"#);

        // An ended engagement fails the eligible-owner assertion before work.
        sqlx::query("UPDATE hr_engagements SET end_date='2026-01-02' WHERE id='owner-engagement'")
            .execute(holding.raw())
            .await
            .unwrap();
        let generate = action_command(
            "preventive-generate-one",
            &plan_id,
            MaintenancePreventiveAction::Generate,
            2,
        );
        let ineligible = expect_error(
            run(&holding, "owner", "branch", false, &generate).await,
        );
        assert_eq!(business(&ineligible).code, "CONFLICT");
        sqlx::query("UPDATE hr_engagements SET end_date=NULL WHERE id='owner-engagement'")
            .execute(holding.raw())
            .await
            .unwrap();

        // Generation creates one work order with its own `:work` receipt.
        let generated = run(&holding, "owner", "branch", false, &generate)
            .await
            .unwrap();
        assert_eq!(
            generated.result.as_str(),
            r#"{"id":"maintenance-plan-preventive-create-one","nextDueAt":1772953200000,"nextDueLocal":"2026-03-08T02:30","status":"active","version":3,"workOrderId":"maintenance-preventive-generate-one:work"}"#
        );
        let replayed = run(&holding, "owner", "branch", false, &generate)
            .await
            .unwrap();
        assert_eq!(replayed.result, generated.result);

        let (status, next_local, next_at, version): (String, String, i64, i64) = sqlx::query_as(
            "SELECT status,next_due_local,next_due_at,version FROM maintenance_preventive_plans WHERE id=$1",
        )
        .bind(&plan_id)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            (status.as_str(), next_local.as_str(), next_at, version),
            ("active", "2026-03-08T02:30", 1_772_953_200_000, 3)
        );
        let (work_id, due_local, due_at, plan_version, next_resolved, asset_version): (
            String,
            String,
            i64,
            i64,
            String,
            i64,
        ) = sqlx::query_as(
            "SELECT o.work_order_id,o.due_local,o.due_at,o.plan_version,o.definition_json->>'nextResolvedLocal',o.source_asset_version \
             FROM maintenance_preventive_occurrences o WHERE o.plan_id=$1",
        )
        .bind(&plan_id)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(work_id, "maintenance-preventive-generate-one:work");
        assert_eq!(
            (due_local.as_str(), due_at, plan_version, next_resolved.as_str(), asset_version),
            ("2026-03-07T02:30", 1_772_868_600_000, 2, "2026-03-08T03:00", 1)
        );
        let (title, status, scheduled_at, version, created_by): (String, String, i64, i64, String) =
            sqlx::query_as(
                "SELECT title,status,scheduled_at,version,created_by FROM maintenance_work_orders WHERE id=$1",
            )
            .bind(&work_id)
            .fetch_one(holding.raw())
            .await
            .unwrap();
        assert_eq!(
            (title.as_str(), status.as_str(), scheduled_at, version, created_by.as_str()),
            ("Daily safety inspection", "scheduled", 1_772_868_600_000, 1, "owner")
        );
        let receipts: Vec<String> = sqlx::query_scalar(
            "SELECT command_id FROM command_receipts ORDER BY command_id",
        )
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            receipts,
            vec![
                "preventive-activate-one",
                "preventive-create-one",
                "preventive-generate-one",
                "preventive-generate-one:work",
            ]
        );
        let plan_notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type='maintenance_plan' AND record_id=$1",
        )
        .bind(&plan_id)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        let work_notices: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM staff_notifications WHERE record_type='maintenance_work' AND record_id=$1",
        )
        .bind("maintenance-preventive-generate-one:work")
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!((plan_notices, work_notices), (3, 1));
        let audited: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id IN ('preventive-generate-one','preventive-generate-one:work')",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        let outboxed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id IN ('preventive-generate-one','preventive-generate-one:work')",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!((audited, outboxed), (2, 2));

        // The existing work-creation path refuses a second active work order.
        let blocked = action_command(
            "preventive-generate-blocked",
            &plan_id,
            MaintenancePreventiveAction::Generate,
            3,
        );
        let conflict = expect_error(
            run(&holding, "owner", "branch", false, &blocked).await,
        );
        assert_eq!(business(&conflict).code, "CONFLICT");
        let occurrences: i64 =
            sqlx::query_scalar("SELECT count(*) FROM maintenance_preventive_occurrences WHERE plan_id=$1")
                .bind(&plan_id)
                .fetch_one(holding.raw())
                .await
                .unwrap();
        assert_eq!(occurrences, 1);
        sqlx::query("UPDATE maintenance_work_orders SET status='completed',completion_evidence='Synthetic completed inspection',resulting_condition='good' WHERE id=$1")
            .bind(&work_id)
            .execute(holding.raw())
            .await
            .unwrap();

        // Next occurrence advances the wall clock from the stored minute.
        let second = action_command(
            "preventive-generate-two",
            &plan_id,
            MaintenancePreventiveAction::Generate,
            3,
        );
        let second = run(&holding, "owner", "branch", false, &second)
            .await
            .unwrap();
        assert_eq!(
            second.result.as_str(),
            r#"{"id":"maintenance-plan-preventive-create-one","nextDueAt":1773037800000,"nextDueLocal":"2026-03-09T02:30","status":"active","version":4,"workOrderId":"maintenance-preventive-generate-two:work"}"#
        );

        // Pause blocks generation; revision refuses a retained occurrence.
        let pause = action_command(
            "preventive-pause-one",
            &plan_id,
            MaintenancePreventiveAction::Pause,
            4,
        );
        run(&holding, "reviewer", "branch", true, &pause)
            .await
            .unwrap();
        let inactive = action_command(
            "preventive-generate-three",
            &plan_id,
            MaintenancePreventiveAction::Generate,
            5,
        );
        let inactive = expect_error(
            run(&holding, "owner", "branch", false, &inactive).await,
        );
        assert_eq!(business(&inactive).code, "MAINTENANCE_PLAN_INACTIVE");
        let mut retained = definition("owner", 1_772_868_659_000);
        retained.expected_version = Some(5);
        retained.reason = "Move the plan behind retained occurrences".into();
        let retained = PreventivePlanCommand::revise(
            CommandKey::new("preventive-revise-retained").unwrap(),
            RecordId::new(plan_id.clone()).unwrap(),
            retained.clone(),
            receipt(command_parts(&retained)),
        );
        let retained = expect_error(
            run(&holding, "requester", "branch", false, &retained).await,
        );
        assert_eq!(business(&retained).code, "CONFLICT");

        // A later first due date is admitted while paused, then re-activated.
        let mut revised = definition("owner", 1_775_000_000_000);
        revised.title = "Weekly safety inspection".into();
        revised.priority = "high".into();
        revised.interval_days = 7;
        revised.lead_days = 7;
        revised.expected_version = Some(5);
        revised.reason = "Reviewed weekly interval after inspection history".into();
        let revise = PreventivePlanCommand::revise(
            CommandKey::new("preventive-revise-one").unwrap(),
            RecordId::new(plan_id.clone()).unwrap(),
            revised.clone(),
            receipt(command_parts(&revised)),
        );
        let revised = run(&holding, "requester", "branch", false, &revise)
            .await
            .unwrap();
        assert_eq!(revised.result.as_str(), r#"{"id":"maintenance-plan-preventive-create-one","status":"paused","version":6}"#);
        let activate = action_command(
            "preventive-activate-two",
            &plan_id,
            MaintenancePreventiveAction::Activate,
            6,
        );
        run(&holding, "reviewer", "branch", true, &activate)
            .await
            .unwrap();
        let cancel = action_command(
            "preventive-cancel-one",
            &plan_id,
            MaintenancePreventiveAction::Cancel,
            7,
        );
        run(&holding, "reviewer", "branch", true, &cancel)
            .await
            .unwrap();
        let history: Vec<String> = sqlx::query_scalar(
            "SELECT action FROM maintenance_preventive_events WHERE plan_id=$1 ORDER BY version",
        )
        .bind(&plan_id)
        .fetch_all(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            history,
            vec![
                "create", "activate", "generate", "generate", "pause", "revise", "activate",
                "cancel"
            ]
        );
        let final_state: (String, i64, String, i32) = sqlx::query_as(
            "SELECT status,version,title,interval_days FROM maintenance_preventive_plans WHERE id=$1",
        )
        .bind(&plan_id)
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!(
            final_state,
            ("cancelled".to_owned(), 8, "Weekly safety inspection".to_owned(), 7)
        );

        // Occurrence evidence is immutable.
        let immutable = sqlx::query("DELETE FROM maintenance_preventive_occurrences WHERE plan_id=$1")
            .bind(&plan_id)
            .execute(holding.raw())
            .await;
        assert!(immutable.is_err());

        // Selected branch scope keeps the plan invisible.
        let scoped = action_command(
            "preventive-pause-foreign",
            &plan_id,
            MaintenancePreventiveAction::Pause,
            8,
        );
        let hidden = expect_error(
            run(&holding, "reviewer", "other", true, &scoped).await,
        );
        assert_eq!(business(&hidden).code, "MAINTENANCE_PLAN_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn preventive_generation_rejects_a_stale_plan_version_without_side_effects() {
    fixture::with_database(2, |holding| async move {
        setup(&holding).await;
        let create = definition("owner", 1_772_868_659_000);
        let created = PreventivePlanCommand::create(
            CommandKey::new("preventive-stale-create").unwrap(),
            create.clone(),
            receipt(command_parts(&create)),
        )
        .unwrap();
        let plan_id = created.id().as_str().to_owned();
        run(&holding, "requester", "branch", false, &created)
            .await
            .unwrap();
        let activate = action_command(
            "preventive-stale-activate",
            &plan_id,
            MaintenancePreventiveAction::Activate,
            1,
        );
        // The retained activation assertion refuses a retired asset source.
        sqlx::query("UPDATE operational_assets SET lifecycle_status='retirement_pending' WHERE id='asset'")
            .execute(holding.raw())
            .await
            .unwrap();
        let retired = expect_error(run(&holding, "reviewer", "branch", true, &activate).await);
        assert_eq!(business(&retired).code, "CONFLICT");
        sqlx::query("UPDATE operational_assets SET lifecycle_status='registered' WHERE id='asset'")
            .execute(holding.raw())
            .await
            .unwrap();
        run(&holding, "reviewer", "branch", true, &activate)
            .await
            .unwrap();
        let stale = action_command(
            "preventive-stale-generate",
            &plan_id,
            MaintenancePreventiveAction::Generate,
            99,
        );
        let error = expect_error(
            run(&holding, "owner", "branch", false, &stale).await,
        );
        assert_eq!(business(&error).code, "CONFLICT");
        let works: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance_work_orders")
            .fetch_one(holding.raw())
            .await
            .unwrap();
        let occurrences: i64 =
            sqlx::query_scalar("SELECT count(*) FROM maintenance_preventive_occurrences")
                .fetch_one(holding.raw())
                .await
                .unwrap();
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='preventive-stale-generate'",
        )
        .fetch_one(holding.raw())
        .await
        .unwrap();
        assert_eq!((works, occurrences, receipts), (0, 0, 0));
    })
    .await;
}
