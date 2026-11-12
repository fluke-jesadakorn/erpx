#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::canonical_purchase_write::inspect_canonical_purchase_write_boundary;
use erpx_server_commercial::public::purchase_after::{
    inspect_purchase_after, write_purchase_shadow_checked_observation,
};
use erpx_server_commercial::public::purchase_command_boundary::*;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, CompanyPurchaseReadPort, PackageEnforcementMode,
    PgCompanyPurchaseReader, current_package_catalogue, lock_purchase_write_sources,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn holding() -> HoldingId {
    HoldingId::new("holding").unwrap()
}

fn scope(company: &str, capability: &str, purpose: CapabilityPurpose) -> PurchaseCommandScope {
    PurchaseCommandScope {
        company_id: CompanyId::new(company).unwrap(),
        capability: capability.into(),
        purpose,
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
    // The boundary samples the database clock, so retained fixtures stay open.
    for (company, package, state, ends_at) in [
        ("company", "legal", "active", None),
        ("settle", "legal", "draining", None),
        ("revoked", "legal", "inactive", None),
        ("second", "legal", "active", None),
    ] {
        insert_grant(&mut tx, company, package, state, 0, ends_at, 1).await;
    }
    tx.commit().await.unwrap();
}

async fn insert_grant(
    connection: &mut PgConnection,
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

async fn audit_current_grant(connection: &mut PgConnection, company: &str, package: &str) {
    sqlx::query(
        "INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT $1,company_id,package_id,'synthetic-actor','synthetic-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1000 FROM company_package_grants WHERE company_id=$2 AND package_id=$3",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(company)
    .bind(package)
    .execute(&mut *connection)
    .await
    .unwrap();
}

async fn assertions(connection: &mut PgConnection, key: &str) -> Vec<(String, i64)> {    sqlx::query_as("SELECT step,ok FROM command_assertions WHERE command_id=$1 ORDER BY step")
        .bind(key)
        .fetch_all(&mut *connection)
        .await
        .unwrap()
}

async fn assertion_count(pool: &DatabasePool, key: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE command_id=$1")
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn advisory_available(pool: &DatabasePool, key: &str) -> bool {
    sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn company_fence_held(connection: &mut PgConnection) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='company_package_grants'::regclass AND granted AND mode='RowExclusiveLock')",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap()
}

async fn advisory_lock_count(connection: &mut PgConnection) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='advisory' AND granted",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap()
}

fn business(error: Error) -> BusinessError {
    match error {
        Error::Business(error) => error,
        Error::Persistence(error) => panic!("expected a business failure, got {error}"),
    }
}

#[tokio::test]
async fn enforced_boundary_holds_retained_locks_and_guards_both_phases() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let probe = pool.clone();
        let committed = pool.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                let probe = probe.clone();
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = PurchaseCommandBoundary::new(
                        PackageEnforcementMode::Enforced,
                        "commercial-command-001",
                        scope("company", "legal.contracts", CapabilityPurpose::NewWork),
                    )?
                    .expect("enforced boundary");
                    boundary.begin(c, &holding(), &catalogue).await?;

                    assert!(company_fence_held(c).await);
                    assert!(advisory_lock_count(c).await >= 1);
                    assert_eq!(
                        assertions(c, "commercial-command-001").await,
                        vec![("commercial-purchase-before".to_owned(), 1)]
                    );
                    assert!(
                        !advisory_available(&probe, "commercial.package:company:company").await
                    );
                    assert!(
                        advisory_available(&probe, "commercial.package:company:unrelated").await
                    );

                    let receipt = boundary.finish(c, &holding(), &catalogue).await?;
                    let decision = receipt.decision.expect("enforced decision");
                    assert!(decision.allowed);
                    assert_eq!(decision.code.as_code(), "GRANTED");
                    assert!(receipt.observation.is_none());
                    assert_eq!(
                        assertions(c, "commercial-command-001").await,
                        vec![
                            ("commercial-purchase-after".to_owned(), 1),
                            ("commercial-purchase-before".to_owned(), 1),
                        ]
                    );

                    boundary.cleanup(c).await?;
                    assert!(assertions(c, "commercial-command-001").await.is_empty());
                    Ok(())
                })
            })
            .await
            .unwrap();
        assert_eq!(assertion_count(&committed, "commercial-command-001").await, 0);
    })
    .await;
}

