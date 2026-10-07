import type {AttachmentPolicy} from "./types";
import {ATTACHMENT_MAX_BYTES} from "./feature";
type SensitivePolicySeed = Omit<AttachmentPolicy, "version" | "componentKinds" | "mediaTypes" | "maxFiles" | "maxBytes" | "requiredCount" | "classification" | "retentionPolicyKey" | "publication" | "portalUpload"> & Partial<Pick<AttachmentPolicy, "componentKinds" | "mediaTypes" | "maxFiles" | "maxBytes" | "requiredCount" | "classification" | "retentionPolicyKey">>;
const documents = [
        "image/jpeg",
        "image/png",
        "image/webp",
        "image/heic",
        "image/heif",
        "application/pdf",
    ] as const;
const sensitiveSeeds: SensitivePolicySeed[] = [];
function add(targetType: string, purposeKeys: readonly string[], options: Omit<SensitivePolicySeed, "targetType" | "purposeKey"> = {}) {
        for (const purposeKey of purposeKeys)
            sensitiveSeeds.push({ targetType, purposeKey, ...options });
    }
add("hr_person", ["identity_document", "work_permit", "tax_document", "bank_document"], { classification: "restricted" });
add("hr_person", ["employment_contract", "employment_amendment", "professional_certificate"], { classification: "confidential" });
add("hr_person", ["compensation_authorization"], {
        classification: "restricted",
    });
add("hr_evaluation_assessment", ["evaluation_evidence"], { classification: "restricted" });
add("people_lifecycle_case", ["offer_document", "transfer_document", "offboarding_document"], {
        classification: "restricted",
    });
add("people_clearance_task", ["clearance_evidence"], {
        componentKinds: ["action"],
        classification: "restricted",
    });
add("payroll_run", ["correction_support"], {
        componentKinds: ["action"],
        classification: "restricted",
    });
add("payroll_advance", ["advance_support"], { classification: "restricted" });
add("payroll_final_pay", ["final_pay_support"], { classification: "restricted" });
add("talent_candidate", ["cv", "portfolio", "work_sample", "background_document"], {
        classification: "restricted",
        maxFiles: 50,
    });
add("talent_offer", ["offer_document"], { classification: "restricted" });
add("learning_course", ["course_material"], {
        classification: "confidential",
        maxFiles: 50,
    });
add("learning_assignment", ["certificate"], {
        componentKinds: ["record", "action"],
        classification: "confidential",
    });
add("benefit_plan", ["benefit_booklet"], {
        classification: "confidential",
        maxFiles: 50,
    });
add("benefit_dependent", ["dependent_proof"], {
        componentKinds: ["record", "action"],
        classification: "restricted",
    });
add("benefit_election", ["election_document", "provider_proof"], {
        componentKinds: ["record", "action"],
        classification: "restricted",
    });
const sensitiveAttachmentPolicies: readonly AttachmentPolicy[] = Object.freeze(sensitiveSeeds.map((seed) => ({
        targetType: seed.targetType,
        purposeKey: seed.purposeKey,
        version: 1,
        componentKinds: seed.componentKinds ?? (["record"] as const),
        mediaTypes: seed.mediaTypes ?? documents,
        maxFiles: seed.maxFiles ?? 10,
        maxBytes: seed.maxBytes ?? ATTACHMENT_MAX_BYTES,
        requiredCount: seed.requiredCount ?? 0,
        classification: seed.classification ?? "restricted",
        retentionPolicyKey: seed.retentionPolicyKey ?? null,
        publication: false,
        portalUpload: false,
    })));
const sensitiveTargetTypes = new Set(sensitiveAttachmentPolicies.map((policy) => policy.targetType));
function isSensitiveAttachmentTarget(targetType: string) {
        return sensitiveTargetTypes.has(targetType);
    }
export {sensitiveAttachmentPolicies,isSensitiveAttachmentTarget};
