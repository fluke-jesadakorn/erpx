#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commercial::public::purchases::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, TimestampMillis};
use erpx_server_shared::public::{Clock, FailureKind, UserId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn holding() -> HoldingId {
    HoldingId::new("holding").unwrap()
}
fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}
fn request(purpose: CapabilityPurpose, mode: PackageEnforcementMode) -> PurchaseRequest {
    PurchaseRequest {
        capabilities: vec!["legal.contracts".into(), "legal.documents".into()],
        purpose,
        mode,
    }
}
fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}
fn history_now() -> TimestampMillis {
    now(4_000_000_000_000)
}

struct CommandClock;
impl Clock for CommandClock {
    fn now(&self) -> TimestampMillis {
        now(2000)
    }
}
/// Deliberately synthetic current-policy sources in this leased test database.
/// This is not the unimplemented production IAM/review/obligation assembly.
struct GrantPolicy;
impl PgPackageGrantPolicy for GrantPolicy {
    async fn prepare_sources(
        &mut self,
        connection: &mut PgConnection,
        _input: &GrantChange,
    ) -> Result<(), Error> {
        sqlx::query("LOCK TABLE fixture_grant_authority,fixture_package_obligations IN SHARE MODE")
            .execute(connection)
            .await?;
        Ok(())
    }
    async fn authorize(
        &mut self,
        connection: &mut PgConnection,
        holding_id: &HoldingId,
        company_id: &CompanyId,
    ) -> Result<UserId, Error> {
        assert_eq!(holding_id, &holding());
        let user = sqlx::query_scalar::<_, String>(
            "SELECT user_id FROM fixture_grant_authority WHERE company_id=$1 AND allowed=true",
        )
        .bind(company_id.as_str())
        .fetch_optional(connection)
        .await?;
        UserId::new(user.ok_or_else(|| {
            BusinessError::new(
                FailureKind::Forbidden,
                "FIXTURE_GRANT_FORBIDDEN",
                "Fixture authority denied.",
            )
        })?)
        .map_err(Error::from)
    }
    async fn obligations(
        &mut self,
        connection: &mut PgConnection,
        company_id: &CompanyId,
        package: &str,
    ) -> Result<i64, Error> {
        let value=sqlx::query_scalar::<_,i64>("SELECT remaining FROM fixture_package_obligations WHERE company_id=$1 AND package_id=$2").bind(company_id.as_str()).bind(package).fetch_optional(connection).await?;
        value.ok_or_else(|| {
            BusinessError::invalid(
                "FIXTURE_OBLIGATION_SOURCE_REQUIRED",
                "Explicit fixture source required.",
            )
            .into()
        })
    }
}
async fn setup_commands(pool: &DatabasePool) {
    setup(pool).await;
    sqlx::raw_sql("CREATE TABLE fixture_grant_authority(company_id text PRIMARY KEY,allowed boolean NOT NULL,user_id text NOT NULL);CREATE TABLE fixture_package_obligations(company_id text,package_id text,remaining bigint,PRIMARY KEY(company_id,package_id));INSERT INTO fixture_grant_authority VALUES('company',true,'synthetic-actor');INSERT INTO fixture_package_obligations VALUES('company','legal',0)").execute(pool.raw()).await.unwrap();
}
fn command(state: &str, version: i64, key: &str) -> GrantChange {
    GrantChange {
        grant: CompanyPackageGrant {
            company_id: "company".into(),
            package_id: "legal".into(),
            package_version: 1,
            state: state.into(),
            starts_at: 0,
            ends_at: Some(3000),
            version,
            reason: "Synthetic reviewed change".into(),
            evidence: "Synthetic owner evidence".into(),
        },
        expected_version: version - 1,
        idempotency_key: key.into(),
    }
}
async fn image(connection: &mut PgConnection, company: &str) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id=$1 AND package_id='legal'").bind(company).fetch_one(connection).await.unwrap()
}
async fn audit(connection: &mut PgConnection, company: &str, before: Option<serde_json::Value>) {
    let after = image(connection, company).await;
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES($1,$2,'legal','synthetic-actor','synthetic-hash',$3,$4,1000)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(company).bind(before).bind(after).execute(connection).await.unwrap();
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
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'legal',1,'active',0,3000,1,'Synthetic approval','PRIVATE_GRANT_EVIDENCE',1000)").bind(company).execute(&mut *tx).await.unwrap();
        audit(&mut tx, company, None).await;
    }
    tx.commit().await.unwrap();
}
async fn revoke(pool: &DatabasePool) {
    let mut tx = pool.raw().begin().await.unwrap();
    let before = image(&mut tx, "company").await;
    sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'").execute(&mut *tx).await.unwrap();
    audit(&mut tx, "company", Some(before)).await;
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn purchase_proof_is_company_scoped_clock_sensitive_and_rolls_back_with_owner() {
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let outcome=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let catalogue=current_package_catalogue()?;
            let mut reader=PgCompanyPurchaseReader::new(holding(),company(),connection,&catalogue);
            let proof=reader.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(1000)).await?;
            assert!(proof.access.iter().all(|a|a.allowed_by_mode));proof.require_allowed()?;assert_eq!(proof.witness.as_str().len(),64);
            assert!(!serde_json::to_string(&proof.access).unwrap().contains("PRIVATE_GRANT_EVIDENCE"));
            reader.revalidate(&proof.witness,now(1001)).await?;
            assert!(matches!(reader.revalidate(&proof.witness,now(3000)).await,Err(Error::Business(ref e))if e.code=="COMMERCIAL_PURCHASE_CHANGED"));
            let expired=reader.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(3000)).await?;
            assert!(expired.access.iter().all(|a|!a.allowed_by_mode&&a.decision.code==PackageDecisionCode::PackageDraining));assert_eq!(expired.require_allowed().err().unwrap().code,"PACKAGE_DRAINING");
            assert!(reader.read(&CompanyId::new("foreign").unwrap()).await.is_err());drop(reader);
            sqlx::query("UPDATE company_package_grants SET state='inactive',version=2 WHERE company_id='company'").execute(&mut *connection).await?;
            let mut reader=PgCompanyPurchaseReader::new(holding(),company(),connection,&catalogue);
            assert!(matches!(reader.revalidate(&proof.witness,now(1000)).await,Err(Error::Business(ref e))if e.code=="COMMERCIAL_PURCHASE_CHANGED"));
            Err(BusinessError::invalid("TEST_OWNER_ABORT","Rollback owner fixture.").into())
        })).await;assert!(outcome.is_err());
        assert_eq!(sqlx::query_scalar::<_,i32>("SELECT version FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn retained_history_and_explicit_shadow_disabled_modes_do_not_become_identity_grants() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        revoke(&pool).await;
        PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let catalogue = current_package_catalogue()?;
                    let mut reader =
                        PgCompanyPurchaseReader::new(holding(), company(), connection, &catalogue);
                    let history = reader
                        .inspect(
                            &request(CapabilityPurpose::History, PackageEnforcementMode::Enforced),
                            history_now(),
                        )
                        .await?;
                    assert!(history.access.iter().all(|a| a.allowed_by_mode));
                    for purpose in [
                        CapabilityPurpose::NewWork,
                        CapabilityPurpose::SettleExisting,
                    ] {
                        let denied = reader
                            .inspect(
                                &request(purpose, PackageEnforcementMode::Enforced),
                                history_now(),
                            )
                            .await?;
                        assert!(denied.access.iter().all(|a| !a.allowed_by_mode));
                        for mode in [
                            PackageEnforcementMode::Shadow,
                            PackageEnforcementMode::Disabled,
                        ] {
                            let observed = reader
                                .inspect(&request(purpose, mode), history_now())
                                .await?;
                            assert!(
                                observed
                                    .access
                                    .iter()
                                    .all(|a| a.allowed_by_mode && !a.decision.allowed)
                            );
                        }
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
async fn committed_revocation_after_an_earlier_snapshot_remains_serialization_retryable() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT company_id FROM company_package_grants")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        revoke(&pool).await;
        let catalogue = current_package_catalogue().unwrap();
        let result = PgCompanyPurchaseReader::new(holding(), company(), &mut tx, &catalogue)
            .inspect(
                &request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced),
                now(1000),
            )
            .await;
        assert!(
            matches!(result,Err(Error::Persistence(ref error))if error.sql_state()==Some("40001"))
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn purchase_witness_cannot_cross_transactions_on_the_same_backend() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let catalogue = current_package_catalogue().unwrap();
        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(
            PgCompanyPurchaseReader::new(holding(), company(), &mut connection, &catalogue)
                .inspect(
                    &request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced),
                    now(1000)
                )
                .await
                .is_err()
        );
        let mut tx = connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let witness = PgCompanyPurchaseReader::new(holding(), company(), &mut tx, &catalogue)
            .inspect(
                &request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced),
                now(1000),
            )
            .await
            .unwrap()
            .witness;
        tx.commit().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            PgCompanyPurchaseReader::new(holding(), company(), &mut tx, &catalogue)
                .revalidate(&witness, now(1000))
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn native_grant_command_commits_audit_history_and_replays_without_another_version() {
    fixture::with_database(2,|pool|async move {
        setup_commands(&pool).await;
        for _ in 0..2 {
            PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
                let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let input=command("draining",2,"native-grant-command-001");
                let result=PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&input).await?;
                assert_eq!(result,input.grant);Ok(())
            })).await.unwrap();
        }
        assert_eq!(sqlx::query_scalar::<_,i32>("SELECT version FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE id='native-grant-command-001'").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT request_hash FROM company_package_grant_events WHERE id='native-grant-command-001'").fetch_one(pool.raw()).await.unwrap(),grant_change_digest(&command("draining",2,"native-grant-command-001")).unwrap());
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_history_rights WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn native_grant_command_requires_current_authority_receipt_identity_and_obligation_sources() {
    fixture::with_database(2,|pool|async move {
        setup_commands(&pool).await;
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;
            PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"native-grant-command-001")).await?;Ok(())
        })).await.unwrap();
        sqlx::query("UPDATE fixture_grant_authority SET allowed=false").execute(pool.raw()).await.unwrap();
        let denied=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;
            PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"native-grant-command-001")).await?;Ok(())
        })).await;
        assert!(matches!(denied,Err(Error::Business(ref e))if e.code=="FIXTURE_GRANT_FORBIDDEN"));
        sqlx::query("UPDATE fixture_grant_authority SET allowed=true").execute(pool.raw()).await.unwrap();
        for (key,remaining,expected) in [("native-grant-command-001",0,"IDEMPOTENCY_CONFLICT"),("native-grant-command-002",1,"PACKAGE_OBLIGATIONS_REMAIN")] {
            sqlx::query("UPDATE fixture_package_obligations SET remaining=$1").bind(remaining as i64).execute(pool.raw()).await.unwrap();
            let outcome=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection|Box::pin(async move {
                let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;
                PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("inactive",3,key)).await?;Ok(())
            })).await;assert!(matches!(outcome,Err(Error::Business(ref e))if e.code==expected));
        }
        let aborted=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;
            PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("active",3,"native-grant-command-003")).await?;
            Err(BusinessError::invalid("TEST_OWNER_ABORT","Abort enclosing owner work.").into())
        })).await;assert!(matches!(aborted,Err(Error::Business(ref e))if e.code=="TEST_OWNER_ABORT"));
        assert_eq!(sqlx::query_scalar::<_,i32>("SELECT version FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE id='native-grant-command-003'").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn concurrent_same_key_grants_retry_the_whole_owner_unit_and_apply_once() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    struct PausingPolicy {
        calls: usize,
        ready: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }
    impl PgPackageGrantPolicy for PausingPolicy {
        async fn prepare_sources(
            &mut self,
            c: &mut PgConnection,
            i: &GrantChange,
        ) -> Result<(), Error> {
            GrantPolicy.prepare_sources(c, i).await
        }
        async fn authorize(
            &mut self,
            c: &mut PgConnection,
            h: &HoldingId,
            co: &CompanyId,
        ) -> Result<UserId, Error> {
            let actor = GrantPolicy.authorize(c, h, co).await?;
            self.calls += 1;
            if self.calls == 3 {
                self.ready.notify_one();
                self.release.notified().await;
            }
            Ok(actor)
        }
        async fn obligations(
            &mut self,
            c: &mut PgConnection,
            co: &CompanyId,
            p: &str,
        ) -> Result<i64, Error> {
            GrantPolicy.obligations(c, co, p).await
        }
    }
    fixture::with_database(3,|pool|async move {
        setup_commands(&pool).await;
        let ready=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
        let first_pool=pool.clone();let first_ready=ready.clone();let first_release=release.clone();
        let first=tokio::spawn(async move {
            PgUnitOfWork::new(first_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection|{
                let ready=first_ready.clone();let release=first_release.clone();Box::pin(async move {
                    let catalogue=current_package_catalogue()?;let mut policy=PausingPolicy{calls:0,ready,release};
                    PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"concurrent-native-grant-001")).await?;Ok(())
                })
            }).await
        });
        if tokio::time::timeout(Duration::from_secs(5),ready.notified()).await.is_err(){first.abort();let _=first.await;panic!("First grant did not reach retained owner writes");}
        let attempts=Arc::new(AtomicUsize::new(0));let counted=attempts.clone();let second_pool=pool.clone();
        let second=tokio::spawn(async move {
            PgUnitOfWork::new(second_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|connection|{
                counted.fetch_add(1,Ordering::SeqCst);Box::pin(async move {
                    let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;
                    PgPackageGrantCommands::new(holding(),company(),connection,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"concurrent-native-grant-001")).await?;Ok(())
                })
            }).await
        });
        let waiting=tokio::time::timeout(Duration::from_secs(5),async {
            loop {let blocked=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND pid<>pg_backend_pid() AND backend_type='client backend' AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        }).await.is_ok();
        release.notify_one();first.await.unwrap().unwrap();second.await.unwrap().unwrap();assert!(waiting);assert_eq!(attempts.load(Ordering::SeqCst),2);
        assert_eq!(sqlx::query_scalar::<_,i32>("SELECT version FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE id='concurrent-native-grant-001'").fetch_one(pool.raw()).await.unwrap(),1);
    }).await;
}

