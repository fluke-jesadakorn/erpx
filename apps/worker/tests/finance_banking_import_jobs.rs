use chrono::{Duration, NaiveDate, Utc};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext},
    finance::{FinanceAccessPath, FinanceAuthorizationOperation, FinanceAuthorizationRequest},
};
use erpx_infrastructure::{
    BankProviderAccountBinding, BankProviderAccountRef, BankProviderCredentialKey,
    ControlPlaneDatabaseConfig, ControlPlanePools, SqlxControlPlane,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::{
    FinanceBankProviderImportAuthorization, FinanceBankProviderImportEvidence,
    FinanceBankProviderImportFailureClass, FinanceBankProviderImportJobSource,
    FinanceBankProviderImportRequest, FinanceBankProviderImportRollbackEvidence,
    SqlxFinanceBankProviderImportJobSource,
};
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
    }
}

fn build_request(suffix: &str) -> FinanceBankProviderImportRequest {
    let holding_id = Uuid::from_u128(1);
    let company_id = Uuid::from_u128(2);
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let account = BankProviderAccountBinding::try_new(
        Uuid::from_u128(3),
        BankProviderAccountRef::try_new("fixturebank", "account-opaque-1").expect("account ref"),
        scope.clone(),
        holding_id,
        company_id,
        "THB",
        "Fixture operating account",
    )
    .expect("account binding");
    let context = CommandContext::try_new(
        format!("finance-bank-import-{suffix}"),
        ActorAttribution {
            principal_ref: "finance-fixture-user".into(),
            kind: ActorKind::HoldingUser,
            session_id: Some(Uuid::from_u128(5)),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "authorized finance bank import fixture",
    )
    .expect("command context");
    FinanceBankProviderImportRequest {
        provider_account_ref: "fixturebank:account-opaque-1".into(),
        provider_key: "fixturebank".into(),
        holding_id: format!("holding-finance-bank-{suffix}"),
        data_profile_key: format!("holding-finance-bank-{suffix}"),
        credential_key: BankProviderCredentialKey::try_new("vault/fixture/finance-bank")
            .expect("credential key"),
        account,
        starts_on: NaiveDate::from_ymd_opt(2026, 1, 1).expect("start date"),
        ends_on: NaiveDate::from_ymd_opt(2026, 1, 31).expect("end date"),
        page_size: 100,
        import_id: Uuid::new_v4(),
        context,
    }
}

fn build_authorization(
    request: &FinanceBankProviderImportRequest,
) -> FinanceBankProviderImportAuthorization {
    let decision = FinanceAuthorizationRequest {
        context: request.context.clone(),
        operation: FinanceAuthorizationOperation::ImportBankStatement,
        mfa_verified: true,
        approver_principal_ref: None,
        access_path: FinanceAccessPath::Normal,
    }
    .evaluate()
    .expect("MFA authorization decision");
    FinanceBankProviderImportAuthorization::from_decision(&decision, &request.context)
        .expect("authorization handoff")
}

async fn register_holding(control: &SqlxControlPlane, request: &FinanceBankProviderImportRequest) {
    sqlx::query(
        "INSERT INTO erpx_control.holding_registry
            (holding_id, data_profile_key, write_database_key, read_database_key,
             storage_namespace, credential_key_ref, backup_chain_ref, state, registry_version)
         VALUES ($1,$2,'fixture-write','fixture-read','fixture-storage',
                 'fixture-credential','fixture-backup','active',1)",
    )
    .bind(&request.holding_id)
    .bind(&request.data_profile_key)
    .execute(&control.pools().write)
    .await
    .expect("holding registry fixture");
}

#[tokio::test]
#[ignore = "requires two disposable control PostgreSQL databases"]
async fn bank_import_jobs_persist_claim_fence_retry_reconciliation_and_read_status() {
    let pools = ControlPlanePools::connect(&test_config())
        .await
        .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("control write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("control read migration");
    let control = SqlxControlPlane::new(pools);
    let source = SqlxFinanceBankProviderImportJobSource::new(&control);
    let request = build_request(&Uuid::new_v4().to_string());
    let authorization = build_authorization(&request);
    register_holding(&control, &request).await;

    let available_at = Utc::now() - Duration::seconds(1);
    let job_id = source
        .enqueue(&request, &authorization, 7, available_at, 3)
        .await
        .expect("enqueue authorized job");
    assert_eq!(
        source
            .enqueue(&request, &authorization, 7, available_at, 3)
            .await
            .expect("idempotent enqueue"),
        job_id
    );
    assert_eq!(
        source
            .status(job_id)
            .await
            .expect("read status")
            .unwrap()
            .state,
        "pending"
    );

    let worker_id = Uuid::new_v4();
    let claim = source
        .claim_due(worker_id, Utc::now(), 1, Duration::seconds(60))
        .await
        .expect("claim due job")
        .pop()
        .expect("one claim");
    assert_eq!(claim.source_fence, 7);
    assert_eq!(claim.attempt_count, 1);
    assert_eq!(
        source
            .status(job_id)
            .await
            .expect("claimed status")
            .unwrap()
            .state,
        "claimed"
    );

    let rollback_at = Utc::now();
    source
        .record_rollback_evidence(
            worker_id,
            claim.claim_id,
            &FinanceBankProviderImportRollbackEvidence {
                import_id: request.import_id,
                source_fence: 7,
                reason: FinanceBankProviderImportFailureClass::Reconciliation,
                authoritative_write_retained: true,
                read_model_rebuild_required: true,
                recorded_at: rollback_at,
            },
        )
        .await
        .expect("append rollback evidence");
    source
        .retry_import(
            worker_id,
            claim.claim_id,
            7,
            FinanceBankProviderImportFailureClass::Retryable,
            rollback_at,
        )
        .await
        .expect("retry claim");
    assert_eq!(
        source
            .status(job_id)
            .await
            .expect("retry status")
            .unwrap()
            .state,
        "retryable"
    );

    let retry_at = rollback_at + Duration::seconds(31);
    let second_claim = source
        .claim_due(worker_id, retry_at, 1, Duration::seconds(60))
        .await
        .expect("reclaim retryable job")
        .pop()
        .expect("second claim");
    let evidence = FinanceBankProviderImportEvidence {
        import_id: request.import_id,
        account_id: request.account.local_account_id(),
        idempotency_key: request.context.idempotency_key.clone(),
        source_fence: 7,
        transaction_count: 0,
        persisted_transaction_count: 0,
        outbox_event_count: 1,
        match_proposals_generated: 0,
        recorded_at: retry_at,
    };
    let reconciliation = source
        .complete_import(worker_id, second_claim.claim_id, &evidence)
        .await
        .expect("complete with reconciled evidence");
    assert!(reconciliation.healthy);
    assert_eq!(reconciliation.persisted_transaction_count, 0);
    assert_eq!(
        source
            .status(job_id)
            .await
            .expect("completed status")
            .unwrap()
            .state,
        "completed"
    );

    let claims: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.finance_bank_provider_import_job_claims WHERE job_id=$1",
    )
    .bind(job_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("claim history count");
    let evidence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_control.finance_bank_provider_import_evidence WHERE job_id=$1",
    )
    .bind(job_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("evidence count");
    assert_eq!(claims, 2);
    assert_eq!(evidence_count, 2);
    let read_state: String = sqlx::query_scalar(
        "SELECT state
         FROM erpx_control.finance_bank_provider_import_job_status
         WHERE job_id=$1",
    )
    .bind(job_id)
    .fetch_one(&control.pools().read)
    .await
    .expect("read model status row");
    assert_eq!(read_state, "completed");
    let sensitive_read_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM information_schema.columns
         WHERE table_schema='erpx_control'
           AND table_name='finance_bank_provider_import_job_status'
           AND column_name IN ('request', 'authorization_facts', 'credential_key_ref')",
    )
    .fetch_one(&control.pools().read)
    .await
    .expect("read model schema inspection");
    assert_eq!(sensitive_read_columns, 0);

    let terminal_request = build_request(&Uuid::new_v4().to_string());
    let terminal_authorization = build_authorization(&terminal_request);
    register_holding(&control, &terminal_request).await;
    let terminal_job_id = source
        .enqueue(
            &terminal_request,
            &terminal_authorization,
            11,
            Utc::now() - Duration::seconds(1),
            1,
        )
        .await
        .expect("enqueue terminal job");
    let terminal_claim = source
        .claim_due(worker_id, Utc::now(), 1, Duration::seconds(60))
        .await
        .expect("claim terminal job")
        .pop()
        .expect("terminal claim");
    let terminal_retry_at = Utc::now();
    source
        .retry_import(
            worker_id,
            terminal_claim.claim_id,
            11,
            FinanceBankProviderImportFailureClass::Retryable,
            terminal_retry_at,
        )
        .await
        .expect("retry terminal claim once");
    assert!(
        source
            .claim_due(
                worker_id,
                terminal_retry_at + Duration::seconds(31),
                1,
                Duration::seconds(60),
            )
            .await
            .expect("dead-letter exhausted terminal job")
            .is_empty()
    );
    assert_eq!(
        source
            .status(terminal_job_id)
            .await
            .expect("terminal status")
            .unwrap()
            .state,
        "dead_letter"
    );
    let terminal_claim_state: String = sqlx::query_scalar(
        "SELECT state
         FROM erpx_control.finance_bank_provider_import_job_claims
         WHERE job_id=$1",
    )
    .bind(terminal_job_id)
    .fetch_one(&control.pools().write)
    .await
    .expect("terminal claim state");
    assert_eq!(terminal_claim_state, "failed");

    control.pools().write.close().await;
    control.pools().read.close().await;
}
