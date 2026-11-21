//! Native AI-gateway policy, audit, statement, verified cost and
//! reconciliation over a leased PostgreSQL database.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_agent::public::gateway::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use serde_json::json;

const NOW: i64 = 1_780_000_000_000;
const HOLDING: &str = "gateway-holding";

fn code(error: AiGatewayError) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence failure: {error}"),
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) \
         VALUES(1,'gateway-holding','gateway-owner',0) ON CONFLICT(singleton) DO NOTHING;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn statement() -> AiProviderStatement {
    AiProviderStatement {
        id: "statement-1".into(),
        holding_id: HOLDING.into(),
        provider: "api.provider.example".into(),
        month: billing_month(NOW),
        currency: "USD".into(),
        amount_minor: 20,
        statement_digest: "a".repeat(64),
        verified_by: "platform-admin".into(),
    }
}

fn request(id: &str, funding: &str, status: &str, at: i64) -> String {
    format!(
        "INSERT INTO ai_gateway_requests(id,usage_id,holding_id,company_id,branch_id,connection_id,\
         funding_source,provider,model,feature,status,input_tokens,output_tokens,units_json,\
         reservation_satang,created_at,completed_at) \
         VALUES('{id}','{id}','{HOLDING}','gateway-company',NULL,NULL,'{funding}',\
         'api.provider.example','model-1','test','{status}',10,5,'{{\"input_tokens\":10}}',1000,{at},{completed})",
        completed = if status == "reserved" {
            "NULL".to_owned()
        } else {
            at.to_string()
        }
    )
}

async fn read(pool: &DatabasePool, now: i64) -> AiGatewayPolicyView {
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                read_ai_gateway_policy(&mut ports, now).await
            })
        })
        .await
        .unwrap()
}

async fn set_enabled(
    pool: &DatabasePool,
    input: AiSetEnabledInput,
    audit_id: &str,
) -> Result<AiGatewayPolicyView, AiGatewayError> {
    let audit_id = audit_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            let audit_id = audit_id.clone();
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                set_ai_platform_key_enabled(&mut ports, NOW, &audit_id, &input, "platform-admin")
                    .await
            })
        })
        .await
}

async fn update_policy(
    pool: &DatabasePool,
    input: AiPolicyChange,
    audit_id: &str,
) -> Result<AiGatewayPolicyView, AiGatewayError> {
    let audit_id = audit_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            let audit_id = audit_id.clone();
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                update_ai_gateway_policy(&mut ports, NOW, &audit_id, &input, "platform-admin").await
            })
        })
        .await
}

async fn record_statement(
    pool: &DatabasePool,
    input: AiProviderStatement,
) -> Result<serde_json::Value, AiGatewayError> {
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                record_ai_provider_statement(&mut ports, NOW, &input).await
            })
        })
        .await
}

async fn record_cost(
    pool: &DatabasePool,
    input: AiVerifiedCost,
) -> Result<serde_json::Value, AiGatewayError> {
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                record_verified_ai_cost(&mut ports, NOW, &input).await
            })
        })
        .await
}

async fn reconcile(
    pool: &DatabasePool,
    statement_id: &str,
) -> Result<serde_json::Value, AiGatewayError> {
    let statement_id = statement_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run_once(TransactionOptions::default(), move |connection| {
            let statement_id = statement_id.clone();
            Box::pin(async move {
                let mut ports = PgAiGateway::new(connection);
                reconcile_ai_provider_statement(&mut ports, NOW, &statement_id).await
            })
        })
        .await
}

