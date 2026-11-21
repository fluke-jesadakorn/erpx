//! Native Customer Portal invitation delivery on migrated PostgreSQL.
//!
//! Proves leased claim, exact source/purchase re-checks, signed token
//! reproduction against the stored digest, the private enrollment link and
//! idempotency key, recovery of interrupted/expired invitations, terminal
//! revocation evidence and bounded retry/failure states.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_customer_authority::public::portal_invitation_delivery::{
    InvitationClaims, MAX_ATTEMPTS, PgPortalInvitationDelivery, PortalEmail, PortalEmailFuture,
    PortalInvitationEmailPort, deliver_one, invitation_token, invitation_token_hash,
    require_invitation_token,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;
const SECRET: &str = "vector-secret-2026";
const CUSTOMER_EMAIL: &str = "customer@example.invalid";

#[derive(Clone, Copy, PartialEq, Eq)]
enum SendOutcome {
    Accepted,
    Failed(&'static str),
}

#[derive(Clone)]
struct TestEmail {
    outcome: SendOutcome,
    sent: Arc<Mutex<Vec<PortalEmail>>>,
}

impl TestEmail {
    fn new(outcome: SendOutcome) -> Self {
        Self {
            outcome,
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn sent(&self) -> Vec<PortalEmail> {
        self.sent.lock().unwrap().clone()
    }
}

impl PortalInvitationEmailPort for TestEmail {
    fn send<'a>(&'a self, email: PortalEmail) -> PortalEmailFuture<'a, ()> {
        self.sent.lock().unwrap().push(email);
        let outcome = self.outcome;
        Box::pin(async move {
            match outcome {
                SendOutcome::Accepted => Ok(()),
                SendOutcome::Failed(code) => Err(BusinessError::new(
                    FailureKind::Unavailable,
                    code,
                    "declared provider failure",
                )),
            }
        })
    }
}

async fn cycle_mode(
    pool: &DatabasePool,
    email: &TestEmail,
    production: bool,
    mode: PackageEnforcementMode,
) -> Result<bool, PgTransactionError<BusinessError>> {
    let unit = PgPortalInvitationDelivery::new(pool.clone(), mode, production);
    let sign = |claims: &InvitationClaims, stored: &str| {
        require_invitation_token(Some(SECRET), claims, stored)
    };
    deliver_one(
        &unit,
        email,
        &sign,
        Some("https://account.example.test"),
        "lease-test",
        NOW,
    )
    .await
}

async fn cycle(
    pool: &DatabasePool,
    email: &TestEmail,
    production: bool,
) -> Result<bool, PgTransactionError<BusinessError>> {
    cycle_mode(pool, email, production, PackageEnforcementMode::Disabled).await
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

async fn seed_source(pool: &DatabasePool) {
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
}

/// One approved binding over its own original sales order.
async fn seed_binding(pool: &DatabasePool, index: i32, email: &str) -> String {
    let order = format!("order-{index}");
    let binding = format!("binding-{index}");
    sqlx::query(
        "INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,\
         warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,\
         version,created_by,approved_by,created_at,updated_at) \
         VALUES($1,'company','branch',NULL,'sales',$2,'customer-1','warehouse-1','THB',\
         '2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'creator','reviewer',100,300)",
    )
    .bind(&order)
    .bind(format!("SO-{index}"))
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,\
         status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) \
         VALUES($1,'company',$2,'customer-1',$3,9000000000000,'approved',\
         'Declared approved contact','creator','reviewer',200,2,100,200)",
    )
    .bind(&binding)
    .bind(&order)
    .bind(email)
    .execute(pool.raw())
    .await
    .unwrap();
    binding
}

fn token_for(invitation_id: &str, binding_id: &str, expires_at: i64) -> String {
    invitation_token(
        SECRET,
        &InvitationClaims {
            invitation_id: invitation_id.to_owned(),
            binding_id: binding_id.to_owned(),
            holding_slug: "acme".into(),
            generation: 1,
            expires_at,
            key_version: 1,
        },
    )
}

/// Seed one invitation with its exact digest and command receipt.
#[allow(clippy::too_many_arguments)]
async fn seed_invitation(
    pool: &DatabasePool,
    id: &str,
    binding_id: &str,
    state: &str,
    attempts: i32,
    lease_until: Option<i64>,
    token_expires_at: i64,
    email: &str,
) {
    let token = token_for(id, binding_id, token_expires_at);
    let hash = invitation_token_hash(&token);
    let command = format!("command-{id}");
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) \
         VALUES($1,'commerce.customer-portal-invitation.issue','creator',$2,$3,$4,100)",
    )
    .bind(&command)
    .bind("0".repeat(64))
    .bind(binding_id)
    .bind(serde_json::json!({"id": id}).to_string())
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,\
         token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,\
         lease_token,lease_until,last_error,created_by,created_at,command_id) \
         VALUES($1,$2,'acme',1,$3,$4,1,$5,$6,$7,100,$8,$9,$10,'creator',100,$11)",
    )
    .bind(id)
    .bind(binding_id)
    .bind(email)
    .bind(&hash)
    .bind(token_expires_at)
    .bind(state)
    .bind(attempts)
    .bind(if state == "running" {
        Some("lease-old")
    } else {
        None
    })
    .bind(lease_until)
    .bind(if state == "running" {
        Some("DELIVERY_INTERRUPTED")
    } else {
        None
    })
    .bind(&command)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn invitation(pool: &DatabasePool, id: &str) -> (String, i32, Option<i64>, Option<String>) {
    let row = sqlx::query(
        "SELECT state,attempts,sent_at,last_error FROM customer_portal_invitations WHERE id=$1",
    )
    .bind(id)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (
        row.get("state"),
        row.get("attempts"),
        row.get("sent_at"),
        row.get("last_error"),
    )
}

