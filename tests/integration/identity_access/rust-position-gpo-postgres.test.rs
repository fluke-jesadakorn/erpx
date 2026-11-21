//! Native Position GPO mapping/delivery slice over migrated PostgreSQL.
//! These prove owner SQL, reviewed-preview authority, version/CAS, immutable
//! revision evidence, reconciliation and Directory apply results. They are not
//! a browser, connector or Account/SSO journey. A bounded in-memory connector
//! stands in for the installed Directory provider only inside these tests.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::position_gpo::{
    ConfigRetryInput, ConnectionRow, FreshMfa, GpoAction, GpoFuture, IamRoleActor, Inventory,
    InventoryObject, JobRetryInput, PgPositionGpo, PositionGpoDirectory, PositionGpoInput,
    PublishInput, UnmapInput, UnmapPublishInput, drain_position_gpo_locked, preview_position_gpo,
    preview_unmap_position_gpo, publish_position_gpo, reconcile_position_gpo, retry_position_gpo,
    retry_position_gpo_config, unmap_position_gpo,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;
use sqlx::Row;

const NOW: i64 = 500;
const KEY: &str = "command-1";

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn actor(role: &str, permissions: &[&str]) -> IamRoleActor {
    IamRoleActor {
        user_id: UserId::new("actor").unwrap(),
        session_id: id("session-actor"),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-actor").unwrap(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        role: Some(role.into()),
        role_revision: Some(99),
        assignment_version: Some(1),
        identity_version: Some(1),
        authority_revision: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        grantable_permissions: vec![],
        person_user_ids: vec![UserId::new("actor").unwrap()],
        person_id: Some(id("person-1")),
        platform: false,
        emergency_until: None,
        expires_at: 1_000_000,
    }
}

fn it_actor() -> IamRoleActor {
    actor(
        "it_admin",
        &["iam.read", "iam.manage", "iam.directory.apply"],
    )
}

fn input(expected_version: i64) -> PositionGpoInput {
    PositionGpoInput::parse(&json!({
        "positionId": "position-1",
        "expectedVersion": expected_version,
        "connectionId": "connection-1",
        "gpoId": "gpo-1",
        "userFilterGroupId": "group-1",
        "computerTargetOuId": "ou-1",
        "reason": "Publish the reviewed mapping",
    }))
    .unwrap()
}