#[tokio::test]
async fn preview_does_not_create_revision_or_audit_and_apply_rechecks_current_authority() {
    fixture::with_database(2, |pool| async move {
        setup_commands(&pool).await;
        sqlx::query("DELETE FROM company_package_revisions WHERE company_id='company'")
            .execute(pool.raw()).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(
            TransactionOptions::default(), |connection| Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                let result = PgPackageGrantCommands::new(holding(), company(), connection,
                    &catalogue, &mut policy, &CommandClock)
                    .preview(&command("draining", 2, "native-grant-preview-001")).await?;
                assert_eq!(result.before.unwrap().version, 1);
                assert_eq!(result.after.version, 2);
                Ok(())
            })).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM company_package_revisions WHERE company_id='company'")
            .fetch_one(pool.raw()).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i32>("SELECT version FROM company_package_grants WHERE company_id='company'")
            .fetch_one(pool.raw()).await.unwrap(), 1);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM company_package_grant_events WHERE id='native-grant-preview-001'")
            .fetch_one(pool.raw()).await.unwrap(), 0);
        sqlx::query("UPDATE fixture_grant_authority SET allowed=false")
            .execute(pool.raw()).await.unwrap();
        let denied = PgUnitOfWork::new(pool.clone()).run::<(), BusinessError, _>(
            TransactionOptions::default(), |connection| Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                PgPackageGrantCommands::new(holding(), company(), connection,
                    &catalogue, &mut policy, &CommandClock)
                    .apply(&command("draining", 2, "native-grant-preview-001")).await?;
                Ok(())
            })).await;
        assert!(matches!(denied, Err(Error::Business(ref e)) if e.code=="FIXTURE_GRANT_FORBIDDEN"));
        assert_eq!(sqlx::query_scalar::<_, i32>("SELECT version FROM company_package_grants WHERE company_id='company'")
            .fetch_one(pool.raw()).await.unwrap(), 1);
    }).await;
}

