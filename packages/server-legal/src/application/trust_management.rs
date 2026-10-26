use std::future::Future;

use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, RecordId, TimestampMillis, UserId,
};

use crate::domain::{
    pdf_verification::{TrustMaterialKind, TrustMaterialState},
    trust_management::{
        InspectedTrustMaterial, LegalTrustActor, LegalTrustCommand, LegalTrustReceipt,
        LegalTrustRecord,
    },
};

pub trait LegalTrustInspector {
    fn inspect(
        &self,
        kind: TrustMaterialKind,
        content: &str,
        now: TimestampMillis,
    ) -> Result<InspectedTrustMaterial, BusinessError>;
}

/// All reads use the calling owner's transaction and current authority sources.
/// Replays still require current manage permission. Same-person checks use the
/// Identity owner's person identity, not merely unequal account/user IDs.
pub trait LegalTrustCommandPorts {
    type Error: From<BusinessError> + Send;
    fn authorize_manage(
        &mut self,
        actor: &LegalTrustActor,
    ) -> impl Future<Output = Result<String, Self::Error>> + Send;
    fn replay(
        &mut self,
        actor: &LegalTrustActor,
        key: &CommandKey,
        command: &LegalTrustCommand,
    ) -> impl Future<Output = Result<Option<LegalTrustReceipt>, Self::Error>> + Send;
    fn lock_company(
        &mut self,
        actor: &LegalTrustActor,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn material(
        &mut self,
        actor: &LegalTrustActor,
        id: &RecordId,
    ) -> impl Future<Output = Result<Option<LegalTrustRecord>, Self::Error>> + Send;
    fn same_person(
        &mut self,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn active_count(
        &mut self,
        actor: &LegalTrustActor,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}

pub enum PreparedLegalTrustCommand {
    Replay(LegalTrustReceipt),
    Change(Box<LegalTrustChangePlan>),
}

pub struct LegalTrustChangePlan {
    pub(crate) actor: LegalTrustActor,
    pub(crate) key: CommandKey,
    pub(crate) authority_witness: String,
    pub(crate) now: TimestampMillis,
    pub(crate) change: LegalTrustChange,
    pub(crate) receipt: LegalTrustReceipt,
}

pub(crate) enum LegalTrustChange {
    Add {
        id: RecordId,
        kind: TrustMaterialKind,
        name: String,
        source: String,
        material: InspectedTrustMaterial,
    },
    Decision {
        source: LegalTrustRecord,
        state: TrustMaterialState,
    },
}

impl LegalTrustChangePlan {
    pub fn actor(&self) -> &LegalTrustActor {
        &self.actor
    }
    pub fn command_key(&self) -> &CommandKey {
        &self.key
    }
    pub fn authority_witness(&self) -> &str {
        &self.authority_witness
    }
    pub fn receipt(&self) -> &LegalTrustReceipt {
        &self.receipt
    }
}

/// Prepare does not commit. Persistence, command receipt, audit and any outbox
/// effects must share the existing Holding unit of work and retained Control lease.
pub async fn prepare_legal_trust_command<Ports: LegalTrustCommandPorts>(
    ports: &mut Ports,
    inspector: &impl LegalTrustInspector,
    clock: &impl Clock,
    actor: &LegalTrustActor,
    key: &CommandKey,
    command: &LegalTrustCommand,
) -> Result<PreparedLegalTrustCommand, Ports::Error> {
    let authority_witness = ports.authorize_manage(actor).await?;
    if actor.branch_id.is_some() || actor.department_id.is_some() {
        return Err(invalid("LEGAL_INVALID", "Manage trust at Company scope.").into());
    }
    if authority_witness.is_empty() || authority_witness.len() > 200 {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "LEGAL_AUTHORITY_WITNESS_REQUIRED",
            "Current Legal authority evidence is required.",
        )
        .into());
    }
    if let Some(receipt) = ports.replay(actor, key, command).await? {
        if receipt.id != actor.company_id || receipt.version != 1 {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "LEGAL_COMMAND_RECEIPT_INVALID",
                "The command receipt does not match this Company.",
            )
            .into());
        }
        return Ok(PreparedLegalTrustCommand::Replay(receipt));
    }
    ports.lock_company(actor).await?;
    let now = clock.now();
    let change = match command {
        LegalTrustCommand::Add {
            kind,
            name,
            source,
            content,
        } => {
            let name = name.trim();
            if !(3..=200).contains(&name.encode_utf16().count())
                || !(20..=2_000_000).contains(&content.encode_utf16().count())
            {
                return Err(invalid(
                    "LEGAL_INVALID",
                    "Use a name of 3–200 characters and bounded trust content.",
                )
                .into());
            }
            let source_url = url::Url::parse(source).map_err(|_| {
                Ports::Error::from(invalid("LEGAL_INVALID", "Use an HTTPS source URL."))
            })?;
            if source_url.scheme() != "https" {
                return Err(invalid("LEGAL_INVALID", "Use an HTTPS source URL.").into());
            }
            if ports.active_count(actor).await? >= 100 {
                return Err(invalid(
                    "LEGAL_TRUST_LIMIT",
                    "Retire obsolete trust material before adding more.",
                )
                .into());
            }
            let material = inspector
                .inspect(*kind, content, now)
                .map_err(Ports::Error::from)?;
            if material.content.len() > 2_000_000
                || material.checksum_sha256.len() != 64
                || !material
                    .checksum_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(invalid("LEGAL_INVALID", "Trust material inspection failed.").into());
            }
            LegalTrustChange::Add {
                id: RecordId::new(key.as_str()).map_err(Ports::Error::from)?,
                kind: *kind,
                name: name.to_owned(),
                source: source.clone(),
                material,
            }
        }
        LegalTrustCommand::Approve { id, version } | LegalTrustCommand::Retire { id, version } => {
            let source = ports
                .material(actor, id)
                .await?
                .ok_or_else(|| Ports::Error::from(changed()))?;
            if *version == 0 || source.version != *version || source.company_id != actor.company_id
            {
                return Err(changed().into());
            }
            let state = if matches!(command, LegalTrustCommand::Approve { .. }) {
                if source.state != TrustMaterialState::Proposed
                    || source.created_by == actor.user_id
                    || ports.same_person(actor, &source.created_by).await?
                {
                    return Err(independent_review().into());
                }
                TrustMaterialState::Approved
            } else {
                TrustMaterialState::Retired
            };
            LegalTrustChange::Decision { source, state }
        }
    };
    Ok(PreparedLegalTrustCommand::Change(Box::new(
        LegalTrustChangePlan {
            actor: actor.clone(),
            key: key.clone(),
            authority_witness,
            now,
            change,
            receipt: LegalTrustReceipt {
                id: actor.company_id.clone(),
                version: 1,
            },
        },
    )))
}

pub(crate) fn independent_review() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_INDEPENDENT_REVIEW",
        "A different person must verify the source and fingerprint.",
    )
}
pub(crate) fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "LEGAL_VERSION_CONFLICT",
        "Trust material changed. Refresh.",
    )
}
fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
