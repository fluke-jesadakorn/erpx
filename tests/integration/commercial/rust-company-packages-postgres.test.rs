#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::purchases::*;
use erpx_server_commercial::public::subscriptions::PgCompanySubscriptionReader;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use sqlx::Connection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn holding() -> HoldingId {
    HoldingId::new("holding").unwrap()
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}

fn request(purpose: CapabilityPurpose, mode: PackageEnforcementMode) -> PurchaseRequest {
    PurchaseRequest {
        capabilities: vec!["legal.contracts".into(), "legal.documents".into()],
        purpose,
        mode,
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
    for company in ["company", "foreign"] {
        sqlx::query(
            "INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'legal',1,'active',0,3000,1,'Synthetic approval','PRIVATE_GRANT_EVIDENCE',1000)",
        )
        .bind(company)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES('event','company','legal','synthetic-actor','synthetic-hash',NULL,'{\"companyId\":\"company\",\"packageId\":\"legal\",\"packageVersion\":1,\"state\":\"active\",\"startsAt\":0,\"endsAt\":3000,\"version\":1,\"reason\":\"Synthetic approval\",\"evidence\":\"PRIVATE_GRANT_EVIDENCE\"}',1000)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES('foreign-event','foreign','legal','synthetic-actor','synthetic-hash',NULL,'{\"companyId\":\"foreign\",\"packageId\":\"legal\",\"packageVersion\":1,\"state\":\"active\",\"startsAt\":0,\"endsAt\":3000,\"version\":1,\"reason\":\"Synthetic approval\",\"evidence\":\"PRIVATE_GRANT_EVIDENCE\"}',1000)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO company_package_history_rights(company_id,package_id,package_version,source_event_id,source_image,source_grant_version,retained_at) VALUES('company','legal',1,'event','after',1,4000)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn overview(pool: &DatabasePool, at: i64) -> CompanyPackageOverview {
    PgUnitOfWork::new(pool.clone())
        .run::<CompanyPackageOverview, BusinessError, _>(
            TransactionOptions::default(),
            move |connection| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut reader =
                        PgCompanyPurchaseReader::new(holding(), company(), connection, &catalogue);
                    company_package_overview(&mut reader, &catalogue, &company(), now(at)).await
                })
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn package_overview_is_company_scoped_clocked_and_rolls_back_with_the_owner_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let initial = overview(&pool, 1000).await;
        assert_eq!(initial.company_id, "company");
        assert_eq!(initial.observed_at, 1000);
        assert_eq!(initial.billing_mode, "sandbox");
        assert_eq!(initial.grants.len(), 1);
        assert_eq!(initial.grants[0].company_id, "company");
        assert!(initial.grants.iter().all(|grant| grant.company_id == "company"));
        assert_eq!(initial.history.len(), 1);
        assert_eq!(initial.history[0].company_id, "company");
        assert!(initial.revision.len() == 64);
        assert_eq!(
            initial.revision,
            package_grant_revision("company", &initial.grants).unwrap()
        );
        assert_eq!(
            initial.packages.len(),
            current_package_catalogue().unwrap().definitions().len()
        );

        let initial_revision = initial.revision.clone();
        let outcome = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                let initial_revision = initial_revision.clone();
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE company_package_grants SET state='draining',version=2,updated_at=2000 WHERE company_id='company'",
                    )
                    .execute(&mut *connection)
                    .await?;
                    let catalogue = current_package_catalogue()?;
                    let changed = {
                        let mut reader = PgCompanyPurchaseReader::new(
                            holding(),
                            company(),
                            connection,
                            &catalogue,
                        );
                        company_package_overview(
                            &mut reader,
                            &catalogue,
                            &company(),
                            now(1000),
                        )
                        .await?
                    };
                    assert_eq!(changed.grants[0].state, "draining");
                    assert_eq!(changed.grants[0].version, 2);
                    assert_ne!(changed.revision, initial_revision);
                    Err(BusinessError::invalid(
                        "TEST_OWNER_ABORT",
                        "Rollback the owner readback fixture.",
                    )
                    .into())
                })
            })
            .await;
        assert!(outcome.is_err());

        let restored = overview(&pool, 1000).await;
        assert_eq!(restored.grants[0].state, "active");
        assert_eq!(restored.grants[0].version, 1);
        assert_eq!(restored.revision, initial.revision);
    })
    .await;
}

