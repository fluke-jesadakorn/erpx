//! Native Executive external connections on disposable PostgreSQL databases:
//! one-time hashed secret, final Control audit atomicity, current
//! assignment/organization/engagement authority, derived status, owner-only
//! revocation, the retained 20-connection cap and the shared MCP minute budget.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::executive_connections::{
    ConnectionActor, ExecutiveConnectionsStore, PgExecutiveConnections, commit_create, connections,
    consume_rate, hash_secret, list, prepare_create,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{Clock, TimestampMillis};
use serde_json::json;
use std::path::Path;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}

async fn migrations(pool: &DatabasePool, track: MigrationTrack) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, track).await.unwrap();
    migrate_database(pool, track, &files).await.unwrap();
}

fn actor(assignment_version: i64) -> ConnectionActor {
    ConnectionActor {
        user_id: "u1".into(),
        holding_id: "h001".into(),
        company_id: "company".into(),
        assignment_id: "assignment-u1".into(),
        assignment_version,
        role: "executive_author".into(),
        scope_kind: "holding".into(),
        scope_node_id: None,
        branch_id: None,
        department_id: None,
    }
}

async fn seed(control: &DatabasePool, holding: &DatabasePool) {
    migrations(control, MigrationTrack::Control).await;
    migrations(holding, MigrationTrack::HoldingWrite).await;
    sqlx::query(
        "INSERT INTO \"user\"(id,name,email,\"createdAt\",\"updatedAt\") VALUES('u1','Owner','owner@example.test',1,1),('u2','Other','other@example.test',1,1)",
    )
    .execute(control.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) \
         VALUES('h001','acme','Acme Group','H001_DB','H001_READ_DB','H001_FILES','active')",
    )
    .execute(control.raw())
    .await
    .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES\
         ('company','company','acme','Acme Group',NULL,'company',NULL,'active',1),\
         ('branch','branch','bangkok','Bangkok','company','company',NULL,'active',1),\
         ('department','department','finance','Finance','branch','company','branch','active',1);\
         INSERT INTO staff_levels(id,company_id,code,name,rank,active,version,created_at,updated_at,level_number) VALUES\
         ('level-1','company','L1','Executive',1,1,1,1,1,1),\
         ('level-3','company','L3','Contributor',3,1,1,1,1,3);\
         INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES\
         ('person-u1','u1','company','Owner','TH','active',1,'seed',1,1);\
         INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,job_title,currency,version,created_by,created_at,updated_at,level_id) VALUES\
         ('engagement-u1','person-u1','company','branch','department','employee','salaried','active','2020-01-01',NULL,'Owner','THB',1,'seed',1,1,'level-1');\
         INSERT INTO iam_permissions(id,definition) VALUES\
         ('executive.access','{\"id\":\"executive.access\",\"availability\":\"live\"}'::jsonb),\
         ('company.owner','{\"id\":\"company.owner\",\"availability\":\"live\"}'::jsonb)\
         ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;\
         INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES\
         ('company','executive_author','Executive author',true,1,'active');\
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES\
         ('company','executive_author',1,'[\"executive.access\",\"executive.report.create\"]'::jsonb,'[]','[\"holding\",\"branch\",\"department\"]'::jsonb,'seed',1);\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES\
         ('assignment-u1','u1','executive_author','[]','company',0,9999999999999,'holding',NULL);",
    )
    .execute(holding.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn create_list_revoke_and_capacity_keep_the_retained_contract() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(3, |holding| async move {
            seed(&control, &holding).await;
            let store = PgExecutiveConnections::new(control.clone(), holding.clone());
            let secret = "erpx_exec_contract_secret_0000000000000000000";
            let raw = json!({"name":"MCP agent","scopeId":"company","expiresDays":7});
            let prepared = prepare_create(
                &store,
                NOW,
                &actor(1),
                &raw,
                KEY,
                secret,
                &hash_secret(secret),
            )
            .await
            .unwrap();
            commit_create(&store, &prepared).await.unwrap();

            let row = sqlx::query(
                "SELECT secret_hash,name,user_id,holding_id,company_id,assignment_id,assignment_version,role,\
                 assignment_scope_kind,assignment_scope_node_id,branch_id,department_id,scope_label,expires_at \
                 FROM executive_external_tokens WHERE id=$1",
            )
            .bind(KEY)
            .fetch_one(control.raw())
            .await
            .unwrap();
            use sqlx::Row;
            assert_eq!(row.get::<String, _>("secret_hash"), hash_secret(secret));
            assert_ne!(row.get::<String, _>("secret_hash"), secret);
            assert_eq!(row.get::<i32, _>("assignment_version"), 1);
            assert_eq!(row.get::<String, _>("role"), "executive_author");
            assert_eq!(row.get::<String, _>("scope_label"), "Acme Group · Company-wide");
            assert_eq!(row.get::<i64, _>("expires_at"), NOW + 7 * 86_400_000);
            let audit: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM executive_mcp_audit WHERE token_id=$1 AND action='connection.create' AND outcome='success'",
            )
            .bind(KEY)
            .fetch_one(control.raw())
            .await
            .unwrap();
            assert_eq!(audit, 1);

            assert_eq!(
                commit_create(&store, &prepared).await.unwrap_err().code,
                "CONNECTION_ALREADY_CREATED"
            );
            let tokens: i64 =
                sqlx::query_scalar("SELECT count(*) FROM executive_external_tokens")
                    .fetch_one(control.raw())
                    .await
                    .unwrap();
            assert_eq!(tokens, 1);

            let listed = list(&store, NOW, &actor(1), true, false).await.unwrap();
            assert!(listed.can_create);
            assert_eq!(listed.connections.len(), 1);
            assert_eq!(listed.connections[0].status.as_str(), "active");
            assert!(!serde_json::to_string(&listed).unwrap().contains(secret));
            assert_eq!(listed.scopes.len(), 3);

            sqlx::query("UPDATE assignments SET version=2 WHERE id='assignment-u1'")
                .execute(holding.raw())
                .await
                .unwrap();
            let changed = list(&store, NOW, &actor(1), true, false).await.unwrap();
            assert_eq!(changed.connections[0].status.as_str(), "access_changed");
            sqlx::query("UPDATE assignments SET version=1 WHERE id='assignment-u1'")
                .execute(holding.raw())
                .await
                .unwrap();

            let mut stranger = actor(1);
            stranger.user_id = "u2".into();
            assert!(!store.revoke(&stranger, KEY, NOW).await.unwrap());
            assert!(store.revoke(&actor(1), KEY, NOW).await.unwrap());
            assert!(store.revoke(&actor(1), KEY, NOW).await.unwrap());
            let revoked = connections(&store, NOW, &actor(1)).await.unwrap();
            assert_eq!(revoked[0].status.as_str(), "revoked");

            sqlx::query("DELETE FROM executive_external_tokens").execute(control.raw()).await.unwrap();
            for index in 0..20 {
                let id = format!("cccccccc-cccc-cccc-cccc-{index:012}");
                let token_secret = format!("erpx_exec_capacity_{index:0>20}");
                let prepared = prepare_create(
                    &store,
                    NOW,
                    &actor(1),
                    &json!({"name":format!("Agent {index}"),"scopeId":"company"}),
                    &id,
                    &token_secret,
                    &hash_secret(&token_secret),
                )
                .await
                .unwrap();
                commit_create(&store, &prepared).await.unwrap();
            }
            let limited = prepare_create(
                &store,
                NOW,
                &actor(1),
                &json!({"name":"One too many","scopeId":"company"}),
                OTHER,
                "erpx_exec_limited",
                &hash_secret("erpx_exec_limited"),
            )
            .await
            .unwrap();
            assert_eq!(
                commit_create(&store, &limited).await.unwrap_err().code,
                "CONNECTION_LIMIT"
            );

            sqlx::query("UPDATE hr_engagements SET level_id='level-3' WHERE id='engagement-u1'")
                .execute(holding.raw())
                .await
                .unwrap();
            assert!(!store.eligible(&actor(1)).await.unwrap());
            assert_eq!(
                prepare_create(
                    &store,
                    NOW,
                    &actor(1),
                    &json!({"name":"Ineligible","scopeId":"company"}),
                    OTHER,
                    "erpx_exec_ineligible",
                    &hash_secret("erpx_exec_ineligible"),
                )
                .await
                .unwrap_err()
                .code,
                "EXECUTIVE_CONNECTION_FORBIDDEN"
            );
            sqlx::query("UPDATE hr_engagements SET level_id='level-1' WHERE id='engagement-u1'")
                .execute(holding.raw())
                .await
                .unwrap();

            sqlx::query("UPDATE executive_external_tokens SET revoked_at=NULL")
                .execute(control.raw())
                .await
                .unwrap();
            assert_eq!(
                prepare_create(
                    &store,
                    NOW,
                    &actor(1),
                    &json!({"name":"Foreign scope","scopeId":"foreign-node"}),
                    OTHER,
                    "erpx_exec_foreign",
                    &hash_secret("erpx_exec_foreign"),
                )
                .await
                .unwrap_err()
                .code,
                "CONNECTION_SCOPE_FORBIDDEN"
            );
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn audit_atomicity_and_the_shared_minute_budget_fail_closed() {
    fixture::with_database(3, |control| async move {
        fixture::with_database(3, |holding| async move {
            seed(&control, &holding).await;
            let store = PgExecutiveConnections::new(control.clone(), holding.clone());
            let secret = "erpx_exec_atomic_secret_00000000000000000000";
            let prepared = prepare_create(
                &store,
                NOW,
                &actor(1),
                &json!({"name":"Atomic","scopeId":"company"}),
                KEY,
                secret,
                &hash_secret(secret),
            )
            .await
            .unwrap();
            sqlx::query(
                "CREATE FUNCTION executive_connection_final_audit() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN RAISE EXCEPTION 'EXECUTIVE_CONNECTION_FINAL_AUDIT' USING ERRCODE='P0001'; END;$$",
            )
            .execute(control.raw())
            .await
            .unwrap();
            sqlx::query(
                "CREATE TRIGGER executive_connection_final_audit AFTER INSERT ON executive_mcp_audit \
                 FOR EACH ROW WHEN(NEW.token_id='aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa' AND NEW.action='connection.create') \
                 EXECUTE FUNCTION executive_connection_final_audit()",
            )
            .execute(control.raw())
            .await
            .unwrap();
            assert!(commit_create(&store, &prepared).await.is_err());
            let tokens: i64 =
                sqlx::query_scalar("SELECT count(*) FROM executive_external_tokens")
                    .fetch_one(control.raw())
                    .await
                    .unwrap();
            let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM executive_mcp_audit")
                .fetch_one(control.raw())
                .await
                .unwrap();
            assert_eq!((tokens, audit), (0, 0));
            sqlx::query("DROP TRIGGER executive_connection_final_audit ON executive_mcp_audit")
                .execute(control.raw())
                .await
                .unwrap();
            sqlx::query("DROP FUNCTION executive_connection_final_audit()")
                .execute(control.raw())
                .await
                .unwrap();
            commit_create(&store, &prepared).await.unwrap();

            for request in 1..=60 {
                assert_eq!(
                    consume_rate(&control, &hash_secret(secret), NOW).await.unwrap(),
                    request
                );
            }
            assert_eq!(
                consume_rate(&control, &hash_secret(secret), NOW).await.unwrap_err().code,
                "RATE_LIMITED"
            );
            assert_eq!(
                consume_rate(&control, &hash_secret(secret), NOW + 60_000)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                consume_rate(&control, &hash_secret("erpx_exec_unknown"), NOW)
                    .await
                    .unwrap_err()
                    .code,
                "INVALID_TOKEN"
            );
            let clock = FixedClock(NOW);
            assert_eq!(clock.now().get(), NOW);
        })
        .await;
    })
    .await;
}
