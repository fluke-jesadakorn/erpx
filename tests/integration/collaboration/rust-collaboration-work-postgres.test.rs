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

#[tokio::test]
async fn card_history_returns_versioned_events_with_scope_authority() {
    use erpx_server_collaboration::public::collaboration_work::CardAction;
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_collaboration_work_sources(c).await?;
            let key = CommandKey::new("collaboration-history-card")?;
            let receipt = apply_plan(c, &actor(), &key, &plan(), 100).await?;
            // Retained `actionHistory` parity: create event first, ordered by version.
            let history = read_card_history(c, &CompanyId::new("company")?, &receipt.id, None, None).await?;
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].get("action").and_then(|v| v.as_str()), Some("create"));
            assert_eq!(history[0].get("version").and_then(|v| v.as_i64()), Some(1));
            assert_eq!(history[0].get("card_id").and_then(|v| v.as_str()), Some(receipt.id.as_str()));
            // Complete the card; the history must gain the versioned complete event.
            let (stored, _) = read_card(c, &CompanyId::new("company")?, &receipt.id, None, None).await?;
            let current = CurrentCard {
                id: stored.id,
                version: stored.version,
                open: stored.open,
                owner_id: stored.owner_id,
                due_at: stored.due_at,
                owner_eligible: true,
                source: stored.source,
            };
            let done = OwnerPlan::ChangeCard {
                current,
                action: CardAction::Complete,
                owner_id: None,
                due_at: None,
                evidence: Some("Verified on site".into()),
                reason: "Done".into(),
            };
            let receipt2 = apply_plan(c, &actor(), &CommandKey::new("collaboration-history-complete")?, &done, 101).await?;
            assert_eq!(receipt2.version, 2);
            let history = read_card_history(c, &CompanyId::new("company")?, &receipt.id, None, None).await?;
            assert_eq!(history.len(), 2);
            assert_eq!(history[1].get("action").and_then(|v| v.as_str()), Some("complete"));
            assert_eq!(history[1].get("version").and_then(|v| v.as_i64()), Some(2));
            // Authority rechecked in owner scope: cross-company and
            // out-of-scope branch cards stay not-found, so no foreign events leak.
            let foreign = read_card_history(c, &CompanyId::new("other")?, &receipt.id, None, None).await;
            assert!(matches!(foreign, Err(PgTransactionError::Business(ref e)) if e.code == "ACTION_CARD_NOT_FOUND"));
            let scoped = read_card_history(c, &CompanyId::new("company")?, &receipt.id, Some("other-branch"), None).await;
            assert!(matches!(scoped, Err(PgTransactionError::Business(ref e)) if e.code == "ACTION_CARD_NOT_FOUND"));
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn card_history_statement_matches_retained_read_with_scope_guard() {
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_collaboration_work_sources(c).await?;
            let key = CommandKey::new("collaboration-statement-card")?;
            let receipt = apply_plan(c, &actor(), &key, &plan(), 100).await?;
            // Pure constructor: owner-issued text with fixed bind order.
            let statement = card_history_statement("company", None, None, &receipt.id)?;
            assert!(statement.sql.contains("collaboration_action_events"));
            assert!(statement.sql.contains("EXISTS"));
            assert_eq!(statement.values.len(), 7);
            assert!(card_history_statement("", None, None, &receipt.id).is_err());
            assert!(card_history_statement("company", None, None, "").is_err());
            // Execute the issued statement on the caller transaction: same
            // rows as the retained read, in version order.
            let rows = run_statement(c, &statement).await?;
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get("action").and_then(|v| v.as_str()), Some("create"));
            // Scope guard: foreign company and out-of-scope branch/department
            // selections yield no rows instead of leaking events.
            let foreign = card_history_statement("other", None, None, &receipt.id)?;
            assert!(run_statement(c, &foreign).await?.is_empty());
            let scoped = card_history_statement("company", Some("other-branch"), None, &receipt.id)?;
            assert!(run_statement(c, &scoped).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}

