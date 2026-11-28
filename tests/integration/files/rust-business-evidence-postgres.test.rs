//! Shared Records business-evidence port through the Files-owned adapter:
//! exact Company/scope/target/version/context, scanned links, required counts,
//! binding writes, audit event and rollback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_files_server::public::business_evidence::{
    PgBusinessEvidenceFiles, PreparedBusinessEvidence, evidence_context, policy_fact,
};
use erpx_records_server::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[tokio::test]
async fn legal_citation_metadata_matches_legacy_sql_and_retains_absence() {
    use erpx_files_server::public::legal_citation_metadata::{LegalCitationMetadataRequest, inspect_legal_citation_metadata, lock_legal_citation_metadata};
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE attachment_links SET state='bound',bound_at=1,binding_type='legal_document',binding_id='doc' WHERE id='link-1'").execute(pool.raw()).await.unwrap();
        let script = r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {finalRootOwnerProjectionSources}=await import('./modules/files/server/src/adapters/final-root-owner-projection-sources.ts');const text=await Bun.file('modules/legal/server/src/adapters/legal/knowledge.ts').text();const match=text.match(/export const legalEvidenceDocumentAvailableSql = (`[^`]*`);/);if(!match)throw Error('LEGAL_PREDICATE_NOT_FOUND');process.stdout.write(new Function('filesProjectionSources',`return ${match[1]}`)(finalRootOwnerProjectionSources));"#;
        let result=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .arg("--conditions=react-server").arg("-e").arg(script).output().unwrap();
        assert!(result.status.success(),"actual Legal TS predicate unavailable");
        let predicate=String::from_utf8(result.stdout).unwrap();
        let sql=format!("SELECT ({predicate}) FROM (SELECT $1::text object_key,$2::text attachment_link_id,$3::text checksum,'missing-document'::text id) d CROSS JOIN (SELECT $4::text company_id,'missing-item'::text id) i");
        let mut tx=pool.raw().begin().await.unwrap(); serial(&mut tx).await;
        let input=|object:Option<&str>,link:Option<&str>,checksum:&str,company:&str| LegalCitationMetadataRequest {company:CompanyId::new(company).unwrap(),object_key:object.map(str::to_owned),attachment_link_id:link.map(str::to_owned),checksum:checksum.to_owned()};
        assert!(inspect_legal_citation_metadata(&mut tx,input(Some("object"),Some("link-1"),&"a".repeat(64),"company")).await.is_err());
        lock_legal_citation_metadata(&mut tx).await.unwrap();
        for (object,link,checksum,company,expected) in [(None,None,"a", "company",true),(Some("object"),Some("link-1"),"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","company",true),(Some("different-object"),Some("link-1"),"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","company",true),(Some("object"),Some("link-1"),"wrong","company",false),(Some("object"),Some("link-1"),"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","foreign",false),(Some("object"),Some("missing"),"a","company",false),(Some("object"),Some("link-2"),"b","company",false)] {
            let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(object).bind(link).bind(checksum).bind(company).fetch_one(&mut *tx).await.unwrap();
            let proof=inspect_legal_citation_metadata(&mut tx,input(object,link,checksum,company)).await.unwrap();
            assert_eq!((proof.available(),actual),(expected,expected));proof.require_current(&mut tx).await.unwrap();
        }
        let proof=inspect_legal_citation_metadata(&mut tx,input(Some("object"),Some("link-1"),&"a".repeat(64),"company")).await.unwrap();
        let draft=inspect_legal_citation_metadata(&mut tx,input(Some("object"),Some("link-2"),&"b".repeat(64),"company")).await.unwrap();
        sqlx::query("SAVEPOINT unrelated_caption").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachment_links SET caption='display only' WHERE id='link-2'").execute(&mut *tx).await.unwrap();
        draft.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT unrelated_caption").execute(&mut *tx).await.unwrap();
        let missing=inspect_legal_citation_metadata(&mut tx,input(Some("object"),Some("phantom-link"),&"a".repeat(64),"company")).await.unwrap();assert!(!missing.available());
        sqlx::query("SAVEPOINT phantom").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('phantom-link','set-2','attachment-1','bound','fixture',0,4102444800000,1,'legal_document','doc')").execute(&mut *tx).await.unwrap();
        assert!(missing.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT phantom").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT files_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE attachments SET revoked_at=1 WHERE id='attachment-1'").execute(&mut *tx).await.unwrap();
        assert!(proof.require_current(&mut tx).await.is_err());
        let revoked=inspect_legal_citation_metadata(&mut tx,input(Some("object"),Some("link-1"),&"a".repeat(64),"company")).await.unwrap();assert!(!revoked.available());
        let oracle:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(Some("object")).bind(Some("link-1")).bind("a".repeat(64)).bind("company").fetch_one(&mut *tx).await.unwrap();assert!(!oracle);
        sqlx::query("ROLLBACK TO SAVEPOINT files_drift").execute(&mut *tx).await.unwrap();proof.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();serial(&mut other).await;lock_legal_citation_metadata(&mut other).await.unwrap();assert!(proof.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut lost=pool.raw().begin().await.unwrap();serial(&mut lost).await;sqlx::query("SAVEPOINT before_files_lock").execute(&mut *lost).await.unwrap();lock_legal_citation_metadata(&mut lost).await.unwrap();let source=inspect_legal_citation_metadata(&mut lost,input(None,None,"a","company")).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_files_lock").execute(&mut *lost).await.unwrap();assert!(source.require_current(&mut lost).await.is_err());lost.rollback().await.unwrap();
        let mut owner=pool.raw().begin().await.unwrap();serial(&mut owner).await;lock_legal_citation_metadata(&mut owner).await.unwrap();
        let owner_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
        let other_pool=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let worker=tokio::spawn(async move {let mut c=other_pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();sqlx::query("UPDATE attachments SET revoked_at=1 WHERE id='attachment-1'").execute(&mut *c).await.unwrap();});
        let pid=receive.await.unwrap();let mut blocked=false;
        for _ in 0..200 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(owner_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked { break; }tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
        assert!(blocked,"Files writer must wait for metadata fence");owner.rollback().await.unwrap();worker.await.unwrap();
    }).await;
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at)          VALUES('upload-1','holding','company','delivery','proof.png','image/png',10,'staging-1','ready','fixture',4102444800000,0,0),         ('upload-2','holding','company','delivery','proof-2.png','image/png',10,'staging-2','ready','fixture',4102444800000,0,0);         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-1','upload-1','holding','company','delivery','original-1',1,'proof.png','image/png',10,repeat('a',64),'fixture',0,'company','image/png','available',1,1),\
         ('attachment-2','upload-2','holding','company','delivery','original-2',1,'proof-2.png','image/png',10,repeat('b',64),'fixture',0,'company','image/png','available',1,1);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-1','company',NULL,NULL,'commerce_order_delivery','delivery-1','action','delivery','proof_of_delivery',1,1,'internal','[]',1,2,'draft','fixture',0,0),\
         ('set-2','company',NULL,NULL,'commerce_order_delivery','delivery-1','action','delivery-extra','proof_of_delivery',1,1,'internal','[]',1,2,'draft','fixture',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-1','set-1','attachment-1','draft','fixture',0,4102444800000),\
         ('link-2','set-2','attachment-2','draft','fixture',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(c: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(c)
        .await
        .unwrap();
}

fn request(target: &str, version: i64, sets: &[&str]) -> BusinessEvidenceRequest {
    BusinessEvidenceRequest {
        attachment_set_ids: sets.iter().map(|value| (*value).to_string()).collect(),
        target_types: vec!["commerce_order_delivery".into()],
        contexts: vec![evidence_context("proof_of_delivery", "action", None)],
        target_id: target.into(),
        expected_target_version: version,
        command_id: "commerce-delivery-0001".into(),
        binding_type: "commerce_fulfillment".into(),
        binding_id: "delivery-1".into(),
    }
}

fn policies() -> Vec<erpx_records_server::public::business_evidence::EvidencePolicyFact> {
    vec![policy_fact(
        "commerce_order_delivery",
        "proof_of_delivery",
        &["action"],
        1,
    )]
}

#[tokio::test]
async fn binds_exact_scanned_sets_and_retains_bound_state() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = PgBusinessEvidenceFiles::new(
            &mut tx,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        let prepared =
            prepare_business_evidence(&mut port, request("delivery-1", 1, &["set-1", "set-2"]))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(prepared.set_ids, ["set-1", "set-2"]);
        assert_eq!(prepared.link_ids, ["link-1", "link-2"]);
        assert_eq!(prepared.binding_id, "delivery-1");
        let sets: Vec<(String, String)> =
            sqlx::query_as("SELECT state,bound_command_id FROM attachment_sets ORDER BY id")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            sets,
            [
                ("bound".into(), "commerce-delivery-0001".into()),
                ("bound".into(), "commerce-delivery-0001".into())
            ]
        );
        let links: Vec<(String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT state,binding_type,binding_id,attachment_id FROM attachment_links ORDER BY id",
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(
            links,
            [
                (
                    "bound".into(),
                    Some("commerce_fulfillment".into()),
                    Some("delivery-1".into()),
                    Some("attachment-1".into())
                ),
                (
                    "bound".into(),
                    Some("commerce_fulfillment".into()),
                    Some("delivery-1".into()),
                    Some("attachment-2".into())
                )
            ]
        );
        let events: i64 =
            sqlx::query_scalar("SELECT count(*) FROM attachment_events WHERE action='set.bound'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(events, 2);
        tx.rollback().await.unwrap();
        let state: Vec<(String, String)> = sqlx::query_as(
            "SELECT state,coalesce(bound_command_id,'') FROM attachment_sets ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            state,
            [
                ("draft".into(), String::new()),
                ("draft".into(), String::new())
            ]
        );
    })
    .await;
}

#[tokio::test]
async fn rejects_foreign_scope_wrong_target_missing_context_and_short_sets() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        sqlx::query("UPDATE attachment_sets SET required_count=2 WHERE id='set-1'")
            .execute(pool.raw())
            .await
            .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut foreign = PgBusinessEvidenceFiles::new(
            &mut tx,
            CompanyId::new("foreign").unwrap(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        assert!(
            prepare_business_evidence(&mut foreign, request("delivery-1", 1, &["set-1"]))
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = PgBusinessEvidenceFiles::new(
            &mut tx,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        assert!(
            prepare_business_evidence(&mut port, request("delivery-other", 1, &["set-1"]))
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = PgBusinessEvidenceFiles::new(
            &mut tx,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        // set-1 now requires two links but has one.
        match prepare_business_evidence(&mut port, request("delivery-1", 1, &["set-1"])).await {
            Err(erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ATTACHMENT_REQUIRED")
            }
            _ => panic!("expected insufficient-link conflict"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

async fn assert_binding_changed(
    pool: &DatabasePool,
    prepared: &PreparedBusinessEvidence,
) {
    let mut tx = pool.raw().begin().await.unwrap();
    serial(&mut tx).await;
    let mut port = PgBusinessEvidenceFiles::new(
        &mut tx,
        company(),
        "fixture".into(),
        None,
        None,
        policies(),
        TimestampMillis::new(NOW),
    );
    match port.require_current(prepared).await {
        Err(PgTransactionError::Business(error)) => {
            assert_eq!(error.code, "ATTACHMENT_BINDING_CHANGED");
        }
        other => panic!("expected ATTACHMENT_BINDING_CHANGED, got {other:?}"),
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn require_current_rejects_binding_drift_and_keeps_unit_guards() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let prepared = {
            let mut tx = pool.raw().begin().await.unwrap();
            serial(&mut tx).await;
            let mut port = PgBusinessEvidenceFiles::new(
                &mut tx,
                company(),
                "fixture".into(),
                None,
                None,
                policies(),
                TimestampMillis::new(NOW),
            );
            let prepared =
                prepare_business_evidence(&mut port, request("delivery-1", 1, &["set-1", "set-2"]))
                    .await
                    .unwrap()
                    .unwrap();
            port.require_current(&prepared).await.unwrap();
            tx.commit().await.unwrap();
            prepared
        };

        // The exact committed binding stays current in a fresh serializable unit.
        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = PgBusinessEvidenceFiles::new(
            &mut tx,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        port.require_current(&prepared).await.unwrap();
        tx.rollback().await.unwrap();

        // Bound sets and links are trigger-immutable, so the reachable drift is
        // an extra link added after the binding. Files rejects it.
        {
            let mut tx = pool.raw().begin().await.unwrap();
            serial(&mut tx).await;
            sqlx::query(
                "INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
                 VALUES('link-extra','set-1','attachment-2','draft','fixture',0,4102444800000)",
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            let mut port = PgBusinessEvidenceFiles::new(
                &mut tx,
                company(),
                "fixture".into(),
                None,
                None,
                policies(),
                TimestampMillis::new(NOW),
            );
            match port.require_current(&prepared).await {
                Err(PgTransactionError::Business(error)) => {
                    assert_eq!(error.code, "ATTACHMENT_BINDING_CHANGED");
                }
                other => panic!("expected ATTACHMENT_BINDING_CHANGED, got {other:?}"),
            }
            tx.rollback().await.unwrap();
        }

        // The retained witness must keep matching every re-read fact.
        for stale in [
            PreparedBusinessEvidence {
                command_id: "other-command".into(),
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                binding_type: "other_binding".into(),
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                binding_id: "delivery-other".into(),
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                target_type: "other_target".into(),
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                target_id: "other-delivery".into(),
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                expected_target_version: 2,
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                set_ids: vec!["set-1".into()],
                ..prepared.clone()
            },
            PreparedBusinessEvidence {
                link_ids: vec!["link-1".into()],
                ..prepared.clone()
            },
        ] {
            assert_binding_changed(&pool, &stale).await;
        }

        // A foreign Company cannot revalidate the retained binding.
        {
            let mut tx = pool.raw().begin().await.unwrap();
            serial(&mut tx).await;
            let mut foreign = PgBusinessEvidenceFiles::new(
                &mut tx,
                CompanyId::new("foreign").unwrap(),
                "fixture".into(),
                None,
                None,
                policies(),
                TimestampMillis::new(NOW),
            );
            match foreign.require_current(&prepared).await {
                Err(PgTransactionError::Business(error)) => {
                    assert_eq!(error.code, "ATTACHMENT_BINDING_CHANGED");
                }
                other => panic!("expected ATTACHMENT_BINDING_CHANGED, got {other:?}"),
            }
            tx.rollback().await.unwrap();
        }

        let mut connection = pool.raw().acquire().await.unwrap();
        let mut port = PgBusinessEvidenceFiles::new(
            &mut connection,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        match port.require_current(&prepared).await {
            Err(PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "FILES_TRANSACTION_REQUIRED")
            }
            other => panic!("expected FILES_TRANSACTION_REQUIRED, got {other:?}"),
        }
        drop(port);
        drop(connection);

        let mut tx = pool.raw().begin().await.unwrap();
        let mut port = PgBusinessEvidenceFiles::new(
            &mut tx,
            company(),
            "fixture".into(),
            None,
            None,
            policies(),
            TimestampMillis::new(NOW),
        );
        match port.require_current(&prepared).await {
            Err(PgTransactionError::Persistence(error)) => {
                assert_eq!(error.code, "FILES_SERIALIZABLE_REQUIRED")
            }
            other => panic!("expected FILES_SERIALIZABLE_REQUIRED, got {other:?}"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}
