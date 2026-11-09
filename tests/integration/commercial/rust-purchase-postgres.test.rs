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

// Existing leased database only. Node executes the real ordered presentation
// source and runtime PG parser; credentials remain in the inherited env.
fn node_presentation_revision(database: &str, company: &str, timestamp: i64) -> String {
    let node = std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
    let script = r#"
import { PostgresDatabase } from '@erpx/server-runtime/adapters/runtime/postgres';
import { readCompanyPurchaseFacts } from '@erpx/server-commercial/adapters/package-grants';
import { capabilityRevisionSource } from '@erpx/server-commercial/domain/capability-decision';
import { createHash } from 'node:crypto';
const url = new URL(process.env.ERPX_TEST_ADMIN_URL); url.pathname = '/' + process.argv[1];
const db = new PostgresDatabase({ connectionString: url.toString() });
try { const company=process.argv[2], facts=await readCompanyPurchaseFacts(db,company);
  process.stdout.write(createHash('sha256').update(capabilityRevisionSource(company,facts.grants,Number(process.argv[3]),facts.history)).digest('hex'));
} finally { await db.close(); }
"#;
    let output = std::process::Command::new(node)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args(["--conditions=react-server", "--import", "tsx", "--input-type=module", "--eval", script, database, company, &timestamp.to_string()])
        .output().expect("Node presentation oracle unavailable");
    assert!(output.status.success(), "Node presentation oracle failed");
    String::from_utf8(output.stdout).unwrap()
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

#[tokio::test]
async fn presentation_wire_revision_matches_persisted_node_runtime_snapshot() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        // Equal under Node localeCompare but byte-distinct. Persist raw unsafe
        // BIGINT values so the runtime's JSONB/JSON.parse path is exercised.
        let mut tx = pool.raw().begin().await.unwrap();
        for (index, package, start, end) in [
            (0, "é", 0_i64, Some(3000_i64)),
            (1, "e\u{301}", 0, Some(3000)),
            (2, "quote\"key", 1000, Some(3000)),
            (3, "unsafe", 9_007_199_254_740_993, Some(9_007_199_254_740_995)),
        ] {
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company',$1,1,'active',$2,$3,1,'Synthetic approval','synthetic evidence',1000)")
                .bind(package).bind(start).bind(end).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT $1,company_id,package_id,'synthetic-actor','synthetic-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1000 FROM company_package_grants WHERE company_id='company' AND package_id=$2")
                .bind(format!("wire-revision-event-{index}")).bind(package).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let node = std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
        let version = std::process::Command::new(&node).arg("--version").output().expect("Node unavailable");
        assert!(version.status.success());
        assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), "v26.11.1");
        let script = r#"
import { PostgresDatabase } from '@erpx/server-runtime/adapters/runtime/postgres';
import { readCompanyPurchaseFacts } from '@erpx/server-commercial/adapters/package-grants';
import { capabilityRevisionSource } from '@erpx/server-commercial/domain/capability-decision';
import { createHash } from 'node:crypto';
const url = new URL(process.env.ERPX_TEST_ADMIN_URL);
url.pathname = '/' + process.argv[1];
const db = new PostgresDatabase({ connectionString: url.toString() });
try {
  const result = {};
  for (const company of ['company', 'foreign']) {
    const facts = await readCompanyPurchaseFacts(db, company);
    if (!facts.history.length) throw new Error('EXPECTED_RETAINED_HISTORY');
    const first = facts.history[0];
    const boundary = first.retainedAt;
    if (!Number.isSafeInteger(boundary) || boundary < 1) throw new Error('INVALID_RETAINED_BOUNDARY');
    const times = [0, 1000, 3000, 9007199254740993, boundary - 1, boundary];
    const revisions = times.map(now =>
      createHash('sha256').update(capabilityRevisionSource(company, facts.grants, now, facts.history)).digest('hex'));
    const phase = now => JSON.parse(capabilityRevisionSource(company, facts.grants, now, facts.history)).history.find(h => h[3] === first.sourceEventId)?.[5];
    result[company] = { revisions, boundary, phaseBefore: phase(boundary - 1), phaseAt: phase(boundary), grants: facts.grants.map(g => g.packageId), history: facts.history.map(h => [h.packageId,h.packageVersion,h.sourceEventId,h.retainedAt]) };
  }
  result.collationEqual = JSON.stringify(['é',1,1,'active',0,3000,'current']).localeCompare(JSON.stringify(['e\u0301',1,1,'active',0,3000,'current'])) === 0;
  process.stdout.write(JSON.stringify(result));
} finally { await db.close(); }
"#;
        let output = std::process::Command::new(&node)
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server", "--import", "tsx", "--input-type=module", "--eval", script, &database])
            .output().expect("Node oracle unavailable");
        // Never emit child stderr: it can contain connection configuration.
        assert!(output.status.success(), "Node presentation oracle failed");
        let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(oracle["collationEqual"], true);
        for selected in ["company", "foreign"] {
            let company = CompanyId::new(selected).unwrap();
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let catalogue = current_package_catalogue().unwrap();
            let mut reader = PgCompanyPurchaseReader::new(holding(), company.clone(), &mut tx, &catalogue);
            let inspection = reader.inspect(&request(CapabilityPurpose::History, PackageEnforcementMode::Shadow), now(1000)).await.unwrap();
            let facts = reader.read(&company).await.unwrap();
            let grants: Vec<_> = facts.grants.iter().map(|g| g.package_id.as_str()).collect();
            assert!(!facts.history.is_empty(), "the oracle must exercise persisted retained history");
            if selected == "company" {
                assert!(facts.history.iter().any(|h| h.package_id == "é" && h.source_event_id == "wire-revision-event-0"));
                assert!(facts.history.iter().any(|h| h.package_id == "e\u{301}" && h.source_event_id == "wire-revision-event-1"));
                assert!(!facts.history.iter().any(|h| h.source_event_id == "wire-revision-event-3"));
            }
            let history: Vec<_> = facts.history.iter().map(|h| (h.package_id.as_str(), h.package_version, h.source_event_id.as_str(), h.retained_at)).collect();
            assert_eq!(serde_json::to_value(grants).unwrap(), oracle[selected]["grants"]);
            assert_eq!(serde_json::to_value(history).unwrap(), oracle[selected]["history"]);
            let boundary = oracle[selected]["boundary"].as_i64().unwrap();
            assert_eq!(boundary, facts.history[0].retained_at);
            assert_eq!(oracle[selected]["phaseBefore"], "future");
            assert_eq!(oracle[selected]["phaseAt"], "retained");
            for (index, timestamp) in [0_i64, 1000, 3000, 9_007_199_254_740_993, boundary - 1, boundary].into_iter().enumerate() {
                let revision = purchase_wire_revision(selected, &facts.grants, &facts.history, now(timestamp)).unwrap();
                assert_eq!(revision, oracle[selected]["revisions"][index].as_str().unwrap());
            }
            // The separate presentation revision must not alter the native
            // transaction-bound witness or its internal revision.
            let retained = reader.revalidate(&inspection.witness, now(1000)).await.unwrap();
            assert_eq!(retained.revision, inspection.revision);
            assert_eq!(retained.witness.as_str(), inspection.witness.as_str());
            tx.rollback().await.unwrap();

            // Exercise the guarded API in a fresh transaction: its SHARE
            // fence is acquired before the first MVCC snapshot. Both wire and
            // native proof must be derived from this same retained fact set.
            let mut guarded = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *guarded).await.unwrap();
            let locks = lock_purchase_read_sources(&mut guarded).await.unwrap().bind().await.unwrap();
            let mut reader = PgReadOnlyCompanyPurchaseReader::new(holding(), company.clone(), &mut guarded, &catalogue, &locks);
            let shadow = request(CapabilityPurpose::History, PackageEnforcementMode::Shadow);
            for (index, timestamp) in [0_i64, 1000, 3000, 9_007_199_254_740_993, boundary - 1, boundary].into_iter().enumerate() {
                let observation = reader.inspect_presentation(&shadow, now(timestamp)).await.unwrap();
                assert_eq!(observation.wire_revision(), oracle[selected]["revisions"][index].as_str().unwrap());
                let retained = reader.revalidate_presentation(&observation, now(timestamp)).await.unwrap();
                assert_eq!(retained.inspection().revision, observation.inspection().revision);
                assert_eq!(retained.inspection().witness.as_str(), observation.inspection().witness.as_str());
                assert_eq!(retained.wire_revision(), observation.wire_revision());
            }
            let before = reader.inspect_presentation(&shadow, now(boundary - 1)).await.unwrap();
            assert!(reader.revalidate_presentation(&before, now(boundary)).await.is_err());
            assert!(reader.inspect_presentation(&request(CapabilityPurpose::History, PackageEnforcementMode::Disabled), now(1000)).await.is_err());
            drop(reader);
            guarded.rollback().await.unwrap();
        }
    }).await;
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

