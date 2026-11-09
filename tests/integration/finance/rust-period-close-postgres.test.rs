//! Native Finance period close over leased PostgreSQL, current Account/IAM
//! authority, immutable close checks/reconciliations, independent-person
//! controls and the caller-owned command receipt.
#![allow(dead_code)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use erpx_server_files::public::business_evidence::{
    PgBusinessEvidenceFiles, PreparedBusinessEvidence, evidence_context, policy_fact,
};
use erpx_server_finance::public::period_close::*;
use erpx_server_finance::public::sales_invoice_create::SalesInvoiceCommandActor;
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use facts::{account, setup};
use serde_json::Value;
use sqlx::PgConnection;

const NOW: i64 = 1_800_000_000_000;

struct CloseWorkpapers {
    company: CompanyId,
    actor_id: String,
    now: i64,
}

impl CloseWorkpapers {
    fn new(actor: &SalesInvoiceCommandActor) -> Self {
        Self {
            company: CompanyId::new(actor.company_id.clone()).unwrap(),
            actor_id: actor.user_id.clone(),
            now: NOW,
        }
    }
}

impl PgPeriodCloseEvidence for CloseWorkpapers {
    type Prepared = PreparedBusinessEvidence;

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: PeriodCloseEvidenceRequest,
    ) -> Result<Self::Prepared, PgTransactionError<BusinessError>> {
        let mut port = PgBusinessEvidenceFiles::new(
            c,
            self.company.clone(),
            self.actor_id.clone(),
            None,
            None,
            vec![policy_fact(
                PERIOD_CLOSE_WORKPAPER_TARGET_TYPE,
                PERIOD_CLOSE_WORKPAPER_PURPOSE_KEY,
                &[PERIOD_CLOSE_WORKPAPER_COMPONENT_KIND],
            )],
            TimestampMillis::new(self.now),
        );
        Ok(prepare_business_evidence(
            &mut port,
            BusinessEvidenceRequest {
                attachment_set_ids: request.set_ids,
                target_types: vec![PERIOD_CLOSE_WORKPAPER_TARGET_TYPE.into()],
                contexts: vec![evidence_context(
                    PERIOD_CLOSE_WORKPAPER_PURPOSE_KEY,
                    PERIOD_CLOSE_WORKPAPER_COMPONENT_KIND,
                    Some(PERIOD_CLOSE_WORKPAPER_COMPONENT_REF),
                )],
                target_id: request.target_id,
                expected_target_version: request.expected_target_version,
                command_id: request.command_id,
                binding_type: request.binding_type,
                binding_id: request.binding_id,
            },
        )
        .await?
        .expect("nonempty close workpapers"))
    }

    async fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        PgBusinessEvidenceFiles::new(
            c,
            self.company.clone(),
            self.actor_id.clone(),
            None,
            None,
            Vec::new(),
            TimestampMillis::new(self.now),
        )
        .require_current(prepared)
        .await
    }
}

fn actor(user: &str, assignment: &str) -> SalesInvoiceCommandActor {
    SalesInvoiceCommandActor {
        user_id: user.into(),
        assignment_id: assignment.into(),
        role: "custom_finance_review".into(),
        session_id: format!("session-{user}"),
        revision: 7,
        assignment_version: 3,
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        scope_kind: "holding".into(),
        scope_node_id: "company".into(),
        iam_json: "{}".into(),
    }
}