fn publish_input(token: String) -> PublishInput {
    PublishInput::parse(&json!({
        "positionId": "position-1",
        "expectedVersion": 0,
        "connectionId": "connection-1",
        "gpoId": "gpo-1",
        "userFilterGroupId": "group-1",
        "computerTargetOuId": "ou-1",
        "reason": "Publish the reviewed mapping",
        "previewToken": token,
    }))
    .unwrap()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

fn inventory() -> Inventory {
    Inventory {
        observed_at: 900,
        objects: vec![
            InventoryObject {
                id: "gpo-1".into(),
                kind: "gpo".into(),
                version: "7".into(),
                properties: json!({}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "ou-1".into(),
                kind: "ou".into(),
                version: "3".into(),
                properties: json!({"gpoLinks": []}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "group-1".into(),
                kind: "group".into(),
                version: "2".into(),
                properties: json!({}),
                member_ids: vec![],
            },
            InventoryObject {
                id: "CN=Ada".into(),
                kind: "account".into(),
                version: "1".into(),
                properties: json!({}),
                member_ids: vec![],
            },
        ],
    }
}

/// A bounded in-memory Directory connector that applies only the requested
/// object effect, so read-back and ownership behaviour remain observable.
struct FakeDirectory {
    inventory: Inventory,
    operations: Vec<(String, GpoAction)>,
    ignore_apply: bool,
}

impl FakeDirectory {
    fn new() -> Self {
        Self {
            inventory: inventory(),
            operations: vec![],
            ignore_apply: false,
        }
    }
}

impl PositionGpoDirectory for FakeDirectory {
    fn inventory<'a>(
        &'a mut self,
        _connection: &'a ConnectionRow,
    ) -> GpoFuture<'a, Inventory, BusinessError> {
        Box::pin(async move { Ok(self.inventory.clone()) })
    }

    fn apply<'a>(
        &'a mut self,
        _connection: &'a ConnectionRow,
        operation_id: &'a str,
        action: &'a GpoAction,
        before: &'a Inventory,
    ) -> GpoFuture<'a, (), BusinessError> {
        Box::pin(async move {
            if self.ignore_apply {
                return Ok(());
            }
            self.operations
                .push((operation_id.to_owned(), action.clone()));
            match action {
                GpoAction::Filter {
                    object_id,
                    group_ids,
                } => {
                    if let Some(object) = self
                        .inventory
                        .objects
                        .iter_mut()
                        .find(|object| &object.id == object_id && object.kind == "gpo")
                    {
                        object.properties = json!({"securityFilterGroupIds": group_ids});
                        object.version = format!("{}-applied", object.version);
                    }
                }
                GpoAction::Link {
                    object_id,
                    parent_id,
                    enabled,
                    remove,
                    ..
                } => {
                    if let Some(object) = self
                        .inventory
                        .objects
                        .iter_mut()
                        .find(|object| &object.id == parent_id && object.kind == "ou")
                    {
                        let mut links = object
                            .properties
                            .get("gpoLinks")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        if *remove {
                            links.retain(|link| link["gpoId"].as_str() != Some(object_id));
                        } else {
                            links.push(json!({"gpoId": object_id, "enabled": enabled}));
                        }
                        object.properties = json!({"gpoLinks": links});
                        object.version = format!("{}-applied", object.version);
                    }
                }
                GpoAction::Members {
                    object_id,
                    member_ids,
                    add,
                } => {
                    if let Some(object) = self
                        .inventory
                        .objects
                        .iter_mut()
                        .find(|object| &object.id == object_id && object.kind == "group")
                    {
                        for member in member_ids {
                            object.member_ids.retain(|current| current != member);
                            if *add {
                                object.member_ids.push(member.clone());
                            }
                        }
                        object.version = format!("{}-applied", object.version);
                    }
                }
            }
            let _ = before;
            Ok(())
        })
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
         ('*','it_admin','IT administrator',true,99,'active'),\
         ('*','employee','Employee',true,1,'active') \
         ON CONFLICT(company_id,id) DO UPDATE SET published_revision=EXCLUDED.published_revision,status='active';\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
         ('*','it_admin',99,'[\"iam.read\",\"iam.manage\",\"iam.directory.apply\"]','[]','[\"holding\"]','system',0),\
         ('*','employee',1,'[\"iam.self\"]','[]','[\"holding\"]','system',0) \
         ON CONFLICT(company_id,role_id,revision) DO NOTHING;\
         INSERT INTO staff_levels(id,company_id,code,name,rank,active,version,created_at,updated_at) \
         VALUES('level-1','company','L1','Level 1',1,1,1,0,0);\
         INSERT INTO organization_positions(id,company_id,title,level_id,active,version,created_by,created_at,updated_at) \
         VALUES('position-1','company','Head of Operations','level-1',1,1,'system',0,0);\
         INSERT INTO hr_people(id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) \
         VALUES('person-1','company','Ada Lovelace','GB','active',1,'system',0,0);\
         INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,version,created_by,created_at,updated_at,position_id) \
         VALUES('engagement-1','person-1','company','employee','salaried','active','2020-01-01','Head of Operations','GBP',1,'system',0,0,'position-1');\
         INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,created_by,created_at) \
         VALUES('connection-1','company','HQ Directory','active_directory','{}','active',1,'system',0);\
         INSERT INTO people_directory_state(connection_id,company_id,version,tested_version,write_enabled) \
         VALUES('connection-1','company',3,3,true);\
         INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,groups_json,observed_at,person_id) \
         VALUES('identity-1','company','connection-1','CN=Ada','ada','Ada Lovelace',true,'[]',0,'person-1');\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id,version) \
         VALUES('company','actor','local:actor','person-1','active','local','connection-1','identity-1',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
         VALUES('assignment-actor','actor','it_admin','[]','company',NULL,0,1000000,NULL,1,'holding',NULL,NULL,'manual',NULL);\
         INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) \
         VALUES('session-actor','actor','company','assignment-actor',1,1,1000000,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn preview(pool: &DatabasePool, expected_version: i64) -> (Value, String) {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut directory = FakeDirectory::new();
    let result = {
        let mut repository = PgPositionGpo::new(&mut tx);
        preview_position_gpo(
            &mut repository,
            &mut directory,
            &it_actor(),
            &input(expected_version),
            NOW,
        )
        .await
        .unwrap()
    };
    tx.rollback().await.unwrap();
    (result.value, result.token)
}

async fn publish(
    pool: &DatabasePool,
    token: String,
    key: &str,
    actor: &IamRoleActor,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let mut directory = FakeDirectory::new();
    let result = {
        let mut repository = PgPositionGpo::new(&mut tx);
        publish_position_gpo(
            &mut repository,
            &mut directory,
            actor,
            FreshMfa(true),
            key,
            &publish_input(token),
            NOW,
        )
        .await
    };
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            tx.rollback().await.unwrap();
            Err(error)
        }
    }
}