/// Qualifies the real Commercial review/evidence writer, still using the
/// explicitly synthetic GrantPolicy above; not operator/MFA acceptance.
#[tokio::test]
async fn native_lifecycle_review_rolls_back_with_batch_and_replays_original_evidence() {
    use erpx_server_commercial::public::package_review::*;
    fixture::with_database(2,|pool|async move {
        let input=setup_batch(&pool,"native-reviewed-lifecycle-001").await;
        let catalogue=current_package_catalogue().unwrap();
        let actor=PackageReviewActor{user_id:"synthetic-actor".into(),session_id:"fixture-session".into(),assignment_id:"fixture-assignment".into(),role:"platform_admin".into(),role_revision:1};
        let signer=PackageLifecycleReviewSigner::new("fixture-only-secret-32-characters-minimum").unwrap();
        let token=signer.issue(&catalogue,actor.clone(),"holding",&input,&input.expected_revision,1000).unwrap().token;
        sqlx::raw_sql("CREATE FUNCTION reject_reviewed_fixture_grant() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.package_id='finance-advanced' THEN RAISE EXCEPTION 'synthetic reviewed batch failure';END IF;RETURN NEW;END$$;CREATE TRIGGER reject_reviewed_fixture_grant BEFORE INSERT ON company_package_grants FOR EACH ROW EXECUTE FUNCTION reject_reviewed_fixture_grant()").execute(pool.raw()).await.unwrap();
        for fail in [true,false] {
            let input=input.clone();let actor=actor.clone();let token=token.clone();
            let result=PgUnitOfWork::new(pool.clone()).run::<GrantBatchPreview,BusinessError,_>(TransactionOptions::default(),move|c|{let input=input.clone();let actor=actor.clone();let token=token.clone();Box::pin(async move {
                let catalogue=current_package_catalogue()?;
                let signer=PackageLifecycleReviewSigner::new("fixture-only-secret-32-characters-minimum")?;
                let mut review=PgPackageLifecycleReview::new(&signer,&CommandClock,&catalogue,&actor,"holding",&input,&token)?;
                let mut policy=GrantPolicy;
                PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&CommandClock).apply_batch(&input,&mut review).await
            })}).await;
            if fail {
                assert!(result.is_err());
                let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_review_evidence),(SELECT count(*) FROM company_package_change_sets),(SELECT count(*) FROM company_package_grants WHERE package_id IN('finance','finance-advanced'))").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(0,0,0));
                sqlx::raw_sql("DROP TRIGGER reject_reviewed_fixture_grant ON company_package_grants;DROP FUNCTION reject_reviewed_fixture_grant()").execute(pool.raw()).await.unwrap();
            }else{assert_eq!(result.unwrap().changes.len(),2);}
        }
        let original:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(e) FROM company_package_review_evidence e WHERE change_set_id=$1").bind(&input.idempotency_key).fetch_one(pool.raw()).await.unwrap();
        assert_eq!(original["actor_id"],"synthetic-actor");assert_eq!(original["reviewed_at"],1000);assert_eq!(original["applied_at"],2000);assert_eq!(original["comparison_json"]["source"]["kind"],"package-lifecycle");assert_eq!(original["review_hash"],signer.review_hash(&token));
        struct ExpiredClock;
        impl Clock for ExpiredClock{fn now(&self)->TimestampMillis{now(301000)}}
        let key=input.idempotency_key.clone();
        PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let input=input.clone();let actor=actor.clone();let token=token.clone();Box::pin(async move{
            let catalogue=current_package_catalogue()?;
            let signer=PackageLifecycleReviewSigner::new("fixture-only-secret-32-characters-minimum")?;
            let mut review=PgPackageLifecycleReview::new(&signer,&ExpiredClock,&catalogue,&actor,"holding",&input,&token)?;
            let mut policy=GrantPolicy;
            let result=PgPackageGrantCommands::new(holding(),company(),c,&catalogue,&mut policy,&ExpiredClock).apply_batch(&input,&mut review).await?;
            assert_eq!(result.changes.len(),2);Ok(())
        })}).await.unwrap();
        let retained:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(e) FROM company_package_review_evidence e WHERE change_set_id=$1").bind(&key).fetch_one(pool.raw()).await.unwrap();assert_eq!(retained,original);
    }).await;
}
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