async fn events(pool: &DatabasePool, id: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT action,actor_id,reason FROM customer_portal_invitation_events \
         WHERE invitation_id=$1 ORDER BY created_at,id",
    )
    .bind(id)
    .fetch_all(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn claims_and_delivers_the_exact_private_invitation() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let binding = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-1",
            &binding,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Accepted);

        assert!(cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-1").await,
            ("sent".into(), 1, Some(NOW), None)
        );
        let sent = email.sent();
        assert_eq!(sent.len(), 1);
        let message = &sent[0];
        assert_eq!(
            message.delivery_key,
            "customer-portal-invitation-invitation-1"
        );
        assert_eq!(message.recipient, CUSTOMER_EMAIL);
        assert_eq!(message.subject, "Your ERPX Customer Portal invitation");
        let prefix = "https://account.example.test/customer-portal/enroll#token=";
        let token = message
            .text
            .split(prefix)
            .nth(1)
            .and_then(|rest| rest.split(['\n', ' ']).next())
            .expect("delivered private token");
        assert_eq!(
            token,
            token_for("invitation-1", &binding, NOW + 86_400_000),
            "the worker reproduces the immutable token"
        );
        assert!(
            message
                .text
                .ends_with("If you did not request this, you can ignore this email.")
        );
        assert_eq!(
            events(&pool, "invitation-1").await,
            vec![
                (
                    "delivery-started".into(),
                    "system:customer-portal-invitation".into(),
                    "".into()
                ),
                (
                    "delivery-sent".into(),
                    "system:customer-portal-invitation".into(),
                    "".into()
                ),
            ]
        );
        assert!(!cycle(&pool, &email, false).await.unwrap());
        assert_eq!(email.sent().len(), 1, "a sent invitation is not resent");
    })
    .await;
}

#[tokio::test]
async fn production_release_gate_holds_without_an_engineering_context() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let binding = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-2",
            &binding,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Accepted);
        assert!(
            !cycle_mode(&pool, &email, true, PackageEnforcementMode::Shadow)
                .await
                .unwrap(),
            "production without an engineering context does not deliver"
        );
        assert_eq!(
            invitation(&pool, "invitation-2").await,
            ("queued".into(), 0, None, None)
        );
        assert!(email.sent().is_empty());
        sqlx::query(
            "INSERT INTO engineering_verification_contexts(company_id,run_id,source_fingerprint,\
             software_version,release_manifest_sha256,enabled,expires_at,reason,created_at) \
             VALUES('company','abcdefabcdef',$1,'release-'||$1,$1,true,$2,\
             'Declared isolated engineering verification',100)",
        )
        .bind("a".repeat(64))
        .bind(NOW + 60_000)
        .execute(pool.raw())
        .await
        .unwrap();
        assert!(
            cycle_mode(&pool, &email, true, PackageEnforcementMode::Shadow)
                .await
                .unwrap()
        );
        assert_eq!(email.sent().len(), 1);
        assert!(!cycle(&pool, &email, false).await.unwrap());
    })
    .await;
}

#[tokio::test]
async fn expired_invitations_become_terminal_evidence_without_a_send() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let binding = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-3",
            &binding,
            "queued",
            0,
            None,
            NOW - 1,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Accepted);

        assert!(!cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-3").await,
            ("expired".into(), 0, None, Some("INVITATION_EXPIRED".into()))
        );
        assert!(email.sent().is_empty());
        assert_eq!(
            events(&pool, "invitation-3").await,
            vec![(
                "expired".into(),
                "system:customer-portal-invitation".into(),
                "INVITATION_EXPIRED".into()
            )]
        );
    })
    .await;
}

