//! Native Customer Portal invitation commands on migrated PostgreSQL.
//!
//! Proves the leased source read, generation/supersession evidence, issued
//! event, idempotent replay, immutable receipt and the exact history read.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_customer_authority::public::access::AccessActor;
use erpx_server_customer_authority::public::portal_invitations::{
    InvitationAction, InvitationCommand, InvitationReceipt, PgPortalInvitation, invitation_history,
    issue_invitation,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CompanyId, HoldingId, RecordId, UserId,
};
use sqlx::Row;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &str = "vector-secret-2026";

fn actor(user: &str, permissions: &[&str]) -> AccessActor {
    AccessActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment-1").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn creator() -> AccessActor {
    actor("creator", &["commerce.read", "commerce.sales"])
}

fn command(action: InvitationAction, version: i64, reason: &str) -> InvitationCommand {
    InvitationCommand {
        action,
        expected_version: version,
        reason: reason.into(),
    }
}

async fn run_issue(
    pool: &DatabasePool,
    actor: AccessActor,
    binding: &str,
    key: &str,
    command: InvitationCommand,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    run_issue_at(pool, actor, binding, key, command, NOW).await
}

async fn run_issue_at(
    pool: &DatabasePool,
    actor: AccessActor,
    binding: &str,
    key: &str,
    command: InvitationCommand,
    now: i64,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let binding = binding.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let binding = binding.clone();
            let key = key.clone();
            let command = command.clone();
            Box::pin(async move {
                let mut repository = PgPortalInvitation::new(connection);
                issue_invitation(
                    &mut repository,
                    &actor,
                    &binding,
                    &key,
                    command,
                    SECRET,
                    "acme",
                    now,
                )
                .await
            })
        })
        .await
}

async fn history(pool: &DatabasePool, binding: &str) -> serde_json::Value {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let binding = binding.to_owned();
            Box::pin(async move {
                let mut repository = PgPortalInvitation::new(connection);
                Ok(invitation_history(&mut repository, &binding)
                    .await?
                    .to_wire())
            })
        })
        .await
        .unwrap()
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