// Regression for absent-row snapshots, not a claim of actual Quality IAM.
// The writer uses the real grant command with the suite's synthetic policy.
#[tokio::test]
async fn absent_purchase_sources_must_not_survive_a_committed_company_grant() {
    use std::time::Duration;
    fixture::with_database(3,|pool|async move {
        setup_commands(&pool).await;
        sqlx::raw_sql("INSERT INTO fixture_grant_authority VALUES('phantom-company',true,'synthetic-actor'); INSERT INTO fixture_package_obligations VALUES('phantom-company','legal',0)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("phantom-company").unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let mut grant=command("active",1,"phantom-native-grant-001");grant.grant.company_id="phantom-company".into();
        let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
        PgPackageGrantCommands::new(holding(),company.clone(),&mut writer,&catalogue,&mut policy,&CommandClock).apply(&grant).await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let reader=tokio::spawn(async move {
            let mut c=reader_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            lock_commercial_authority_sources(&mut tx).await.unwrap();
            let initial:i64=sqlx::query_scalar("SELECT count(*) FROM company_package_grants WHERE company_id='phantom-company'").fetch_one(&mut *tx).await.unwrap();assert_eq!(initial,0);send.send(pid).unwrap();
            let catalogue=current_package_catalogue().unwrap();
            let mut reader=PgCompanyPurchaseReader::new(holding(),CompanyId::new("phantom-company").unwrap(),&mut tx,&catalogue);
            let result=reader.read(&CompanyId::new("phantom-company").unwrap()).await.map(|f|(f.grants.len(),f.history.len()));
            tx.rollback().await.unwrap();result
        });
        let reader_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..500 { blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await; }
        assert!(blocked,"reader must actually await the real company grant writer");writer.commit().await.unwrap();
        let result=tokio::time::timeout(Duration::from_secs(10),reader).await.unwrap().unwrap();
        match result { Ok((grants,history))=>assert_eq!((grants,history),(1,1),"a retained pre-insert snapshot must not be treated as fresh"),Err(Error::Persistence(error))=>assert_eq!(error.sql_state(),Some("40001")),Err(error)=>panic!("unexpected purchase read error: {error:?}") }
    }).await;
}

#[tokio::test]
async fn absent_purchase_reader_holds_company_fence_until_its_unit_finishes() {
    use std::time::Duration;
    fixture::with_database(3,|pool|async move {
        setup_commands(&pool).await;
        sqlx::raw_sql("INSERT INTO fixture_grant_authority VALUES('phantom-company',true,'synthetic-actor'); INSERT INTO fixture_package_obligations VALUES('phantom-company','legal',0)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("phantom-company").unwrap();
        let mut reader_tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader_tx).await.unwrap();
        lock_commercial_authority_sources(&mut reader_tx).await.unwrap();let catalogue=current_package_catalogue().unwrap();
        let mut reader=PgCompanyPurchaseReader::new(holding(),company.clone(),&mut reader_tx,&catalogue);
        let proof=reader.inspect(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),history_now()).await.unwrap();
        assert!(proof.access.iter().all(|item|!item.allowed_by_mode));
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader_tx).await.unwrap();
        let writer_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {
            let mut connection=writer_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut tx=connection.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();send.send(pid).unwrap();
            let mut grant=command("active",1,"phantom-native-grant-002");grant.grant.company_id="phantom-company".into();
            let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
            PgPackageGrantCommands::new(holding(),CompanyId::new("phantom-company").unwrap(),&mut tx,&catalogue,&mut policy,&CommandClock).apply(&grant).await.unwrap();tx.commit().await.unwrap();
        });
        let writer_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..500 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(reader_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        assert!(blocked,"the actual grant writer must await the retained Company fence");
        let mut reader=PgCompanyPurchaseReader::new(holding(),company,&mut reader_tx,&catalogue);reader.revalidate(&proof.witness,history_now()).await.unwrap();drop(reader);
        reader_tx.commit().await.unwrap();tokio::time::timeout(Duration::from_secs(10),writer).await.unwrap().unwrap();
        let rows:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_grants WHERE company_id='phantom-company'),(SELECT count(*) FROM company_package_history_rights WHERE company_id='phantom-company')").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,(1,1));
    }).await;
}

#[tokio::test]
async fn guarded_purchase_read_retries_before_snapshot_and_sees_real_new_grant_and_history() {
    use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
    use std::time::Duration;
    fixture::with_database(3,|pool|async move {
        setup_commands(&pool).await;
        let database: String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        sqlx::raw_sql("INSERT INTO fixture_grant_authority VALUES('guarded-company',true,'synthetic-actor'); INSERT INTO fixture_package_obligations VALUES('guarded-company','legal',0)").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let mut grant=command("active",1,"guarded-native-grant-001");grant.grant.company_id="guarded-company".into();let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
        PgPackageGrantCommands::new(holding(),CompanyId::new("guarded-company").unwrap(),&mut writer,&catalogue,&mut policy,&CommandClock).apply(&grant).await.unwrap();
        let contended=Arc::new(tokio::sync::Notify::new());let attempts=Arc::new(AtomicUsize::new(0));let other_pool=pool.clone();let signal=contended.clone();let counted=attempts.clone();
        let reader=tokio::spawn(async move {
            PgUnitOfWork::new(other_pool).run::<String,BusinessError,_>(TransactionOptions::default(),move|c|{let signal=signal.clone();let counted=counted.clone();Box::pin(async move {
                counted.fetch_add(1,Ordering::SeqCst);
                let prep=match lock_purchase_read_sources(c).await { Ok(prep)=>prep,Err(error)=>{signal.notify_one();return Err(error)} };
                let locks=prep.bind().await?;let catalogue=current_package_catalogue()?;
                let mut reader=PgReadOnlyCompanyPurchaseReader::new(holding(),CompanyId::new("guarded-company")?,c,&catalogue,&locks);
                let history=reader.inspect(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),history_now()).await?;assert!(history.access.iter().all(|entry|entry.allowed_by_mode));reader.revalidate(&history.witness,history_now()).await?;
                let active=reader.inspect(&request(CapabilityPurpose::SettleExisting,PackageEnforcementMode::Enforced),now(1000)).await?;assert!(active.access.iter().all(|entry|entry.allowed_by_mode));
                let presentation=reader.inspect_presentation(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),history_now()).await?;
                presentation.inspection().require_allowed()?;
                let retained=reader.revalidate_presentation(&presentation,history_now()).await?;
                assert_eq!(retained.wire_revision(),presentation.wire_revision());
                Ok(presentation.wire_revision().to_owned())
            })}).await
        });
        tokio::time::timeout(Duration::from_secs(5),contended.notified()).await.expect("reader did not fail its NOWAIT fence against the writer");writer.commit().await.unwrap();
        let wire=tokio::time::timeout(Duration::from_secs(10),reader).await.unwrap().unwrap().unwrap();assert!(attempts.load(Ordering::SeqCst)>=2);
        assert_eq!(wire,node_presentation_revision(&database,"guarded-company",history_now().get()));
        let rows:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM company_package_grants WHERE company_id='guarded-company'),(SELECT count(*) FROM company_package_history_rights WHERE company_id='guarded-company')").fetch_one(pool.raw()).await.unwrap();assert_eq!(rows,(1,1));
    }).await;
}

