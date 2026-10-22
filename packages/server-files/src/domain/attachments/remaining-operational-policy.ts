import type { AttachmentPolicy } from "@erpx/server-files/domain/attachments/types";
const MiB = 1024 * 1024;
const documents = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
    "application/pdf",
] as const;
const images = [
    "image/jpeg",
    "image/png",
    "image/webp",
    "image/heic",
    "image/heif",
] as const;
function policy(targetType: string, purposeKey: string, componentKinds: AttachmentPolicy["componentKinds"], options: Partial<Omit<AttachmentPolicy, "targetType" | "purposeKey" | "componentKinds">> = {}): AttachmentPolicy {
    return {
        targetType,
        purposeKey,
        componentKinds,
        version: 1,
        mediaTypes: options.mediaTypes ?? documents,
        maxFiles: options.maxFiles ?? 10,
        maxBytes: options.maxBytes ?? 10 * MiB,
        requiredCount: options.requiredCount ?? 0,
        classification: options.classification ?? "internal",
        retentionPolicyKey: options.retentionPolicyKey ?? null,
        publication: options.publication ?? false,
        portalUpload: false,
    };
}
const remainingOperationalAttachmentPolicies: readonly AttachmentPolicy[] = Object.freeze([
    policy("purchase_order", "supplier_promise", ["action"], {
        classification: "confidential",
    }),
    policy("supplier_bill_match", "invoice_document", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("supplier_bill_match", "tax_document", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("supplier_bill_match", "bill_match_support", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("commerce_order", "signed_quote", ["record"], {
        classification: "confidential",
    }),
    policy("inventory_transfer", "direct_receipt_evidence", ["action"], {
        classification: "confidential",
    }),
    policy("inventory_quality_spec", "controlled_specification", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("asset", "acceptance", ["record"], { maxFiles: 50 }),
    policy("asset", "handover", ["record"], { maxFiles: 50 }),
    policy("asset", "data_wipe", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("maintenance_work", "checklist", ["record"]),
    policy("maintenance_work", "agreement", ["record"], {
        classification: "confidential",
    }),
    policy("maintenance_work", "sla", ["record"], {
        classification: "confidential",
    }),
    policy("maintenance_work", "warranty", ["record"]),
    policy("ecommerce_listing", "listing_media", ["record"], {
        mediaTypes: images,
        maxFiles: 50,
        publication: true,
    }),
    policy("ecommerce_listing", "channel_document", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("ecommerce_listing", "compliance_document", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("marketing_asset", "creative", ["record"], {
        maxFiles: 50,
        publication: true,
    }),
    policy("marketing_asset", "source", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("marketing_asset", "consent", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("manufacturing_spec", "drawing", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("manufacturing_spec", "sop", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("manufacturing_spec", "formula", ["record"], {
        classification: "restricted",
        maxFiles: 50,
    }),
    policy("manufacturing_spec", "controlled_specification", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("production_run", "run_report_evidence", ["action"], {
        classification: "confidential",
    }),
    policy("production_run", "qc_evidence", ["action"], {
        classification: "confidential",
    }),
    policy("production_run", "rework_action_evidence", ["action"], {
        classification: "confidential",
    }),
    policy("planning_scenario", "assumption_source", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
    policy("planning_scenario", "capacity_source", ["record"], {
        classification: "confidential",
        maxFiles: 50,
    }),
]);
const ownedTargets = new Set([
    "supplier_bill_match",
    "inventory_quality_spec",
    "ecommerce_listing",
    "marketing_asset",
    "manufacturing_spec",
]);
const moduleScopedTargets = { "asset": { "module": "assets" }, "maintenance_work": { "module": "maintenance" }, "production_run": { "module": "manufacturing" }, "planning_scenario": { "module": "planning" } } as const;
export { remainingOperationalAttachmentPolicies, ownedTargets, moduleScopedTargets };