/// Review source is an explicit synthetic fixture, not provider MFA/RFC approval.
struct BatchReview;
impl PgPackageBatchReview for BatchReview {
    async fn prepare_sources(
        &mut self,
        c: &mut PgConnection,
        _: &GrantChange,
    ) -> Result<(), Error> {
        sqlx::query("LOCK TABLE fixture_batch_review IN SHARE MODE")
            .execute(c)
            .await?;
        Ok(())
    }
    async fn verify(&mut self, c: &mut PgConnection, p: &GrantBatchPreview) -> Result<(), Error> {
        let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fixture_batch_review WHERE allowed=true AND revision=$1 AND prospective=$2)")
            .bind(&p.expected_revision).bind(serde_json::to_value(&p.after).unwrap()).fetch_one(c).await?;
        if !matches {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "FIXTURE_BATCH_REVIEW_DENIED",
                "Exact reviewed fixture required.",
            )
            .into());
        }
        Ok(())
    }
    async fn retain(&mut self, c: &mut PgConnection, p: &GrantBatchPreview) -> Result<(), Error> {
        self.verify(c, p).await
    }
}
async fn setup_batch(pool: &DatabasePool, key: &str) -> GrantBatch {
    setup_commands(pool).await;
    let grants = PgUnitOfWork::new(pool.clone())
        .run::<Vec<CompanyPackageGrant>, BusinessError, _>(TransactionOptions::default(), |c| {
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                Ok(
                    PgCompanyPurchaseReader::new(holding(), company(), c, &catalogue)
                        .read(&company())
                        .await?
                        .grants,
                )
            })
        })
        .await
        .unwrap();
    let mut base = command("active", 1, "unused-key-native-grant").grant;
    base.package_id = "finance".into();
    let mut addon = base.clone();
    addon.package_id = "finance-advanced".into();
    let input = GrantBatch {
        company_id: "company".into(),
        expected_revision: package_grant_revision("company", &grants).unwrap(),
        idempotency_key: key.into(),
        changes: vec![
            BatchChange {
                grant: addon.clone(),
                expected_version: 0,
            },
            BatchChange {
                grant: base.clone(),
                expected_version: 0,
            },
        ],
    };
    let mut after = grants;
    after.extend([base, addon]);
    after.sort_by(|a, b| a.package_id.cmp(&b.package_id));
    sqlx::query("CREATE TABLE fixture_batch_review(allowed boolean NOT NULL,revision text NOT NULL,prospective jsonb NOT NULL)").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO fixture_batch_review VALUES(true,$1,$2)")
        .bind(&input.expected_revision)
        .bind(serde_json::to_value(after).unwrap())
        .execute(pool.raw())
        .await
        .unwrap();
    input
}
async fn batch_apply(pool: &DatabasePool, input: GrantBatch) -> Result<GrantBatchPreview, Error> {
    PgUnitOfWork::new(pool.clone())
        .run::<GrantBatchPreview, BusinessError, _>(TransactionOptions::default(), move |c| {
            let input = input.clone();
            Box::pin(async move {
                let catalogue = current_package_catalogue()?;
                let mut policy = GrantPolicy;
                let mut review = BatchReview;
                PgPackageGrantCommands::new(
                    holding(),
                    company(),
                    c,
                    &catalogue,
                    &mut policy,
                    &CommandClock,
                )
                .apply_batch(&input, &mut review)
                .await
            })
        })
        .await
}