#[tokio::test]
async fn purchase_admission_is_bounded_company_scoped_and_never_returns_a_witness() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut reader =
                        PgCompanyPurchaseReader::new(holding(), company(), connection, &catalogue);
                    let current = reader
                        .inspect_admission(
                            &request(
                                CapabilityPurpose::NewWork,
                                PackageEnforcementMode::Enforced,
                            ),
                            now(1000),
                        )
                        .await?;
                    assert_eq!(current.company_id, "company");
                    assert_eq!(current.access.len(), 2);
                    assert!(current.access.iter().all(|access| access.allowed_by_mode));
                    assert!(current.access.iter().all(|access| access.decision.allowed));
                    assert_eq!(current.revision.len(), 64);
                    let text = serde_json::to_string(&current).unwrap();
                    assert!(!text.contains("witness"));
                    assert!(!text.contains("PRIVATE_GRANT_EVIDENCE"));

                    let invalid = reader
                        .inspect_admission(
                            &PurchaseRequest {
                                capabilities: vec![],
                                purpose: CapabilityPurpose::NewWork,
                                mode: PackageEnforcementMode::Enforced,
                            },
                            now(1000),
                        )
                        .await
                        .unwrap_err();
                    assert!(matches!(invalid, Error::Business(ref error) if error.code == "COMMERCIAL_CAPABILITIES_REQUIRED"));

                    let history = reader
                        .inspect_admission(
                            &request(
                                CapabilityPurpose::History,
                                PackageEnforcementMode::Enforced,
                            ),
                            now(5000),
                        )
                        .await?;
                    assert!(history.access.iter().all(|access| access.allowed_by_mode));

                    let settled = reader
                        .inspect_admission(
                            &request(
                                CapabilityPurpose::SettleExisting,
                                PackageEnforcementMode::Enforced,
                            ),
                            now(5000),
                        )
                        .await?;
                    assert!(settled.access.iter().all(|access| access.allowed_by_mode));

                    let denied = reader
                        .inspect_admission(
                            &request(
                                CapabilityPurpose::NewWork,
                                PackageEnforcementMode::Enforced,
                            ),
                            now(5000),
                        )
                        .await?;
                    assert!(
                        denied
                            .access
                            .iter()
                            .all(|access| !access.allowed_by_mode)
                    );
                    for mode in [
                        PackageEnforcementMode::Shadow,
                        PackageEnforcementMode::Disabled,
                    ] {
                        let observed = reader
                            .inspect_admission(
                                &request(CapabilityPurpose::NewWork, mode),
                                now(5000),
                            )
                            .await?;
                        assert!(
                            observed
                                .access
                                .iter()
                                .all(|access| access.allowed_by_mode)
                        );
                        assert!(
                            observed
                                .access
                                .iter()
                                .all(|access| !access.decision.allowed)
                        );
                    }
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn package_reads_require_a_serializable_owner_transaction() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let catalogue = current_package_catalogue().unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        let outside =
            PgCompanyPurchaseReader::new(holding(), company(), &mut connection, &catalogue)
                .inspect_admission(
                    &request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced),
                    now(1000),
                )
                .await;
        assert!(outside.is_err());

        let mut tx = connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await
            .unwrap();
        let weak = PgCompanyPurchaseReader::new(holding(), company(), &mut tx, &catalogue)
            .inspect_admission(
                &request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced),
                now(1000),
            )
            .await;
        assert!(weak.is_err());
        tx.rollback().await.unwrap();
    })
    .await;
}

struct PgBackend {
    pool: DatabasePool,
}

