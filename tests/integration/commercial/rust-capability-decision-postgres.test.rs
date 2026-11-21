#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::capability_decision::*;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageDecisionCode, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::Connection;

type Error = PgTransactionError<BusinessError>;

fn company(value: &str) -> CompanyId {
    CompanyId::new(value).unwrap()
}

fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}

fn gate(allowed: bool, revision: &str) -> CapabilityGate {
    CapabilityGate {
        allowed,
        revision: revision.into(),
    }
}

async fn insert_grant(
    connection: &mut sqlx::PgConnection,
    company: &str,
    package: &str,
    state: &str,
    starts_at: i64,
    ends_at: Option<i64>,
    version: i64,
) {
    sqlx::query(
        "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,$2,1,$3,$4,$5,$6,'Synthetic approval','PRIVATE_GRANT_EVIDENCE',1000)",
    )
    .bind(company)
    .bind(package)
    .bind(state)
    .bind(starts_at)
    .bind(ends_at)
    .bind(version)
    .execute(&mut *connection)
    .await
    .unwrap();
    let after = serde_json::json!({
        "companyId": company,
        "packageId": package,
        "packageVersion": 1,
        "state": state,
        "startsAt": starts_at,
        "endsAt": ends_at,
        "version": version,
        "reason": "Synthetic approval",
        "evidence": "PRIVATE_GRANT_EVIDENCE",
    });
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES($1,$2,$3,'synthetic-actor','synthetic-hash',NULL,$4,1000)",
    )
    .bind(format!("event-{company}-{package}-{version}"))
    .bind(company)
    .bind(package)
    .bind(after)
    .execute(&mut *connection)
    .await
    .unwrap();
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
    insert_grant(&mut tx, "company", "legal", "active", 0, Some(3000), 1).await;
    insert_grant(&mut tx, "settle", "legal", "draining", 0, Some(5000), 2).await;
    insert_grant(&mut tx, "history", "legal", "inactive", 0, Some(3000), 2).await;
    insert_grant(&mut tx, "foreign", "legal", "active", 0, None, 1).await;
    sqlx::query(
        "INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES('history','legal',1,'event-history-legal-2','after',1,1500)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn access_at(
    pool: &DatabasePool,
    value: &'static str,
    capability: &'static str,
    mode: PackageEnforcementMode,
    at: i64,
) -> PurchasedAccess {
    PgUnitOfWork::new(pool.clone())
        .run::<PurchasedAccess, BusinessError, _>(
            TransactionOptions::default(),
            move |connection| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut reader =
                        PgCapabilityDecisionReader::new(company(value), connection, &catalogue);
                    reader.access(capability, mode, now(at)).await
                })
            },
        )
        .await
        .unwrap()
}

async fn require_error(
    pool: &DatabasePool,
    value: &'static str,
    capability: &'static str,
    purpose: CapabilityPurpose,
    mode: PackageEnforcementMode,
    at: i64,
) -> Error {
    PgUnitOfWork::new(pool.clone())
        .run::<BusinessError, BusinessError, _>(TransactionOptions::default(), move |connection| {
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut reader =
                    PgCapabilityDecisionReader::new(company(value), connection, &catalogue);
                Err(reader
                    .require(capability, purpose, mode, now(at))
                    .await
                    .unwrap_err())
            })
        })
        .await
        .unwrap_err()
}