#[tokio::test]
async fn batch_base_addon_preview_and_atomic_apply_use_prospective_dependencies_and_exact_receipts()
{
    fixture::with_database(2,|pool|async move {
        let input=setup_batch(&pool,"native-package-batch-001").await;
        let clone=input.clone();
        let preview=PgUnitOfWork::new(pool.clone()).run::<GrantBatchPreview,BusinessError,_>(TransactionOptions::default(),move|c|{let input=clone.clone();Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let mut review=BatchReview;
            PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).preview_batch(&input,&mut review).await
        })}).await.unwrap();
        assert_eq!(preview.before.len(),1);assert_eq!(preview.after.len(),3);
        assert_eq!(preview.changes[0].grant.package_id,"finance");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),1);
        let result=batch_apply(&pool,input.clone()).await.unwrap();assert_eq!(result,preview);
        let replay=batch_apply(&pool,input.clone()).await.unwrap();assert_eq!(replay,result);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_change_sets WHERE id='native-package-batch-001'").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE package_id IN ('finance','finance-advanced')").fetch_one(pool.raw()).await.unwrap(),2);
        let mut conflicting=input.clone();conflicting.changes[0].grant.reason="Different reviewed reason".into();
        assert!(matches!(batch_apply(&pool,conflicting).await,Err(Error::Business(ref e))if e.code=="IDEMPOTENCY_CONFLICT"));
        sqlx::query("UPDATE fixture_grant_authority SET allowed=false").execute(pool.raw()).await.unwrap();
        assert!(matches!(batch_apply(&pool,input).await,Err(Error::Business(ref e))if e.code=="FIXTURE_GRANT_FORBIDDEN"));
    }).await;
}