#[tokio::test]
async fn guarded_purchase_retry_exhaustion_has_no_owner_effect() {
    use std::sync::{Arc,atomic::{AtomicUsize,Ordering}};
    fixture::with_database(3,|pool|async move {
        setup_commands(&pool).await;sqlx::query("CREATE TABLE purchase_retry_effects(id text primary key)").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let counted=attempts.clone();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let counted=counted.clone();Box::pin(async move {
            counted.fetch_add(1,Ordering::SeqCst);let prep=lock_purchase_read_sources(c).await?;let _locks=prep.bind().await?;
            sqlx::query("INSERT INTO purchase_retry_effects VALUES('should-not-exist')").execute(c).await?;Ok(())
        })}).await;
        assert!(matches!(result,Err(Error::Persistence(ref error)) if error.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && error.sql_state()==Some("55P03")));
        assert_eq!(attempts.load(Ordering::SeqCst),4);let count:i64=sqlx::query_scalar("SELECT count(*) FROM purchase_retry_effects").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,0);writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn guarded_purchase_readers_share_fence_across_companies_without_upgrade() {
    use std::time::Duration;
    fixture::with_database(4,|pool|async move {
        setup_commands(&pool).await;let catalogue=current_package_catalogue().unwrap();
        let database: String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let mut first=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        let first_locks=lock_purchase_read_sources(&mut first).await.unwrap().bind().await.unwrap();
        let mut second=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *second).await.unwrap();
        let second_locks=lock_purchase_read_sources(&mut second).await.unwrap().bind().await.unwrap();
        let mut first_reader=PgReadOnlyCompanyPurchaseReader::new(holding(),company(),&mut first,&catalogue,&first_locks);
        let proof=first_reader.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(1000)).await.unwrap();assert!(proof.access.iter().all(|a|a.allowed_by_mode));
        let presentation=first_reader.inspect_presentation(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),now(1000)).await.unwrap();
        assert_eq!(presentation.wire_revision(),node_presentation_revision(&database,"company",1000));
        assert!(first_reader.revalidate(&proof.witness,now(3000)).await.is_err());first_reader.revalidate(&proof.witness,now(1000)).await.unwrap();drop(first_reader);
        let mut second_reader=PgReadOnlyCompanyPurchaseReader::new(holding(),CompanyId::new("foreign").unwrap(),&mut second,&catalogue,&second_locks);
        assert!(second_reader.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(1000)).await.unwrap().access.iter().all(|a|a.allowed_by_mode));
        let foreign=second_reader.inspect_presentation(&request(CapabilityPurpose::History,PackageEnforcementMode::Shadow),now(1000)).await.unwrap();
        assert_eq!(foreign.wire_revision(),node_presentation_revision(&database,"foreign",1000));
        assert!(second_reader.revalidate_presentation(&presentation,now(1000)).await.is_err());
        assert!(second_reader.revalidate(&proof.witness,now(1000)).await.is_err());drop(second_reader);
        let first_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *first).await.unwrap();
        let writer_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {
            let mut c=writer_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();
            let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();send.send(pid).unwrap();
            let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;PgPackageGrantCommands::new(holding(),company(),&mut tx,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"guarded-reader-first-001")).await.unwrap();tx.commit().await.unwrap();
        });
        let writer_pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..500 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(first_pid).bind(writer_pid).fetch_one(pool.raw()).await.unwrap();if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        assert!(blocked,"writer must wait for guarded read-only fence");
        let mut retained_reader=PgReadOnlyCompanyPurchaseReader::new(holding(),company(),&mut first,&catalogue,&first_locks);
        assert_eq!(retained_reader.revalidate_presentation(&presentation,now(1000)).await.unwrap().wire_revision(),node_presentation_revision(&database,"company",1000));
        drop(retained_reader);
        first.commit().await.unwrap();second.commit().await.unwrap();tokio::time::timeout(Duration::from_secs(10),writer).await.unwrap().unwrap();
        let state:String=sqlx::query_scalar("SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='legal'").fetch_one(pool.raw()).await.unwrap();assert_eq!(state,"draining");
        let after_node=node_presentation_revision(&database,"company",1000);
        assert_ne!(after_node,presentation.wire_revision());
        assert_eq!(foreign.wire_revision(),node_presentation_revision(&database,"foreign",1000));
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();
        let locks=lock_purchase_read_sources(&mut fresh).await.unwrap().bind().await.unwrap();
        let mut fresh_reader=PgReadOnlyCompanyPurchaseReader::new(holding(),company(),&mut fresh,&catalogue,&locks);
        let after=fresh_reader.inspect_presentation(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),now(1000)).await.unwrap();
        assert_eq!(after.wire_revision(),after_node);
        assert!(fresh_reader.revalidate_presentation(&presentation,now(1000)).await.is_err());
        drop(fresh_reader);fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn guarded_purchase_read_accepts_stronger_locks_and_rejects_cross_unit() {
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        let locks=bind_purchase_read_sources(&mut tx).await.unwrap();let catalogue=current_package_catalogue().unwrap();
        let mut reader=PgReadOnlyCompanyPurchaseReader::new(holding(),company(),&mut tx,&catalogue,&locks);let proof=reader.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(1000)).await.unwrap();drop(reader);
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(bind_purchase_read_sources(&mut other).await.is_err());
        let mut wrong=PgReadOnlyCompanyPurchaseReader::new(holding(),company(),&mut other,&catalogue,&locks);assert!(wrong.inspect(&request(CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced),now(1000)).await.is_err());assert!(wrong.revalidate(&proof.witness,now(1000)).await.is_err());drop(wrong);other.rollback().await.unwrap();tx.rollback().await.unwrap();
        let mut plain=pool.raw().acquire().await.unwrap();assert!(lock_purchase_read_sources(&mut plain).await.is_err());
    }).await;
}

