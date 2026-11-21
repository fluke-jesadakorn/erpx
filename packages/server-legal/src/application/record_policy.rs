use crate::domain::record_policy::*;
use erpx_server_shared::public::{BusinessError, FailureKind};

/// Exact existing visibility: a broad role never substitutes for confidential
/// membership, and Legal requesters see only their own request aggregates.
pub fn authorize_legal_record(
    actor: &LegalRecordActor,
    request: &LegalRecordRequest,
    facts: &LegalRecordFacts,
    release: &LegalReleaseObservation,
) -> Result<Vec<LegalPurchaseRequirement>, BusinessError> {
    if !actor.permissions.contains(request.operation.permission()) {
        return Err(policy_error(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "Your selected role does not grant this permission.",
        ));
    }
    let item = &facts.item;
    if item.id != request.item_id
        || item.company_id != actor.company_id
        || item.version < 1
        || actor
            .branch_id
            .as_ref()
            .is_some_and(|id| item.branch_id.as_ref() != Some(id))
        || actor
            .department_id
            .as_ref()
            .is_some_and(|id| item.department_id.as_ref() != Some(id))
        || (item.classification != "internal" && !facts.member)
        || (actor.role == "legal_requester"
            && (item.kind != "request" || item.created_by != actor.user_id))
    {
        return Err(not_found());
    }
    let capability = legal_kind_capability(&item.kind)?;
    if request.operation.permission() != "legal.read" && !facts.enabled {
        return Err(policy_error(
            FailureKind::Conflict,
            "LEGAL_DISABLED",
            "Legal is read-only until enabled for this Company.",
        ));
    }
    if request.operation == LegalRecordOperation::Edit && !facts.editor {
        return Err(policy_error(
            FailureKind::Forbidden,
            "LEGAL_EDITOR_REQUIRED",
            "An editor membership is required.",
        ));
    }
    if request.operation.document() {
        if !matches!((&request.document_id,&facts.document),(Some(id),Some(document)) if document.id==*id&&document.item_id==item.id)
        {
            return Err(not_found());
        }
        if facts.disposed {
            return Err(policy_error(
                FailureKind::Conflict,
                "LEGAL_DOCUMENT_DISPOSED",
                "Document content was destroyed under an approved retention policy. Metadata and destruction evidence remain.",
            ));
        }
        if facts.destruction_executing {
            return Err(policy_error(
                FailureKind::Conflict,
                "LEGAL_RETENTION_FROZEN",
                "Document retention is in progress. Review or resume the preserved manifest before reading content.",
            ));
        }
    }
    let preparation = actor.branch_id.is_none()
        && actor.department_id.is_none()
        && actor.permissions.contains("legal.manage")
        && matches!(item.kind.as_str(), "rule" | "template");
    if request.mode == LegalCommercialMode::Enforced && !release.allowed && !preparation {
        return Err(policy_error(
            FailureKind::Forbidden,
            "FEATURE_COMING_SOON",
            "Legal is unavailable for this Company.",
        ));
    }
    if request.operation.processing()
        && (!facts.enabled || !release.allowed || facts.destruction_completed)
    {
        return Err(policy_error(
            FailureKind::Conflict,
            "LEGAL_PROCESSING_UNAVAILABLE",
            "Legal document processing is not ready for this Company.",
        ));
    }
    let mut requirements = vec![LegalPurchaseRequirement {
        capability: capability.into(),
        purpose: LegalPurchasePurpose::History,
    }];
    if request.operation.document() {
        requirements.push(LegalPurchaseRequirement {
            capability: "legal.documents".into(),
            purpose: LegalPurchasePurpose::History,
        });
    }
    if request.operation.processing() && request.operation != LegalRecordOperation::ProcessingReplay
    {
        let purpose = if request.operation == LegalRecordOperation::QueueProcessing {
            LegalPurchasePurpose::NewWork
        } else {
            LegalPurchasePurpose::SettleExisting
        };
        requirements.extend([capability, "legal.documents"].map(|capability| {
            LegalPurchaseRequirement {
                capability: capability.into(),
                purpose,
            }
        }));
    }
    requirements.sort();
    requirements.dedup();
    Ok(requirements)
}
