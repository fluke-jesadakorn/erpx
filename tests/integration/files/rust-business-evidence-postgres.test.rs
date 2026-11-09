//! Shared Records business-evidence port through the Files-owned adapter:
//! exact Company/scope/target/version/context, scanned links, required counts,
//! binding writes, audit event and rollback.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, evidence_context, policy_fact,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
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

fn policies() -> Vec<erpx_server_records::public::business_evidence::EvidencePolicyFact> {
    vec![policy_fact(
        "commerce_order_delivery",
        "proof_of_delivery",
        &["action"],
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
            Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) => {
                assert_eq!(error.code, "ATTACHMENT_BINDING_CHANGED")
            }
            _ => panic!("expected insufficient-link conflict"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}