#[tokio::test]
async fn guarded_presentation_retains_same_denial_across_shadow_and_enforced_modes() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let absent_company = CompanyId::new("company-without-grant").unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks = lock_purchase_read_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let catalogue = current_package_catalogue().unwrap();
        let mut reader = PgReadOnlyCompanyPurchaseReader::new(holding(), absent_company.clone(), &mut tx, &catalogue, &locks);
        let shadow = reader.inspect_presentation(&request(CapabilityPurpose::NewWork, PackageEnforcementMode::Shadow), now(1000)).await.unwrap();
        let enforced = reader.inspect_presentation(&request(CapabilityPurpose::NewWork, PackageEnforcementMode::Enforced), now(1000)).await.unwrap();
        assert_eq!(shadow.wire_revision(), enforced.wire_revision());
        assert_eq!(shadow.wire_revision(), node_presentation_revision(&database, absent_company.as_str(), 1000));
        assert_eq!(shadow.inspection().revision, enforced.inspection().revision);
        assert_ne!(shadow.inspection().witness.as_str(), enforced.inspection().witness.as_str(), "the existing token remains bound to mode");
        assert_eq!(shadow.inspection().access.len(), enforced.inspection().access.len());
        assert!(!shadow.inspection().access.is_empty());
        for (observed, denied) in shadow.inspection().access.iter().zip(&enforced.inspection().access) {
            assert_eq!(observed.capability, denied.capability);
            assert_eq!(observed.purpose, denied.purpose);
            assert_eq!(observed.decision, denied.decision);
            assert!(!observed.decision.allowed);
            assert!(observed.allowed_by_mode);
            assert!(!denied.allowed_by_mode);
        }
        assert!(shadow.inspection().require_allowed().is_ok());
        let denial = enforced.inspection().require_allowed().err().expect("enforced denial required");
        assert_eq!(denial.code, enforced.inspection().access[0].decision.code.as_code());
        let retained_shadow = reader.revalidate_presentation(&shadow, now(1000)).await.unwrap();
        let retained_enforced = reader.revalidate_presentation(&enforced, now(1000)).await.unwrap();
        assert_eq!(retained_shadow.inspection().access, shadow.inspection().access);
        assert_eq!(retained_enforced.inspection().access, enforced.inspection().access);
        assert_eq!(retained_shadow.wire_revision(), shadow.wire_revision());
        assert_eq!(retained_enforced.inspection().witness.as_str(), enforced.inspection().witness.as_str());
        drop(reader);

        // Commercial proves Company purchase facts only, never a user identity.
        let mut other = PgReadOnlyCompanyPurchaseReader::new(holding(), company(), &mut tx, &catalogue, &locks);
        assert!(other.revalidate_presentation(&shadow, now(1000)).await.is_err());
        assert!(other.revalidate_presentation(&enforced, now(1000)).await.is_err());
        drop(other);
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn commercial_authority_require_checks_writer_fence_compatibility() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut outside=pool.raw().acquire().await.unwrap();assert!(require_commercial_authority_sources_held(&mut outside).await.is_err());drop(outside);
        for sql in [
            "LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE",
            "LOCK TABLE company_package_history_rights IN ROW EXCLUSIVE MODE",
            "LOCK TABLE company_package_grants,company_package_history_rights IN SHARE MODE",
            "LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE; LOCK TABLE company_package_history_rights IN SHARE MODE",
            "LOCK TABLE company_package_grants IN SHARE MODE; LOCK TABLE company_package_history_rights IN ROW EXCLUSIVE MODE",
        ] {
            let mut tx=pool.raw().begin().await.unwrap();sqlx::raw_sql(sql).execute(&mut *tx).await.unwrap();
            assert!(matches!(require_commercial_authority_sources_held(&mut tx).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="COMMERCIAL_AUTHORITY_LOCKS_REQUIRED"),"insufficient: {sql}");tx.rollback().await.unwrap();
        }
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_commercial_fence").execute(&mut *tx).await.unwrap();
        lock_commercial_authority_sources(&mut tx).await.unwrap();require_commercial_authority_sources_held(&mut tx).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT count(DISTINCT relation) FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='RowExclusiveLock' AND relation IN ('company_package_grants'::regclass,'company_package_history_rights'::regclass)").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,2);
        sqlx::query("ROLLBACK TO SAVEPOINT before_commercial_fence").execute(&mut *tx).await.unwrap();assert!(require_commercial_authority_sources_held(&mut tx).await.is_err());
        sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN SHARE UPDATE EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_commercial_authority_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        let mut strong=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN SHARE ROW EXCLUSIVE MODE").execute(&mut *strong).await.unwrap();require_commercial_authority_sources_held(&mut strong).await.unwrap();strong.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn writer_capable_purchase_fence_serializes_real_grants_and_is_unit_bound() {
    use std::time::Duration;
    fixture::with_database(3, |pool| async move {
        setup_commands(&pool).await;
        let catalogue=current_package_catalogue().unwrap();
        // A real owner writer arrives first. The fence returns a retryable
        // NOWAIT conflict without establishing a purchase snapshot.
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let mut policy=GrantPolicy;
        PgPackageGrantCommands::new(holding(),company(),&mut writer,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"write-fence-writer-first")).await.unwrap();
        let mut contender=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *contender).await.unwrap();
        assert!(matches!(lock_purchase_write_sources(&mut contender).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        contender.rollback().await.unwrap();writer.commit().await.unwrap();
        let mut first=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        let locks=lock_purchase_write_sources(&mut first).await.unwrap().bind().await.unwrap();
        locks.require_current(&mut first).await.unwrap();
        let state:String=sqlx::query_scalar("SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='legal'").fetch_one(&mut *first).await.unwrap();
        assert_eq!(state,"draining");
        // General purchase reader's nested ROW EXCLUSIVE does not upgrade
        // the retained SRE fence and remains usable in the writer unit.
        let mut reader=PgCompanyPurchaseReader::new(holding(),company(),&mut first,&catalogue);
        let _=reader.inspect(&request(CapabilityPurpose::History,PackageEnforcementMode::Enforced),now(1000)).await.unwrap();
        drop(reader);locks.require_current(&mut first).await.unwrap();
        let first_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *first).await.unwrap();
        let (send,receive)=tokio::sync::oneshot::channel();let other=pool.clone();
        let mut writer=tokio::spawn(async move {
            let mut tx=other.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::query("SET LOCAL statement_timeout='15s'").execute(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();let _=send.send(pid);
            let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
            PgPackageGrantCommands::new(holding(),company(),&mut tx,&catalogue,&mut policy,&CommandClock).apply(&command("active",3,"write-fence-reader-first")).await.unwrap();
            tx.commit().await.unwrap();
        });
        let writer_pid=tokio::time::timeout(Duration::from_secs(5),receive).await.ok().and_then(Result::ok);
        let blocked=if let Some(pid)=writer_pid {tokio::time::timeout(Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(first_pid).bind(pid).fetch_one(&mut *first).await.unwrap();
            if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await.is_ok()} else {false};
        let release=first.commit().await;
        let result=match tokio::time::timeout(Duration::from_secs(20),&mut writer).await {Ok(result)=>Some(result),Err(_)=>{writer.abort();let _=writer.await;None}};
        release.unwrap();assert!(blocked,"real writer must wait on the exact fence holder");result.expect("writer timed out").expect("writer failed");
        let state:String=sqlx::query_scalar("SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='legal'").fetch_one(pool.raw()).await.unwrap();assert_eq!(state,"active");
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        let other_locks=lock_purchase_write_sources(&mut other).await.unwrap().bind().await.unwrap();
        assert!(locks.require_current(&mut other).await.is_err());other_locks.require_current(&mut other).await.unwrap();other.rollback().await.unwrap();
        let mut save=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *save).await.unwrap();
        sqlx::query("SAVEPOINT before_write_fence").execute(&mut *save).await.unwrap();let retained=lock_purchase_write_sources(&mut save).await.unwrap().bind().await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_write_fence").execute(&mut *save).await.unwrap();
        assert!(matches!(retained.require_current(&mut save).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="COMMERCIAL_WRITE_LOCKS_REQUIRED"));save.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn writer_purchase_fence_retries_whole_unit_after_real_grant_commit() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use std::time::Duration;
    fixture::with_database(3, |pool| async move {
        setup_commands(&pool).await;
        sqlx::query("CREATE TABLE fixture_write_fence_effects(id text PRIMARY KEY)").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
        PgPackageGrantCommands::new(holding(),company(),&mut writer,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"sre-retry-writer")).await.unwrap();
        let notified=Arc::new(tokio::sync::Notify::new());let committed=Arc::new(tokio::sync::Notify::new());let attempts=Arc::new(AtomicUsize::new(0));
        let signal=notified.clone();let released=committed.clone();let count=attempts.clone();let retry_pool=pool.clone();
        let mut task=tokio::spawn(async move {
            PgUnitOfWork::new(retry_pool).run::<String,BusinessError,_>(TransactionOptions::default(),move|c|{
                let signal=signal.clone();let released=released.clone();let count=count.clone();Box::pin(async move {
                    let attempt=count.fetch_add(1,Ordering::SeqCst);
                    let prep=match lock_purchase_write_sources(c).await {Ok(prep)=>prep,Err(error)=>{
                        if attempt==0 { signal.notify_one(); released.notified().await; }
                        return Err(error)
                    }};
                    let locks=prep.bind().await?;
                    let state:String=sqlx::query_scalar("SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='legal'").fetch_one(&mut *c).await?;
                    sqlx::query("INSERT INTO fixture_write_fence_effects VALUES('successful-unit')").execute(&mut *c).await?;
                    locks.require_current(c).await?;Ok(state)
                })
            }).await
        });
        let conflict=tokio::time::timeout(Duration::from_secs(5),notified.notified()).await;
        let commit=writer.commit().await;
        committed.notify_one();
        let result=match tokio::time::timeout(Duration::from_secs(10),&mut task).await {Ok(result)=>Some(result),Err(_)=>{task.abort();let _=task.await;None}};
        commit.unwrap();assert!(conflict.is_ok(),"expected real grant writer to trigger 55P03");
        assert_eq!(result.expect("retry task timed out").expect("retry task failed").unwrap(),"draining");
        assert!(attempts.load(Ordering::SeqCst)>=2);
        let (effects,events):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM fixture_write_fence_effects),(SELECT count(*) FROM company_package_grant_events WHERE company_id='company' AND package_id='legal')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!((effects,events),(1,2));
    }).await;
}

