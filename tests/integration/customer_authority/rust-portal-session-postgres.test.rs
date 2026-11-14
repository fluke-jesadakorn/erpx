//! PostgreSQL enrollment read for the verified external portal customer proof.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::portal_session::{
    PgPortalEnrollment, PortalCustomerProof, PortalCustomerRequest, PortalEnrollmentSource,
    PortalSessionEvidence,
};
use erpx_server_runtime::public::postgres::{
    load_migrations, migrate_database, DatabasePool, MigrationTrack, PgTransactionError,
    PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;

const NOW: i64 = 1_800_000_000_000;

fn session(user: &str, email: &str) -> PortalSessionEvidence {
    PortalSessionEvidence {
        session_id: format!("session-{user}"),
        user_id: user.into(),
        email: email.into(),
        email_verified: true,
        authenticated_at: NOW - 1_000,
        expires_at: NOW + 60_000,
    }
}

fn request(slug: &str) -> PortalCustomerRequest<'_> {
    PortalCustomerRequest {
        token: "opaque-session-token",
        audience: Some("portal"),
        environment: "development",
        holding_id: "h001",
        holding_slug: slug,
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES
             ('company','company','company','Company',NULL,'company',NULL,'active',1);
           INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES
             ('customer-main','company','customer','C-001','Synthetic Customer','THB','active',1,10,10);
           INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES
             ('order-main','company',NULL,NULL,'quote','QT-100','customer-main',NULL,'THB','2026-09-20','2026-09-25','Main','confirmed',100,0,3,'requester',100,100);
           INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES
             ('binding-valid','company','order-main','customer-main','valid@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-other-holding','company','order-main','customer-main','wrong-holding@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-expired','company','order-main','customer-main','expired@example.test',101,'approved','Expired portal access','requester','reviewer',100,2,100,100),
             ('binding-revoked','company','order-main','customer-main','revoked@example.test',1800000060000,'revoked','Revoked portal access','requester','reviewer',100,2,100,100),
             ('binding-unenrolled','company','order-main','customer-main','unenrolled@example.test',1800000060000,'approved','No accepted invitation','requester','reviewer',100,2,100,100);
           INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES
             ('binding-valid','customer-valid','valid@example.test',100),
             ('binding-other-holding','customer-wrong-holding','wrong-holding@example.test',100),
             ('binding-expired','customer-expired','expired@example.test',100),
             ('binding-revoked','customer-revoked','revoked@example.test',100),
             ('binding-unenrolled','customer-unenrolled','unenrolled@example.test',100);
           INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES
             ('invite-valid','customer.portal.invite','requester',repeat('a',64),'binding-valid','{}',100),
             ('invite-other','customer.portal.invite','requester',repeat('b',64),'binding-other-holding','{}',100),
             ('invite-expired','customer.portal.invite','requester',repeat('c',64),'binding-expired','{}',100),
             ('invite-revoked','customer.portal.invite','requester',repeat('d',64),'binding-revoked','{}',100);
           INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,created_by,created_at,accepted_at,accepted_user_id,command_id) VALUES
             ('invitation-valid','binding-valid','acme',1,'valid@example.test',repeat('1',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-valid','invite-valid'),
             ('invitation-other','binding-other-holding','other-tenant',1,'wrong-holding@example.test',repeat('2',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-wrong-holding','invite-other'),
             ('invitation-expired','binding-expired','acme',1,'expired@example.test',repeat('3',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-expired','invite-expired'),
             ('invitation-revoked','binding-revoked','acme',1,'revoked@example.test',repeat('4',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-revoked','invite-revoked');"#,
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected a business failure, got {other:?}"),
    }
}

async fn enrolled(
    pool: &DatabasePool,
    session: PortalSessionEvidence,
    request: PortalCustomerRequest<'static>,
) -> Option<PortalCustomerProof> {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let session = session.clone();
            let request = request;
            Box::pin(async move {
                let mut source = PgPortalEnrollment::new(connection);
                source.enrolled_customer(&session, &request, NOW).await
            })
        })
        .await
        .map_err(business)
        .unwrap()
}

#[tokio::test]
async fn resolves_only_the_accepted_invitation_in_the_exact_holding() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let proof = enrolled(
            &pool,
            session("customer-valid", "Valid@Example.Test"),
            request("acme"),
        )
        .await
        .expect("accepted identity and matching holding slug resolve");
        assert_eq!(proof.user_id, "customer-valid");
        assert_eq!(proof.email, "valid@example.test");
        assert_eq!(proof.holding_id.as_deref(), Some("h001"));

        for (user, email) in [
            ("customer-wrong-holding", "wrong-holding@example.test"),
            ("customer-expired", "expired@example.test"),
            ("customer-revoked", "revoked@example.test"),
            ("customer-unenrolled", "unenrolled@example.test"),
        ] {
            assert!(
                enrolled(&pool, session(user, email), request("acme"))
                    .await
                    .is_none(),
                "{user} must not resolve a customer proof"
            );
        }

        assert!(
            enrolled(
                &pool,
                session("customer-valid", "valid@example.test"),
                request("other-tenant")
            )
            .await
            .is_none(),
            "the expected Holding slug must match the accepted invitation"
        );
        assert!(
            enrolled(
                &pool,
                session("customer-valid", "other@example.test"),
                request("acme")
            )
            .await
            .is_none(),
            "the session email must equal the enrolled identity email"
        );
    })
    .await;
}