#[tokio::test]
async fn enforced_denial_before_effects_is_the_retained_conflict_and_leaves_no_assertion() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let count = pool.clone();
        let outcome = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = PurchaseCommandBoundary::new(
                        PackageEnforcementMode::Enforced,
                        "commercial-command-denied",
                        scope("unpurchased", "legal.contracts", CapabilityPurpose::NewWork),
                    )?
                    .expect("enforced boundary");
                    boundary.begin(c, &holding(), &catalogue).await?;
                    Ok(())
                })
            })
            .await;
        let failure = business(outcome.unwrap_err());
        assert_eq!(failure.code, "PACKAGE_CAPABILITY_CHANGED");
        assert_eq!(failure.kind, FailureKind::Conflict);
        assert_eq!(assertion_count(&count, "commercial-command-denied").await, 0);
    })
    .await;
}

#[tokio::test]
async fn enforced_after_guard_rejects_a_source_change_made_by_owner_effects() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let failure = PgUnitOfWork::new(pool.clone())
            .run::<BusinessError, BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = PurchaseCommandBoundary::new(
                        PackageEnforcementMode::Enforced,
                        "commercial-command-after",
                        scope("settle", "legal.contracts", CapabilityPurpose::SettleExisting),
                    )?
                    .expect("enforced boundary");
                    boundary.begin(c, &holding(), &catalogue).await?;
                    sqlx::query(
                        "UPDATE company_package_grants SET state='inactive' WHERE company_id='settle'",
                    )
                    .execute(&mut *c)
                    .await
                    .unwrap();
                    audit_current_grant(c, "settle", "legal").await;
                    match boundary.finish(c, &holding(), &catalogue).await {
                        Err(error) => Ok(business(error)),
                        Ok(_) => panic!("a changed purchase source must not pass the after guard"),
                    }
                })
            })
            .await
            .unwrap();
        assert_eq!(failure.code, "PACKAGE_CAPABILITY_CHANGED");
        assert_eq!(failure.kind, FailureKind::Conflict);
    })
    .await;
}

#[tokio::test]
async fn enforced_after_guard_rejects_expiry_during_the_command() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let failure = PgUnitOfWork::new(pool.clone())
            .run::<BusinessError, BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = PurchaseCommandBoundary::new(
                        PackageEnforcementMode::Enforced,
                        "commercial-command-expiry",
                        scope("company", "legal.contracts", CapabilityPurpose::NewWork),
                    )?
                    .expect("enforced boundary");
                    boundary.begin(c, &holding(), &catalogue).await?;
                    // The retained clock now reads past this end boundary.
                    sqlx::query(
                        "UPDATE company_package_grants SET ends_at=1000 WHERE company_id='company'",
                    )
                    .execute(&mut *c)
                    .await
                    .unwrap();
                    audit_current_grant(c, "company", "legal").await;
                    match boundary.finish(c, &holding(), &catalogue).await {
                        Err(error) => Ok(business(error)),
                        Ok(_) => panic!("an expired purchase must not pass the after guard"),
                    }
                })
            })
            .await
            .unwrap();
        assert_eq!(failure.code, "PACKAGE_CAPABILITY_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn shadow_boundary_observes_without_assertions_or_refusal() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let receipt = PgUnitOfWork::new(pool.clone())
            .run::<PurchaseCommandReceipt, BusinessError, _>(
                TransactionOptions::default(),
                move |c| {
                    Box::pin(async move {
                        let catalogue = current_package_catalogue()?;
                        let mut boundary = PurchaseCommandBoundary::new(
                            PackageEnforcementMode::Shadow,
                            "commercial-command-shadow",
                            scope("company", "legal.contracts", CapabilityPurpose::NewWork),
                        )?
                        .expect("shadow boundary");
                        boundary.begin(c, &holding(), &catalogue).await?;
                        assert!(assertions(c, "commercial-command-shadow").await.is_empty());
                        boundary.complete(c, &holding(), &catalogue).await
                    })
                },
            )
            .await
            .unwrap();
        assert!(receipt.decision.expect("shadow decision").allowed);
        let observation = receipt.observation.expect("shadow observation");
        let event = serde_json::to_value(&observation).unwrap();
        assert_eq!(event["event"], "commercial.purchase.shadow");
        assert_eq!(event["phase"], "checked");
        assert_eq!(event["companyId"], "company");
        assert_eq!(event["capability"], "legal.contracts");
        assert_eq!(event["purpose"], "new-work");
        assert_eq!(event["code"], "GRANTED");
        assert_eq!(event["purchaseRevision"].as_str().unwrap().len(), 64);
        assert!(event.get("grants").is_none() && event.get("history").is_none());

        let mut sink = Vec::new();
        write_purchase_shadow_checked_observation(&mut sink, &observation);
        assert_eq!(
            String::from_utf8(sink).unwrap(),
            format!("{}\n", serde_json::to_string(&observation).unwrap())
        );
    })
    .await;
}