#[tokio::test]
async fn publish_commits_revision_policy_receipt_audit_and_replays() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        let result = publish(&pool, token.clone(), KEY, &it_actor())
            .await
            .unwrap();
        assert_eq!(result["version"], 1);
        assert_eq!(result["revision"], 1);
        assert_eq!(result["queued"], 1);
        assert_eq!(result["awaitingAccountLinkage"], 0);
        assert_eq!(result["directoryStatus"], "pending");

        let policy = sqlx::query(
            "SELECT version,revision,connection_id,gpo_id,directory_config_status \
             FROM position_gpo_policies WHERE company_id='company' AND position_id='position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(policy.get::<i32, _>("version"), 1);
        assert_eq!(policy.get::<i32, _>("revision"), 1);
        assert_eq!(policy.get::<String, _>("gpo_id"), "gpo-1");
        assert_eq!(
            policy.get::<String, _>("directory_config_status"),
            "pending"
        );

        let revision = sqlx::query(
            "SELECT affected_person_ids,published_by,command_id,directory_gpo_version \
             FROM position_gpo_revisions WHERE company_id='company' AND position_id='position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            revision.get::<Value, _>("affected_person_ids"),
            json!(["person-1"])
        );
        assert_eq!(revision.get::<String, _>("command_id"), KEY);
        assert_eq!(revision.get::<String, _>("directory_gpo_version"), "7");

        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} WHERE command_id=$1"
            )))
            .bind(KEY)
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(count, 1, "{table} evidence");
        }

        // The same key and normalized request replay the stored result.
        let replay = publish(&pool, token, KEY, &it_actor()).await.unwrap();
        assert_eq!(replay, result);
        let revisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM position_gpo_revisions WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revisions, 1);
    })
    .await;
}

