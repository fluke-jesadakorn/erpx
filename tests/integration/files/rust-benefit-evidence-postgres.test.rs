//! Files-owned Benefits evidence port against PostgreSQL attachment sets.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_benefits::public::command::{
    BenefitEvidenceRequest, BenefitRecordKind, BenefitsEvidencePort, benefit_evidence_binding_type,
    benefit_evidence_policy,
};
use erpx_server_files::public::benefit_evidence::PgBenefitsEvidence;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn request(
    kind: BenefitRecordKind,
    action: &str,
    target_id: &str,
    set_id: &str,
) -> BenefitEvidenceRequest {
    let policy = benefit_evidence_policy(kind, action).unwrap();
    BenefitEvidenceRequest {
        attachment_set_ids: vec![set_id.into()],
        target_type: policy.target_type.into(),
        purpose: policy.purpose.into(),
        component_ref: policy.component_ref.into(),
        target_id: target_id.into(),
        expected_target_version: 1,
        command_id: format!("benefits-command-{set_id}"),
        binding_type: benefit_evidence_binding_type(kind, action),
        now: NOW,
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
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) \
         VALUES('company','company','company','Company',NULL,'company',NULL,'active',1);\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) \
         VALUES('upload-dependent','benefit_dependent','dependent-1','dependent_proof','dependent.pdf','application/pdf',10,'staging-dependent','ready','fixture',4102444800000,0,0),\
         ('upload-submit','benefit_election','election-1','election_document','submit.pdf','application/pdf',10,'staging-submit','ready','fixture',4102444800000,0,0),\
         ('upload-acknowledge','benefit_election','election-1','provider_proof','acknowledge.pdf','application/pdf',10,'staging-acknowledge','ready','fixture',4102444800000,0,0),\
         ('upload-pending','benefit_election','election-1','election_document','pending.pdf','application/pdf',10,'staging-pending','ready','fixture',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) \
         VALUES('attachment-dependent','upload-dependent','benefit_dependent','dependent-1','dependent_proof','original-dependent',1,'dependent.pdf','application/pdf',10,repeat('a',64),'fixture',0,'company','application/pdf','available',1,1),\
         ('attachment-submit','upload-submit','benefit_election','election-1','election_document','original-submit',1,'submit.pdf','application/pdf',10,repeat('b',64),'fixture',0,'company','application/pdf','available',1,1),\
         ('attachment-acknowledge','upload-acknowledge','benefit_election','election-1','provider_proof','original-acknowledge',1,'acknowledge.pdf','application/pdf',10,repeat('c',64),'fixture',0,'company','application/pdf','available',1,1),\
         ('attachment-pending','upload-pending','benefit_election','election-1','election_document','original-pending',1,'pending.pdf','application/pdf',10,repeat('d',64),'fixture',0,'company','application/pdf','quarantined',NULL,NULL);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) \
         VALUES('set-dependent','company',NULL,NULL,'benefit_dependent','dependent-1','action','verification','dependent_proof',1,1,'restricted','[]',0,10,'draft','reviewer',0,0),\
         ('set-submit','company',NULL,NULL,'benefit_election','election-1','action','employee-submit','election_document',1,1,'restricted','[]',0,10,'draft','employee',0,0),\
         ('set-acknowledge','company',NULL,NULL,'benefit_election','election-1','action','provider-acknowledgement','provider_proof',1,1,'restricted','[]',0,10,'draft','reviewer',0,0),\
         ('set-pending','company',NULL,NULL,'benefit_election','election-1','action','employee-submit','election_document',1,1,'restricted','[]',0,10,'draft','pending-fixture',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) \
         VALUES('link-dependent','set-dependent','attachment-dependent','draft','reviewer',0,4102444800000),\
         ('link-submit','set-submit','attachment-submit','draft','employee',0,4102444800000),\
         ('link-acknowledge','set-acknowledge','attachment-acknowledge','draft','reviewer',0,4102444800000),\
         ('link-pending','set-pending','attachment-pending','draft','employee',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn serial(connection: &mut PgConnection) {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(connection)
        .await
        .unwrap();
}

fn benefits_port() -> PgBenefitsEvidence {
    PgBenefitsEvidence::new(
        company(),
        "reviewer".into(),
        None,
        None,
        TimestampMillis::new(NOW),
    )
}

#[tokio::test]
async fn binds_dependent_and_election_evidence_and_rechecks_the_current_binding() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        for (kind, action, target, set, link, binding) in [
            (
                BenefitRecordKind::Dependent,
                "verify",
                "dependent-1",
                "set-dependent",
                "link-dependent",
                "benefits_dependent_verify",
            ),
            (
                BenefitRecordKind::Election,
                "submit",
                "election-1",
                "set-submit",
                "link-submit",
                "benefits_election_submit",
            ),
            (
                BenefitRecordKind::Election,
                "acknowledge",
                "election-1",
                "set-acknowledge",
                "link-acknowledge",
                "benefits_election_acknowledge",
            ),
        ] {
            let mut tx = pool.raw().begin().await.unwrap();
            serial(&mut tx).await;
            let mut port = benefits_port();
            let prepared = port
                .prepare(Some(&mut tx), request(kind, action, target, set))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(prepared.set_ids, [set]);
            assert_eq!(prepared.link_ids, [link]);
            assert_eq!(
                prepared.target_type,
                request(kind, action, target, set).target_type
            );
            assert_eq!(prepared.target_id, target);
            assert_eq!(prepared.policy_version, 1);
            assert_eq!(prepared.binding_type, binding);
            assert_eq!(prepared.binding_id, target);
            let state: (String, String, Option<String>, Option<String>) = sqlx::query_as(
                "SELECT s.state,l.state,l.binding_type,l.binding_id FROM attachment_sets s \
                 JOIN attachment_links l ON l.set_id=s.id WHERE s.id=$1",
            )
            .bind(set)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(
                state,
                (
                    "bound".into(),
                    "bound".into(),
                    Some(binding.into()),
                    Some(target.into())
                )
            );
            port.require_current(Some(&mut tx), &prepared)
                .await
                .unwrap();
            tx.rollback().await.unwrap();
        }
        let drafts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM attachment_sets WHERE state='draft' \
             AND id IN ('set-dependent','set-submit','set-acknowledge')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(drafts, 3);
    })
    .await;
}