async fn run_statement(
    c: &mut sqlx::PgConnection,
    statement: &erpx_server_collaboration::public::notification_statement::NotificationStatement,
) -> Result<Vec<serde_json::Value>, PgTransactionError<BusinessError>> {
    // The bridge carries `?` binds like every caller-executed statement; the
    // TypeScript runtime numbers them, so the parity probe does the same.
    let mut numbered = String::with_capacity(statement.sql.len() + 8);
    let mut index = 0;
    for part in statement.sql.split('?') {
        numbered.push_str(part);
        index += 1;
        if index <= statement.values.len() {
            numbered.push_str(&format!("${index}"));
        }
    }
    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(numbered));
    for value in &statement.values {
        let text = match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        };
        query = query.bind(text);
    }
    let raws: Vec<String> = query.fetch_all(&mut *c).await?;
    raws.into_iter()
        .map(|raw| serde_json::from_str(&raw).map_err(|_| PgTransactionError::Business(BusinessError::new(erpx_server_shared::public::FailureKind::InvalidInput, "COLLABORATION_CARD_HISTORY_INVALID", "The card history could not be read."))))
        .collect()
}

#[tokio::test]
async fn eligible_people_statement_matches_retained_read() {
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            // One records.write-capable employee: custom published revision on
            // the seeded employee role (later migrations already publish rev
            // 2+, so take max+1; revision inserts stay allowed, only
            // updates/deletes are immutable).
            sqlx::query("INSERT INTO iam_role_revisions SELECT '*','employee',COALESCE(MAX(revision),0)+1,'[\"iam.self\",\"records.write\",\"module.collaboration.read\",\"module.people_payroll.read\",\"workflow.employee\",\"self.requests\",\"self.tasks\"]','[]','[\"holding\",\"branch\",\"department\"]','test',0 FROM iam_role_revisions WHERE company_id='*' AND role_id='employee'").execute(&mut *c).await?;
            sqlx::query("UPDATE iam_roles SET published_revision=(SELECT MAX(revision) FROM iam_role_revisions WHERE company_id='*' AND role_id='employee') WHERE id='employee' AND company_id='*'").execute(&mut *c).await?;
            sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('pp-elig','elig-user','employee','[\"collaboration\"]','company',NULL,0,9999999999999,1,'holding',NULL)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('person-elig','elig-user','company','Eligible Eva','TH','active','fixture',0,0)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('eng-elig','person-elig','company',NULL,'employee','salaried','active','2025-01-01','Member','THB','fixture',0,0)").execute(&mut *c).await?;
            // A person without an assignment is never eligible.
            sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('person-lonely',NULL,'company','Lonely','TH','active','fixture',0,0)").execute(&mut *c).await?;
            let retained = "SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p JOIN assignments x ON x.user_id=p.user_id AND x.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name";
            let via_statement = run_rows_statement(c, &eligible_people_statement("company")?).await?;
            let via_retained = run_rows_raw(c, retained, &[Some("company".to_owned())]).await?;
            assert_eq!(via_statement.len(), 1);
            assert_eq!(via_statement, via_retained);
            assert_eq!(via_statement[0].get("id").and_then(|v| v.as_str()), Some("elig-user"));
            // Foreign company sees nobody; empty company is rejected.
            assert!(run_rows_statement(c, &eligible_people_statement("other")?).await?.is_empty());
            assert!(eligible_people_statement("").is_err());
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn workspace_statements_match_retained_reads_with_scope_guard() {
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            lock_collaboration_work_sources(c).await?;
            let key = CommandKey::new("collaboration-workspace-card")?;
            // Workspace-visible source kind: the retained reads exclude
            // field-service orders (served natively), so parity uses a ticket.
            let ticket = SourceRecord {
                kind: SourceKind::SupportTicket,
                id: "synthetic-ticket-source".into(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                version: 2,
                title: "Source".into(),
                status: "open".into(),
            };
            let receipt = apply_plan(c, &actor(), &key, &OwnerPlan::CreateCard {
                source: ticket,
                title: "Review visit".into(),
                owner_id: "synthetic-owner".into(),
                due_at: 2000000000000,
                reason: "Review evidence".into(),
            }, 100).await?;
            // Retained `recordRelations` literal (retired TypeScript read).
            let retained_relations = "SELECT * FROM collaboration_record_relations WHERE company_id=? AND status='active' AND source_type<>'field_service_order' AND target_type<>'field_service_order' ORDER BY updated_at DESC LIMIT 100";
            sqlx::query("INSERT INTO collaboration_record_relations(id,company_id,branch_id,department_id,source_type,source_id,source_version,target_type,target_id,target_version,relation,reason,created_by,created_at,updated_at) VALUES('rel-1','company',NULL,NULL,'support_ticket','s1',1,'support_ticket','s2',1,'related','synthetic', 'synthetic',100,100)").execute(&mut *c).await?;
            let via_statement = run_rows_statement(c, &record_relations_statement("company")?).await?;
            let via_retained = run_rows_raw(c, retained_relations, &[Some("company".to_owned())]).await?;
            assert_eq!(via_statement.len(), 1);
            assert_eq!(via_statement, via_retained);
            // Retained `actionCards` literal with its composed fragments.
            let retained_cards = "SELECT c.*,CASE WHEN EXISTS(SELECT 1 FROM assignments x JOIN hr_people p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=c.company_id AND x.user_id=c.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=c.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=c.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active') THEN 1 ELSE 0 END owner_eligible,(SELECT legal_name FROM hr_people WHERE company_id=c.company_id AND user_id=c.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM collaboration_action_cards c WHERE c.company_id=? AND c.source_type<>'field_service_order' AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC LIMIT 100";
            let via_statement = run_rows_statement(c, &action_cards_statement("company", None, None)?).await?;
            let via_retained = run_rows_raw(
                c,
                retained_cards,
                &[
                    Some("company".to_owned()),
                    None,
                    None,
                    None,
                    None,
                ],
            )
            .await?;
            assert_eq!(via_statement.len(), 1);
            assert_eq!(via_statement, via_retained);
            assert_eq!(via_statement[0].get("id").and_then(|v| v.as_str()), Some(receipt.id.as_str()));
            // Scope guard: foreign company and out-of-scope branch yield nothing.
            let foreign = action_cards_statement("other", None, None)?;
            assert!(run_rows_statement(c, &foreign).await?.is_empty());
            let scoped = action_cards_statement("company", Some("other-branch"), None)?;
            assert!(run_rows_statement(c, &scoped).await?.is_empty());
            let foreign_relations = record_relations_statement("other")?;
            assert!(run_rows_statement(c, &foreign_relations).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn pos_registers_statement_matches_retained_read_with_scope_guard() {
    use erpx_server_collaboration::public::notification_statement::pos_registers_statement;
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            // Minimal seed: company + two branches, then two commerce
            // warehouses feeding one register each in different branches.
            sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('b1','branch','b1','Branch 1','company','company',NULL,'active'),('b2','branch','b2','Branch 2','company','company',NULL,'active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES('pos-wh-1','company','b1','POSW1','Main','active'),('pos-wh-2','company','b2','POSW2','Second','active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES('pos-reg-1','company','b1','pos-wh-1','Alpha','retail',true),('pos-reg-2','company','b2','pos-wh-2','Beta','retail',true)").execute(&mut *c).await?;
            // Pure constructor: owner-issued text with fixed bind order.
            let statement = pos_registers_statement("company", None)?;
            assert!(statement.sql.contains("pos_registers"));
            assert!(statement.sql.contains("warehouse_name"));
            assert_eq!(statement.values.len(), 3);
            assert!(pos_registers_statement("", None).is_err());
            // Retained `readPosRegisters1` literal (retired TypeScript read)
            // with the commerce warehouse fragment inlined byte-identically.
            let retained = "SELECT r.*,w.name warehouse_name FROM pos_registers r JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"code\",\"name\",\"status\",\"version\" FROM commerce_warehouses) w ON w.id=r.warehouse_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY r.name";
            let via_statement = run_rows_statement(c, &statement).await?;
            let via_retained = run_rows_raw(
                c,
                retained,
                &[
                    Some("company".to_owned()),
                    None,
                    None,
                ],
            )
            .await?;
            assert_eq!(via_statement.len(), 2);
            assert_eq!(via_statement, via_retained);
            assert_eq!(via_statement[0].get("name").and_then(|v| v.as_str()), Some("Alpha"));
            assert_eq!(via_statement[0].get("warehouse_name").and_then(|v| v.as_str()), Some("Main"));
            // Branch-scoped selection sees only its own register.
            let scoped = pos_registers_statement("company", Some("b1"))?;
            let scoped_rows = run_rows_statement(c, &scoped).await?;
            assert_eq!(scoped_rows.len(), 1);
            assert_eq!(scoped_rows[0].get("id").and_then(|v| v.as_str()), Some("pos-reg-1"));
            // Scope guard: foreign company and out-of-scope branch yield nothing.
            let foreign = pos_registers_statement("other", None)?;
            assert!(run_rows_statement(c, &foreign).await?.is_empty());
            let denied = pos_registers_statement("company", Some("other-branch"))?;
            assert!(run_rows_statement(c, &denied).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}

async fn run_rows_statement(
    c: &mut sqlx::PgConnection,
    statement: &erpx_server_collaboration::public::notification_statement::NotificationStatement,
) -> Result<Vec<serde_json::Value>, PgTransactionError<BusinessError>> {
    let values: Vec<Option<String>> = statement
        .values
        .iter()
        .map(|value| match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        })
        .collect();
    run_rows_raw(c, &statement.sql, &values).await
}

async fn run_rows_raw(
    c: &mut sqlx::PgConnection,
    sql: &str,
    values: &[Option<String>],
) -> Result<Vec<serde_json::Value>, PgTransactionError<BusinessError>> {
    // Row-returning statements are compared through their JSON image so the
    // native text and the retained literal are judged on identical row sets.
    let wrapped = format!("SELECT to_jsonb(s)::text FROM ({sql}) s");
    let mut numbered = String::with_capacity(wrapped.len() + 16);
    let parts: Vec<&str> = wrapped.split('?').collect();
    for (index, part) in parts.iter().enumerate() {
        numbered.push_str(part);
        if index < values.len() {
            numbered.push_str(&format!("${}", index + 1));
        }
    }
    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(numbered));
    for value in values {
        query = query.bind(value.clone());
    }
    let raws: Vec<String> = query.fetch_all(&mut *c).await?;
    raws.into_iter()
        .map(|raw| serde_json::from_str(&raw).map_err(|_| PgTransactionError::Business(BusinessError::new(erpx_server_shared::public::FailureKind::InvalidInput, "COLLABORATION_WORKSPACE_INVALID", "The workspace could not be read."))))
        .collect()
}

async fn run_raw(
    c: &mut sqlx::PgConnection,
    sql: &str,
    binds: &[&str],
) -> Result<Vec<serde_json::Value>, PgTransactionError<BusinessError>> {
    let mut numbered = String::with_capacity(sql.len() + 8);
    let parts: Vec<&str> = sql.split('?').collect();
    for (index, part) in parts.iter().enumerate() {
        numbered.push_str(part);
        if index < binds.len() {
            numbered.push_str(&format!("${}", index + 1));
        }
    }
    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(numbered));
    for bind in binds {
        query = query.bind((*bind).to_owned());
    }
    let raws: Vec<String> = query.fetch_all(&mut *c).await?;
    raws.into_iter()
        .map(|raw| serde_json::from_str(&raw).map_err(|_| PgTransactionError::Business(BusinessError::new(erpx_server_shared::public::FailureKind::InvalidInput, "COLLABORATION_WORKSPACE_INVALID", "The workspace could not be read."))))
        .collect()
}

#[tokio::test]
async fn brand_attachment_statements_match_retained_reads() {
    use erpx_server_collaboration::public::notification_statement::{
        brand_attachment_image_statement, brand_organization_image_statement,
    };
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            sqlx::raw_sql(
                "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES \
                 ('company','company','company','Company','company'), \
                 ('branch','branch','branch','Branch','company'); \
                 INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES \
                 ('holding','company',NULL,'holding','fixture',0), \
                 ('branch','branch','company','both','fixture',0); \
                 INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at) VALUES \
                 ('company','Store','#3659c9','fixture',0);",
            ).execute(&mut *c).await?;
            // Constructor parity: kind mapping, bind order, fail-closed inputs.
            assert!(brand_attachment_image_statement("company", "nope", None).is_err());
            assert!(brand_attachment_image_statement("", "holding_brand", Some("company")).is_err());
            assert!(brand_organization_image_statement("", None).is_err());
            let retained_brand = "SELECT jsonb_build_object('profile',(SELECT to_jsonb(profile) FROM organization_brand_profiles profile WHERE profile.scope_kind=? AND profile.scope_id=?),\
                 'storefront',(SELECT to_jsonb(storefront) FROM storefront_app_brands storefront WHERE storefront.company_id=? AND ?::text='company_storefront_brand'))::text";
            let retained_org = "SELECT jsonb_build_object('company',(SELECT to_jsonb(company) FROM organization_nodes company WHERE company.id=? AND company.kind='company'),\
                 'branch',(SELECT to_jsonb(branch) FROM organization_nodes branch WHERE branch.id=? AND branch.company_id=? AND branch.kind='branch'))::text";
            // Brand half: identical rows for every target; foreign company
            // sees a null storefront while the holding profile stays visible.
            for (kind, scope) in [
                ("holding_brand", Some("company".to_owned())),
                ("branch_brand", Some("branch".to_owned())),
                ("company_storefront_brand", None),
            ] {
                let statement = brand_attachment_image_statement("company", kind, scope.as_deref())?;
                let image = run_image_text(c, &statement).await?;
                let scope_kind = if kind == "holding_brand" { "holding" } else { "branch" };
                let retained = run_text_raw(
                    c,
                    retained_brand,
                    &[
                        Some(scope_kind.to_owned()),
                        scope.clone(),
                        Some("company".to_owned()),
                        Some(kind.to_owned()),
                    ],
                )
                .await?;
                assert_eq!(image, retained, "parity for {kind}");
            }
            let foreign = brand_attachment_image_statement("other", "holding_brand", Some("company"))?;
            assert!(run_image_text(c, &foreign).await?.get("storefront").is_some_and(serde_json::Value::is_null));
            // Organization half: company+branch snapshots; null branch for
            // non-branch targets; nulls for foreigners.
            let organization = brand_organization_image_statement("company", Some("branch"))?;
            let snapshot = run_image_text(c, &organization).await?;
            let retained_snapshot = run_text_raw(
                c,
                retained_org,
                &[Some("company".to_owned()), Some("branch".to_owned()), Some("company".to_owned())],
            )
            .await?;
            assert_eq!(snapshot, retained_snapshot);
            assert_eq!(snapshot.get("branch").and_then(|v| v.get("id")).and_then(serde_json::Value::as_str), Some("branch"));
            let company_only = brand_organization_image_statement("company", None)?;
            assert!(run_image_text(c, &company_only).await?.get("branch").is_some_and(serde_json::Value::is_null));
            let foreign_org = brand_organization_image_statement("other", None)?;
            assert!(run_image_text(c, &foreign_org).await?.get("company").is_some_and(serde_json::Value::is_null));
            Ok(())
        })).await.unwrap();
    }).await;
}