#[tokio::test]
async fn writer_purchase_fence_retry_exhaustion_leaves_no_partial_effects() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    fixture::with_database(3, |pool| async move {
        setup_commands(&pool).await;
        sqlx::query("CREATE TABLE fixture_write_fence_effects(id text PRIMARY KEY)").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        let catalogue=current_package_catalogue().unwrap();let mut policy=GrantPolicy;
        PgPackageGrantCommands::new(holding(),company(),&mut writer,&catalogue,&mut policy,&CommandClock).apply(&command("draining",2,"sre-exhaust-writer")).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let count=attempts.clone();
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let count=count.clone();Box::pin(async move {
            count.fetch_add(1,Ordering::SeqCst);
            let locks=lock_purchase_write_sources(c).await?.bind().await?;
            sqlx::query("INSERT INTO fixture_write_fence_effects VALUES('must-not-exist')").execute(&mut *c).await?;
            locks.require_current(c).await?;Ok(())
        })}).await;
        let rollback=writer.rollback().await;
        assert!(matches!(result,Err(Error::Persistence(ref error)) if error.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && error.sql_state()==Some("55P03")));
        assert_eq!(attempts.load(Ordering::SeqCst),4);rollback.unwrap();
        let (effects,state,events):(i64,String,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM fixture_write_fence_effects),(SELECT state FROM company_package_grants WHERE company_id='company' AND package_id='legal'),(SELECT count(*) FROM company_package_grant_events WHERE company_id='company' AND package_id='legal')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!((effects,state.as_str(),events),(0,"active",1));
    }).await;
}

#[tokio::test]
async fn purchase_write_fence_can_bind_only_current_strong_transaction_locks() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut missing=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *missing).await.unwrap();
        assert!(matches!(bind_purchase_write_sources(&mut missing).await,Err(Error::Persistence(ref e)) if e.code=="COMMERCIAL_WRITE_LOCKS_REQUIRED"));
        sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN ROW EXCLUSIVE MODE").execute(&mut *missing).await.unwrap();
        assert!(matches!(bind_purchase_write_sources(&mut missing).await,Err(Error::Persistence(ref e)) if e.code=="COMMERCIAL_WRITE_LOCKS_REQUIRED"));
        missing.rollback().await.unwrap();
        let mut weak=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *weak).await.unwrap();
        sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN SHARE MODE").execute(&mut *weak).await.unwrap();
        assert!(matches!(bind_purchase_write_sources(&mut weak).await,Err(Error::Persistence(ref e)) if e.code=="COMMERCIAL_WRITE_LOCKS_REQUIRED"));
        weak.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT before_bound_fence").execute(&mut *tx).await.unwrap();
        sqlx::query("LOCK TABLE company_package_grants,company_package_history_rights IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        let proof=bind_purchase_write_sources(&mut tx).await.unwrap();proof.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_bound_fence").execute(&mut *tx).await.unwrap();
        assert!(matches!(proof.require_current(&mut tx).await,Err(Error::Persistence(ref e)) if e.code=="COMMERCIAL_WRITE_LOCKS_REQUIRED"));
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_purchase_write_sources(&mut other).await.unwrap().bind().await.unwrap();
        assert!(proof.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let mut no_transaction=pool.raw().acquire().await.unwrap();
        assert!(bind_purchase_write_sources(&mut no_transaction).await.is_err());
    }).await;
}

#[tokio::test]
async fn canonical_purchase_write_boundary_retains_public_locks_and_unit() {
    use erpx_server_commercial::public::canonical_purchase_write::inspect_canonical_purchase_write_boundary;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut weak=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *weak).await.unwrap();
        assert!(bind_purchase_write_sources(&mut weak).await.is_err());weak.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT canonical_locks").execute(&mut *tx).await.unwrap();
        let locks=lock_purchase_write_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let source=inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.unwrap();source.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT canonical_locks").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE company_package_grants(id text);CREATE TEMP TABLE company_package_history_rights(id text);SET LOCAL search_path TO pg_temp, public").execute(&mut *tx).await.unwrap();
        let locks=lock_purchase_write_sources(&mut tx).await.unwrap().bind().await.unwrap();
        sqlx::query("LOCK TABLE public.company_package_grants,public.company_package_history_rights IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        assert!(inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.is_err(),"canonical locks do not make shadow-resolved reads safe");
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
        let source=inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.unwrap();source.require_current(&mut tx).await.unwrap();
        sqlx::query("SET LOCAL search_path TO pg_catalog, pg_temp, public").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();tx.rollback().await.unwrap();
    }).await;
}

