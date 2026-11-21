#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::purchases::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, TimestampMillis, UserId,
};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn holding() -> HoldingId {
    HoldingId::new("holding").unwrap()
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

struct CommandClock;

impl Clock for CommandClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(2000)
    }
}

/// Synthetic current-policy sources in this leased test database. Production
/// Platform authority and owner obligation ports are composition concerns.
struct GrantPolicy;

impl PgPackageGrantPolicy for GrantPolicy {
    async fn prepare_sources(
        &mut self,
        _: &mut PgConnection,
        _: &GrantChange,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &HoldingId,
        _: &CompanyId,
    ) -> Result<UserId, Error> {
        UserId::new("synthetic-actor").map_err(Error::from)
    }
    async fn obligations(
        &mut self,
        _: &mut PgConnection,
        _: &CompanyId,
        _: &str,
    ) -> Result<i64, Error> {
        Ok(0)
    }
}

struct Review;

impl PgPackageBatchReview for Review {
    async fn prepare_sources(
        &mut self,
        _: &mut PgConnection,
        _: &GrantChange,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn verify(&mut self, _: &mut PgConnection, _: &GrantBatchPreview) -> Result<(), Error> {
        Ok(())
    }
    async fn retain(&mut self, _: &mut PgConnection, _: &GrantBatchPreview) -> Result<(), Error> {
        Ok(())
    }
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company','legal',1,'active',0,3000,1,'Synthetic approval','PRIVATE_GRANT_EVIDENCE',1000)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES('event','company','legal','synthetic-actor','synthetic-hash',NULL,'{\"companyId\":\"company\",\"packageId\":\"legal\",\"packageVersion\":1,\"state\":\"active\",\"startsAt\":0,\"endsAt\":3000,\"version\":1,\"reason\":\"Synthetic approval\",\"evidence\":\"PRIVATE_GRANT_EVIDENCE\"}',1000)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn current() -> CompanyPackageGrant {
    CompanyPackageGrant {
        company_id: "company".into(),
        package_id: "legal".into(),
        package_version: 1,
        state: "active".into(),
        starts_at: 0,
        ends_at: Some(3000),
        version: 1,
        reason: "Synthetic approval".into(),
        evidence: "PRIVATE_GRANT_EVIDENCE".into(),
    }
}

fn change(state: &str, version: i64, key: &str) -> GrantChange {
    GrantChange {
        grant: CompanyPackageGrant {
            state: state.into(),
            version,
            reason: format!("Reviewed transition to {state}"),
            ..current()
        },
        expected_version: version - 1,
        idempotency_key: key.into(),
    }
}

fn batch(state: &str, version: i64, key: &str) -> GrantBatch {
    let grants = [current()];
    GrantBatch {
        company_id: "company".into(),
        expected_revision: package_grant_revision("company", &grants).unwrap(),
        changes: vec![BatchChange {
            expected_version: version - 1,
            grant: CompanyPackageGrant {
                state: state.into(),
                version,
                reason: format!("Reviewed transition to {state}"),
                ..current()
            },
        }],
        idempotency_key: key.into(),
    }
}

async fn preview(pool: &DatabasePool, input: GrantChange) -> Result<GrantPreview, Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let input = input.clone();
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                PgPackageGrantCommands::new(
                    holding(),
                    company(),
                    connection,
                    &catalogue,
                    &mut policy,
                    &CommandClock,
                )
                .preview(&input)
                .await
            })
        })
        .await
}

async fn apply(pool: &DatabasePool, input: GrantChange) -> Result<CompanyPackageGrant, Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let input = input.clone();
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                PgPackageGrantCommands::new(
                    holding(),
                    company(),
                    connection,
                    &catalogue,
                    &mut policy,
                    &CommandClock,
                )
                .apply(&input)
                .await
            })
        })
        .await
}

async fn preview_batch(pool: &DatabasePool, input: GrantBatch) -> Result<GrantBatchPreview, Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let input = input.clone();
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                PgPackageGrantCommands::new(
                    holding(),
                    company(),
                    connection,
                    &catalogue,
                    &mut policy,
                    &CommandClock,
                )
                .preview_batch(&input, &mut Review)
                .await
            })
        })
        .await
}

async fn apply_batch(pool: &DatabasePool, input: GrantBatch) -> Result<GrantBatchPreview, Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let input = input.clone();
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                PgPackageGrantCommands::new(
                    holding(),
                    company(),
                    connection,
                    &catalogue,
                    &mut policy,
                    &CommandClock,
                )
                .apply_batch(&input, &mut Review)
                .await
            })
        })
        .await
}

#[tokio::test]
async fn package_grant_preview_and_apply_replay_their_exact_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let batch_preview = batch("draining", 2, "native-grant-batch-001");
        let comparison = preview_batch(&pool, batch_preview.clone()).await.unwrap();
        assert_eq!(comparison.before.len(), 1);
        assert_eq!(comparison.before[0].state, "active");
        assert_eq!(comparison.after[0].state, "draining");
        assert_eq!(comparison.after[0].version, 2);

        let applied = apply_batch(&pool, batch_preview.clone()).await.unwrap();
        assert_eq!(applied.after[0].state, "draining");
        let replay = apply_batch(&pool, batch_preview.clone()).await.unwrap();
        assert_eq!(replay.after, applied.after);

        let mut changed = batch_preview;
        changed.changes[0].grant.reason = "A different reviewed transition".into();
        assert!(matches!(
            apply_batch(&pool, changed).await,
            Err(Error::Business(error)) if error.code == "IDEMPOTENCY_CONFLICT"
        ));

        let single_preview = change("inactive", 3, "native-grant-single-01");
        let previewed = preview(&pool, single_preview.clone()).await.unwrap();
        assert_eq!(previewed.before.as_ref().unwrap().state, "draining");
        assert_eq!(previewed.after.state, "inactive");
        assert_eq!(previewed.expected_version, 2);

        let receipt = apply(&pool, single_preview.clone()).await.unwrap();
        assert_eq!(receipt.state, "inactive");
        let replay = apply(&pool, single_preview.clone()).await.unwrap();
        assert_eq!(replay, receipt);

        let mut changed = single_preview;
        changed.grant.reason = "A different reviewed transition".into();
        assert!(matches!(
            apply(&pool, changed).await,
            Err(Error::Business(error)) if error.code == "IDEMPOTENCY_CONFLICT"
        ));
    })
    .await;
}
