import "server-only";

import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import { ApiError } from "../http";
import type { SqlDatabase } from "../runtime/types";
import { salesRecipientEligibility } from "../sales-recipient";
import { ATTACHMENT_MAX_BYTES } from "./feature";
import type {
  AttachmentComponentKind,
  AttachmentOperation,
  AttachmentPolicy,
  AttachmentTargetScope,
} from "./types";
import { authorizePortalCollaborationAttachmentTarget,portalCollaborationAttachmentPolicies } from "./portal-collaboration-policy";
import { authorizeSensitiveAttachmentTarget,sensitiveAttachmentPolicies } from "./sensitive-policy";
import { authorizeFinanceOperationsAttachmentTarget,financeOperationsAttachmentPolicies } from "./finance-operations-policy";
import { authorizeRemainingOperationalAttachmentTarget,remainingOperationalAttachmentPolicies } from "./remaining-operational-policy";

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

const retentionPolicies = Object.freeze({
  "legal-managed": { version: 1, retainUntil: null, destructionMode: "review" as const },
});

export function attachmentRetentionSnapshot(policy: AttachmentPolicy) {
  if (!policy.retentionPolicyKey)
    return { key: null, version: null, retainUntil: null, destructionMode: null } as const;
  const resolved=retentionPolicies[policy.retentionPolicyKey as keyof typeof retentionPolicies];
  if(!resolved)throw new ApiError(422,"ATTACHMENT_RETENTION_POLICY_UNKNOWN","This file purpose has no approved retention policy revision.");
  return {key:policy.retentionPolicyKey,...resolved};
}

export function attachmentPolicy(targetType: string, purposeKey: string, componentKind: AttachmentComponentKind) {
  const policy = attachmentPolicyRegistry.find((item) => item.targetType === targetType && item.purposeKey === purposeKey && item.componentKinds.includes(componentKind));
  if (!policy) throw new ApiError(422, "ATTACHMENT_POLICY_UNKNOWN", "This file purpose is not registered for the selected record and context.");
  return policy;
}

