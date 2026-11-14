//! Native Customer Portal invitation enrollment on migrated PostgreSQL.
//!
//! Proves the one-hour attempt window, exact reservation release, acceptance
//! identity binding with immutable evidence, replay and every source gate.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::portal_enrollment::{
    EnrollmentFuture, PgPortalEnrollmentCommand, PortalSession, PortalSessionPort, accept, release,
    reserve,
};
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    InvitationClaims, invitation_token, invitation_token_hash,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &str = "vector-secret-2026";
const EMAIL: &str = "customer@example.invalid";

struct FixedSession {
    proof: PortalSession,
    fail: bool,
}

impl PortalSessionPort for FixedSession {
    fn current<'a>(
        &'a mut self,
        expected: Option<&'a PortalSession>,
    ) -> EnrollmentFuture<'a, PortalSession, BusinessError> {
        Box::pin(async move {
            if self.fail {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unauthenticated,
                    "CUSTOMER_PORTAL_SESSION_REQUIRED",
                    "Sign in again to accept this invitation.",
                ));
            }
            if expected.is_some_and(|value| value != &self.proof) {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Unauthenticated,
                    "CUSTOMER_PORTAL_SESSION_REQUIRED",
                    "Sign in again to accept this invitation.",
                ));
            }
            Ok(self.proof.clone())
        })
    }
}

fn session() -> FixedSession {
    FixedSession {
        proof: PortalSession {
            session_id: "session-1".into(),
            user_id: "portal-user".into(),
            email: EMAIL.into(),
            email_verified: true,
            authenticated_at: NOW - 1_000,
            expires_at: NOW + 60_000,
        },
        fail: false,
    }
}

fn claims() -> InvitationClaims {
    InvitationClaims {
        invitation_id: "customer-portal-invitation-1".into(),
        binding_id: "binding-1".into(),
        holding_slug: "acme".into(),
        generation: 1,
        expires_at: NOW + 86_400_000,
        key_version: 1,
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool, email: &str, order_state: &str, binding_status: &str) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('customer-1','company','customer','C1','Portal customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,\
         warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,\
         version,created_by,approved_by,created_at,updated_at) \
         VALUES('order-1','company','branch',NULL,'sales','SO-1','customer-1','warehouse-1','THB',\
         '2026-11-01','2026-11-08','',$1,1000,0,NULL,3,'creator','reviewer',100,300)",
    )
    .bind(order_state)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,\
         status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) \
         VALUES('binding-1','company','order-1','customer-1',$1,9000000000000,$2,\
         'Declared approved contact','creator','reviewer',200,2,100,200)",
    )
    .bind(email)
    .bind(binding_status)
    .execute(pool.raw())
    .await
    .unwrap();
    let token = invitation_token(SECRET, &claims());
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) \
         VALUES('command-1','customer.access.invite','creator',$1,'binding-1','{}',100)",
    )
    .bind("0".repeat(64))
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,\
         token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,created_by,\
         created_at,command_id) VALUES('customer-portal-invitation-1','binding-1','acme',1,$1,$2,1,\
         $3,'sent',0,100,'creator',100,'command-1')",
    )
    .bind(email)
    .bind(invitation_token_hash(&token))
    .bind(NOW + 86_400_000)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn reserve_at(
    pool: &DatabasePool,
    token: &str,
    now: i64,
) -> Result<
    erpx_server_customer_authority::public::portal_enrollment::EnrollmentInvite,
    PgTransactionError<BusinessError>,
> {
    let token = token.to_owned();
    let claims = claims();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let token = token.clone();
            let claims = claims.clone();
            Box::pin(async move {
                let mut session = session();
                let mut command = PgPortalEnrollmentCommand::new(
                    connection,
                    &mut session,
                    erpx_server_commercial::public::purchases::PackageEnforcementMode::Disabled,
                    false,
                );
                reserve(&mut command, &claims, &token, now).await
            })
        })
        .await
}

async fn release_at(
    pool: &DatabasePool,
    reservation: &erpx_server_customer_authority::public::portal_enrollment::EnrollmentReservation,
) -> Result<(), PgTransactionError<BusinessError>> {
    let reservation = reservation.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let reservation = reservation.clone();
            Box::pin(async move {
                let mut session = session();
                let mut command = PgPortalEnrollmentCommand::new(
                    connection,
                    &mut session,
                    erpx_server_commercial::public::purchases::PackageEnforcementMode::Disabled,
                    false,
                );
                release(&mut command, &reservation).await
            })
        })
        .await
}

async fn accept_at(
    pool: &DatabasePool,
    token: &str,
    now: i64,
    session_fail: bool,
) -> Result<
    erpx_server_customer_authority::public::portal_enrollment::AcceptedInvitation,
    PgTransactionError<BusinessError>,
