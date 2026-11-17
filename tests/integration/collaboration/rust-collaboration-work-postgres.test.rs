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

#[tokio::test]
async fn push_preferences_preserve_enabled_and_roll_back_on_source_drift() {
    use erpx_server_collaboration::public::web_push::{CurrentPushPreferenceAuthority, lock_push_preference_sources, parse_push_preferences, save_push_preferences};
    use sqlx::PgConnection;
    struct SyntheticAuthority;
    impl CurrentPushPreferenceAuthority for SyntheticAuthority {
        fn company_id(&self) -> &str { "company" }
        fn user_id(&self) -> &str { "user" }
        async fn require_current(&self, _: &mut PgConnection) -> Result<(), PgTransactionError<BusinessError>> { Ok(()) }
    }
    fixture::with_database(2, |pool| async move {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let migrations = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        let input = parse_push_preferences(&serde_json::json!({"work":true,"mention":false,"system":true,"calendar":true,"notice":false,"quietStartMinute":1320,"quietEndMinute":420})).unwrap();
        let input1 = input.clone();
        PgUnitOfWork::new(pool.clone()).run_once(TransactionOptions::default(), move |c| {
            let input = input1.clone();
            Box::pin(async move {
                lock_push_preference_sources(c).await?;
                let proof = save_push_preferences(c, &SyntheticAuthority, &input, 100).await?;
                proof.require_current(c, &SyntheticAuthority).await?;
                Ok::<_, PgTransactionError<BusinessError>>(())
            })
        }).await.unwrap();
        sqlx::query("UPDATE push_preferences SET enabled=true WHERE company_id='company' AND user_id='user'").execute(pool.raw()).await.unwrap();
        let input2 = input.clone();
        let result = PgUnitOfWork::new(pool.clone()).run_once(TransactionOptions::default(), move |c| {
            let input = input2.clone();
            Box::pin(async move {
                lock_push_preference_sources(c).await?;
                let proof = save_push_preferences(c, &SyntheticAuthority, &input, 101).await?;
                sqlx::query("UPDATE push_preferences SET work_enabled=false WHERE company_id='company' AND user_id='user'").execute(&mut *c).await?;
                proof.require_current(c, &SyntheticAuthority).await
            })
        }).await;
        assert!(matches!(result, Err(PgTransactionError::Business(ref e)) if e.code == "PUSH_CONTEXT_CHANGED"));
        let row: (bool, bool, i64) = sqlx::query_as("SELECT enabled,work_enabled,updated_at FROM push_preferences WHERE company_id='company' AND user_id='user'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row, (true, true, 100));
        PgUnitOfWork::new(pool.clone()).run_once(TransactionOptions::default(), move |c| {
            let mut input = input.clone();
            input.work = false;
            Box::pin(async move {
                lock_push_preference_sources(c).await?;
                let proof = save_push_preferences(c, &SyntheticAuthority, &input, 102).await?;
                proof.require_current(c, &SyntheticAuthority).await
            })
        }).await.unwrap();
        let row: (bool, bool, i64) = sqlx::query_as("SELECT enabled,work_enabled,updated_at FROM push_preferences WHERE company_id='company' AND user_id='user'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row, (true, false, 102));
        sqlx::raw_sql("CREATE FUNCTION push_preference_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.notice_enabled:=true; RETURN NEW; END $$; CREATE TRIGGER push_preference_tamper BEFORE UPDATE ON push_preferences FOR EACH ROW EXECUTE FUNCTION push_preference_tamper()")
            .execute(pool.raw()).await.unwrap();
        let attempted = parse_push_preferences(&serde_json::json!({"work":true,"mention":false,"system":true,"calendar":true,"notice":false,"quietStartMinute":1320,"quietEndMinute":420})).unwrap();
        let tampered = PgUnitOfWork::new(pool.clone()).run_once(TransactionOptions::default(), move |c| {
            let input = attempted.clone();
            Box::pin(async move {
                lock_push_preference_sources(c).await?;
                save_push_preferences(c, &SyntheticAuthority, &input, 103).await?;
                Ok::<_, PgTransactionError<BusinessError>>(())
            })
        }).await;
        assert!(matches!(tampered, Err(PgTransactionError::Business(ref e)) if e.code == "PUSH_CONTEXT_CHANGED"));
        let row: (bool, bool, bool, i64) = sqlx::query_as("SELECT enabled,work_enabled,notice_enabled,updated_at FROM push_preferences WHERE company_id='company' AND user_id='user'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(row, (true, false, false, 102));
    }).await;
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
