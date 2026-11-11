//! Files-owned adapter for the Learning completion/waiver evidence port.
//!
//! Learning supplies the exact `learning_assignment` target, expected version
//! and command identity. Files reuses the shared business-evidence binding over
//! the existing `attachment_sets`/`attachment_links`/`attachments` ledger, so
//! there is one scan/quarantine implementation and no Learning row or byte
//! storage here. The caller's serializable transaction is required and never
//! committed by this adapter.
use crate::adapters::business_evidence::{
    PgBusinessEvidenceFiles, PreparedBusinessEvidence, PreparedEvidenceLink, evidence_context,
    policy_fact,
};
use erpx_server_learning::public::learning::{
    ASSIGNMENT_EVIDENCE_BINDING_TYPE, ASSIGNMENT_EVIDENCE_COMPONENT_KIND,
    ASSIGNMENT_EVIDENCE_PURPOSE, ASSIGNMENT_EVIDENCE_TARGET_TYPE, EvidenceManifestItem,
    LearningEvidencePort, LearningEvidenceRequest, LearningFuture, PreparedLearningEvidence,
};
use erpx_server_records::public::business_evidence::{
    BusinessEvidenceRequest, prepare_business_evidence,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// Files-owned policy version for the Learning assignment certificate action.
/// Learning file sets are created with this exact version.
pub const LEARNING_EVIDENCE_POLICY_VERSION: i64 = 1;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_LEARNING_EVIDENCE_UNAVAILABLE",
        "The learning completion evidence is temporarily unavailable.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEARNING_EVIDENCE_CHANGED",
        "The learning completion evidence changed while it was being retained.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// The exact Files policy facts Learning registers for completion and waiver.
fn policies() -> Vec<erpx_server_records::public::business_evidence::EvidencePolicyFact> {
    vec![policy_fact(
        ASSIGNMENT_EVIDENCE_TARGET_TYPE,
        ASSIGNMENT_EVIDENCE_PURPOSE,
        &[ASSIGNMENT_EVIDENCE_COMPONENT_KIND],
        LEARNING_EVIDENCE_POLICY_VERSION,
    )]
}

/// Translate a Learning request into the shared business-evidence request.
/// Only the exact assignment/certificate/action/completion|waiver shape is
/// accepted; anything else fails closed before any Files row is read.
pub fn learning_business_request(
    request: &LearningEvidenceRequest,
) -> Result<BusinessEvidenceRequest, BusinessError> {
    request.validate()?;
    if request.target_type != ASSIGNMENT_EVIDENCE_TARGET_TYPE
        || request.purpose_key != ASSIGNMENT_EVIDENCE_PURPOSE
        || request.component_kind != ASSIGNMENT_EVIDENCE_COMPONENT_KIND
        || !matches!(
            request.component_ref.as_deref(),
            Some("completion" | "waiver")
        )
        || request.binding_type != ASSIGNMENT_EVIDENCE_BINDING_TYPE
    {
        return Err(BusinessError::invalid(
            "LEARNING_EVIDENCE_INVALID",
            "Choose current completion or waiver evidence for this learning assignment.",
        ));
    }
    Ok(BusinessEvidenceRequest {
        attachment_set_ids: request.attachment_set_ids.clone(),
        target_types: vec![ASSIGNMENT_EVIDENCE_TARGET_TYPE.to_owned()],
        contexts: vec![evidence_context(
            ASSIGNMENT_EVIDENCE_PURPOSE,
            ASSIGNMENT_EVIDENCE_COMPONENT_KIND,
            request.component_ref.as_deref(),
        )],
        target_id: request.target_id.clone(),
        expected_target_version: request.expected_target_version,
        command_id: request.command_id.clone(),
        binding_type: ASSIGNMENT_EVIDENCE_BINDING_TYPE.to_owned(),
        binding_id: request.binding_id.clone(),
    })
}

fn prepared_business(
    prepared: &PreparedLearningEvidence,
) -> Result<PreparedBusinessEvidence, BusinessError> {
    if prepared.target_type != ASSIGNMENT_EVIDENCE_TARGET_TYPE
        || prepared.set_ids.is_empty()
        || prepared.link_ids.is_empty()
        || prepared.manifest.len() != prepared.link_ids.len()
        || prepared.policy_version != LEARNING_EVIDENCE_POLICY_VERSION
    {
        return Err(changed());
    }
    Ok(PreparedBusinessEvidence {
        set_ids: prepared.set_ids.clone(),
        link_ids: prepared.link_ids.clone(),
        manifest: prepared
            .manifest
            .iter()
            .map(|item| PreparedEvidenceLink {
                set_id: item.set_id.clone(),
                link_id: item.link_id.clone(),
                source_id: item.source_id.clone(),
                purpose_key: item.purpose_key.clone(),
                component_kind: item.component_kind.clone(),
                component_ref: item.component_ref.clone(),
                checksum_sha256: item.checksum_sha256.clone(),
                media_type: item.media_type.clone(),
                bytes: item.bytes,
            })
            .collect(),
        target_type: prepared.target_type.clone(),
        target_id: prepared.target_id.clone(),
        expected_target_version: prepared.expected_target_version,
        policy_version: prepared.policy_version,
        binding_type: prepared.binding_type.clone(),
        binding_id: prepared.binding_id.clone(),
        command_id: prepared.command_id.clone(),
    })
}

fn prepared_learning(prepared: PreparedBusinessEvidence) -> PreparedLearningEvidence {
    PreparedLearningEvidence {
        set_ids: prepared.set_ids,
        link_ids: prepared.link_ids,
        manifest: prepared
            .manifest
            .into_iter()
            .map(|item| EvidenceManifestItem {
                set_id: item.set_id,
                link_id: item.link_id,
                source_id: item.source_id,
                purpose_key: item.purpose_key,
                component_kind: item.component_kind,
                component_ref: item.component_ref,
                checksum_sha256: item.checksum_sha256,
                media_type: item.media_type,
                bytes: item.bytes,
            })
            .collect(),
        target_type: prepared.target_type,
        target_id: prepared.target_id,
        expected_target_version: prepared.expected_target_version,
        policy_version: prepared.policy_version,
        binding_type: prepared.binding_type,
        binding_id: prepared.binding_id,
        command_id: prepared.command_id,
    }
}

/// Files-owned Learning evidence adapter. The caller supplies the current
/// Company/actor/scope and the retained transaction clock; the adapter only
/// prepares and re-reads the exact Files ledger rows.
pub struct PgLearningEvidence {
    company: CompanyId,
    actor_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    now: TimestampMillis,
}

impl PgLearningEvidence {
    pub fn new(
        company: CompanyId,
        actor_id: String,
        branch_id: Option<String>,
        department_id: Option<String>,
        now: TimestampMillis,
    ) -> Self {
        Self {
            company,
            actor_id,
            branch_id,
            department_id,
            now,
        }
    }

    fn business<'c>(&self, connection: &'c mut PgConnection) -> PgBusinessEvidenceFiles<'c> {
        PgBusinessEvidenceFiles::new(
            connection,
            self.company.clone(),
            self.actor_id.clone(),
            self.branch_id.clone(),
            self.department_id.clone(),
            policies(),
            self.now,
        )
    }
}

impl LearningEvidencePort for PgLearningEvidence {
    fn prepare<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: LearningEvidenceRequest,
    ) -> LearningFuture<'a, Option<PreparedLearningEvidence>, BusinessError> {
        Box::pin(async move {
            let request = learning_business_request(&request)?;
            let prepared = prepare_business_evidence(&mut self.business(connection), request)
                .await
                .map_err(business)?;
            Ok(prepared.map(prepared_learning))
        })
    }

    fn require_current<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        prepared: &'a PreparedLearningEvidence,
    ) -> LearningFuture<'a, (), BusinessError> {
        Box::pin(async move {
            let prepared = prepared_business(prepared)?;
            self.business(connection)
                .require_current(&prepared)
                .await
                .map_err(business)
        })
    }
}