#[tokio::test]
async fn cancelled_source_or_unapproved_binding_revokes_before_claim() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let queued = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        let sent = seed_binding(&pool, 2, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-4",
            &queued,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        seed_invitation(
            &pool,
            "invitation-5",
            &sent,
            "sent",
            1,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        sqlx::query("UPDATE commerce_orders SET state='cancelled',version=version+1 WHERE id IN ('order-1','order-2')")
            .execute(pool.raw())
            .await
            .unwrap();
        let email = TestEmail::new(SendOutcome::Accepted);

        assert!(!cycle(&pool, &email, false).await.unwrap());
        for id in ["invitation-4", "invitation-5"] {
            assert_eq!(
                invitation(&pool, id).await.0,
                "revoked",
                "{id} source cancellation revokes the invitation"
            );
            assert_eq!(
                invitation(&pool, id).await.3.as_deref(),
                Some("SOURCE_UNAVAILABLE")
            );
        }
        assert_eq!(
            events(&pool, "invitation-4").await,
            vec![(
                "revoked".into(),
                "system:customer-portal-invitation".into(),
                "SOURCE_UNAVAILABLE".into()
            )]
        );
        assert!(email.sent().is_empty());

        // An unapproved binding is the same unavailable source.
        let unapproved = seed_binding(&pool, 3, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-6",
            &unapproved,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        sqlx::query("UPDATE customer_access_bindings SET status='revoked',version=version+1 WHERE id=$1")
            .bind(&unapproved)
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(!cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-6").await,
            (
                "revoked".into(),
                0,
                None,
                Some("SOURCE_UNAVAILABLE".into())
            )
        );
    })
    .await;
}

#[tokio::test]
async fn interrupted_leases_are_retried_until_the_attempt_cap() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let retry = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        let exhausted = seed_binding(&pool, 2, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-7",
            &retry,
            "running",
            2,
            Some(NOW - 1),
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        seed_invitation(
            &pool,
            "invitation-8",
            &exhausted,
            "running",
            MAX_ATTEMPTS,
            Some(NOW - 1),
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Accepted);

        // Recover retries the interrupted lease under the cap and fails the
        // exhausted one; then the retried invitation is claimed and delivered.
        assert!(cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-8").await,
            (
                "failed".into(),
                MAX_ATTEMPTS,
                None,
                Some("DELIVERY_ATTEMPTS_EXHAUSTED".into())
            )
        );
        let retried = invitation(&pool, "invitation-7").await;
        assert_eq!(retried.0, "sent");
        assert_eq!(retried.1, 3);
        assert!(
            events(&pool, "invitation-7").await.contains(&(
                "delivery-retried".into(),
                "system:customer-portal-invitation".into(),
                "DELIVERY_INTERRUPTED".into()
            )),
            "the interrupted lease leaves recovery evidence"
        );
    })
    .await;
}

#[tokio::test]
async fn delivery_failures_persist_only_safe_codes_and_bounded_backoff() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let binding = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-9",
            &binding,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Failed("EMAIL_SENSITIVE_RECEIPT_INVALID"));

        assert!(cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-9").await,
            (
                "queued".into(),
                1,
                None,
                Some("EMAIL_SENSITIVE_RECEIPT_INVALID".into())
            )
        );
        let next: i64 = sqlx::query_scalar(
            "SELECT next_attempt_at FROM customer_portal_invitations WHERE id='invitation-9'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(next, NOW + 60_000);
        assert_eq!(
            events(&pool, "invitation-9").await.last(),
            Some(&(
                "delivery-retried".into(),
                "system:customer-portal-invitation".into(),
                "EMAIL_SENSITIVE_RECEIPT_INVALID".into()
            ))
        );
    })
    .await;
}

#[tokio::test]
async fn changed_binding_email_or_signing_secret_fail_closed() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed_source(&pool).await;
        let mismatched = seed_binding(&pool, 1, CUSTOMER_EMAIL).await;
        let rotated = seed_binding(&pool, 2, CUSTOMER_EMAIL).await;
        seed_invitation(
            &pool,
            "invitation-10",
            &mismatched,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            "other@example.invalid",
        )
        .await;
        seed_invitation(
            &pool,
            "invitation-11",
            &rotated,
            "queued",
            0,
            None,
            NOW + 86_400_000,
            CUSTOMER_EMAIL,
        )
        .await;
        let email = TestEmail::new(SendOutcome::Accepted);

        // The invitation email no longer matches the approved binding email.
        assert!(cycle(&pool, &email, false).await.unwrap());
        assert_eq!(
            invitation(&pool, "invitation-10").await,
            ("revoked".into(), 1, None, Some("SOURCE_UNAVAILABLE".into()))
        );

        // A rotated signing secret cannot reproduce the stored digest.
        let unit =
            PgPortalInvitationDelivery::new(pool.clone(), PackageEnforcementMode::Disabled, false);
        let wrong = |claims: &InvitationClaims, stored: &str| {
            require_invitation_token(Some("rotated-secret"), claims, stored)
        };
        assert!(
            deliver_one(
                &unit,
                &email,
                &wrong,
                Some("https://account.example.test"),
                "lease-test",
                NOW
            )
            .await
            .unwrap()
        );
        assert_eq!(
            invitation(&pool, "invitation-11").await,
            (
                "queued".into(),
                1,
                None,
                Some("INVITATION_SIGNING_SECRET_CHANGED".into())
            )
        );
        assert!(
            email.sent().is_empty(),
            "neither the revoked nor the unverifiable notice is sent"
        );
    })
    .await;
}