#[tokio::test]
async fn shadow_denial_observes_the_retained_code_without_failing_the_command() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let receipt = PgUnitOfWork::new(pool.clone())
            .run::<PurchaseCommandReceipt, BusinessError, _>(
                TransactionOptions::default(),
                move |c| {
                    Box::pin(async move {
                        let catalogue = current_package_catalogue()?;
                        let mut boundary = PurchaseCommandBoundary::new(
                            PackageEnforcementMode::Shadow,
                            "commercial-command-shadow-denied",
                            scope("unpurchased", "legal.contracts", CapabilityPurpose::NewWork),
                        )?
                        .expect("shadow boundary");
                        boundary.begin(c, &holding(), &catalogue).await?;
                        boundary.complete(c, &holding(), &catalogue).await
                    })
                },
            )
            .await
            .unwrap();
        let decision = receipt.decision.expect("shadow decision");
        assert!(!decision.allowed);
        assert_eq!(decision.code.as_code(), "PACKAGE_REQUIRED");
        let event = serde_json::to_value(receipt.observation.expect("shadow observation")).unwrap();
        assert_eq!(event["code"], "PACKAGE_REQUIRED");
        assert_eq!(event["purchaseRevision"].as_str().unwrap().len(), 64);
    })
    .await;
}

#[tokio::test]
async fn holding_and_read_only_fences_keep_the_retained_advisory_keys() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let probe = pool.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                let probe = probe.clone();
                Box::pin(async move {
                    lock_holding_purchase_read_sources(c).await?;
                    assert!(company_fence_held(c).await);
                    // One advisory key per distinct Company, in Company order.
                    for company in ["company", "settle", "revoked", "second"] {
                        assert!(
                            !advisory_available(
                                &probe,
                                &format!("commercial.package:company:{company}")
                            )
                            .await
                        );
                    }
                    assert!(advisory_available(&probe, "commercial.package:company:absent").await);
                    Ok(())
                })
            })
            .await
            .unwrap();

        let probe = pool.clone();
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                let probe = probe.clone();
                Box::pin(async move {
                    lock_company_purchase_advisory_sources(
                        c,
                        &CompanyId::new("company").unwrap(),
                    )
                    .await?;
                    assert!(
                        !advisory_available(&probe, "commercial.package:company:company").await
                    );
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

#[tokio::test]
async fn a_boundary_cannot_finish_in_another_transaction() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        let boundary = PgUnitOfWork::new(pool.clone())
            .run::<PurchaseCommandBoundary, BusinessError, _>(
                TransactionOptions::default(),
                move |c| {
                    Box::pin(async move {
                        let catalogue = current_package_catalogue()?;
                        let mut boundary = PurchaseCommandBoundary::new(
                            PackageEnforcementMode::Enforced,
                            "commercial-command-reuse",
                            scope("company", "legal.contracts", CapabilityPurpose::NewWork),
                        )?
                        .expect("enforced boundary");
                        boundary.begin(c, &holding(), &catalogue).await?;
                        Ok(boundary)
                    })
                },
            )
            .await
            .unwrap();
        let outcome = PgUnitOfWork::new(pool.clone())
            .run_once::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = boundary;
                    boundary.finish(c, &holding(), &catalogue).await?;
                    Ok(())
                })
            })
            .await;
        let failure = business(outcome.unwrap_err());
        assert_eq!(failure.code, "COMMERCIAL_PURCHASE_CHANGED");

        // The guard order itself fails closed rather than silently writing.
        let out_of_order = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut boundary = PurchaseCommandBoundary::new(
                        PackageEnforcementMode::Shadow,
                        "commercial-command-order",
                        scope("company", "legal.contracts", CapabilityPurpose::NewWork),
                    )?
                    .expect("shadow boundary");
                    boundary.finish(c, &holding(), &catalogue).await?;
                    Ok(())
                })
            })
            .await
            .unwrap_err();
        match out_of_order {
            Error::Persistence(error) => {
                assert_eq!(error.code, "COMMERCIAL_PURCHASE_COMMAND_ORDER");
            }
            Error::Business(error) => panic!("expected an order failure, got {error}"),
        }
    })
    .await;
}