#[tokio::test]
async fn batch_review_staleness_and_outer_rollback_leave_no_partial_grants_or_receipts() {
    fixture::with_database(2,|pool|async move {
        let input=setup_batch(&pool,"native-package-batch-002").await;
        sqlx::query("UPDATE fixture_batch_review SET allowed=false").execute(pool.raw()).await.unwrap();
        assert!(matches!(batch_apply(&pool,input.clone()).await,Err(Error::Business(ref e))if e.code=="FIXTURE_BATCH_REVIEW_DENIED"));
        sqlx::query("UPDATE fixture_batch_review SET allowed=true").execute(pool.raw()).await.unwrap();
        let mut stale=input.clone();stale.expected_revision="0".repeat(64);
        assert!(matches!(batch_apply(&pool,stale).await,Err(Error::Business(ref e))if e.code=="STALE_PACKAGE_BATCH"));
        let outcome=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let input=input.clone();Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let mut review=BatchReview;
            PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).apply_batch(&input,&mut review).await?;
            Err(BusinessError::invalid("TEST_OWNER_ABORT","Rollback complete enclosing command.").into())
        })}).await;
        assert!(matches!(outcome,Err(Error::Business(ref e))if e.code=="TEST_OWNER_ABORT"));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_change_sets").fetch_one(pool.raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE package_id IN ('finance','finance-advanced')").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn concurrent_batch_replays_retry_the_whole_unit_without_duplicate_grant_audits() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    struct PausingReview {
        ready: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }
    impl PgPackageBatchReview for PausingReview {
        async fn prepare_sources(
            &mut self,
            c: &mut PgConnection,
            i: &GrantChange,
        ) -> Result<(), Error> {
            BatchReview.prepare_sources(c, i).await
        }
        async fn verify(
            &mut self,
            c: &mut PgConnection,
            p: &GrantBatchPreview,
        ) -> Result<(), Error> {
            BatchReview.verify(c, p).await?;
            self.ready.notify_one();
            self.release.notified().await;
            Ok(())
        }
        async fn retain(
            &mut self,
            c: &mut PgConnection,
            p: &GrantBatchPreview,
        ) -> Result<(), Error> {
            BatchReview.retain(c, p).await
        }
    }
    fixture::with_database(3,|pool|async move {
        let input=setup_batch(&pool,"concurrent-native-package-batch").await;
        let ready=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
        let first_pool=pool.clone();let first_ready=ready.clone();let first_release=release.clone();let first_input=input.clone();
        let first=tokio::spawn(async move {
            PgUnitOfWork::new(first_pool).run::<GrantBatchPreview,BusinessError,_>(TransactionOptions::default(),move|c|{
                let input=first_input.clone();let ready=first_ready.clone();let release=first_release.clone();Box::pin(async move {
                    let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let mut review=PausingReview{ready,release};
                    PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).apply_batch(&input,&mut review).await
                })
            }).await
        });
        if tokio::time::timeout(Duration::from_secs(5),ready.notified()).await.is_err(){first.abort();let _=first.await;panic!("Batch did not reach retained review");}
        let attempts=Arc::new(AtomicUsize::new(0));let counted=attempts.clone();let second_pool=pool.clone();
        let second=tokio::spawn(async move {
            PgUnitOfWork::new(second_pool).run::<GrantBatchPreview,BusinessError,_>(TransactionOptions::default(),move|c|{
                counted.fetch_add(1,Ordering::SeqCst);let input=input.clone();Box::pin(async move {
                    let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let mut review=BatchReview;
                    PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).apply_batch(&input,&mut review).await
                })
            }).await
        });
        let blocked=tokio::time::timeout(Duration::from_secs(5),async {
            loop {let waiting=sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND pid<>pg_backend_pid() AND backend_type='client backend' AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap();if waiting{break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        }).await.is_ok();
        release.notify_one();let a=first.await.unwrap().unwrap();let b=second.await.unwrap().unwrap();
        assert!(blocked);assert_eq!(a,b);assert_eq!(attempts.load(Ordering::SeqCst),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_change_sets").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE package_id IN ('finance','finance-advanced')").fetch_one(pool.raw()).await.unwrap(),2);
    }).await;
}