type Descriptor = {
  table: string;
  version: string;
  branch?: string;
  department?: string;
  readPermission: string;
  writePermission: string;
  extra?: string;
};
const descriptors: Record<string, Descriptor> = {
  procurement_rfq: { table: "procurement_rfqs", version: "version", branch: "branch_id", department: "department_id", readPermission: "commerce.read", writePermission: "commerce.purchase" },
  purchase_order: { table: "commerce_orders", version: "version", branch: "branch_id", readPermission: "commerce.read", writePermission: "commerce.purchase", extra: "kind='purchase'" },
  supplier_bill: { table: "finance_invoices", version: "version", readPermission: "finance.read", writePermission: "finance.write" },
  landed_cost: { table: "inventory_landed_costs", version: "version", branch: "branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  stock_count: { table: "inventory_counts", version: "version", branch: "branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  inventory_stock_count: { table: "inventory_counts", version: "version", branch: "branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  stock_transfer: { table: "inventory_transfers", version: "version", branch: "source_branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  inventory_transfer: { table: "inventory_transfers", version: "version", branch: "source_branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  inventory_transit: { table: "inventory_transit", version: "version", branch: "source_branch_id", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  product: { table: "commerce_products", version: "version", readPermission: "commerce.read", writePermission: "commerce.purchase" },
  inventory_lot: { table: "inventory_lots", version: "version", readPermission: "inventory.read", writePermission: "inventory.prepare" },
  quality_hold: { table: "inventory_quality_holds", version: "version", branch: "branch_id", readPermission: "quality.read", writePermission: "quality.hold" },
  inventory_quality_hold: { table: "inventory_quality_holds", version: "version", branch: "branch_id", readPermission: "quality.read", writePermission: "quality.hold" },
  commerce_order: { table: "commerce_orders", version: "version", branch: "branch_id", readPermission: "commerce.read", writePermission: "commerce.sales" },
  commerce_order_delivery: { table: "commerce_orders", version: "version", branch: "branch_id", readPermission: "commerce.read", writePermission: "commerce.sales" },
  commerce_return: { table: "commerce_returns", version: "version", branch: "branch_id", readPermission: "commerce.read", writePermission: "commerce.sales" },
  asset: { table: "operational_assets", version: "version", branch: "branch_id", department: "department_id", readPermission: "records.read", writePermission: "records.write" },
  maintenance_work: { table: "maintenance_work_orders", version: "version", branch: "branch_id", department: "department_id", readPermission: "records.read", writePermission: "records.write" },
  field_service_visit: { table: "field_service_orders", version: "version", branch: "branch_id", department: "department_id", readPermission: "records.read", writePermission: "records.write" },
  production_run: { table: "manufacturing_orders", version: "version", branch: "branch_id", readPermission: "records.read", writePermission: "records.write" },
  planning_scenario: { table: "planning_scenarios", version: "version", branch: "branch_id", readPermission: "records.read", writePermission: "records.write" },
  project: { table: "staff_projects", version: "version", branch: "branch_id", department: "department_id", readPermission: "records.read", writePermission: "records.write" },
};

function permissionFor(operation: AttachmentOperation, descriptor: Descriptor) {
  return operation === "read" ? descriptor.readPermission : descriptor.writePermission;
}

async function descriptorScope(db: SqlDatabase, actor: WorkContext, targetType: string, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
  const descriptor = descriptors[targetType];
  if (!descriptor) throw new ApiError(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This record does not have a live attachment adapter.");
  if (!hasPermission(actor, permissionFor(operation, descriptor))) throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Your current assignment cannot access files for this record.");
  const version = descriptor.version === "1" ? "1" : descriptor.version;
  const branch = descriptor.branch ?? "NULL";
  const department = descriptor.department ?? "NULL";
  const extra = descriptor.extra ? ` AND ${descriptor.extra}` : "";
  const row = await db.prepare(`SELECT company_id,${branch} AS branch_id,${department} AS department_id,${version} AS version FROM ${descriptor.table} WHERE id=? AND company_id=?${extra}`)
    .bind(targetId, actor.companyId).first<{ company_id: string; branch_id: string | null; department_id: string | null; version: number }>();
  if (!row) throw new ApiError(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
  if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id) throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
  if (actor.departmentId && row.department_id && actor.departmentId !== row.department_id) throw new ApiError(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
  return { companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id, version: Number(row.version), editable: operation !== "read" };
}

async function crmAccountAttachmentScope(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
    throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Your current Sales assignment cannot access files for this CRM account.");
  const row = await db.prepare(`SELECT account.company_id,account.version
    FROM crm_accounts account
    WHERE account.id=? AND account.company_id=?
      AND (?::text IS NULL OR account.created_by=? OR EXISTS(
        SELECT 1 FROM crm_leads lead
        WHERE lead.account_id=account.id AND lead.company_id=account.company_id
          AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?)
      ))`)
    .bind(targetId,actor.companyId,actor.branchId,actor.userId,actor.branchId,actor.departmentId,actor.departmentId)
    .first<{company_id:string;version:number}>();
  if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","CRM account not found in this Sales scope.");
  return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
}

async function crmOpportunityAttachmentScope(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
    throw new ApiError(403, "ATTACHMENT_FORBIDDEN", "Your current Sales assignment cannot access files for this opportunity.");
  const ownerEligible=salesRecipientEligibility("l","l.owner_id");
  const row=await db.prepare(`SELECT l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,
      CASE WHEN ${ownerEligible} THEN 1 ELSE 0 END owner_eligible
    FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
    WHERE o.id=? AND l.company_id=?
      AND (?::text IS NULL OR l.branch_id=?)
      AND (?::text IS NULL OR l.department_id=?)`)
    .bind(targetId,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId)
    .first<{company_id:string;branch_id:string;department_id:string|null;owner_id:string;version:number;owner_eligible:number}>();
  if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","Opportunity not found in this Sales scope.");
  if(operation!=="read"&&(row.owner_id!==actor.userId||!Number(row.owner_eligible)))
    throw new ApiError(403,"ATTACHMENT_CRM_OWNER_REQUIRED","Only the current eligible salesperson can manage opportunity files.");
  return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
}

async function marketingCampaignAttachmentScope(
  db: SqlDatabase,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  const permission=operation==="read"?"records.read":"records.write";
  if(!hasPermission(actor,permission)||!hasModuleAccess(actor.modules,"marketing"))
    throw new ApiError(403,"ATTACHMENT_FORBIDDEN","Your current Marketing assignment cannot access files for this campaign.");
  const row=await db.prepare("SELECT company_id,version FROM marketing_campaigns WHERE id=? AND company_id=?")
    .bind(targetId,actor.companyId).first<{company_id:string;version:number}>();
  if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","Marketing campaign not found in this Company.");
  return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
}

export async function authorizeAttachmentTarget(db: SqlDatabase, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
  const portalCollaboration=await authorizePortalCollaborationAttachmentTarget(db,actor,policy,targetId,operation);
  if(portalCollaboration)return portalCollaboration;
  const sensitive=await authorizeSensitiveAttachmentTarget(db,actor,policy,targetId,operation);
  if(sensitive)return sensitive;
  const financeOperations=await authorizeFinanceOperationsAttachmentTarget(db,actor,policy,targetId,operation);
  if(financeOperations)return financeOperations;
  const remainingOperational=await authorizeRemainingOperationalAttachmentTarget(db,actor,policy,targetId,operation);
  if(remainingOperational)return remainingOperational;
  if(policy.targetType==="messaging_room"){
    const row=await (await import("../messaging")).messagingRoom(db,actor,targetId) as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="legal_item"){
    const row=await (await import("../legal/access")).access(db,actor,targetId,operation==="read"?"legal.read":"legal.edit") as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "leave_request") {
    const leaveModule = await import("../leave") as unknown as { assertLeaveAttachmentAccess?: (database: SqlDatabase, workActor: WorkContext, input: { leaveRequestId: string; purposeKey: "medical_certificate" | "leave_supporting_photo"; operation: "read" | "write" | "bind" }) => Promise<AttachmentTargetScope> };
    if (leaveModule.assertLeaveAttachmentAccess) return leaveModule.assertLeaveAttachmentAccess(db, actor, { leaveRequestId: targetId, purposeKey: policy.purposeKey as "medical_certificate" | "leave_supporting_photo", operation: operation === "publish" ? "read" : operation });
    throw new ApiError(503, "LEAVE_ATTACHMENT_ADAPTER_UNAVAILABLE", "Leave evidence access is not ready.");
  }
  if (policy.targetType === "purchase_receipt") {
    const row = await db.prepare("SELECT o.company_id,o.branch_id,o.department_id,o.version FROM commerce_orders o WHERE o.id=? AND o.company_id=? AND o.kind='purchase'").bind(targetId,actor.companyId).first<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>();
    if (!row || !hasPermission(actor, operation === "read" ? "commerce.read" : "inventory.prepare")&&!hasPermission(actor,"commerce.purchase")) throw new ApiError(row ? 403 : 404,"ATTACHMENT_TARGET_NOT_FOUND","Purchase receipt not found or unavailable.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "commerce_fulfillment") {
    const row = await db.prepare("SELECT o.company_id,o.branch_id,NULL::text department_id,o.kind,f.version FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=? AND o.company_id=?").bind(targetId,actor.companyId).first<{company_id:string;branch_id:string|null;department_id:null;kind:string;version:number}>();
    const permission=row?.kind==="purchase"?"commerce.purchase":"commerce.sales";
    if (!row || !hasPermission(actor, operation === "read" ? "commerce.read" : permission)) throw new ApiError(row ? 403 : 404,"ATTACHMENT_TARGET_NOT_FOUND","Fulfillment not found or unavailable.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="purchase_order"&&["delivery_note","packing_list","damage_photo"].includes(policy.purposeKey)||policy.targetType==="commerce_order"&&policy.purposeKey==="purchase_receipt"){
    const row=await db.prepare("SELECT company_id,branch_id,department_id,version FROM commerce_orders WHERE id=? AND company_id=? AND kind='purchase'").bind(targetId,actor.companyId).first<{company_id:string;branch_id:string|null;department_id:string|null;version:number}>();
    const allowed=operation==="read"?hasPermission(actor,"commerce.read"):hasPermission(actor,"inventory.prepare")||hasPermission(actor,"commerce.purchase");
    if(!row||!allowed)throw new ApiError(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Purchase order not found or unavailable.");
    if(actor.branchId&&actor.branchId!==row.branch_id)throw new ApiError(403,"ATTACHMENT_SCOPE_FORBIDDEN","The purchase receipt is outside your Branch scope.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if(["quality_hold","inventory_quality_hold"].includes(policy.targetType)){
    const row=await db.prepare("SELECT company_id,branch_id,NULL::text department_id,version FROM inventory_quality_holds WHERE id=? AND company_id=?").bind(targetId,actor.companyId).first<{company_id:string;branch_id:string|null;department_id:null;version:number}>();
    const writePermission=["quality_inspection_evidence","inspection_evidence"].includes(policy.purposeKey)?"quality.inspect":["disposition_evidence"].includes(policy.purposeKey)?"quality.release":"quality.hold";
    if(!row||!hasPermission(actor,operation==="read"?"quality.read":writePermission))throw new ApiError(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Quality hold not found or unavailable.");
    if(actor.branchId&&actor.branchId!==row.branch_id)throw new ApiError(403,"ATTACHMENT_SCOPE_FORBIDDEN","The quality hold is outside your Branch scope.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="inventory_transit"){
    const row=await db.prepare("SELECT company_id,source_branch_id,target_branch_id,version FROM inventory_transit WHERE id=? AND company_id=?").bind(targetId,actor.companyId).first<{company_id:string;source_branch_id:string;target_branch_id:string;version:number}>();
    const branch=policy.purposeKey==="dispatch_evidence"?row?.source_branch_id:row?.target_branch_id;
    if(!row||!hasPermission(actor,operation==="read"?"inventory.read":"inventory.prepare"))throw new ApiError(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Transit record not found or unavailable.");
    if(actor.branchId&&actor.branchId!==branch)throw new ApiError(403,"ATTACHMENT_SCOPE_FORBIDDEN","Use the responsible warehouse Branch for this transit evidence.");
    return {companyId:row.company_id,branchId:branch??null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="product"){
    const row=await db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM commerce_products WHERE id=? AND company_id=?").bind(targetId,actor.companyId).first<{company_id:string;branch_id:null;department_id:null;version:number}>();
    const allowed=operation==="read"
      ? hasPermission(actor,"commerce.read")||hasPermission(actor,"inventory.read")
      : hasPermission(actor,"commerce.purchase")||hasPermission(actor,"inventory.prepare");
    if(!row||!allowed)throw new ApiError(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Product not found or unavailable.");
    return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="project"){
    const row=await db.prepare(`SELECT p.company_id,p.branch_id,p.department_id,p.version,
      CASE WHEN iam_same_person(p.owner_user_id,?,p.company_id) THEN 1 ELSE 0 END is_owner
      FROM staff_projects p WHERE p.id=? AND p.company_id=? AND
      (iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))`)
      .bind(actor.userId,targetId,actor.companyId,actor.userId,actor.userId).first<{company_id:string;branch_id:string|null;department_id:string|null;version:number;is_owner:number}>();
    const permission=operation==="read"?"records.read":"records.write";
    if(!row||!hasPermission(actor,permission))throw new ApiError(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Project not found or unavailable.");
    if(operation!=="read"&&!Number(row.is_owner))throw new ApiError(403,"ATTACHMENT_PROJECT_OWNER_REQUIRED","Only the current project owner can manage project documents.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="crm_account")return crmAccountAttachmentScope(db,actor,targetId,operation);
  if(policy.targetType==="crm_opportunity")return crmOpportunityAttachmentScope(db,actor,targetId,operation);
  if(policy.targetType==="marketing_campaign")return marketingCampaignAttachmentScope(db,actor,targetId,operation);
  if (policy.targetType === "support_ticket") {
    const row = await (await import("../support")).getSupportTicket(actor,targetId) as Record<string,unknown>;
    const manager=hasPermission(actor,"support.manage"),assigned=hasPermission(actor,"support.work")&&row.owner_id===actor.userId&&Number(row.owner_eligible)>0;
    if(policy.purposeKey==="internal_evidence"&&!manager&&!assigned)throw new ApiError(403,"ATTACHMENT_FORBIDDEN","Internal evidence is limited to the current assigned Support colleague and Support managers.");
    if(operation!=="read"){
      if(policy.purposeKey==="customer_shareable"&&!manager)throw new ApiError(403,"ATTACHMENT_CUSTOMER_SHARE_REVIEW","Only a Support manager may stage customer-shareable evidence.");
      if(!manager&&!assigned)throw new ApiError(403,"ATTACHMENT_FORBIDDEN","Current Support responsibility is required.");
      await (await import("../support-evidence")).supportEvidenceGuard(db,actor,targetId,Number(row.version),`attachment-policy-${targetId}`);
    }
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "employee_request") {
    const service = await import("../employee-requests");
    const row = await service.getEmployeeRequest(db,actor,targetId) as Record<string,unknown>;
    if (operation !== "read") await service.employeeRequestUpload(db,actor,targetId,policy.purposeKey);
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "expense_claim") {
    const service = await import("../expenses");
    const row = await service.getExpense(actor,targetId) as unknown as Record<string,unknown>;
    if (operation !== "read" && !service.canEditExpense(actor,row as never)) throw new ApiError(403,"ATTACHMENT_FORBIDDEN","Only the claimant or preparer can change files on this claim.");
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "expense_payment") {
    const result = await (await import("../expense-payments")).paymentDraft(actor,targetId,operation !== "read");
    const row = {...result.expense,...result.draft} as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(result.draft.version),editable:operation!=="read"};
  }
  if (policy.targetType === "invoice_payment") {
    const result = await (await import("../invoice-payment-evidence")).invoicePaymentOwner(db,actor,targetId,operation !== "read");
    const row = result as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "staff_task") {
    const service = await import("../staff-tasks"), row = await service.getTask(db,actor,targetId) as unknown as Record<string,unknown>;
    if (operation !== "read" && !service.canAttachTask(actor,row as never)) throw new ApiError(403,"ATTACHMENT_FORBIDDEN","This task no longer accepts evidence from your assignment.");
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "social_draft") {
    const row = await (await import("../social-home")).socialUploadOwner(db,actor,targetId,operation !== "read") as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (["holding_brand","branch_brand","company_storefront_brand"].includes(policy.targetType)) {
    if(operation!=="read")await (await import("../branding")).requireBrandManagement(db,actor,policy.targetType as "holding_brand"|"branch_brand"|"company_storefront_brand",targetId);
    else if(policy.targetType==="holding_brand"&&targetId!==actor.holdingId)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","Workspace brand not found.");
    else if(policy.targetType==="company_storefront_brand"&&targetId!==actor.companyId)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","Storefront brand not found.");
    else if(policy.targetType==="branch_brand"&&!await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(targetId,actor.companyId).first())throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","Branch brand not found.");
    return {companyId:actor.companyId,branchId:policy.targetType==="branch_brand"?targetId:null,departmentId:null,version:1,editable:operation!=="read"};
  }
  return descriptorScope(db, actor, policy.targetType, targetId, operation);
}

export function assertAttachmentMedia(policy: AttachmentPolicy, mediaType: string, bytes: number) {
  if (!policy.mediaTypes.includes(mediaType)) throw new ApiError(422, "ATTACHMENT_MEDIA_TYPE", "This file type is not allowed for the selected purpose.");
  if (!Number.isSafeInteger(bytes) || bytes < 1 || bytes > policy.maxBytes) throw new ApiError(422, "ATTACHMENT_SIZE", `Files for this purpose must be at most ${Math.floor(policy.maxBytes / 1024 / 1024)} MiB.`);
}