// Uses the actual TypeScript SQL generator and the Rust decision's sampled clock.
#[tokio::test]
async fn purchase_after_decision_matches_ts_sql_in_same_fenced_unit() {
    use erpx_server_commercial::public::canonical_purchase_write::inspect_canonical_purchase_write_boundary;
    use erpx_server_commercial::public::purchase_after::inspect_purchase_after;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        // Real audited facts include ICU-equal identifiers and unsafe BIGINTs.
        // The independently executed oracle query does not attest aggregate
        // order from the evaluator's statement; do not treat it as that proof.
        let mut seed=pool.raw().begin().await.unwrap();
        for (index,package,start,end) in [(0,"é",0_i64,Some(3000_i64)),(1,"e\u{301}",0,Some(3000)),(2,"unsafe",9_007_199_254_740_993,Some(9_007_199_254_740_995))] {
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company',$1,1,'active',$2,$3,1,'Synthetic approval','synthetic evidence',1000)").bind(package).bind(start).bind(end).execute(&mut *seed).await.unwrap();
            sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT $1,company_id,package_id,'synthetic-actor','synthetic-hash',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1000 FROM company_package_grants WHERE company_id='company' AND package_id=$2").bind(format!("checked-wire-event-{index}")).bind(package).execute(&mut *seed).await.unwrap();
        }
        seed.commit().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks=lock_purchase_write_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let boundary=inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.unwrap();
        let catalogue=current_package_catalogue().unwrap();
        assert!(inspect_purchase_after(&mut tx,None,&catalogue,&company(),"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Disabled).await.unwrap().is_none());
        for (purpose,name) in [(CapabilityPurpose::NewWork,"new-work"),(CapabilityPurpose::SettleExisting,"settle-existing"),(CapabilityPurpose::History,"history")] {
            for capability in ["legal.contracts","unknown.capability"] {
                let script="import {purchasedCapabilityPredicate as p} from '@erpx/server-commercial/adapters/purchased-capability-sql';process.stdout.write(p('s.company_id',process.argv[1],process.argv[2],'s.now_ms'))";
                let node=std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
                let out=std::process::Command::new(node).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
                    .args(["--conditions=react-server","--import","tsx","--input-type=module","--eval",script,capability,name]).output().unwrap();
                assert!(out.status.success(),"actual TS predicate unavailable");
                let predicate=String::from_utf8(out.stdout).unwrap();
                for mode in [PackageEnforcementMode::Shadow,PackageEnforcementMode::Enforced] {
                    let observed=inspect_purchase_after(&mut tx,Some(&boundary),&catalogue,&company(),capability,purpose,mode).await.unwrap().unwrap();
                    let sql=format!("WITH s AS MATERIALIZED (SELECT $1::text company_id,$2::bigint now_ms) SELECT ({predicate}) FROM s");
                    let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(company().as_str()).bind(observed.sampled_at).fetch_one(&mut *tx).await.unwrap();
                    assert_eq!(observed.allowed,actual,"{name} {capability}");
                    if mode==PackageEnforcementMode::Shadow && capability=="legal.contracts" {
                        let grants:serde_json::Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version)),'[]'::jsonb) FROM public.company_package_grants WHERE company_id='company'").fetch_one(&mut *tx).await.unwrap();
                        let history:serde_json::Value=sqlx::query_scalar("SELECT COALESCE(jsonb_agg(jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'sourceGrantVersion',source_grant_version,'sourceEventId',source_event_id,'retainedAt',retained_at)),'[]'::jsonb) FROM public.company_package_history_rights WHERE company_id='company'").fetch_one(&mut *tx).await.unwrap();
                        assert!(history.as_array().is_some_and(|rows| !rows.is_empty()),"real retained history required");
                        assert!(grants.as_array().is_some_and(|rows| rows.iter().any(|g| g["packageId"]=="é") && rows.iter().any(|g| g["packageId"]=="e\u{301}") && rows.iter().any(|g| g["startsAt"]==9_007_199_254_740_993_i64)));
                        let script=r#"import {purchasedCapability as p} from '@erpx/server-commercial/domain/packages';import {capabilityRevisionSource as r} from '@erpx/server-commercial/domain/capability-decision';import {createHash} from 'node:crypto';const grants=JSON.parse(process.argv[1]),history=JSON.parse(process.argv[2]),now=Number(process.argv[3]),purpose=process.argv[4];process.stdout.write(JSON.stringify({collationEqual:JSON.stringify(['é',1,1,'active',0,3000,'expired']).localeCompare(JSON.stringify(['e\u0301',1,1,'active',0,3000,'expired']))===0,code:p(grants,'company','legal.contracts',purpose,now,history).code,purchaseRevision:createHash('sha256').update(r('company',grants,now,history)).digest('hex')}));"#;
                        let node=std::env::var("ERPX_NODE_BIN").expect("pinned Node required");let oracle=std::process::Command::new(node).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","--import","tsx","--input-type=module","--eval",script,&grants.to_string(),&history.to_string(),&observed.sampled_at.to_string(),name]).output().unwrap();assert!(oracle.status.success(),"actual TS checked decision unavailable");let expected:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();assert_eq!(expected["collationEqual"],true);let actual=serde_json::to_value(observed.shadow.as_ref().expect("checked shadow observation")).unwrap();assert_eq!(actual["phase"],"checked");assert_eq!(actual["code"],expected["code"]);assert_eq!(actual["purchaseRevision"],expected["purchaseRevision"]);
                    }
                    match (mode,actual) { (PackageEnforcementMode::Enforced,false)=>assert_eq!(observed.require_allowed().unwrap_err().code,"PACKAGE_CAPABILITY_CHANGED"), _=>assert!(observed.require_allowed().is_ok()) }
                }
            }
        }
        // Same unit, same accepted capability despite a different unrelated fact.
        sqlx::query("UPDATE public.company_package_grants SET ends_at=NULL WHERE company_id='company' AND package_id='legal'").execute(&mut *tx).await.unwrap();
        let allowed=inspect_purchase_after(&mut tx,Some(&boundary),&catalogue,&company(),"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced).await.unwrap().unwrap();
        assert!(allowed.allowed);allowed.require_allowed().unwrap();
        sqlx::query("UPDATE public.company_package_grants SET reason='unrelated changed reason' WHERE company_id='company' AND package_id='legal'").execute(&mut *tx).await.unwrap();
        let still_allowed=inspect_purchase_after(&mut tx,Some(&boundary),&catalogue,&company(),"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced).await.unwrap().unwrap();
        assert!(still_allowed.allowed);still_allowed.require_allowed().unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn writer_shadow_read_matches_actual_typescript_observation() {
    use erpx_server_commercial::public::canonical_purchase_write::inspect_canonical_purchase_write_boundary;
    use erpx_server_commercial::public::purchase_shadow_read::{inspect_purchase_shadow_read, write_purchase_shadow_read_observation};
    use erpx_server_commercial::public::purchase_pre_command::inspect_purchase_pre_command;
    struct CaseClock(i64);
    impl Clock for CaseClock {fn now(&self)->TimestampMillis{now(self.0)}}
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let database: String = sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let node = std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
        let script = r#"
import { PostgresDatabase } from '@erpx/server-runtime/adapters/runtime/postgres';
import { readCompanyPurchaseFacts } from '@erpx/server-commercial/adapters/package-grants';
import { purchasedCapability } from '@erpx/server-commercial/domain/packages';
import { capabilityRevisionSource } from '@erpx/server-commercial/domain/capability-decision';
import { createHash } from 'node:crypto';
const url=new URL(process.env.ERPX_TEST_ADMIN_URL); url.pathname='/'+process.argv[1];
const db=new PostgresDatabase({connectionString:url.toString()});
try { const facts=await readCompanyPurchaseFacts(db,'company');
const cases=[['legal.contracts','new-work',1000],['legal.contracts','new-work',3000],['legal.contracts','history',3000],['unknown.capability','settle-existing',1000]];
process.stdout.write(JSON.stringify(cases.map(([capability,purpose,now])=>({event:'commercial.purchase.shadow',phase:'read',companyId:'company',capability,purpose,code:purchasedCapability(facts.grants,'company',capability,purpose,now,facts.history).code,purchaseRevision:createHash('sha256').update(capabilityRevisionSource('company',facts.grants,now,facts.history)).digest('hex')}))));
} finally { await db.close(); }
"#;
        let output=std::process::Command::new(node).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server","--import","tsx","--input-type=module","--eval",script,&database]).output().expect("Node oracle unavailable");
        assert!(output.status.success(), "actual TypeScript oracle failed");
        let expected: Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks=lock_purchase_write_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let boundary=inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.unwrap();
        let catalogue=current_package_catalogue().unwrap();
        for (index,(capability,purpose,time)) in [("legal.contracts",CapabilityPurpose::NewWork,1000),("legal.contracts",CapabilityPurpose::NewWork,3000),("legal.contracts",CapabilityPurpose::History,3000),("unknown.capability",CapabilityPurpose::SettleExisting,1000)].into_iter().enumerate() {
            let observation=inspect_purchase_shadow_read(&mut tx,&boundary,&holding(),&company(),&catalogue,capability,purpose,PackageEnforcementMode::Shadow,now(time)).await.unwrap().unwrap();
            assert_eq!(serde_json::to_value(&observation).unwrap(),expected[index]);
            let pre=inspect_purchase_pre_command(&mut tx,Some(&boundary),&holding(),&company(),&catalogue,capability,purpose,PackageEnforcementMode::Shadow,&CaseClock(time)).await.unwrap().unwrap();
            assert_eq!(serde_json::to_value(pre.shadow.as_ref().unwrap()).unwrap(),expected[index]);assert!(pre.require_allowed().is_ok());
            let enforced=inspect_purchase_pre_command(&mut tx,Some(&boundary),&holding(),&company(),&catalogue,capability,purpose,PackageEnforcementMode::Enforced,&CaseClock(time)).await.unwrap().unwrap();
            assert!(enforced.shadow.is_none());assert_eq!(enforced.decision.code.as_code(),expected[index]["code"].as_str().unwrap());
            if enforced.decision.allowed {assert!(enforced.require_allowed().is_ok());} else {let err=enforced.require_allowed().unwrap_err();assert_eq!(err.kind,FailureKind::Forbidden);assert_eq!(err.code,expected[index]["code"].as_str().unwrap());assert_eq!(err.message,"This package does not permit the requested work.");}
            let mut buffer=Vec::new();write_purchase_shadow_read_observation(&mut buffer,&observation);
            assert_eq!(buffer.last(),Some(&b'\n'));
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&buffer).unwrap(),expected[index]);
            struct BrokenSink;
            impl std::io::Write for BrokenSink {
                fn write(&mut self, _: &[u8]) -> std::io::Result<usize> { Err(std::io::Error::other("sink unavailable")) }
                fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
            }
            write_purchase_shadow_read_observation(&mut BrokenSink,&observation);
        }
        assert!(inspect_purchase_pre_command(&mut tx,None,&holding(),&company(),&catalogue,"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Disabled,&CaseClock(0)).await.unwrap().is_none());
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        assert!(inspect_purchase_pre_command(&mut other,Some(&boundary),&holding(),&company(),&catalogue,"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Enforced,&CaseClock(1000)).await.is_err());
        assert!(inspect_purchase_shadow_read(&mut other,&boundary,&holding(),&company(),&catalogue,"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Shadow,now(1000)).await.is_err());
        other.rollback().await.unwrap();
        for mode in [PackageEnforcementMode::Disabled,PackageEnforcementMode::Enforced] {
            assert!(inspect_purchase_shadow_read(&mut tx,&boundary,&holding(),&company(),&catalogue,"legal.contracts",CapabilityPurpose::NewWork,mode,now(1000)).await.unwrap().is_none());
        }
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn writer_shadow_read_matches_typescript_bigint_number_boundaries() {
    use erpx_server_commercial::public::canonical_purchase_write::inspect_canonical_purchase_write_boundary;
    use erpx_server_commercial::public::purchase_shadow_read::inspect_purchase_shadow_read;
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        // Each case is a legal persisted grant with the normal deferred audit
        // guard intact. JS JSON.parse rounds the unsafe bigint values.
        let cases = [
            ("unsafe-start", 9_007_199_254_740_993_i64, None, 9_007_199_254_740_992_i64),
            ("unsafe-end", 0, Some(9_007_199_254_740_993), 9_007_199_254_740_992),
            ("max-start", i64::MAX - 1, None, i64::MAX),
            ("max-end", 0, Some(i64::MAX), i64::MAX - 1),
        ];
        let mut seed=pool.raw().begin().await.unwrap();
        for (id,start,end,_) in cases {
            sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES($1,'legal',1,'active',$2,$3,1,'Synthetic approval','synthetic evidence',1000)")
                .bind(id).bind(start).bind(end).execute(&mut *seed).await.unwrap();
            audit(&mut seed,id,None).await;
        }
        seed.commit().await.unwrap();
        let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let node=std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
        let script=r#"
import {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {readCompanyPurchaseFacts} from '@erpx/server-commercial/adapters/package-grants';
import {purchasedCapability} from '@erpx/server-commercial/domain/packages';
import {capabilityRevisionSource} from '@erpx/server-commercial/domain/capability-decision';
import {createHash} from 'node:crypto';
const url=new URL(process.env.ERPX_TEST_ADMIN_URL); url.pathname='/'+process.argv[1];
const db=new PostgresDatabase({connectionString:url.toString()});
try { const out=[]; for (const [company,time] of [['unsafe-start','9007199254740992'],['unsafe-end','9007199254740992'],['max-start','9223372036854775807'],['max-end','9223372036854775806']]) {
const facts=await readCompanyPurchaseFacts(db,company), now=Number(time), capability='legal.contracts', purpose='new-work';
out.push({event:'commercial.purchase.shadow',phase:'read',companyId:company,capability,purpose,code:purchasedCapability(facts.grants,company,capability,purpose,now,facts.history).code,purchaseRevision:createHash('sha256').update(capabilityRevisionSource(company,facts.grants,now,facts.history)).digest('hex')});
} process.stdout.write(JSON.stringify(out)); } finally { await db.close(); }
"#;
        let output=std::process::Command::new(node).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(["--conditions=react-server","--import","tsx","--input-type=module","--eval",script,&database]).output().expect("Node oracle unavailable");
        assert!(output.status.success(),"actual TypeScript bigint oracle failed");
        let expected:Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks=lock_purchase_write_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let boundary=inspect_canonical_purchase_write_boundary(&mut tx,&locks).await.unwrap();
        let catalogue=current_package_catalogue().unwrap();
        for (index,(id,_,_,time)) in cases.into_iter().enumerate() {
            let observed=inspect_purchase_shadow_read(&mut tx,&boundary,&holding(),&CompanyId::new(id).unwrap(),&catalogue,"legal.contracts",CapabilityPurpose::NewWork,PackageEnforcementMode::Shadow,now(time)).await.unwrap().unwrap();
            assert_eq!(serde_json::to_value(observed).unwrap(),expected[index],"{id}");
        }
        // The rounding cases exercise opposite start and expiry outcomes.
        assert_eq!(expected[0]["code"],"GRANTED");
        assert_eq!(expected[1]["code"],"PACKAGE_DRAINING");
        assert_eq!(expected[2]["code"],"GRANTED");
        assert_eq!(expected[3]["code"],"PACKAGE_DRAINING");
        tx.rollback().await.unwrap();
    }).await;
}
