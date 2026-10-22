import type {AttachmentComponentKind,AttachmentPolicy} from "@erpx/server-files/domain/attachments/types";
import {ATTACHMENT_MAX_BYTES} from "@erpx/server-files/domain/attachments/feature";
import {sensitiveAttachmentPolicies} from "@erpx/server-files/domain/attachments/sensitive-policy";
import {portalCollaborationAttachmentPolicies} from "@erpx/server-files/domain/attachments/portal-collaboration-policy";
import {financeOperationsAttachmentPolicies} from "@erpx/server-files/domain/attachments/finance-operations-policy";
import {remainingOperationalAttachmentPolicies} from "@erpx/server-files/domain/attachments/remaining-operational-policy";
const documents = ["image/jpeg", "image/png", "image/webp", "image/heic", "image/heif", "application/pdf"] as const;
const images = ["image/jpeg", "image/png", "image/webp", "image/heic", "image/heif"] as const;
const recordKinds = ["record"] as const;
const actionKinds = ["action"] as const;

type Seed = Omit<AttachmentPolicy, "version" | "mediaTypes" | "maxFiles" | "maxBytes" | "requiredCount" | "classification" | "retentionPolicyKey" | "publication" | "portalUpload"> &
  Partial<Pick<AttachmentPolicy, "mediaTypes" | "maxFiles" | "maxBytes" | "requiredCount" | "classification" | "retentionPolicyKey" | "publication" | "portalUpload">>;

const seeds: Seed[] = [];
function add(targetType: string, purposeKeys: readonly string[], componentKinds: readonly AttachmentComponentKind[], options: Partial<Seed> = {}) {
  for (const purposeKey of purposeKeys) seeds.push({ targetType, purposeKey, componentKinds, ...options });
}

// Existing live attachment owners.
add("support_ticket", ["support_evidence"], actionKinds, { classification: "confidential" });
add("support_ticket", ["internal_evidence"], actionKinds, { classification: "restricted" });
add("support_ticket", ["customer_shareable"], actionKinds, { classification: "confidential", portalUpload: true });
add("employee_request", ["request_evidence", "it_request_evidence", "fulfillment"], ["record","action"], { classification: "confidential" });
add("leave_request", ["medical_certificate"], ["record", "action"], { classification: "restricted" });
add("leave_request", ["leave_supporting_photo"], ["record", "action"], { mediaTypes: images, classification: "confidential" });
add("expense_claim", ["receipt", "merchant_receipt", "supporting_document"], recordKinds, { mediaTypes: [...documents, "text/csv"], classification: "confidential" });
add("expense_payment", ["payment_slip"], actionKinds, { classification: "confidential" });
add("invoice_payment", ["payment_slip"], actionKinds, { classification: "confidential" });
add("staff_task", ["task_evidence"], actionKinds, { classification: "internal" });
add("social_draft", ["post_image"], ["record"], { mediaTypes: images, classification: "internal" });
add("messaging_room", ["message_attachment"], ["message"], { mediaTypes: [...documents,"text/csv"], classification: "confidential" });
add("legal_item", ["legal_document"], ["record"], { mediaTypes: ["application/pdf","image/png","image/jpeg","application/vnd.openxmlformats-officedocument.wordprocessingml.document"], classification: "restricted", retentionPolicyKey: "legal-managed" });
add("legal_item", ["legal_portal_submission"], ["action"], { mediaTypes: ["application/pdf","application/vnd.openxmlformats-officedocument.wordprocessingml.document"], classification: "restricted", retentionPolicyKey: "legal-managed", portalUpload: true });
add("holding_brand", ["primary", "inverse"], ["publication"], { mediaTypes: ["image/png", "image/webp"], maxBytes: 2 * 1024 * 1024, maxFiles: 1, classification: "internal", publication: true });
add("branch_brand", ["primary", "inverse"], ["publication"], { mediaTypes: ["image/png", "image/webp"], maxBytes: 2 * 1024 * 1024, maxFiles: 1, classification: "internal", publication: true });
add("company_storefront_brand", ["logo"], ["publication"], { mediaTypes: ["image/png", "image/webp"], maxBytes: 2 * 1024 * 1024, maxFiles: 1, classification: "internal", publication: true });

