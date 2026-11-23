//! Caller-transaction coverage for the Records-owned `request.ai.prepare`
//! drain (`RequestAi` native port). Proves claim, Records-scope authority,
//! provider result/usage writes, the two Workflow-owned assertion clears and
//! the expired-running/orphaned-usage sweeps on one caller unit with no
//! pool/commit/retry inside the adapter.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_records::public::request_ai_drain::{
    ClaimedProposal, ProviderFuture, ProviderOutput, RequestAiProvider, RequestSnapshot,
    drain_request_ai, lease_token, result_command, run_auth_key,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::AssertSqlSafe;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_000;

#[derive(Clone)]
struct CannedProvider {
    calls: Arc<Mutex<Vec<String>>>,
}

impl RequestAiProvider for CannedProvider {
    fn generate<'a>(
        &'a self,
        proposal: &'a ClaimedProposal,
        _request: &'a RequestSnapshot,
    ) -> ProviderFuture<'a> {
        let calls = self.calls.clone();
        let id = proposal.id.clone();
        Box::pin(async move {
            calls.lock().unwrap().push(id);
            Ok(ProviderOutput::new(
                r#"{"title":"Fixed title","description":null,"category":null,"summary":"Do this next.","sources":["request"]}"#.to_owned(),
                12,
                34,
            ))
        })
    }
}

fn actor_json() -> String {
    serde_json::json!({
        "assignmentId": "assignment-actor",
        "userId": "actor",
        "companyId": "company",
        "role": "operations_manager",
        "authority": { "assignmentVersion": 2 },
        "holdingSlug": "acme",
        "branchId": null,
    })
    .to_string()
}