fn actor_for(company: &str) -> SalesInvoiceCommandActor {
    let mut actor = actor("fixture-a", "finance-fixture-a");
    actor.company_id = company.into();
    actor
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,status,version,created_at,updated_at)VALUES\
         ('close-ar','company','chart','1100','Trade receivables','asset','THB',1,'receivable','active',1,0,0),\
         ('close-ap','company','chart','2100','Trade payables','liability','THB',1,'payable','active',1,0,0),\
         ('close-bank','company','chart','1000','Bank','asset','THB',1,'bank','active',1,0,0) ON CONFLICT DO NOTHING;\
         INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,status,version,imported_by,created_at)VALUES\
         ('close-statement','company','close-bank','ST-CLOSE','2026-01-31',0,0,1000000,0,'close-hash','imported',1,'fixture-a',0) ON CONFLICT DO NOTHING;\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at)VALUES\
         ('close-upload-1','holding','company','delivery','workpaper.png','image/png',10,'close-staging-1','ready','fixture-a',4102444800000,0,0),\
         ('close-upload-2','holding','company','delivery','workpaper-2.png','image/png',10,'close-staging-2','ready','fixture-a',4102444800000,0,0),\
         ('close-upload-3','holding','company','delivery','workpaper-3.png','image/png',10,'close-staging-3','ready','fixture-a',4102444800000,0,0);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at)VALUES\
         ('close-attachment-1','close-upload-1','holding','company','delivery','close-original-1',1,'workpaper.png','image/png',10,repeat('c',64),'fixture-a',0,'company','image/png','available',1,1),\
         ('close-attachment-2','close-upload-2','holding','company','delivery','close-original-2',1,'workpaper-2.png','image/png',10,repeat('d',64),'fixture-a',0,'company','image/png','available',1,1),\
         ('close-attachment-3','close-upload-3','holding','company','delivery','close-original-3',1,'workpaper-3.png','image/png',10,repeat('e',64),'fixture-a',0,'company','image/png','available',1,1);\
         INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at)VALUES\
         ('close-set','company',NULL,NULL,'finance_period','period','action','close-approval','close_workpaper',1,1,'confidential','[]',1,50,'draft','fixture-a',0,0),\
         ('close-set-target','company',NULL,NULL,'finance_period','other-period','action','close-approval','close_workpaper',1,1,'confidential','[]',1,50,'draft','fixture-a',0,0),\
         ('close-set-version','company',NULL,NULL,'finance_period','period','action','close-approval','close_workpaper',2,1,'confidential','[]',1,50,'draft','fixture-a',0,0),\
         ('close-set-short','company',NULL,NULL,'finance_period','period','action','close-approval','close_workpaper',1,1,'confidential','[]',2,50,'draft','fixture-short',0,0),\
         ('close-set-foreign','company-2',NULL,NULL,'finance_period','period','action','close-approval','close_workpaper',1,1,'confidential','[]',1,50,'draft','fixture-a',0,0);\
         INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at)VALUES\
         ('close-link','close-set','close-attachment-1','draft','fixture-a',0,4102444800000),\
         ('close-link-short','close-set-short','close-attachment-2','draft','fixture-a',0,4102444800000);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn begin(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = begin_period_close(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn cancel(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseCancelInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = cancel_period_close(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn close(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let mut evidence = CloseWorkpapers::new(actor);
    let result = close_period_with_evidence(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
        &mut evidence,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

async fn reopen(
    pool: &DatabasePool,
    actor: &SalesInvoiceCommandActor,
    key: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
) -> Result<PeriodCloseResult, PgTransactionError<BusinessError>> {
    let mut tx = pool.raw().begin().await.unwrap();
    let result = reopen_period(
        &mut tx,
        actor,
        &CommandKey::new(key).unwrap(),
        "period",
        input,
        can_approve,
        NOW,
    )
    .await;
    match result {
        Ok(value) => {
            tx.commit().await.unwrap();
            Ok(value)
        }
        Err(error) => {
            let _ = tx.rollback().await;
            Err(error)
        }
    }
}

fn proof(version: i64) -> PeriodCloseProofInput {
    PeriodCloseProofInput {
        proof: "Reviewed close workpapers".into(),
        expected_version: Some(version),
        attachment_set_ids: Vec::new(),
    }
}

fn assert_business<T: std::fmt::Debug>(
    result: Result<T, PgTransactionError<BusinessError>>,
    code: &str,
) {
    match result {
        Err(PgTransactionError::Business(error)) => assert_eq!(error.code, code),
        other => panic!("expected {code}, got {other:?}"),
    }
}

#[tokio::test]
async fn period_close_begin_close_reopen_and_read_retain_evidence() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");

        let begun = begin(pool, &maker, "close-begin-1").await.unwrap();
        assert_eq!(begun.state, "closing");
        assert_eq!(begun.version, 1);
        assert!(!begun.reconciliation_hash.is_empty());
        assert_eq!(begun.reconciliations.len(), 3);

        let workspace = {
            let mut c = pool.raw().acquire().await.unwrap();
            read_period_close(&mut c, &maker, "period").await.unwrap()
        };
        assert_eq!(workspace.period["state"], "closing");
        assert_eq!(workspace.period["close_version"], 1);
        assert_eq!(workspace.blockers, 0);
        assert!(workspace.reconciliation_ready);
        assert!(workspace.reconciliations.iter().all(|row| row.difference_minor == 0));

        let stored: (i64, String, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM finance_close_checks WHERE company_id='company' AND period_id='period'),\
             COALESCE((SELECT reconciliation_hash FROM finance_periods WHERE id='period'),''),\
             (SELECT close_version FROM finance_periods WHERE id='period')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored.0, 5);
        assert_eq!(stored.1, begun.reconciliation_hash);
        assert_eq!(stored.2, 1);

        let closed = close(pool, &reviewer, "close-approve-1", &proof(1), true)
            .await
            .unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.version, 2);
        let (state, closed_by): (String, String) = sqlx::query_as(
            "SELECT state,COALESCE(closed_by,'') FROM finance_periods WHERE id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, "closed");
        assert_eq!(closed_by, "fixture-b");

        let reopened = reopen(pool, &maker, "close-reopen-1", &proof(2), true)
            .await
            .unwrap();
        assert_eq!(reopened.state, "open");
        assert_eq!(reopened.version, 3);
        let (state, version): (String, i64) = sqlx::query_as(
            "SELECT state,close_version FROM finance_periods WHERE id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, "open");
        assert_eq!(version, 3);
    })
    .await;
}

#[tokio::test]
async fn period_close_rejects_unmatched_bank_and_wrong_version() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        sqlx::query(
            "INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency,match_status) \
             VALUES('close-unmatched','close-statement','company','2026-01-15','TX-1','Unmatched',100,'THB','unmatched')",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let maker = actor("fixture-a", "finance-fixture-a");
        assert_business(begin(pool, &maker, "close-begin-blocked").await, "CLOSE_NOT_READY");

        sqlx::query("DELETE FROM finance_bank_transactions WHERE id='close-unmatched'")
            .execute(pool.raw())
            .await
            .unwrap();
        let begun = begin(pool, &maker, "close-begin-2").await.unwrap();
        assert_eq!(begun.version, 1);
        assert_business(
            cancel(
                pool,
                &maker,
                "close-cancel-wrong",
                &PeriodCloseCancelInput {
                    expected_version: 9,
                    reason: "Wrong version".into(),
                },
                false,
            )
            .await,
            "VERSION_CONFLICT",
        );
        let cancelled = cancel(
            pool,
            &maker,
            "close-cancel-1",
            &PeriodCloseCancelInput {
                expected_version: 1,
                reason: "Correct the workpapers".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "open");
        assert_eq!(cancelled.version, 2);
    })
    .await;
}

#[tokio::test]
async fn period_close_requires_independent_approver_and_stale_evidence() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");
        let begun = begin(pool, &maker, "close-begin-3").await.unwrap();
        assert_eq!(begun.version, 1);
        assert_business(
            close(pool, &maker, "close-self", &proof(1), true).await,
            "CLOSE_APPROVAL_SEPARATION",
        );
        sqlx::query(
            "INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,status,version,imported_by,created_at) \
             VALUES('close-statement-2','company','close-bank','ST-CLOSE-2','2026-01-31',0,250,1000000,250,'close-hash-2','imported',1,'fixture-a',1)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        assert_business(
            close(pool, &reviewer, "close-stale", &proof(1), true).await,
            "CLOSE_RECONCILIATION_STALE",
        );
        let cancelled = cancel(
            pool,
            &maker,
            "close-cancel-2",
            &PeriodCloseCancelInput {
                expected_version: 1,
                reason: "Reconcile the changed bank evidence".into(),
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(cancelled.state, "open");
    })
    .await;
}

#[tokio::test]
async fn period_close_replays_command_and_isolates_company() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let first = begin(pool, &maker, "close-replay-1").await.unwrap();
        let replay = begin(pool, &maker, "close-replay-1").await.unwrap();
        assert_eq!(first.reconciliation_hash, replay.reconciliation_hash);
        assert_eq!(first.version, replay.version);
        let checks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM finance_close_checks WHERE company_id='company' AND period_id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(checks, 5);
        assert_business(
            cancel(
                pool,
                &maker,
                "close-replay-1",
                &PeriodCloseCancelInput {
                    expected_version: 1,
                    reason: "Reuse the begin command key".into(),
                },
                false,
            )
            .await,
            "IDEMPOTENCY_CONFLICT",
        );
        let other = actor_for("company-2");
        assert_business(begin(pool, &other, "close-other-company").await, "PERIOD_NOT_FOUND");
    })
    .await;
}