async fn seed(pool: &DatabasePool, identity: bool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('customer-1','company','customer','C1','Portal customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES \
         ('order-1','company','branch',NULL,'sales','SO-1','customer-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'creator','reviewer',100,300);\
         INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES \
         ('binding-1','company','order-1','customer-1','customer@example.invalid',9000000000000,'approved','Declared approved contact','creator','reviewer',200,2,100,200);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
    if identity {
        sqlx::query(
            "INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) \
             VALUES('binding-1','portal-user','customer@example.invalid',100)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }
}

async fn invitations(pool: &DatabasePool) -> Vec<(String, i32, String, String)> {
    sqlx::query_as(
        "SELECT id,generation,state,email FROM customer_portal_invitations \
         WHERE binding_id='binding-1' ORDER BY generation",
    )
    .fetch_all(pool.raw())
    .await
    .unwrap()
}

async fn events(pool: &DatabasePool) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT action,actor_id,reason FROM customer_portal_invitation_events \
         ORDER BY created_at,id",
    )
    .fetch_all(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn issue_persists_one_queued_invitation_receipt_and_event() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let receipt = run_issue(
            &pool,
            creator(),
            "binding-1",
            "invite-key-1",
            command(InvitationAction::Invite, 2, "Issue the approved customer invitation."),
        )
        .await
        .unwrap();
        let parsed: InvitationReceipt = serde_json::from_value(receipt.clone()).unwrap();
        assert_eq!(parsed.id, "binding-1");
        assert_eq!(parsed.binding_id, "binding-1");
        assert_eq!(parsed.generation, 1);
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.state, "queued");
        assert_eq!(parsed.expires_at, NOW + 7 * 86_400_000);
        let rows = invitations(&pool).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, 1);
        assert_eq!(rows[0].2, "queued");
        assert_eq!(rows[0].3, "customer@example.invalid");
        let token_hash: String =
            sqlx::query_scalar("SELECT token_hash FROM customer_portal_invitations WHERE id=$1")
                .bind(&parsed.invitation_id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(token_hash.len(), 64);
        assert_ne!(token_hash, parsed.invitation_id);
        assert_eq!(
            events(&pool).await,
            vec![(
                "issued".into(),
                "creator".into(),
                "Issue the approved customer invitation.".into()
            )]
        );
        let receipt_row = sqlx::query(
            "SELECT operation,record_id,actor_id,result_json FROM command_receipts WHERE command_id='invite-key-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipt_row.get::<String, _>("operation"), "customer.access.invite");
        assert_eq!(receipt_row.get::<String, _>("record_id"), "binding-1");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&receipt_row.get::<String, _>("result_json"))
                .unwrap(),
            receipt
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM outbox_events WHERE command_id='invite-key-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM audit_events WHERE command_id='invite-key-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn replay_returns_the_original_receipt_and_refuses_a_different_payload() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let first = run_issue(
            &pool,
            creator(),
            "binding-1",
            "invite-key-1",
            command(
                InvitationAction::Invite,
                2,
                "Issue the approved customer invitation.",
            ),
        )
        .await
        .unwrap();
        let replay = run_issue(
            &pool,
            creator(),
            "binding-1",
            "invite-key-1",
            command(
                InvitationAction::Invite,
                2,
                "Issue the approved customer invitation.",
            ),
        )
        .await
        .unwrap();
        assert_eq!(replay, first);
        assert_eq!(invitations(&pool).await.len(), 1);
        let changed = business(
            run_issue(
                &pool,
                creator(),
                "binding-1",
                "invite-key-1",
                command(
                    InvitationAction::Invite,
                    2,
                    "A different reason must conflict.",
                ),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(changed.code, "CUSTOMER_ACCESS_IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn a_second_invite_is_refused_while_resend_supersedes_with_evidence() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let _first = run_issue(
            &pool,
            creator(),
            "binding-1",
            "invite-key-1",
            command(
                InvitationAction::Invite,
                2,
                "Issue the approved customer invitation.",
            ),
        )
        .await
        .unwrap();
        let error = business(
            run_issue(
                &pool,
                creator(),
                "binding-1",
                "invite-key-2",
                command(
                    InvitationAction::Invite,
                    2,
                    "A second live invitation is refused.",
                ),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(error.code, "CUSTOMER_PORTAL_INVITATION_ACTIVE");
        let resent = run_issue(
            &pool,
            creator(),
            "binding-1",
            "resend-key-1",
            command(
                InvitationAction::Resend,
                2,
                "Replace the first invitation with a fresh link.",
            ),
        )
        .await
        .unwrap();
        let parsed: InvitationReceipt = serde_json::from_value(resent).unwrap();
        assert_eq!(parsed.generation, 2);
        let rows = invitations(&pool).await;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].2, "superseded");
        assert_eq!(rows[1].2, "queued");
        let history = history(&pool, "binding-1").await;
        assert_eq!(history["invitations"].as_array().unwrap().len(), 2);
        assert_eq!(history["invitations"][0]["generation"], 2);
        assert_eq!(history["invitations"][1]["state"], "superseded");
        assert_eq!(history["invitations"][1]["last_error"], "SUPERSEDED");
        let mut actions: Vec<&str> = history["invitationEvents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["action"].as_str().unwrap())
            .collect();
        // Retained ordering is (created_at,id); identical timestamps break by id.
        actions.sort_unstable();
        assert_eq!(actions, vec!["issued", "issued", "superseded"]);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM audit_events WHERE record_id='binding-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn an_expired_previous_invitation_is_marked_expired_by_a_new_invite() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let _first: InvitationReceipt = serde_json::from_value(
            run_issue(
                &pool,
                creator(),
                "binding-1",
                "invite-key-1",
                command(
                    InvitationAction::Invite,
                    2,
                    "Issue the approved customer invitation.",
                ),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        let later = NOW + 8 * 86_400_000;
        let second = run_issue_at(
            &pool,
            creator(),
            "binding-1",
            "invite-key-2",
            command(
                InvitationAction::Invite,
                2,
                "Issue a fresh invitation after expiry.",
            ),
            later,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_value::<InvitationReceipt>(second)
                .unwrap()
                .generation,
            2
        );
        let rows = invitations(&pool).await;
        assert_eq!(rows[0].2, "expired");
        assert_eq!(rows[1].2, "queued");
    })
    .await;
}

#[tokio::test]
async fn a_bound_identity_is_refused_before_any_write() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, true).await;
        let bound = business(
            run_issue(
                &pool,
                creator(),
                "binding-1",
                "invite-key-1",
                command(
                    InvitationAction::Invite,
                    2,
                    "A bound identity cannot be invited.",
                ),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(bound.code, "CUSTOMER_PORTAL_IDENTITY_ALREADY_BOUND");
        assert!(invitations(&pool).await.is_empty());
        assert!(events(&pool).await.is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM command_receipts")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn a_stale_binding_version_is_refused_before_any_write() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let stale = business(
            run_issue(
                &pool,
                creator(),
                "binding-1",
                "invite-key-2",
                command(
                    InvitationAction::Invite,
                    1,
                    "A stale binding version must conflict.",
                ),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(stale.code, "CUSTOMER_ACCESS_CHANGED");
        assert!(invitations(&pool).await.is_empty());
        assert!(events(&pool).await.is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM command_receipts")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn only_the_original_requester_or_a_sales_approver_may_invite() {
    fixture::with_database(4, |pool| async move {
        migrate(&pool).await;
        seed(&pool, false).await;
        let refused = business(
            run_issue(
                &pool,
                actor("someone-else", &["commerce.read", "commerce.sales"]),
                "binding-1",
                "invite-key-1",
                command(
                    InvitationAction::Invite,
                    2,
                    "Another operator cannot invite this customer.",
                ),
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(refused.code, "CUSTOMER_ACCESS_PERMISSION_REQUIRED");
        let approver = run_issue(
            &pool,
            actor(
                "manager",
                &["commerce.read", "commerce.sales", "commerce.approve"],
            ),
            "binding-1",
            "invite-key-2",
            command(
                InvitationAction::Invite,
                2,
                "An independent sales manager invites the customer.",
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_value::<InvitationReceipt>(approver)
                .unwrap()
                .generation,
            1
        );
        let row = sqlx::query(
            "SELECT created_by,email FROM customer_portal_invitations WHERE binding_id='binding-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("created_by"), "manager");
        assert_eq!(row.get::<String, _>("email"), "customer@example.invalid");
    })
    .await;
}
