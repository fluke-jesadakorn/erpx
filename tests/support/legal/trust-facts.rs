use erpx_server_legal::public::{
    InspectedTrustMaterial, LegalTrustActor, LegalTrustCommand, LegalTrustCommandPorts,
    LegalTrustInspector, LegalTrustReceipt, LegalTrustRecord, TrustMaterialKind,
    TrustMaterialState,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
    UserId,
};

pub fn actor() -> LegalTrustActor {
    LegalTrustActor {
        holding_id: HoldingId::new("holding-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        user_id: UserId::new("reviewer").unwrap(),
        branch_id: None,
        department_id: None,
    }
}
pub fn material() -> LegalTrustRecord {
    LegalTrustRecord {
        id: RecordId::new("material-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        version: 1,
        kind: TrustMaterialKind::Root,
        state: TrustMaterialState::Proposed,
        created_by: UserId::new("creator").unwrap(),
        checksum_sha256: "a".repeat(64),
    }
}
pub fn addition() -> LegalTrustCommand {
    LegalTrustCommand::Add {
        kind: TrustMaterialKind::Root,
        name: "  Reviewed synthetic root  ".to_owned(),
        source: "https://unreachable.invalid/operator-source".to_owned(),
        content: "Synthetic material for application ports".to_owned(),
    }
}

pub struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_793_080_800_000)
    }
}

/// Explicit unit-test facts, never an implementation of production authority.
pub struct TrustFacts {
    pub allow_manage: bool,
    pub same_person: bool,
    pub active_count: u64,
    pub source: Option<LegalTrustRecord>,
    pub replay: Option<LegalTrustReceipt>,
    pub calls: Vec<&'static str>,
}
impl TrustFacts {
    pub fn authorized_company() -> Self {
        Self {
            allow_manage: true,
            same_person: false,
            active_count: 0,
            source: Some(material()),
            replay: None,
            calls: Vec::new(),
        }
    }
}
impl LegalTrustCommandPorts for TrustFacts {
    type Error = BusinessError;
    async fn authorize_manage(&mut self, _: &LegalTrustActor) -> Result<String, Self::Error> {
        self.calls.push("authorize");
        if !self.allow_manage {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_PERMISSION_REQUIRED",
                "Current Legal manage permission is required.",
            ));
        }
        Ok("current-authority-1".to_owned())
    }
    async fn replay(
        &mut self,
        _: &LegalTrustActor,
        _: &CommandKey,
        _: &LegalTrustCommand,
    ) -> Result<Option<LegalTrustReceipt>, Self::Error> {
        self.calls.push("replay");
        Ok(self.replay.clone())
    }
    async fn lock_company(&mut self, _: &LegalTrustActor) -> Result<(), Self::Error> {
        self.calls.push("lock");
        Ok(())
    }
    async fn material(
        &mut self,
        _: &LegalTrustActor,
        _: &RecordId,
    ) -> Result<Option<LegalTrustRecord>, Self::Error> {
        self.calls.push("material");
        Ok(self.source.clone())
    }
    async fn same_person(&mut self, _: &LegalTrustActor, _: &UserId) -> Result<bool, Self::Error> {
        self.calls.push("same-person");
        Ok(self.same_person)
    }
    async fn active_count(&mut self, _: &LegalTrustActor) -> Result<u64, Self::Error> {
        self.calls.push("count");
        Ok(self.active_count)
    }
}

pub struct SyntheticInspector;
impl LegalTrustInspector for SyntheticInspector {
    fn inspect(
        &self,
        _: TrustMaterialKind,
        _: &str,
        _: TimestampMillis,
    ) -> Result<InspectedTrustMaterial, BusinessError> {
        Ok(InspectedTrustMaterial {
            content: vec![0x30, 0x00],
            checksum_sha256: "a".repeat(64),
        })
    }
}