// Procurement, stock, commercial, customer and operational workflows with a
// real aggregate in the holding schema. Aliases keep current screens usable
// while new domains move to the canonical names.
add("procurement_rfq", ["supplier_quotation"], ["record", "action"]);
add("purchase_order", ["order_document", "supplier_acknowledgement"], recordKinds);
add("purchase_order", ["delivery_note", "packing_list"], actionKinds);
add("purchase_order", ["damage_photo"], actionKinds, { mediaTypes: images });
add("purchase_receipt", ["delivery_note", "packing_list"], actionKinds);
add("purchase_receipt", ["damage_photo"], actionKinds, { mediaTypes: images });
add("supplier_bill", ["invoice_document", "tax_document"], recordKinds, { classification: "confidential" });
add("supplier_bill", ["bill_match_support"], actionKinds, { classification: "confidential" });
add("landed_cost", ["freight_document", "customs_document"], recordKinds, { classification: "confidential" });
add("stock_count", ["count_sheet", "count_evidence"], actionKinds);
add("stock_count", ["count_photo"], actionKinds, { mediaTypes: images });
add("inventory_stock_count", ["count_evidence"], actionKinds);
add("stock_transfer", ["dispatch_evidence", "receipt_evidence", "loss_evidence"], actionKinds);
add("inventory_transfer", ["dispatch_evidence", "receipt_evidence", "loss_evidence"], actionKinds);
add("inventory_transit", ["dispatch_evidence"], actionKinds);
add("inventory_transit", ["receipt_evidence", "loss_evidence"], ["event"]);
add("product", ["product_image"], recordKinds, { mediaTypes: images, maxFiles: 50, publication: true });
add("product", ["specification"], recordKinds, { maxFiles: 50 });
add("inventory_lot", ["coa", "certificate", "label_photo"], recordKinds, { maxFiles: 50 });
add("quality_hold", ["defect_photo", "quality_inspection_evidence", "disposition_evidence"], actionKinds);
add("inventory_quality_hold", ["hold_evidence", "inspection_evidence", "disposition_evidence"], actionKinds);
add("commerce_order", ["customer_brief", "customer_po", "contract", "specification"], recordKinds, { classification: "confidential" });
add("commerce_order", ["purchase_receipt"], actionKinds, { classification: "confidential" });
add("commerce_order_delivery", ["proof_of_delivery"], actionKinds, { classification: "confidential", requiredCount: 1 });
add("commerce_fulfillment", ["return_condition"], actionKinds, { classification: "confidential" });
add("commerce_return", ["condition_photo"], actionKinds, { mediaTypes: images, classification: "confidential" });
add("commerce_return", ["return_document"], actionKinds, { classification: "confidential" });
add("crm_account", ["brief", "rfp", "correspondence"], recordKinds, { classification: "confidential" });
add("crm_opportunity", ["brief", "rfp", "correspondence"], recordKinds, { classification: "confidential" });
add("marketing_campaign", ["creative"], recordKinds, { publication: true });
add("marketing_campaign", ["source", "consent"], recordKinds, { classification: "confidential" });
add("asset", ["nameplate_photo", "condition_photo"], recordKinds, { mediaTypes: images, maxFiles: 50 });
add("asset", ["manual", "warranty", "acquisition_document", "disposal_document"], recordKinds, { maxFiles: 50 });
add("maintenance_work", ["fault_photo", "completion_photo"], recordKinds, { mediaTypes: images });
add("maintenance_work", ["vendor_quote", "service_report"], recordKinds);
add("field_service_visit", ["site_photo"], recordKinds, { mediaTypes: images });
add("field_service_visit", ["technician_report", "signed_job_sheet", "exception_evidence"], recordKinds, { classification: "confidential" });
add("production_run", ["run_sheet", "qc_report", "rework_evidence"], recordKinds);
add("production_run", ["output_photo", "scrap_photo"], recordKinds, { mediaTypes: images });
add("planning_scenario", ["source_document"], recordKinds);
add("project", ["project_document", "deliverable", "handoff"], recordKinds, { maxFiles: 50 });

export const attachmentPolicyRegistry: readonly AttachmentPolicy[] = Object.freeze([...seeds.map((seed) => ({
  targetType: seed.targetType,
  purposeKey: seed.purposeKey,
  version: 1,
  componentKinds: seed.componentKinds,
  mediaTypes: seed.mediaTypes ?? documents,
  maxFiles: seed.maxFiles ?? 10,
  maxBytes: seed.maxBytes ?? ATTACHMENT_MAX_BYTES,
  requiredCount: seed.requiredCount ?? 0,
  classification: seed.classification ?? "internal",
  retentionPolicyKey: seed.retentionPolicyKey ?? null,
  publication: seed.publication ?? false,
  portalUpload: seed.portalUpload ?? false,
})),...sensitiveAttachmentPolicies,...portalCollaborationAttachmentPolicies,...financeOperationsAttachmentPolicies,...remainingOperationalAttachmentPolicies]);