#[tokio::test]
async fn publish_requires_it_authority_fresh_mfa_and_the_reviewed_preview() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;

        let employee = actor("employee", &["iam.self"]);
        let denied = publish(&pool, token.clone(), "command-employee", &employee)
            .await
            .unwrap_err();
        assert_eq!(business(denied).code, "IT_AUTHORITY_REQUIRED");

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let mut directory = FakeDirectory::new();
        let no_mfa = {
            let mut repository = PgPositionGpo::new(&mut tx);
            publish_position_gpo(
                &mut repository,
                &mut directory,
                &it_actor(),
                FreshMfa(false),
                "command-mfa",
                &publish_input(token.clone()),
                NOW,
            )
            .await
        };
        tx.rollback().await.unwrap();
        assert_eq!(business(no_mfa.unwrap_err()).code, "FRESH_MFA_REQUIRED");

        // A changed Directory observation invalidates the reviewed token.
        let mut changed = inventory();
        changed.observed_at = 901;
        changed.objects[0].version = "8".into();
        let mut directory = FakeDirectory {
            inventory: changed,
            operations: vec![],
            ignore_apply: false,
        };
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let stale = {
            let mut repository = PgPositionGpo::new(&mut tx);
            publish_position_gpo(
                &mut repository,
                &mut directory,
                &it_actor(),
                FreshMfa(true),
                "command-preview",
                &publish_input(token),
                NOW,
            )
            .await
        };
        tx.rollback().await.unwrap();
        assert_eq!(business(stale.unwrap_err()).code, "GPO_PREVIEW_CHANGED");

        // The published version is the CAS precondition for the next command.
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();
        let (_, stale_token) = preview(&pool, 1).await;
        let conflict = publish(&pool, stale_token, "command-2", &it_actor())
            .await
            .unwrap_err();
        assert_eq!(business(conflict).code, "VERSION_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn unmap_clears_the_mapping_and_records_an_immutable_revision() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();

        let unmap = UnmapInput::parse(&json!({
            "positionId": "position-1",
            "expectedVersion": 1,
            "reason": "Remove the reviewed mapping",
        }))
        .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let preview_value = {
            let mut repository = PgPositionGpo::new(&mut tx);
            preview_unmap_position_gpo(&mut repository, &it_actor(), &unmap, NOW)
                .await
                .unwrap()
        };
        tx.rollback().await.unwrap();
        let token = preview_value["token"].as_str().unwrap().to_owned();
        assert_eq!(preview_value["version"], 2);
        assert_eq!(preview_value["holders"].as_array().unwrap().len(), 1);

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let published = {
            let mut repository = PgPositionGpo::new(&mut tx);
            unmap_position_gpo(
                &mut repository,
                &it_actor(),
                FreshMfa(true),
                "command-unmap",
                &UnmapPublishInput::parse(&json!({
                    "positionId": "position-1",
                    "expectedVersion": 1,
                    "reason": "Remove the reviewed mapping",
                    "previewToken": token,
                }))
                .unwrap(),
                NOW,
            )
            .await
            .unwrap()
        };
        tx.commit().await.unwrap();
        assert_eq!(published["version"], 2);
        assert_eq!(published["mapped"], false);

        let policy = sqlx::query(
            "SELECT version,revision,connection_id,directory_config_status \
             FROM position_gpo_policies WHERE company_id='company' AND position_id='position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(policy.get::<i32, _>("version"), 2);
        assert_eq!(policy.get::<i32, _>("revision"), 2);
        assert!(policy.get::<Option<String>, _>("connection_id").is_none());
        assert_eq!(
            policy.get::<String, _>("directory_config_status"),
            "directory_applied"
        );
        let rule = sqlx::query_scalar::<_, bool>(
            "SELECT active FROM organization_gpo_rules WHERE id='legacy-position:position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(!rule);

        // A stale unmap precondition is rejected without a partial commit.
        let stale = UnmapInput::parse(&json!({
            "positionId": "position-1",
            "expectedVersion": 1,
            "reason": "Remove the reviewed mapping",
        }))
        .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let error = {
            let mut repository = PgPositionGpo::new(&mut tx);
            preview_unmap_position_gpo(&mut repository, &it_actor(), &stale, NOW)
                .await
                .unwrap_err()
        };
        tx.rollback().await.unwrap();
        assert_eq!(business(error).code, "GPO_MAPPING_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn retry_job_and_config_preserve_their_version_guards() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();
        sqlx::query(
            "INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,engagement_id,\
             account_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,\
             computer_target_ou_id,desired_action,status,created_at,updated_at,rule_id,source_id) \
             VALUES('job-1','company','position-1','person-1','engagement-1','CN=Ada',1,1,\
             'connection-1','gpo-1','group-1','ou-1','add','blocked',0,0,\
             'legacy-position:position-1','position-gpo:engagement-1')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "UPDATE position_gpo_policies SET directory_config_status='blocked' \
             WHERE company_id='company' AND position_id='position-1'",
        )
        .execute(pool.raw())
        .await
        .unwrap();

        let retry_input = JobRetryInput::parse(&json!({
            "id": "job-1",
            "expectedVersion": 1,
            "reason": "Retry the delivery",
        }))
        .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let result = {
            let mut repository = PgPositionGpo::new(&mut tx);
            retry_position_gpo(
                &mut repository,
                &it_actor(),
                FreshMfa(true),
                "command-retry",
                &retry_input,
                NOW,
            )
            .await
            .unwrap()
        };
        tx.commit().await.unwrap();
        assert_eq!(result["status"], "queued");
        assert_eq!(result["version"], 2);
        let job =
            sqlx::query("SELECT status,version,last_error FROM position_gpo_jobs WHERE id='job-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(job.get::<String, _>("status"), "queued");
        assert_eq!(job.get::<i32, _>("version"), 2);
        assert!(job.get::<Option<String>, _>("last_error").is_none());

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let stale = {
            let mut repository = PgPositionGpo::new(&mut tx);
            retry_position_gpo(
                &mut repository,
                &it_actor(),
                FreshMfa(true),
                "command-retry-stale",
                &retry_input,
                NOW,
            )
            .await
            .unwrap_err()
        };
        tx.rollback().await.unwrap();
        assert_eq!(business(stale).code, "GPO_JOB_CHANGED");

        let config_input = ConfigRetryInput::parse(&json!({
            "positionId": "position-1",
            "expectedVersion": 1,
            "reason": "Retry the configuration",
        }))
        .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let result = {
            let mut repository = PgPositionGpo::new(&mut tx);
            retry_position_gpo_config(
                &mut repository,
                &it_actor(),
                FreshMfa(true),
                "command-config",
                &config_input,
                NOW,
            )
            .await
            .unwrap()
        };
        tx.commit().await.unwrap();
        assert_eq!(result["status"], "pending");
        let status: String = sqlx::query_scalar(
            "SELECT directory_config_status FROM position_gpo_policies \
             WHERE company_id='company' AND position_id='position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(status, "pending");
    })
    .await;
}

#[tokio::test]
async fn reconcile_and_drain_apply_the_mapping_and_own_the_membership_claim() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();

        let mut directory = FakeDirectory::new();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        {
            let mut repository = PgPositionGpo::new(&mut tx);
            let desired = reconcile_position_gpo(&mut repository, NOW).await.unwrap();
            assert_eq!(desired.len(), 1);
            reconcile_position_gpo(&mut repository, NOW).await.unwrap();
            drain_position_gpo_locked(&mut repository, &mut directory, 10, NOW)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let jobs: i64 =
            sqlx::query_scalar("SELECT count(*) FROM position_gpo_jobs WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            jobs, 1,
            "reconciliation is idempotent through the delivery key"
        );
        let job = sqlx::query(
            "SELECT status,directory_observed_at FROM position_gpo_jobs WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(job.get::<String, _>("status"), "directory_applied");
        assert_eq!(
            job.get::<Option<i64>, _>("directory_observed_at"),
            Some(900)
        );

        let claimed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM people_directory_membership_sources \
             WHERE connection_id='connection-1' AND group_id='group-1' AND member_id='CN=Ada' \
             AND source_id='position-gpo:engagement-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(claimed, 1);
        let claim: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organization_gpo_membership_claims \
             WHERE connection_id='connection-1' AND member_id='CN=Ada'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(claim, 1);

        let rule_status: String = sqlx::query_scalar(
            "SELECT directory_config_status FROM organization_gpo_rules \
             WHERE id='legacy-position:position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rule_status, "directory_applied");
        let ownership: Vec<String> = sqlx::query_scalar(
            "SELECT kind||':'||target_id||':'||status FROM position_gpo_config_ownership \
             ORDER BY kind,target_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            ownership,
            vec![
                "filter_group:group-1:confirmed".to_owned(),
                "ou_link:ou-1:confirmed".to_owned()
            ]
        );
        let kinds: Vec<String> = directory
            .operations
            .iter()
            .map(|(_, action)| match action {
                GpoAction::Filter { .. } => "filter".to_owned(),
                GpoAction::Link { .. } => "link".to_owned(),
                GpoAction::Members { .. } => "members".to_owned(),
            })
            .collect();
        assert!(kinds.contains(&"filter".to_owned()));
        assert!(kinds.contains(&"link".to_owned()));
        assert!(kinds.contains(&"members".to_owned()));
    })
    .await;
}

#[tokio::test]
async fn an_unconfirmed_directory_write_blocks_the_configuration() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();
        let mut directory = FakeDirectory {
            inventory: inventory(),
            operations: vec![],
            ignore_apply: true,
        };
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        {
            let mut repository = PgPositionGpo::new(&mut tx);
            drain_position_gpo_locked(&mut repository, &mut directory, 10, NOW)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let rule = sqlx::query(
            "SELECT directory_config_status,directory_config_error FROM organization_gpo_rules \
             WHERE id='legacy-position:position-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rule.get::<String, _>("directory_config_status"), "blocked");
        assert_eq!(
            rule.get::<Option<String>, _>("directory_config_error")
                .as_deref(),
            Some("DIRECTORY_READBACK_REQUIRED")
        );
    })
    .await;
}

/// The retained Organization oracle only checked that departure queues one
/// native removal job. That producer is now native, so the same fact is proven
/// here instead of through the deleted TypeScript adapter.
#[tokio::test]
async fn departure_queues_the_native_removal_job() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (_, token) = preview(&pool, 0).await;
        publish(&pool, token, KEY, &it_actor()).await.unwrap();
        let mut directory = FakeDirectory::new();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        {
            let mut repository = PgPositionGpo::new(&mut tx);
            reconcile_position_gpo(&mut repository, NOW).await.unwrap();
            drain_position_gpo_locked(&mut repository, &mut directory, 10, NOW)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let claimed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM people_directory_membership_sources \
             WHERE connection_id='connection-1' AND member_id='CN=Ada'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(claimed, 1);

        sqlx::query(
            "UPDATE hr_engagements SET status='ended',version=version+1 WHERE id='engagement-1'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        {
            let mut repository = PgPositionGpo::new(&mut tx);
            let desired = reconcile_position_gpo(&mut repository, NOW + 1)
                .await
                .unwrap();
            assert!(desired.is_empty());
        }
        tx.commit().await.unwrap();

        let removal: (String, String) = sqlx::query_as(
            "SELECT desired_action,status FROM position_gpo_jobs \
             WHERE company_id='company' AND account_id='CN=Ada' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(removal, ("remove".to_owned(), "queued".to_owned()));
    })
    .await;
}
