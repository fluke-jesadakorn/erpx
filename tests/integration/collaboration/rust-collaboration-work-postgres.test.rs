//! Actual owner persistence/rollback; actor/source below are explicitly synthetic.
//! Selected IAM, Workflow ledger and browser acceptance remain separate gates.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::collaboration_work::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::record_source_contract::{SourceKind, SourceRecord};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId};
fn actor() -> AuthorizedActor {
    AuthorizedActor {
        user_id: "synthetic-reviewer".into(),
        company_id: "company".into(),
        can_review: true,
    }
}
fn plan() -> OwnerPlan {
    OwnerPlan::CreateCard {
        source: SourceRecord {
            kind: SourceKind::FieldServiceOrder,
            id: "synthetic-field-source".into(),
            company_id: "company".into(),
            branch_id: Some("source-branch".into()),
            department_id: Some("source-department".into()),
            version: 2,
            title: "Source".into(),
            status: "open".into(),
        },
        title: "Review visit".into(),
        owner_id: "synthetic-owner".into(),
        due_at: 2000000000000,
        reason: "Review evidence".into(),
    }
}
#[tokio::test]
async fn action_effects_preserve_stored_scope_and_notice_tampering_rolls_back() {
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_collaboration_work_sources(c).await?;
            let key = CommandKey::new("collaboration-good-card")?; let p = plan();
            let receipt = apply_plan(c, &actor(), &key, &p, 100).await?;
            let effects = retain_effects(c, &actor(), &key, &p, &receipt, 100).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
            effects.require_current(c).await?;
            let (stored, _) = read_card(c, &CompanyId::new("company")?, &receipt.id, None, None).await?;
            assert_eq!(stored.branch_id.as_deref(), Some("source-branch"));
            assert_eq!(stored.department_id.as_deref(), Some("source-department"));
            assert_eq!(stored.owner_id, "synthetic-owner");
            assert!(read_card(c, &CompanyId::new("company")?, &receipt.id, Some("other-branch"), None).await.is_err());
            Ok(())
        })).await.unwrap();
        let denial = PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_collaboration_work_sources(c).await?;
            let key = CommandKey::new("collaboration-tampered-card")?; let p = plan();
            let receipt = apply_plan(c, &actor(), &key, &p, 101).await?;
            let effects = retain_effects(c, &actor(), &key, &p, &receipt, 101).await?;
            sqlx::query("UPDATE staff_notifications SET message='tampered' WHERE id='action-card-notice-collaboration-tampered-card'").execute(&mut *c).await?;
            effects.require_current(c).await?;
            Ok(())
        })).await;
        assert!(matches!(denial, Err(PgTransactionError::Business(e)) if e.code == "COLLABORATION_SOURCE_CHANGED"));
        let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM collaboration_action_cards),(SELECT count(*) FROM collaboration_action_events),(SELECT count(*) FROM staff_notifications WHERE record_type='collaboration_action')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (1,1,1));
    }).await;
}