#[tokio::test]
async fn policy_audit_and_compare_and_set_are_exact() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let before = read(&pool, NOW).await;
        assert!(before.platform_enabled);
        assert_eq!(before.monthly_request_limit, 1000);

        let input = AiSetEnabledInput {
            enabled: false,
            expected_updated_at: before.updated_at,
            reason: "Provider incident".into(),
        };
        let after = set_enabled(&pool, input.clone(), "audit-1").await.unwrap();
        assert!(!after.platform_enabled);
        assert_eq!(after.updated_by, "platform-admin");
        let audit: (String, String, bool) = sqlx::query_as(
            "SELECT actor_id,reason,platform_enabled FROM ai_gateway_policy_audit WHERE id='audit-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, ("platform-admin".into(), "Provider incident".into(), false));

        assert_eq!(
            code(
                set_enabled(&pool, input, "audit-2")
                    .await
                    .unwrap_err()
            ),
            "AI_GATEWAY_POLICY_CHANGED"
        );
        assert_eq!(
            code(
                set_enabled(
                    &pool,
                    AiSetEnabledInput {
                        enabled: false,
                        expected_updated_at: after.updated_at,
                        reason: "Provider incident".into(),
                    },
                    "audit-3",
                )
                .await
                .unwrap_err()
            ),
            "AI_GATEWAY_POLICY_UNCHANGED"
        );
        assert_eq!(
            code(
                set_enabled(
                    &pool,
                    AiSetEnabledInput {
                        enabled: true,
                        expected_updated_at: after.updated_at,
                        reason: " ".into(),
                    },
                    "audit-4",
                )
                .await
                .unwrap_err()
            ),
            "AI_GATEWAY_REASON"
        );

        let updated = update_policy(
            &pool,
            AiPolicyChange {
                enabled: true,
                monthly_request_limit: 25,
                monthly_spend_cap_satang: 50_000,
                reservation_satang: 500,
                reason: "Restore allowance".into(),
            },
            "audit-5",
        )
        .await
        .unwrap();
        assert_eq!(updated.monthly_request_limit, 25);
        assert_eq!(updated.monthly_spend_cap_satang, 50_000);
        assert_eq!(updated.reservation_satang, 500);
        let audit: (i32, i64, i64) = sqlx::query_as(
            "SELECT monthly_request_limit,monthly_spend_cap_satang,reservation_satang \
             FROM ai_gateway_policy_audit WHERE id='audit-5'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(audit, (25, 50_000, 500));
    })
    .await;
}

#[tokio::test]
async fn policy_caps_use_bangkok_month_bounds_and_safe_rows_hide_no_secrets() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let month = billing_month(NOW);
        let (start, end) = month_bounds(&month).unwrap();
        for sql in [
            request("before", "erpx", "succeeded", start - 1),
            request("first", "erpx", "reserved", start),
            request("second", "erpx", "succeeded", end - 1),
            request("after", "erpx", "succeeded", end),
        ] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(pool.raw())
                .await
                .unwrap();
        }
        sqlx::query(
            "INSERT INTO ai_provider_statements(id,holding_id,provider,month,currency,amount_minor,statement_digest,verified_by,verified_at) \
             VALUES('seed-statement','gateway-holding','api.provider.example',$1,'USD',10,$2,'platform-admin',$3)",
        )
        .bind(&month)
        .bind("b".repeat(64))
        .bind(NOW)
        .execute(pool.raw())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ai_provider_cost_evidence(request_id,provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref,verified_by,verified_at,reconciled_at) \
             VALUES('second','seed-statement','USD',10,350,'bank-rate-1','platform-admin',$1,NULL)",
        )
        .bind(NOW)
        .execute(pool.raw())
        .await
        .unwrap();

        let view = read(&pool, NOW).await;
        assert_eq!(view.monthly_requests, 2);
        assert_eq!(view.monthly_reserved_or_verified_satang, 1350);
        assert_eq!(view.pending_platform_cost_requests, 1);

        let rows = PgUnitOfWork::new(pool.clone())
            .run_once(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let mut ports = PgAiGateway::new(connection);
                    safe_ai_gateway_rows(&mut ports, &month).await
                })
            })
            .await
            .unwrap();
        let ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids, vec!["first", "second"]);
        let second = rows.iter().find(|row| row.id == "second").unwrap();
        assert_eq!(second.thb_satang, Some(350));
        assert_eq!(second.currency.as_deref(), Some("USD"));
        assert_eq!(second.reconciled_at, None);
        assert!(!serde_json::to_string(&rows).unwrap().contains("api-key"));
    })
    .await;
}

