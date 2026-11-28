use std::collections::HashMap;

use erpx_finance_server::public::JournalError;
use erpx_finance_server::public::journal_batch::*;
use erpx_finance_server::public::journal_batch_wire::*;
use erpx_finance_server::public::journal_review::{
    JournalLedgerLine, JournalReviewCommand, JournalReviewResult,
};
use erpx_platform_shared_server::public::{CommandKey, FailureKind, RecordId, TimestampMillis};

fn line(journal: &str, ordinal: i64, debit: i64, credit: i64) -> JournalLedgerLine {
    JournalLedgerLine {
        id: format!("{journal}-{ordinal}"),
        journal_id: journal.into(),
        account_id: if ordinal == 0 { "cash" } else { "sales" }.into(),
        description: "retained".into(),
        debit_minor: debit,
        credit_minor: credit,
        base_debit_minor: debit,
        base_credit_minor: credit,
        dimension_json: "{}".into(),
        ordinal,
    }
}

fn snapshot(id: &str, chart: &str) -> JournalBatchSnapshot {
    JournalBatchSnapshot {
        id: id.into(),
        company_id: "company".into(),
        chart_id: chart.into(),
        period_id: "period".into(),
        currency: "THB".into(),
        status: "draft".into(),
        version: 1,
        source_type: "manual".into(),
        lines: vec![line(id, 0, 100, 0), line(id, 1, 0, 100)],
    }
}

fn input(ids: &[&str]) -> JournalBatchInput {
    JournalBatchInput {
        journals: ids
            .iter()
            .map(|id| BatchJournalRef {
                id: (*id).into(),
                expected_version: 1,
            })
            .collect(),
        reason: "Reviewed by finance controller".into(),
    }
}

fn command(ids: &[&str]) -> JournalBatchCommand {
    JournalBatchCommand {
        key: CommandKey::new("journal-batch-unit-key").unwrap(),
        batch_id: RecordId::new("journal-batch-journal-batch-unit-key").unwrap(),
        input: input(ids),
    }
}

#[test]
fn domain_admission_rejects_bad_size_duplicates_and_reason() {
    let mut empty = input(&[]);
    assert_eq!(
        validate_input(&empty).unwrap_err().code,
        "BATCH_SIZE_INVALID"
    );
    empty.journals = (0..101)
        .map(|i| BatchJournalRef {
            id: format!("j{i}"),
            expected_version: 1,
        })
        .collect();
    assert_eq!(
        validate_input(&empty).unwrap_err().code,
        "BATCH_SIZE_INVALID"
    );
    assert_eq!(
        validate_input(&input(&["j", "j"])).unwrap_err().code,
        "DUPLICATE_JOURNAL"
    );
    let mut bad = input(&["j"]);
    bad.journals[0].expected_version = 0;
    assert_eq!(
        validate_input(&bad).unwrap_err().code,
        "BATCH_EXPECTED_VERSION_INVALID"
    );
    bad.journals[0].expected_version = 1;
    bad.journals[0].id = "x".repeat(129);
    assert_eq!(
        validate_input(&bad).unwrap_err().code,
        "BATCH_JOURNAL_ID_INVALID"
    );
    bad.journals[0].id = "j".into();
    bad.reason = "short".into();
    assert_eq!(
        validate_input(&bad).unwrap_err().code,
        "BATCH_REASON_INVALID"
    );
    assert!(validate_input(&input(&["j"])).is_ok());
}

#[test]
fn journal_guards_enforce_owned_unposted_single_scope_drafts() {
    let first = snapshot("j1", "chart");
    let item = &input(&["j1", "j2"]).journals[1];
    assert!(require_journal(&snapshot("j2", "chart"), item, Some(&first)).is_ok());
    let mut source_owned = snapshot("j2", "chart");
    source_owned.source_type = "sales_invoice".into();
    assert_eq!(
        require_journal(&source_owned, item, Some(&first))
            .unwrap_err()
            .code,
        "BATCH_SOURCE_ADAPTER_REQUIRED"
    );
    let mut changed_version = snapshot("j2", "chart");
    changed_version.version = 2;
    assert_eq!(
        require_journal(&changed_version, item, Some(&first))
            .unwrap_err()
            .code,
        "VERSION_CONFLICT"
    );
    let mut posted = snapshot("j2", "chart");
    posted.status = "posted".into();
    assert_eq!(
        require_journal(&posted, item, Some(&first))
            .unwrap_err()
            .code,
        "VERSION_CONFLICT"
    );
    let other_scope = snapshot("j2", "other-chart");
    assert_eq!(
        require_journal(&other_scope, item, Some(&first))
            .unwrap_err()
            .code,
        "BATCH_SCOPE_MISMATCH"
    );
    assert_eq!(enforce_line_limit(1000), Ok(()));
    assert_eq!(
        enforce_line_limit(1001).unwrap_err().code,
        "BATCH_LINE_LIMIT"
    );
}