fn workpapers(sets: &[&str], version: i64) -> PeriodCloseProofInput {
    PeriodCloseProofInput {
        proof: "Reviewed close workpapers".into(),
        expected_version: Some(version),
        attachment_set_ids: sets.iter().map(|value| (*value).to_string()).collect(),
    }
}

#[tokio::test]
async fn period_close_binds_close_workpapers_and_replays_without_rebinding() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");
        begin(pool, &maker, "close-begin-workpapers").await.unwrap();

        let input = workpapers(&["close-set"], 1);
        let closed = close(pool, &reviewer, "close-workpapers-1", &input, true)
            .await
            .unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.version, 2);

        let set: (String, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT state,bound_command_id,bound_at FROM attachment_sets WHERE id='close-set'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(set.0, "bound");
        assert_eq!(set.1.as_deref(), Some("close-workpapers-1"));
        assert_eq!(set.2, Some(NOW));
        let link: (String, Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT state,binding_type,binding_id,bound_at FROM attachment_links WHERE id='close-link'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(link.0, "bound");
        assert_eq!(link.1.as_deref(), Some("finance_period_close"));
        assert_eq!(link.2.as_deref(), Some("period:2"));
        assert_eq!(link.3, Some(NOW));
        let (events, details): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(MAX(details_json),'') FROM attachment_events WHERE action='set.bound' AND set_id='close-set'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(events, 1);
        let details: Value = serde_json::from_str(&details).unwrap();
        assert_eq!(details["bindingType"], "finance_period_close");
        assert_eq!(details["bindingId"], "period:2");
        assert_eq!(details["targetId"], "period");
        assert_eq!(details["expectedTargetVersion"], 1);
        let (receipts, saved): (i64, String) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(MAX(result_json),'') FROM command_receipts WHERE command_id='close-workpapers-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 1);
        let saved: PeriodCloseResult = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved.state, "closed");
        assert_eq!(saved.version, 2);
        assert_eq!(saved.reconciliation_hash, closed.reconciliation_hash);

        let replay = close(pool, &reviewer, "close-workpapers-1", &input, true)
            .await
            .unwrap();
        assert_eq!(replay.state, closed.state);
        assert_eq!(replay.version, closed.version);
        assert_eq!(replay.reconciliation_hash, closed.reconciliation_hash);
        let (receipts, events, version): (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM command_receipts WHERE command_id='close-workpapers-1'),\
             (SELECT COUNT(*) FROM attachment_events WHERE action='set.bound' AND set_id='close-set'),\
             (SELECT close_version FROM finance_periods WHERE id='period')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((receipts, events, version), (1, 1, 2));

        // A changed body under the same key conflicts before any new binding.
        let changed = workpapers(&["close-set-version"], 1);
        assert_business(
            close(pool, &reviewer, "close-workpapers-1", &changed, true).await,
            "IDEMPOTENCY_CONFLICT",
        );
        let draft: (String, Option<String>) = sqlx::query_as(
            "SELECT state,bound_command_id FROM attachment_sets WHERE id='close-set-version'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(draft, ("draft".into(), None));
    })
    .await;
}