#[tokio::test]
async fn statement_cost_and_reconciliation_require_exact_evidence() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "{};{};{};{}",
            request("first", "erpx", "succeeded", NOW),
            request("second", "erpx", "succeeded", NOW),
            request("byok", "byok", "succeeded", NOW),
            request("late", "erpx", "succeeded", NOW)
        )))
        .execute(pool.raw())
        .await
        .unwrap();

        let input = statement();
        assert_eq!(
            record_statement(&pool, input.clone()).await.unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            record_statement(&pool, input.clone()).await.unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            code(
                record_statement(
                    &pool,
                    AiProviderStatement {
                        amount_minor: 21,
                        ..input.clone()
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_STATEMENT_CONFLICT"
        );
        assert_eq!(
            code(
                record_statement(
                    &pool,
                    AiProviderStatement {
                        holding_id: "other-holding".into(),
                        ..input.clone()
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_STATEMENT_CONFLICT"
        );

        let cost = AiVerifiedCost {
            request_id: "first".into(),
            provider_statement_id: "statement-1".into(),
            currency: "USD".into(),
            amount_minor: 10,
            thb_satang: 350,
            fx_evidence_ref: Some("bank-rate-1".into()),
            verified_by: "platform-admin".into(),
        };
        assert_eq!(
            record_cost(&pool, cost.clone()).await.unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            record_cost(&pool, cost.clone()).await.unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            code(
                record_cost(
                    &pool,
                    AiVerifiedCost {
                        thb_satang: 351,
                        ..cost.clone()
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_COST_EVIDENCE_CONFLICT"
        );
        assert_eq!(
            code(
                record_cost(
                    &pool,
                    AiVerifiedCost {
                        request_id: "byok".into(),
                        ..cost.clone()
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_COST_EVIDENCE_CONFLICT"
        );
        assert_eq!(
            code(
                record_cost(
                    &pool,
                    AiVerifiedCost {
                        fx_evidence_ref: None,
                        ..cost.clone()
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_COST_EVIDENCE_INVALID"
        );

        assert_eq!(
            code(reconcile(&pool, "statement-1").await.unwrap_err()),
            "AI_STATEMENT_UNBALANCED"
        );
        assert_eq!(
            record_cost(
                &pool,
                AiVerifiedCost {
                    request_id: "second".into(),
                    ..cost
                }
            )
            .await
            .unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            reconcile(&pool, "statement-1").await.unwrap(),
            json!({"ok": true})
        );
        assert_eq!(
            reconcile(&pool, "statement-1").await.unwrap(),
            json!({"ok": true})
        );
        let reconciled: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ai_provider_cost_evidence WHERE reconciled_at IS NOT NULL",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reconciled, 2);
        assert_eq!(
            code(
                record_cost(
                    &pool,
                    AiVerifiedCost {
                        request_id: "late".into(),
                        provider_statement_id: "statement-1".into(),
                        currency: "USD".into(),
                        amount_minor: 10,
                        thb_satang: 350,
                        fx_evidence_ref: Some("bank-rate-1".into()),
                        verified_by: "platform-admin".into(),
                    }
                )
                .await
                .unwrap_err()
            ),
            "AI_COST_EVIDENCE_CONFLICT"
        );
        assert_eq!(
            record_cost(
                &pool,
                AiVerifiedCost {
                    request_id: "first".into(),
                    provider_statement_id: "statement-1".into(),
                    currency: "USD".into(),
                    amount_minor: 10,
                    thb_satang: 350,
                    fx_evidence_ref: Some("bank-rate-1".into()),
                    verified_by: "platform-admin".into(),
                }
            )
            .await
            .unwrap(),
            json!({"ok": true})
        );
    })
    .await;
}
