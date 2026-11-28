//! Same-unit assembly of Quality Count and Identity witnesses for Collaboration.
//! This source does not grant command authority or commit any transaction.
use erpx_collaboration_server::public::quality_notification::{
    PgQualityCountNoticeSource, QualityCountNoticeFacts, QualityCountNoticeKind,
};
use erpx_identity_access_server::public::quality_recipients::CurrentQualityRecipients;
use erpx_inventory_server::public::counts::CurrentInventoryQualityCount;
use erpx_quality_server::public::quality_commands::{
    QualityCountChildNoticeProof, QualityCountRequesterNoticeProof,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_COUNT_NOTICE_SOURCE_CHANGED",
        "The Quality count notice source changed.",
    )
    .into()
}

/// Facts are derived from actual retained owner witnesses, never from a request.
pub struct QualityCountChildNoticeSource {
    proof: QualityCountChildNoticeProof<CurrentInventoryQualityCount>,
    audience: Arc<CurrentQualityRecipients>,
    facts: QualityCountNoticeFacts,
}
impl QualityCountChildNoticeSource {
    pub fn new(
        proof: QualityCountChildNoticeProof<CurrentInventoryQualityCount>,
        audience: Arc<CurrentQualityRecipients>,
    ) -> Result<Self, Error> {
        let owner = proof.facts();
        let (company, branch, now) = audience.count_reader_scope().ok_or_else(changed)?;
        if company.as_str() != owner.company_id
            || branch.as_str() != owner.branch_id
            || now != owner.now
        {
            return Err(changed());
        }
        let facts = QualityCountNoticeFacts {
            kind: QualityCountNoticeKind::RemainingChild,
            command_key: owner.command_key.clone(),
            company_id: owner.company_id.clone(),
            hold_id: proof.child_id().to_owned(),
            version: 1,
            created_at: owner.now,
            recipients: audience.recipients().to_vec(),
        };
        Ok(Self {
            proof,
            audience,
            facts,
        })
    }
}
impl PgQualityCountNoticeSource for QualityCountChildNoticeSource {
    fn facts(&self) -> &QualityCountNoticeFacts {
        &self.facts
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.proof.require_current(c).await?;
        self.audience.require_current(c).await?;
        let (company, branch, now) = self.audience.count_reader_scope().ok_or_else(changed)?;
        let owner = self.proof.facts();
        if company.as_str() != owner.company_id
            || branch.as_str() != owner.branch_id
            || now != owner.now
            || self.audience.recipients() != self.facts.recipients
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// The requester and parent version come only from finalized Quality evidence.
pub struct QualityCountRequesterNoticeSource {
    proof: QualityCountRequesterNoticeProof<CurrentInventoryQualityCount>,
    facts: QualityCountNoticeFacts,
}
impl QualityCountRequesterNoticeSource {
    pub fn new(proof: QualityCountRequesterNoticeProof<CurrentInventoryQualityCount>) -> Self {
        let owner = proof.facts();
        let facts = QualityCountNoticeFacts {
            kind: QualityCountNoticeKind::Requester,
            command_key: owner.command_key.clone(),
            company_id: owner.company_id.clone(),
            hold_id: owner.hold_id.clone(),
            version: proof.version(),
            created_at: owner.now,
            recipients: vec![proof.requester().to_owned()],
        };
        Self { proof, facts }
    }
}
impl PgQualityCountNoticeSource for QualityCountRequesterNoticeSource {
    fn facts(&self) -> &QualityCountNoticeFacts {
        &self.facts
    }
    async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.proof.require_current(c).await
    }
}