> {
    let token = token.to_owned();
    let claims = claims();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let token = token.clone();
            let claims = claims.clone();
            Box::pin(async move {
                let mut session = session();
                session.fail = session_fail;
                let mut command = PgPortalEnrollmentCommand::new(
                    connection,
                    &mut session,
                    erpx_server_commercial::public::purchases::PackageEnforcementMode::Disabled,
                    false,
                );
                accept(&mut command, &claims, &token, now).await
            })
        })
        .await
}

async fn attempts(pool: &DatabasePool) -> Option<(i32, i64)> {
    sqlx::query_as(
        "SELECT attempts,window_start FROM customer_portal_enrollment_attempts WHERE invitation_id='customer-portal-invitation-1'",
    )
    .fetch_optional(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn reserve_creates_and_resets_the_one_hour_window() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, EMAIL, "confirmed", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let mut session = session();
        let _ = &mut session;
        assert_eq!(reserve_at(&pool, &token, NOW).await.unwrap().attempts, 1);
        assert_eq!(attempts(&pool).await, Some((1, NOW)));
        assert_eq!(
            reserve_at(&pool, &token, NOW + 1).await.unwrap().attempts,
            2
        );
        assert_eq!(attempts(&pool).await, Some((2, NOW)));
        assert_eq!(
            reserve_at(&pool, &token, NOW + 3_600_001)
                .await
                .unwrap()
                .attempts,
            1
        );
        assert_eq!(attempts(&pool).await, Some((1, NOW + 3_600_001)));
    })
    .await;
}

#[tokio::test]
async fn release_removes_only_the_in_flight_reservation() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, EMAIL, "confirmed", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let mut session = session();
        let _ = &mut session;
        let invite = reserve_at(&pool, &token, NOW).await.unwrap();
        release_at(&pool, &invite.reservation).await.unwrap();
        assert_eq!(attempts(&pool).await, None);
        let mut stale = invite.reservation.clone();
        stale.window_start = NOW - 1;
        assert!(
            release_at(&pool, &stale).await.is_ok(),
            "a stale reservation is a no-op"
        );
    })
    .await;
}

#[tokio::test]
async fn accept_binds_the_identity_and_retains_immutable_evidence() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, EMAIL, "confirmed", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let mut session = session();
        let _ = &mut session;
        let accepted = accept_at(&pool, &token, NOW, false).await.unwrap();
        assert_eq!(accepted.binding_id, "binding-1");
        assert_eq!(accepted.holding_slug, "acme");
        assert!(accepted.accepted);
        let identity = sqlx::query("SELECT user_id,email FROM customer_access_identities WHERE binding_id='binding-1'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(identity.get::<String, _>("user_id"), "portal-user");
        assert_eq!(identity.get::<String, _>("email"), EMAIL);
        let invitation = sqlx::query("SELECT state,accepted_user_id,accepted_at FROM customer_portal_invitations WHERE id='customer-portal-invitation-1'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(invitation.get::<String, _>("state"), "accepted");
        assert_eq!(invitation.get::<String, _>("accepted_user_id"), "portal-user");
        assert_eq!(invitation.get::<i64, _>("accepted_at"), NOW);
        let events: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT action,actor_id,reason FROM customer_portal_invitation_events ORDER BY created_at,id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            events,
            vec![(
                "accepted".into(),
                "portal-user".into(),
                "Verified invited customer accepted this exact binding.".into()
            )]
        );
        // Replay by the same verified identity is idempotent.
        assert!(accept_at(&pool, &token, NOW + 1, false).await.unwrap().accepted);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM customer_portal_invitation_events")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn acceptance_refuses_wrong_email_changed_source_or_revoked_session() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, EMAIL, "cancelled", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let mut session = session();
        let _ = &mut session;
        assert_eq!(
            business(accept_at(&pool, &token, NOW, false).await.unwrap_err()).code,
            "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
        );
        assert!(
            sqlx::query("SELECT 1 FROM customer_access_identities")
                .fetch_optional(pool.raw())
                .await
                .unwrap()
                .is_none()
        );
    })
    .await;
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "other@example.invalid", "confirmed", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let mut session = session();
        let _ = &mut session;
        assert_eq!(
            business(accept_at(&pool, &token, NOW, false).await.unwrap_err()).code,
            "CUSTOMER_PORTAL_INVITATION_UNAVAILABLE"
        );
    })
    .await;
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, EMAIL, "confirmed", "approved").await;
        let token = invitation_token(SECRET, &claims());
        let _ = &mut session;
        assert_eq!(
            business(accept_at(&pool, &token, NOW, true).await.unwrap_err()).code,
            "CUSTOMER_PORTAL_SESSION_REQUIRED"
        );
    })
    .await;
}