#[test]
fn fingerprint_binds_every_retained_line_column_and_its_order() {
    let lines = vec![line("j", 0, 100, 0), line("j", 1, 0, 100)];
    assert_eq!(line_fingerprint(&lines), line_fingerprint(&lines));
    let mut reordered = lines.clone();
    reordered.reverse();
    assert_ne!(line_fingerprint(&lines), line_fingerprint(&reordered));
    let mut altered = lines.clone();
    altered[1].base_credit_minor = 101;
    assert_ne!(line_fingerprint(&lines), line_fingerprint(&altered));
}

#[test]
fn wire_normalizes_only_the_bounded_body_and_ignores_unknown_fields() {
    let body = br#"  {"journals":[{"id":"j1","expectedVersion":1},{"id":"j2","expectedVersion":2}],"reason":"  Reviewed by finance controller  ","permissions":["*"],"request_hash":"attacker"}"#;
    let normalized = normalize_journal_batch(body).unwrap();
    assert_eq!(
        normalized.normalized_json(),
        r#"{"journals":[{"id":"j1","expectedVersion":1},{"id":"j2","expectedVersion":2}],"reason":"Reviewed by finance controller"}"#
    );
    let command = normalized
        .into_command(CommandKey::new("journal-batch-unit-key").unwrap())
        .unwrap();
    assert_eq!(
        command.command().input.reason,
        "Reviewed by finance controller"
    );
    assert_eq!(command.command().input.journals.len(), 2);

    for bad in [
        br#"{"journals":[],"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"journals":[{"id":"j","expectedVersion":1}],"reason":"short"}"#.as_slice(),
        br#"{"journals":[{"id":"j","expectedVersion":0}],"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"journals":[{"id":"j","expectedVersion":9007199254740992}],"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"journals":[{"id":"j","expectedVersion":1.5}],"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"journals":[{"id":"j"}],"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"reason":"Reviewed by finance controller"}"#.as_slice(),
        br#"{"journals":[{"id":"j","expectedVersion":1}],"reason":null}"#.as_slice(),
    ] {
        assert!(normalize_journal_batch(bad).is_err(), "{}", String::from_utf8_lossy(bad));
    }
}

#[derive(Default)]
struct Mock {
    events: Vec<&'static str>,
    snapshots: HashMap<String, JournalBatchSnapshot>,
    replay: Option<JournalBatchResult>,
    posted: Vec<String>,
    commits: Vec<JournalBatchResult>,
    digests: Vec<String>,
    fail_post: bool,
}

impl JournalBatchPorts for Mock {
    type Error = JournalError;
    type Authority = ();

    async fn lock_sources(
        &mut self,
        _: &erpx_finance_server::public::FinanceActor,
        _: &JournalBatchCommand,
    ) -> Result<(), Self::Error> {
        self.events.push("lock");
        Ok(())
    }
    async fn authorize_current(
        &mut self,
        _: &erpx_finance_server::public::FinanceActor,
        _: &JournalBatchCommand,
    ) -> Result<(), Self::Error> {
        self.events.push("authorize");
        Ok(())
    }
    async fn ready(&mut self, _: &(), _: &JournalBatchCommand) -> Result<(), Self::Error> {
        self.events.push("ready");
        Ok(())
    }
    async fn replay_exact(
        &mut self,
        _: &(),
        _: &JournalBatchCommand,
    ) -> Result<Option<JournalBatchResult>, Self::Error> {
        self.events.push("replay");
        Ok(self.replay.clone())
    }
    async fn load_journal(
        &mut self,
        _: &(),
        id: &str,
    ) -> Result<Option<JournalBatchSnapshot>, Self::Error> {
        self.events.push("load");
        Ok(self.snapshots.get(id).cloned())
    }
    fn digest(&self, payload: &str) -> String {
        payload.len().to_string()
    }
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
    async fn post(
        &mut self,
        _: &(),
        child: JournalReviewCommand,
    ) -> Result<JournalReviewResult, Self::Error> {
        self.events.push("post");
        if self.fail_post {
            return Err(JournalError::new("CHILD_POST_FAILED", "child failed"));
        }
        let JournalReviewInput::Post { expected_version } = child.input else {
            panic!("batch must only issue post children");
        };
        self.posted.push(child.key.as_str().into());
        Ok(JournalReviewResult::Posted {
            id: child.journal_id.as_str().into(),
            status: "posted".into(),
            version: expected_version + 1,
        })
    }
    async fn commit(
        &mut self,
        _: &(),
        _: &JournalBatchCommand,
        result: &JournalBatchResult,
        plans: &[BatchItemPlan],
    ) -> Result<(), Self::Error> {
        self.events.push("commit");
        assert_eq!(plans.len(), result.journals.len());
        for plan in plans {
            self.digests.push(plan.evidence_digest.clone());
        }
        self.commits.push(result.clone());
        Ok(())
    }
    async fn require_final_authority(
        &mut self,
        _: &(),
        _: &JournalBatchCommand,
    ) -> Result<(), Self::Error> {
        self.events.push("final-authority");
        Ok(())
    }
}