#[tokio::test]
async fn purchase_command_failure_maps_only_the_retained_assertion_detail() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let error = sqlx::query(
                        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'commercial-purchase-before',0)",
                    )
                    .bind("commercial-command-mapping")
                    .execute(&mut *c)
                    .await
                    .unwrap_err();
                    let failure = purchase_command_failure(&error).expect("purchase failure");
                    assert_eq!(failure.code, "PACKAGE_CAPABILITY_CHANGED");
                    assert_eq!(failure.kind, FailureKind::Conflict);
                    Ok(())
                })
            })
            .await
            .unwrap();

        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    let error = sqlx::query(
                        "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,'owner-authority',0)",
                    )
                    .bind("commercial-command-mapping")
                    .execute(&mut *c)
                    .await
                    .unwrap_err();
                    assert!(purchase_command_failure(&error).is_none());
                    Ok(())
                })
            })
            .await
            .unwrap();
    })
    .await;
}

fn fixture_capabilities(catalogue: &CommercialCatalogue) -> Vec<String> {
    let mut capabilities = std::collections::BTreeSet::new();
    for package in ["legal", "work", "work-social"] {
        if let Some(definition) = catalogue
            .definitions()
            .iter()
            .find(|definition| definition.id == package)
        {
            capabilities.extend(definition.visible_capabilities.iter().cloned());
            capabilities.extend(definition.included_capabilities.iter().cloned());
        }
    }
    // A capability no fixture company owns keeps the denial side represented.
    capabilities.insert("legal.contracts".into());
    // A capability absent from the catalogue exercises the empty-candidate FALSE path.
    capabilities.insert("unknown.capability".into());
    capabilities.into_iter().collect()
}

#[tokio::test]
async fn shared_boundary_predicate_agrees_with_the_catalogue_decision_matrix() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        insert_grant(&mut tx, "addon-only", "work-social", "active", 0, None, 1).await;
        insert_grant(&mut tx, "addon-parent", "work", "active", 0, None, 1).await;
        insert_grant(&mut tx, "addon-parent", "work-social", "active", 0, None, 1).await;
        tx.commit().await.unwrap();

        let catalogue = current_package_catalogue().unwrap();
        let capabilities = fixture_capabilities(&catalogue);
        for company in ["company", "revoked", "addon-only", "addon-parent"] {
            let company = CompanyId::new(company).unwrap();
            let capabilities = capabilities.clone();
            let mismatches = PgUnitOfWork::new(pool.clone())
                .run::<Vec<String>, BusinessError, _>(
                    TransactionOptions::default(),
                    move |c| {
                        let company = company.clone();
                        let capabilities = capabilities.clone();
                        Box::pin(async move {
                            let catalogue = current_package_catalogue()?;
                            let locks = lock_purchase_write_sources(c).await?.bind().await?;
                            let write =
                                inspect_canonical_purchase_write_boundary(c, &locks).await?;
                            let facts = {
                                let mut reader = PgCompanyPurchaseReader::new(
                                    holding(),
                                    company.clone(),
                                    c,
                                    &catalogue,
                                );
                                reader.read(&company).await?
                            };
                            let now: i64 = sqlx::query_scalar(
                                "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
                            )
                            .fetch_one(&mut *c)
                            .await?;
                            let mut mismatches = Vec::new();
                            for capability in &capabilities {
                                for purpose in [
                                    CapabilityPurpose::NewWork,
                                    CapabilityPurpose::SettleExisting,
                                    CapabilityPurpose::History,
                                ] {
                                    let guarded = inspect_purchase_after(
                                        c,
                                        Some(&write),
                                        &catalogue,
                                        &company,
                                        capability,
                                        purpose,
                                        PackageEnforcementMode::Enforced,
                                    )
                                    .await?
                                    .expect("enforced decision");
                                    let expected = catalogue.purchased_capability(
                                        &facts.grants,
                                        company.as_str(),
                                        capability,
                                        purpose,
                                        now,
                                        &facts.history,
                                    );
                                    if guarded.allowed != expected.allowed {
                                        mismatches.push(format!(
                                            "{}/{capability}/{purpose:?}: sql={} catalogue={}",
                                            company.as_str(),
                                            guarded.allowed,
                                            expected.allowed
                                        ));
                                    }
                                }
                            }
                            Ok(mismatches)
                        })
                    },
                )
                .await
                .unwrap();
            assert!(
                mismatches.is_empty(),
                "the boundary predicate and catalogue must not disagree: {mismatches:#?}"
            );
        }
    })
    .await;
}