impl CompanyPackageReadHttpBackend for PgBackend {
    fn read<'a>(
        &'a self,
        request: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse> {
        let pool = self.pool.clone();
        let company = request.company.clone();
        let kind = request.kind.clone();
        let holding_slug = request.holding_slug.to_owned();
        Box::pin(async move {
            PgUnitOfWork::new(pool)
                .run::<CompanyPackageReadHttpResponse, BusinessError, _>(
                    TransactionOptions::default(),
                    move |connection| {
                        let company = company.clone();
                        let kind = kind.clone();
                        let holding_slug = holding_slug.clone();
                        Box::pin(async move {
                            let catalogue = current_package_catalogue()?;
                            let data = match kind {
                                CompanyPackageReadHttpKind::Overview => {
                                    let mut reader = PgCompanyPurchaseReader::new(
                                        holding(),
                                        company.clone(),
                                        connection,
                                        &catalogue,
                                    );
                                    serde_json::to_value(
                                        company_package_overview(
                                            &mut reader,
                                            &catalogue,
                                            &company,
                                            now(1000),
                                        )
                                        .await?,
                                    )
                                    .map_err(|_| {
                                        BusinessError::new(
                                            FailureKind::Unavailable,
                                            "COMMERCIAL_RESPONSE_ENCODING",
                                            "The Commercial package read could not be encoded.",
                                        )
                                    })?
                                }
                                CompanyPackageReadHttpKind::Subscriptions => {
                                    let mut reader = PgCompanySubscriptionReader::new(
                                        company.clone(),
                                        connection,
                                    );
                                    serde_json::to_value(reader.inspect(now(1000)).await?)
                                        .map_err(|_| {
                                            BusinessError::new(
                                                FailureKind::Unavailable,
                                                "COMMERCIAL_RESPONSE_ENCODING",
                                                "The Commercial subscription read could not be encoded.",
                                            )
                                        })?
                                }
                                CompanyPackageReadHttpKind::Admission {
                                    capabilities,
                                    purpose,
                                    mode,
                                } => {
                                    let mut reader = PgCompanyPurchaseReader::new(
                                        holding(),
                                        company.clone(),
                                        connection,
                                        &catalogue,
                                    );
                                    serde_json::to_value(
                                        reader
                                            .inspect_admission(
                                                &PurchaseRequest {
                                                    capabilities,
                                                    purpose,
                                                    mode,
                                                },
                                                now(1000),
                                            )
                                            .await?,
                                    )
                                    .map_err(|_| {
                                        BusinessError::new(
                                            FailureKind::Unavailable,
                                            "COMMERCIAL_RESPONSE_ENCODING",
                                            "The Commercial admission read could not be encoded.",
                                        )
                                    })?
                                }
                            };
                            Ok(CompanyPackageReadHttpResponse {
                                data,
                                context: serde_json::json!({
                                    "userId": "operator",
                                    "holdingId": "holding",
                                    "holdingSlug": holding_slug,
                                    "companyId": company.as_str(),
                                    "platformAssignmentId": "platform-assignment",
                                    "platformRole": "platform_admin",
                                    "roleRevision": 7,
                                    "modules": ["provider"],
                                    "permissions": ["platform.commercial.read"],
                                    "expiresAt": 9_007_199_254_740_991i64,
                                }),
                            })
                        })
                    },
                )
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => BusinessError::new(
                        FailureKind::Unavailable,
                        "COMMERCIAL_READ_UNAVAILABLE",
                        "The Commercial package read is unavailable.",
                    ),
                })
        })
    }
}

async fn http(addr: std::net::SocketAddr, path: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn actual_http_router_reads_overview_and_admission_over_postgres() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let router = company_package_router(vec![CompanyPackageReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(PgBackend { pool: pool.clone() }),
        }])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });

        let (status, body) = http(
            addr,
            "/api/v1/holdings/acme/commercial/companies/company/packages",
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["companyId"], "company");
        assert_eq!(body["data"]["grants"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["history"].as_array().unwrap().len(), 1);
        assert_eq!(body["context"]["holdingSlug"], "acme");

        let (status, body) = http(
            addr,
            "/api/v1/holdings/acme/commercial/companies/company/purchase-admission?capability=legal.contracts&purpose=history&mode=shadow",
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["data"]["companyId"], "company");
        assert_eq!(body["data"]["access"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["access"][0]["allowedByMode"], true);
        assert_eq!(
            body["data"]["access"][0]["decision"]["code"],
            "GRANTED"
        );

        let (status, _) = http(
            addr,
            "/api/v1/holdings/acme/commercial/companies/foreign/packages",
        )
        .await;
        assert_eq!(status, 200);
        server.abort();
    })
    .await;
}