use erpx_finance_server::public::journal_review::JournalReviewInput;

fn actor() -> erpx_finance_server::public::FinanceActor {
    erpx_finance_server::public::FinanceActor {
        holding_id: erpx_platform_shared_server::public::HoldingId::new("holding").unwrap(),
        company_id: erpx_platform_shared_server::public::CompanyId::new("company").unwrap(),
        user_id: erpx_platform_shared_server::public::UserId::new("reviewer").unwrap(),
        assignment_id: erpx_platform_shared_server::public::AssignmentId::new("assignment").unwrap(),
    }
}

fn mock() -> Mock {
    let mut mock = Mock::default();
    mock.snapshots.insert("j1".into(), snapshot("j1", "chart"));
    mock.snapshots.insert("j2".into(), snapshot("j2", "chart"));
    mock
}

#[tokio::test]
async fn orchestration_posts_each_child_then_commits_one_batch_result() {
    let cmd = command(&["j1", "j2"]);
    let mut ports = mock();
    let result = post_journal_batch(&mut ports, &actor(), &cmd)
        .await
        .unwrap();
    assert_eq!(result.id, "journal-batch-journal-batch-unit-key");
    assert_eq!(result.status, "posted");
    assert_eq!(result.line_count, 4);
    assert_eq!(
        result.journals,
        vec![
            PostedJournal {
                id: "j1".into(),
                version: 2
            },
            PostedJournal {
                id: "j2".into(),
                version: 2
            },
        ]
    );
    assert_eq!(
        ports.posted,
        vec!["journal-batch-unit-key:0", "journal-batch-unit-key:1"]
    );
    assert_eq!(ports.commits.len(), 1);
    assert_eq!(ports.digests.len(), 2);
    assert_eq!(
        ports.events,
        vec![
            "lock",
            "authorize",
            "ready",
            "replay",
            "load",
            "load",
            "post",
            "post",
            "commit",
            "final-authority"
        ]
    );
}

#[tokio::test]
async fn exact_batch_replay_returns_history_without_posting_or_committing_again() {
    let cmd = command(&["j1"]);
    let mut ports = mock();
    ports.replay = Some(JournalBatchResult {
        id: cmd.batch_id.as_str().into(),
        status: "posted".into(),
        journals: vec![PostedJournal {
            id: "j1".into(),
            version: 2,
        }],
        line_count: 2,
    });
    let result = post_journal_batch(&mut ports, &actor(), &cmd)
        .await
        .unwrap();
    assert_eq!(result.line_count, 2);
    assert!(ports.posted.is_empty());
    assert!(ports.commits.is_empty());
    assert_eq!(
        ports.events,
        vec!["lock", "authorize", "ready", "replay", "final-authority"]
    );
}

#[tokio::test]
async fn a_failed_child_or_scope_mismatch_never_commits_the_batch() {
    let cmd = command(&["j1", "j2"]);
    let mut ports = mock();
    ports.fail_post = true;
    assert_eq!(
        post_journal_batch(&mut ports, &actor(), &cmd)
            .await
            .unwrap_err()
            .code,
        "CHILD_POST_FAILED"
    );
    assert!(ports.commits.is_empty());

    let mut ports = mock();
    ports.snapshots.get_mut("j2").unwrap().chart_id = "other".into();
    assert_eq!(
        post_journal_batch(&mut ports, &actor(), &cmd)
            .await
            .unwrap_err()
            .code,
        "BATCH_SCOPE_MISMATCH"
    );
    assert!(ports.posted.is_empty());
    assert!(ports.commits.is_empty());

    let mut ports = mock();
    ports.snapshots.remove("j2");
    assert_eq!(
        post_journal_batch(&mut ports, &actor(), &cmd)
            .await
            .unwrap_err()
            .kind,
        FailureKind::NotFound
    );
}