async fn decide_at(
    pool: &DatabasePool,
    value: &'static str,
    release: CapabilityGate,
    authority: CapabilityGate,
    record: CapabilityGate,
    at: i64,
) -> CapabilityDecisionObservation {
    PgUnitOfWork::new(pool.clone())
        .run::<CapabilityDecisionObservation, BusinessError, _>(
            TransactionOptions::default(),
            move |connection| {
                let release = release.clone();
                let authority = authority.clone();
                let record = record.clone();
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut reader =
                        PgCapabilityDecisionReader::new(company(value), connection, &catalogue);
                    reader
                        .decide(
                            "legal.contracts",
                            CapabilityPurpose::NewWork,
                            now(at),
                            &release,
                            &authority,
                            &record,
                        )
                        .await
                })
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn postgres_current_expired_history_and_settlement_match_the_retained_decisions() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let current = access_at(
            &pool,
            "company",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            1000,
        )
        .await;
        assert!(current.new_work && current.settle_existing && current.history);
        assert_eq!(current.codes.new_work, PackageDecisionCode::Granted);
        assert_eq!(current.revision.len(), 64);

        let expired = access_at(
            &pool,
            "company",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            3000,
        )
        .await;
        assert!(!expired.new_work);
        assert!(expired.settle_existing && expired.history);
        assert_eq!(expired.codes.new_work, PackageDecisionCode::PackageDraining);

        let draining = access_at(
            &pool,
            "settle",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            1000,
        )
        .await;
        assert!(!draining.new_work);
        assert!(draining.settle_existing && draining.history);
        assert_eq!(
            draining.codes.new_work,
            PackageDecisionCode::PackageDraining
        );

        // Inactive grants prove no history right by themselves; the retained
        // history row is the only source for this Company.
        let history = access_at(
            &pool,
            "history",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            2000,
        )
        .await;
        assert!(!history.new_work && !history.settle_existing && history.history);
        assert_eq!(history.codes.history, PackageDecisionCode::Granted);

        let error = require_error(
            &pool,
            "company",
            "legal.contracts",
            CapabilityPurpose::NewWork,
            PackageEnforcementMode::Enforced,
            3000,
        )
        .await;
        let Error::Business(error) = error else {
            panic!("expected the retained business refusal");
        };
        assert_eq!(
            error.kind,
            erpx_server_shared::public::FailureKind::Forbidden
        );
        assert_eq!(error.code, "PACKAGE_DRAINING");
    })
    .await;
}

#[tokio::test]
async fn postgres_modes_and_effective_gates_never_block_shadow_or_fabricate_owner_gates() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let disabled = access_at(
            &pool,
            "none",
            "legal.contracts",
            PackageEnforcementMode::Disabled,
            1000,
        )
        .await;
        assert!(disabled.new_work && disabled.settle_existing && disabled.history);
        assert_eq!(disabled.revision, "disabled");
        assert_eq!(disabled.codes.new_work, PackageDecisionCode::Granted);

        let shadow = access_at(
            &pool,
            "none",
            "legal.contracts",
            PackageEnforcementMode::Shadow,
            1000,
        )
        .await;
        assert!(shadow.new_work && shadow.settle_existing && shadow.history);
        assert_eq!(shadow.codes.new_work, PackageDecisionCode::PackageRequired);

        let denied = access_at(
            &pool,
            "none",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            1000,
        )
        .await;
        assert!(!denied.new_work && !denied.settle_existing && !denied.history);
        assert_eq!(denied.codes.new_work, PackageDecisionCode::PackageRequired);

        let observed = decide_at(
            &pool,
            "company",
            gate(true, "release-1"),
            gate(true, "authority-1"),
            gate(true, "record-1"),
            1000,
        )
        .await;
        assert!(observed.decision.allowed);
        assert_eq!(observed.decision.code.as_code(), "GRANTED");
        assert_eq!(observed.decision.revisions.record, "record-1");
        assert_eq!(observed.source_revision.len(), 64);
        assert_eq!(observed.revision.len(), 64);

        let withheld = decide_at(
            &pool,
            "company",
            gate(true, "release-1"),
            gate(false, ""),
            gate(true, "record-1"),
            1000,
        )
        .await;
        assert!(!withheld.decision.allowed);
        assert_eq!(withheld.decision.code.as_code(), "AUTHORITY_REQUIRED");
    })
    .await;
}

#[tokio::test]
async fn postgres_reader_uses_the_callers_connection_and_company_scope() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let catalogue = current_package_catalogue().unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(!connection.is_in_transaction());
        let mut reader =
            PgCapabilityDecisionReader::new(company("company"), &mut connection, &catalogue);
        let facts = reader.facts().await.unwrap();
        assert_eq!(facts.grants.len(), 1);
        assert_eq!(facts.grants[0].company_id, "company");
        let access = reader
            .access(
                "legal.contracts",
                PackageEnforcementMode::Enforced,
                now(1000),
            )
            .await
            .unwrap();
        assert!(access.new_work);
        assert!(!connection.is_in_transaction());

        let mut foreign =
            PgCapabilityDecisionReader::new(company("none"), &mut connection, &catalogue);
        let access = foreign
            .access(
                "legal.contracts",
                PackageEnforcementMode::Enforced,
                now(1000),
            )
            .await
            .unwrap();
        assert!(!access.new_work);
        assert_eq!(access.codes.new_work, PackageDecisionCode::PackageRequired);
    })
    .await;
}