async fn run_image_text(
    c: &mut sqlx::PgConnection,
    statement: &erpx_server_collaboration::public::notification_statement::NotificationStatement,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let values: Vec<Option<String>> = statement
        .values
        .iter()
        .map(|value| match value {
            serde_json::Value::String(text) => Some(text.clone()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        })
        .collect();
    run_text_raw(c, &statement.sql, &values).await
}

async fn run_text_raw(
    c: &mut sqlx::PgConnection,
    sql: &str,
    values: &[Option<String>],
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let mut numbered = String::with_capacity(sql.len() + 8);
    let parts: Vec<&str> = sql.split('?').collect();
    for (index, part) in parts.iter().enumerate() {
        numbered.push_str(part);
        if index < values.len() {
            numbered.push_str(&format!("${}", index + 1));
        }
    }
    let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(numbered));
    for value in values {
        query = query.bind(value.clone());
    }
    let raw: String = query.fetch_one(&mut *c).await?;
    serde_json::from_str(&raw).map_err(|_| {
        PgTransactionError::Business(BusinessError::new(
            erpx_server_shared::public::FailureKind::InvalidInput,
            "BRAND_ATTACHMENT_IMAGE_INVALID",
            "The Brand image could not be read.",
        ))
    })
}

#[tokio::test]
async fn pos_sale_statement_matches_retained_read() {
    use erpx_server_collaboration::public::notification_statement::pos_sale_statement;
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('b1','branch','b1','Branch 1','company','company',NULL,'active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES('pos-wh-1','company','b1','POSW1','Main','active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES('pos-reg-1','company','b1','pos-wh-1','Alpha','retail',true)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor) VALUES('pos-shift-1','pos-reg-1','company','b1','fixture',0,0)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,created_by,created_at,updated_at,accounts_json) VALUES('pos-sale-1','company','b1','pos-reg-1','pos-shift-1','S-1','THB','2026-09-02','fixture',0,0,'{}')").execute(&mut *c).await?;
            // Pure constructor: single-row lookup, empty id rejected.
            let statement = pos_sale_statement("pos-sale-1")?;
            assert!(statement.sql.contains("pos_sales"));
            assert_eq!(statement.values.len(), 1);
            assert!(pos_sale_statement("").is_err());
            // Retained `readPosSales1` literal: identical row.
            let retained = "SELECT * FROM pos_sales WHERE id=?";
            let via_statement = run_rows_statement(c, &statement).await?;
            let via_retained = run_rows_raw(c, retained, &[Some("pos-sale-1".to_owned())]).await?;
            assert_eq!(via_statement.len(), 1);
            assert_eq!(via_statement, via_retained);
            assert_eq!(via_statement[0].get("number").and_then(|v| v.as_str()), Some("S-1"));
            // Unknown id yields no row.
            let missing = pos_sale_statement("pos-sale-missing")?;
            assert!(run_rows_statement(c, &missing).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn pos_settings_statement_matches_retained_read() {
    use erpx_server_collaboration::public::notification_statement::pos_settings_statement;
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            sqlx::query("INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json) VALUES('company',true,'THB','Asia/Bangkok','{}')").execute(&mut *c).await?;
            let statement = pos_settings_statement("company")?;
            assert!(statement.sql.contains("pos_settings"));
            assert_eq!(statement.values.len(), 1);
            assert!(pos_settings_statement("").is_err());
            let retained = "SELECT * FROM pos_settings WHERE company_id=?";
            let via_statement = run_rows_statement(c, &statement).await?;
            let via_retained = run_rows_raw(c, retained, &[Some("company".to_owned())]).await?;
            assert_eq!(via_statement.len(), 1);
            assert_eq!(via_statement, via_retained);
            assert_eq!(via_statement[0].get("currency").and_then(|v| v.as_str()), Some("THB"));
            let foreign = pos_settings_statement("other")?;
            assert!(run_rows_statement(c, &foreign).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}


#[tokio::test]
async fn pos_registers_front_statement_matches_retained_read() {
    use erpx_server_collaboration::public::notification_statement::pos_registers_front_statement;
    fixture::with_database(2, |pool| async move {
        let migrations = load_migrations(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"), MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool, MigrationTrack::HoldingWrite, &migrations).await.unwrap();
        PgUnitOfWork::new(pool.clone()).run::<_, BusinessError, _>(TransactionOptions::default(), |c| Box::pin(async move {
            sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('b1','branch','b1','Branch 1','company','company',NULL,'active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES('pos-wh-1','company','b1','POSW1','Main','active')").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,created_by,updated_at) VALUES('prop-1','company','b1','Hotel One','2026-09-02','open','{\"approvedBy\":\"mgr\"}','fixture',0)").execute(&mut *c).await?;
            sqlx::query("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active,property_id) VALUES('pos-reg-1','company','b1','pos-wh-1','Alpha','restaurant',true,'prop-1'),('pos-reg-2','company','b1','pos-wh-1','Beta','retail',true,NULL)").execute(&mut *c).await?;
            let statement = pos_registers_front_statement("company", None)?;
            assert!(statement.sql.contains("hotel_name"));
            assert!(statement.sql.contains("finance_approved"));
            assert_eq!(statement.values.len(), 3);
            assert!(pos_registers_front_statement("", None).is_err());
            let retained = "SELECT r.id register_id,p.name hotel_name,p.state hotel_state,p.business_date,(p.config_json::jsonb->>'approvedBy' IS NOT NULL) finance_approved FROM pos_registers r LEFT JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties) p ON p.id=r.property_id AND p.company_id=r.company_id AND p.branch_id=r.branch_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND r.active=true";
            let via_statement = run_rows_statement(c, &statement).await?;
            let via_retained = run_rows_raw(c, retained, &[Some("company".to_owned()), None, None]).await?;
            assert_eq!(via_statement.len(), 2);
            assert_eq!(via_statement, via_retained);
            let linked = via_statement.iter().find(|v| v.get("register_id").and_then(serde_json::Value::as_str) == Some("pos-reg-1")).unwrap();
            assert_eq!(linked.get("hotel_name").and_then(serde_json::Value::as_str), Some("Hotel One"));
            assert_eq!(linked.get("finance_approved"), Some(&serde_json::Value::Bool(true)));
            let scoped = pos_registers_front_statement("company", Some("other-branch"))?;
            assert!(run_rows_statement(c, &scoped).await?.is_empty());
            let foreign = pos_registers_front_statement("other", None)?;
            assert!(run_rows_statement(c, &foreign).await?.is_empty());
            Ok(())
        })).await.unwrap();
    }).await;
}
