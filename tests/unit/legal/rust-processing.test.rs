use erpx_server_legal::public::{
    LegalProcessingActor, LegalProcessingCommandPorts, LegalProcessingDocument, LegalProcessingJob,
    LegalProcessingKind, LegalProcessingLease, LegalProcessingQueueAuthority,
    LegalProcessingReceipt, PreparedLegalProcessingQueue, queue_legal_processing,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn actor() -> LegalProcessingActor {
    LegalProcessingActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("user").unwrap(),
        selection_revision: "selection-current".into(),
    }
}
fn document() -> LegalProcessingDocument {
    LegalProcessingDocument {
        item_id: id("item"),
        document_id: id("document"),
        checksum_sha256: "a".repeat(64),
        item_version: 1,
    }
}
fn receipt() -> LegalProcessingReceipt {
    LegalProcessingReceipt {
        id: id("item"),
        job_id: id("processing-key"),
        state: "queued".into(),
    }
}
struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(100_000)
    }
}
fn denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_PERMISSION_REQUIRED",
        "Current permission required.",
    )
}
struct Ports {
    calls: Vec<&'static str>,
    authorized: bool,
    replay: Option<LegalProcessingReceipt>,
    count: u64,
    changed: bool,
    snapshot: String,
    document: LegalProcessingDocument,
}
impl Default for Ports {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            authorized: true,
            replay: None,
            count: 0,
            changed: false,
            snapshot: r#"{"assignmentId":"current"}"#.into(),
            document: document(),
        }
    }
}
impl LegalProcessingCommandPorts for Ports {
    type Error = BusinessError;
    async fn prepare(
        &mut self,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<PreparedLegalProcessingQueue, Self::Error> {
        self.calls.push("authorize-current");
        if !self.authorized {
            return Err(denied());
        }
        Ok(PreparedLegalProcessingQueue {
            document: self.document.clone(),
            authority: LegalProcessingQueueAuthority {
                revision: "current".into(),
                actor_snapshot_json: self.snapshot.clone(),
            },
        })
    }
    async fn replay(
        &mut self,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &RecordId,
        _: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Self::Error> {
        self.calls.push("replay");
        Ok(self.replay.clone())
    }
    async fn recent_count(&mut self, since: i64) -> Result<u64, Self::Error> {
        self.calls.push("rate");
        assert_eq!(since, 40_000);
        Ok(self.count)
    }
    async fn revalidate(
        &mut self,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: LegalProcessingKind,
        _: &PreparedLegalProcessingQueue,
    ) -> Result<(), Self::Error> {
        self.calls.push("revalidate");
        if self.changed { Err(denied()) } else { Ok(()) }
    }
    async fn persist(
        &mut self,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: LegalProcessingKind,
        _: &PreparedLegalProcessingQueue,
        result: &LegalProcessingReceipt,
        now: i64,
    ) -> Result<(), Self::Error> {
        self.calls.push("persist-atomically");
        assert_eq!(*result, receipt());
        assert_eq!(now, 100_000);
        Ok(())
    }
}
async fn queue(ports: &mut Ports) -> Result<LegalProcessingReceipt, BusinessError> {
    queue_legal_processing(
        ports,
        &FixedClock,
        &actor(),
        &CommandKey::new("processing-key").unwrap(),
        &id("document"),
        LegalProcessingKind::Verify,
    )
    .await
}

#[tokio::test]
async fn current_authority_is_required_before_replay() {
    let mut ports = Ports {
        authorized: false,
        replay: Some(receipt()),
        ..Default::default()
    };
    assert_eq!(queue(&mut ports).await.unwrap_err(), denied());
    assert_eq!(ports.calls, ["authorize-current"]);
}

#[tokio::test]
async fn authorized_replay_does_not_consume_quota_or_insert_again() {
    let mut ports = Ports {
        replay: Some(receipt()),
        count: 20,
        ..Default::default()
    };
    assert_eq!(queue(&mut ports).await.unwrap(), receipt());
    assert_eq!(ports.calls, ["authorize-current", "replay"]);
    ports.replay.as_mut().unwrap().job_id = id("another-job");
    assert_eq!(
        queue(&mut ports).await.unwrap_err().code,
        "LEGAL_PROCESSING_SCOPE_MISMATCH"
    );
}

#[tokio::test]
async fn rate_limit_and_current_revalidation_guard_new_work() {
    let mut ports = Ports {
        count: 20,
        ..Default::default()
    };
    assert_eq!(
        queue(&mut ports).await.unwrap_err().code,
        "LEGAL_PROCESSING_RATE_LIMIT"
    );
    assert!(!ports.calls.contains(&"persist-atomically"));
    ports.count = 19;
    ports.calls.clear();
    queue(&mut ports).await.unwrap();
    assert_eq!(
        ports.calls,
        [
            "authorize-current",
            "replay",
            "rate",
            "revalidate",
            "persist-atomically"
        ]
    );
    ports.changed = true;
    ports.calls.clear();
    assert_eq!(queue(&mut ports).await.unwrap_err(), denied());
    assert!(!ports.calls.contains(&"persist-atomically"));
}

#[tokio::test]
async fn corrupted_source_or_oversized_historical_actor_cannot_reach_workflow_replay() {
    for snapshot in ["[]".to_owned(), " ".repeat(1024 * 1024 + 1)] {
        let mut ports = Ports {
            snapshot,
            ..Default::default()
        };
        assert_eq!(
            queue(&mut ports).await.unwrap_err().code,
            "LEGAL_PROCESSING_ACTOR_INVALID"
        );
        assert_eq!(ports.calls, ["authorize-current"]);
    }
    let mut ports = Ports::default();
    ports.document.checksum_sha256 = "broken".into();
    assert_eq!(
        queue(&mut ports).await.unwrap_err().code,
        "LEGAL_PROCESSING_SOURCE_INVALID"
    );
    assert_eq!(ports.calls, ["authorize-current"]);
}

#[test]
fn leases_bind_holding_company_attempt_and_expiry() {
    let actor = actor();
    let job = LegalProcessingJob {
        id: id("job"),
        holding_id: actor.holding_id.clone(),
        company_id: actor.company_id.clone(),
        document: document(),
        kind: LegalProcessingKind::Verify,
        actor_snapshot_json: "{}".into(),
        created_by: actor.user_id,
        created_at: 0,
    };
    let mut lease = LegalProcessingLease {
        job,
        token: id("token"),
        until: 1000,
        attempts: 1,
    };
    lease
        .require_live(&actor.holding_id, &actor.company_id, 1000)
        .unwrap();
    assert_eq!(
        lease
            .require_live(&actor.holding_id, &actor.company_id, 1001)
            .unwrap_err()
            .code,
        "LEGAL_PROCESSING_LEASE_CHANGED"
    );
    assert_eq!(
        lease
            .require_live(&HoldingId::new("other").unwrap(), &actor.company_id, 999)
            .unwrap_err()
            .code,
        "LEGAL_PROCESSING_SCOPE_MISMATCH"
    );
    for attempts in [0, 4] {
        lease.attempts = attempts;
        assert!(
            lease
                .require_live(&actor.holding_id, &actor.company_id, 999)
                .is_err()
        );
    }
}