async fn setup(pool: &DatabasePool) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES ('company','actor','local:actor',NULL,'active','local',1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,\
         valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,parent_id,source,source_id) \
         VALUES ('assignment-actor','actor','operations_manager','[\"operations\"]','company',NULL,\
         0,4102444800000,NULL,2,'holding',NULL,NULL,'manual',NULL)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    for (request_id, version) in [("request-1", 3), ("request-2", 5), ("request-3", 1)] {
        sqlx::query(
            "INSERT INTO employee_requests(id,company_id,kind,category,title,description,\
             requester_user_id,created_by,state,version,evidence_version,created_at,updated_at) \
             VALUES ($1,'company','service','general','Need help','Need help with access',\
             'actor','actor','queued',$2,1,100,100)",
        )
        .bind(request_id)
        .bind(version)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    let actor = actor_json();
    // proposal-1: due and fresh (success). proposal-2: due but stale on
    // version (source 4 vs current 5). proposal-3: expired running with
    // attempts>=3, swept without a claim.
    for (proposal_id, request_id, source_version, state, attempts, claim_until, created_at) in [
        ("proposal-1", "request-1", 3, "queued", 0, None, 100),
        ("proposal-2", "request-2", 4, "queued", 0, None, 200),
        ("proposal-3", "request-3", 1, "running", 3, Some(0), 50),
    ] {
        sqlx::query(
            "INSERT INTO request_ai_proposals(id,company_id,request_id,source_version,\
             provider_json,actor_json,state,attempts,lease_token,claim_until,created_at,updated_at) \
             VALUES ($1,'company',$2,$3,'{\"model\":\"test\"}',$4,$5,$6,$7,$8,$9,$9)",
        )
        .bind(proposal_id)
        .bind(request_id)
        .bind(source_version)
        .bind(&actor)
        .bind(state)
        .bind(attempts)
        .bind(Option::<String>::None)
        .bind(claim_until)
        .bind(created_at)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO request_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) \
             VALUES ($1,'company','actor','2026-11','reserved',$2,100)",
        )
        .bind(proposal_id)
        .bind(9_999_999_999_999i64)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    // Seed the two per-run assertion keys for both due proposals plus one
    // unrelated key. Only the successful proposal's keys may be cleared.
    let lease_1 = lease_token(NOW, "proposal-1");
    let lease_2 = lease_token(NOW, "proposal-2");
    sqlx::raw_sql(AssertSqlSafe(format!(
            "INSERT INTO command_assertion_baselines(command_id,step,ok) VALUES \
             ('{}','request-ai-lease',1),('{}','queued-assignment',1),\
             ('{}','request-ai-lease',1),('{}','queued-assignment',1),\
             ('other-command:key-9','request-ai-lease',1);",
            run_auth_key(&lease_1).replace('\'', "''"),
            result_command(&lease_1).replace('\'', "''"),
            run_auth_key(&lease_2).replace('\'', "''"),
            result_command(&lease_2).replace('\'', "''"),
        )))
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn assertion_steps(pool: &DatabasePool, command_id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT step FROM command_assertions WHERE command_id=$1 ORDER BY step")
        .bind(command_id)
        .fetch_all(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn drain_claims_writes_results_clears_and_sweeps_on_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let provider = CannedProvider {
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let lease_1 = lease_token(NOW, "proposal-1");
        let auth_1 = run_auth_key(&lease_1);
        let result_1 = result_command(&lease_1);
        let lease_2 = lease_token(NOW, "proposal-2");
        let auth_2 = run_auth_key(&lease_2);
        let result_2 = result_command(&lease_2);

        // The drain requires the caller transaction and never commits.
        let mut bare = pool.raw().acquire().await.unwrap();
        let rejected = drain_request_ai(&mut bare, Some("request-1"), NOW, &provider).await;
        assert_eq!(
            rejected.unwrap_err().code,
            "REQUEST_AI_DRAIN_TRANSACTION_REQUIRED"
        );
        drop(bare);

        let mut unit = pool.raw().begin().await.unwrap();
        let outcome = drain_request_ai(&mut unit, None, NOW, &provider)
            .await
            .unwrap();
        unit.commit().await.unwrap();
        assert_eq!(outcome.processed, 2);
        assert_eq!(outcome.succeeded, 1);
        assert_eq!(outcome.stale, 1);
        assert_eq!(outcome.failed, 0);
        // The stale proposal never reaches the provider.
        assert_eq!(
            provider.calls.lock().unwrap().clone(),
            vec!["proposal-1".to_owned()]
        );

        let ready: (String, Option<String>, i64, Option<String>) = sqlx::query_as(
            "SELECT state,result_json,attempts,lease_token FROM request_ai_proposals WHERE id='proposal-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(ready.0, "ready");
        assert_eq!(
            ready.1.unwrap(),
            r#"{"title":"Fixed title","description":null,"category":null,"summary":"Do this next.","sources":["request"]}"#
        );
        assert_eq!(ready.2, 1);
        assert!(ready.3.is_none());

        let usage: (String, i64, i64, i64) = sqlx::query_as(
            "SELECT state,lease_until,input_tokens,output_tokens FROM request_ai_usage WHERE id='proposal-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(usage, ("succeeded".to_owned(), 0, 12, 34));

        let stale: (String, Option<String>, i64) = sqlx::query_as(
            "SELECT state,error,attempts FROM request_ai_proposals WHERE id='proposal-2'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stale.0, "stale");
        assert!(stale.1.unwrap().contains("Prepare a fresh suggestion"));
        assert_eq!(stale.2, 1);
        let stale_usage: String =
            sqlx::query_scalar("SELECT state FROM request_ai_usage WHERE id='proposal-2'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(stale_usage, "failed");

        // Expired running proposals and orphaned reserved usage are swept.
        let swept: (String, Option<String>) = sqlx::query_as(
            "SELECT state,error FROM request_ai_proposals WHERE id='proposal-3'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(swept.0, "failed");
        assert!(swept.1.unwrap().contains("Retry or continue manually"));
        let swept_usage: String =
            sqlx::query_scalar("SELECT state FROM request_ai_usage WHERE id='proposal-3'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(swept_usage, "failed");

        // Only the successful run's keys are cleared in the same unit.
        assert!(assertion_steps(&pool, &auth_1).await.is_empty());
        assert!(assertion_steps(&pool, &result_1).await.is_empty());
        assert_eq!(assertion_steps(&pool, &auth_2).await.len(), 1);
        assert_eq!(assertion_steps(&pool, &result_2).await.len(), 1);
        assert_eq!(
            assertion_steps(&pool, "other-command:key-9").await,
            vec!["request-ai-lease".to_owned()]
        );

        // Replay is a no-op: no due rows remain.
        let mut replay = pool.raw().begin().await.unwrap();
        let repeated = drain_request_ai(&mut replay, None, NOW, &provider)
            .await
            .unwrap();
        replay.commit().await.unwrap();
        assert_eq!(repeated.processed, 0);
        assert_eq!(provider.calls.lock().unwrap().len(), 1);
    })
    .await;
}