#[tokio::test]
async fn final_review_expiry_after_all_batch_effects_rolls_back_grants_audits_and_receipt() {
    struct ExpiredAfterEffects;
    impl PgPackageBatchReview for ExpiredAfterEffects {
        async fn prepare_sources(
            &mut self,
            c: &mut PgConnection,
            i: &GrantChange,
        ) -> Result<(), Error> {
            BatchReview.prepare_sources(c, i).await
        }
        async fn verify(
            &mut self,
            c: &mut PgConnection,
            p: &GrantBatchPreview,
        ) -> Result<(), Error> {
            BatchReview.verify(c, p).await
        }
        async fn retain(
            &mut self,
            c: &mut PgConnection,
            _: &GrantBatchPreview,
        ) -> Result<(), Error> {
            // Explicit synthetic expiry at the actual finalization boundary.
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM company_package_grants WHERE company_id='company'"
                )
                .fetch_one(&mut *c)
                .await?,
                3
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM company_package_change_sets")
                    .fetch_one(c)
                    .await?,
                1
            );
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "FIXTURE_BATCH_REVIEW_EXPIRED",
                "The review expired while owner effects completed.",
            )
            .into())
        }
    }
    fixture::with_database(2,|pool|async move {
        let input=setup_batch(&pool,"native-package-batch-expiry").await;
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let input=input.clone();Box::pin(async move {
            let catalogue=current_package_catalogue()?;let mut policy=GrantPolicy;let mut review=ExpiredAfterEffects;
            PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).apply_batch(&input,&mut review).await?;Ok(())
        })}).await;
        assert!(matches!(result,Err(Error::Business(ref e))if e.code=="FIXTURE_BATCH_REVIEW_EXPIRED"));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grants WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_change_sets").fetch_one(pool.raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM company_package_grant_events WHERE package_id IN ('finance','finance-advanced')").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}