#[tokio::test]
async fn period_close_rejects_invalid_workpapers_without_effects_and_rolls_back() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let pool = &p.system.holdings[0];
        seed(pool).await;
        let maker = actor("fixture-a", "finance-fixture-a");
        let reviewer = actor("fixture-b", "finance-fixture-b");
        begin(pool, &maker, "close-begin-reject-workpapers").await.unwrap();

        for (index, (set, code)) in [
            ("close-set-foreign", "ATTACHMENT_BINDING_CONTEXT"),
            ("close-set-target", "ATTACHMENT_BINDING_CHANGED"),
            ("close-set-version", "ATTACHMENT_BINDING_CHANGED"),
            ("close-set-short", "ATTACHMENT_BINDING_CHANGED"),
        ]
        .into_iter()
        .enumerate()
        {
            assert_business(
                close(
                    pool,
                    &reviewer,
                    &format!("close-reject-{index}"),
                    &workpapers(&[set], 1),
                    true,
                )
                .await,
                code,
            );
            let state: (String, i64) = sqlx::query_as(
                "SELECT state,close_version FROM finance_periods WHERE id='period'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(state, ("closing".into(), 1));
            let effects: (i64, i64, i64) = sqlx::query_as(
                "SELECT (SELECT COUNT(*) FROM command_receipts WHERE command_id LIKE 'close-reject-%'),\
                 (SELECT COUNT(*) FROM attachment_events WHERE action='set.bound'),\
                 (SELECT COUNT(*) FROM attachment_sets WHERE state='bound')",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
            assert_eq!(effects, (0, 0, 0));
        }
        let bound_links: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachment_links WHERE state<>'draft'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(bound_links, 0);

        // A close that reaches every effect rolls back the period, set, link,
        // event and receipt together.
        {
            let mut tx = pool.raw().begin().await.unwrap();
            let mut evidence = CloseWorkpapers::new(&reviewer);
            let closed = close_period_with_evidence(
                &mut tx,
                &reviewer,
                &CommandKey::new("close-workpapers-rollback").unwrap(),
                "period",
                &workpapers(&["close-set"], 1),
                true,
                NOW,
                &mut evidence,
            )
            .await
            .unwrap();
            assert_eq!(closed.version, 2);
            tx.rollback().await.unwrap();
        }
        let state: (String, i64) = sqlx::query_as(
            "SELECT state,close_version FROM finance_periods WHERE id='period'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state, ("closing".into(), 1));
        let effects: (i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM command_receipts WHERE command_id='close-workpapers-rollback'),\
             (SELECT COUNT(*) FROM attachment_events WHERE action='set.bound'),\
             (SELECT COUNT(*) FROM attachment_sets WHERE state='bound'),\
             (SELECT COUNT(*) FROM attachment_links WHERE state='bound')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(effects, (0, 0, 0, 0));

        let closed = close(
            pool,
            &reviewer,
            "close-workpapers-final",
            &workpapers(&["close-set"], 1),
            true,
        )
        .await
        .unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.version, 2);
    })
    .await;
}