#[tokio::test]
async fn unregistered_policy_unscanned_sets_and_absent_units_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;

        let mut tx = pool.raw().begin().await.unwrap();
        serial(&mut tx).await;
        let mut port = benefits_port();
        let mut unsupported = request(
            BenefitRecordKind::Dependent,
            "verify",
            "dependent-1",
            "set-dependent",
        );
        unsupported.target_type = "benefit_plan".into();
        let error = port.prepare(Some(&mut tx), unsupported).await.unwrap_err();
        assert_eq!(error.code, "BENEFITS_EVIDENCE_INVALID");
        assert_eq!(error.kind, FailureKind::InvalidInput);

        let pending = port
            .prepare(
                Some(&mut tx),
                request(
                    BenefitRecordKind::Election,
                    "submit",
                    "election-1",
                    "set-pending",
                ),
            )
            .await
            .unwrap_err();
        assert_eq!(pending.code, "ATTACHMENT_SCAN_PENDING");
        tx.rollback().await.unwrap();

        let mut port = benefits_port();
        let error = port
            .prepare(
                None,
                request(
                    BenefitRecordKind::Dependent,
                    "verify",
                    "dependent-1",
                    "set-dependent",
                ),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "BENEFITS_EVIDENCE_UNIT_REQUIRED");
        assert_eq!(error.kind, FailureKind::Unavailable);

        let mut port = benefits_port();
        let prepared = {
            let mut tx = pool.raw().begin().await.unwrap();
            serial(&mut tx).await;
            let prepared = port
                .prepare(
                    Some(&mut tx),
                    request(
                        BenefitRecordKind::Election,
                        "submit",
                        "election-1",
                        "set-submit",
                    ),
                )
                .await
                .unwrap()
                .unwrap();
            tx.rollback().await.unwrap();
            prepared
        };
        let error = port.require_current(None, &prepared).await.unwrap_err();
        assert_eq!(error.code, "BENEFITS_EVIDENCE_UNIT_REQUIRED");
    })
    .await;
}
