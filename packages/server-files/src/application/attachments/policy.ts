import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission,permissionCatalog} from "@erpx/feature-identity_access/lib/permissions";
import {modulesForRole} from "@erpx/feature-identity_access/lib/business-roles";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Clock} from "@erpx/server-shared/application/ports";
import type {AttachmentComponentKind,AttachmentOperation,AttachmentPolicy,AttachmentTargetScope} from "@erpx/server-files/domain/attachments/types";
import {attachmentPolicyRegistry} from "@erpx/server-files/domain/attachments/policy";
export type AttachmentPolicyUnit={readDescriptor(facts:{targetId:string;companyId:string;targetType:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; } | null>;readCrmAccount(facts:{targetId:string;companyId:string;branchId:string | null;userId:string;departmentId:string | null;}):Promise<{ company_id: string; version: number; } | null>;readCrmOpportunity(facts:{targetId:string;companyId:string;branchId:string | null;departmentId:string | null;}):Promise<{ company_id: string; branch_id: string; department_id: string | null; owner_id: string; version: number; owner_eligible: number; } | null>;readMarketingCampaign(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; version: number; } | null>;readPosProduct(facts:{targetId:string;companyId:string;departmentId:string | null;assignmentId:string;userId:string;role:string;now:number;permission:"pos.read" | "pos.configure";branchId:string | null;}):Promise<{ company_id: string; version: number; } | null>;readCurrentAssignment(facts:{assignmentId:string;companyId:string;userId:string;role:string;now:number;branchId:string | null;departmentId:string | null;}):Promise<{ role: string; modules_json: string; role_permissions: string; permissions: string; } | null>;readSalesDelivery(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; } | null>;readPurchaseReceipt(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; } | null>;readFulfillment(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: null; kind: string; version: number; } | null>;readPurchaseOrder(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; } | null>;readQualityHold(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: null; version: number; } | null>;readTransit(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; source_branch_id: string; target_branch_id: string; version: number; } | null>;readProduct(facts:{targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: null; department_id: null; version: number; } | null>;readProject(facts:{userId:string;targetId:string;companyId:string;}):Promise<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; is_owner: number; lifecycle_status: string; } | null>;readBranchBrand(facts:{targetId:string;companyId:string;}):Promise<Record<string, unknown> | null>;authorizePortalCollaborationAttachmentTarget(actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation):Promise<AttachmentTargetScope|null>;authorizeSensitiveAttachmentTarget(actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation):Promise<AttachmentTargetScope|null>;authorizeFinanceOperationsAttachmentTarget(actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation):Promise<AttachmentTargetScope|null>;authorizeRemainingOperationalAttachmentTarget(actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation):Promise<AttachmentTargetScope|null>;messagingRoom(actor:WorkContext,id:string):Promise<Record<string,unknown>>;legalAccess(actor:WorkContext,id:string,permission:"legal.read"|"legal.edit"):Promise<Record<string,unknown>>;leaveAccess?(actor:WorkContext,input:{leaveRequestId:string;purposeKey:"medical_certificate"|"leave_supporting_photo";operation:"read"|"write"|"bind"}):Promise<AttachmentTargetScope>;supportTicket(actor:WorkContext,id:string):Promise<Record<string,unknown>>;supportEvidenceGuard(actor:WorkContext,id:string,version:number,key:string):Promise<unknown>;employeeRequest(actor:WorkContext,id:string):Promise<Record<string,unknown>>;employeeRequestUpload(actor:WorkContext,id:string,purpose:string):Promise<unknown>;expenseOwner(actor:WorkContext,id:string):Promise<{row:Record<string,unknown>;canEdit:boolean}>;expensePayment(actor:WorkContext,id:string,editable:boolean):Promise<{expense:Record<string,unknown>;draft:Record<string,unknown>}>;invoicePaymentOwner(actor:WorkContext,id:string,editable:boolean):Promise<Record<string,unknown>>;taskOwner(actor:WorkContext,id:string):Promise<{row:Record<string,unknown>;canAttach:boolean}>;socialUploadOwner(actor:WorkContext,id:string,editable:boolean):Promise<Record<string,unknown>>;requireBrandManagement(actor:WorkContext,type:"holding_brand"|"branch_brand"|"company_storefront_brand",id:string):Promise<void>;};
export function createAttachmentPolicy(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){
const retentionPolicies = Object.freeze({
  "legal-managed": { version: 1, retainUntil: null, destructionMode: "review" as const },
});

function attachmentRetentionSnapshot(policy: AttachmentPolicy) {
  if (!policy.retentionPolicyKey)
    return { key: null, version: null, retainUntil: null, destructionMode: null } as const;
  const resolved=retentionPolicies[policy.retentionPolicyKey as keyof typeof retentionPolicies];
  if(!resolved)throw ports.error(422,"ATTACHMENT_RETENTION_POLICY_UNKNOWN","This file purpose has no approved retention policy revision.");
  return {key:policy.retentionPolicyKey,...resolved};
}

function attachmentPolicy(targetType: string, purposeKey: string, componentKind: AttachmentComponentKind) {
  const policy = attachmentPolicyRegistry.find((item) => item.targetType === targetType && item.purposeKey === purposeKey && item.componentKinds.includes(componentKind));
  if (!policy) throw ports.error(422, "ATTACHMENT_POLICY_UNKNOWN", "This file purpose is not registered for the selected record and context.");
  return policy;
}

type Descriptor={readPermission:string;writePermission:string};
const descriptors:Record<string,Descriptor>={"procurement_rfq": {"readPermission": "commerce.read", "writePermission": "commerce.purchase"}, "purchase_order": {"readPermission": "commerce.read", "writePermission": "commerce.purchase"}, "supplier_bill": {"readPermission": "finance.read", "writePermission": "finance.write"}, "landed_cost": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "stock_count": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "inventory_stock_count": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "stock_transfer": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "inventory_transfer": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "inventory_transit": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "product": {"readPermission": "commerce.read", "writePermission": "commerce.purchase"}, "inventory_lot": {"readPermission": "inventory.read", "writePermission": "inventory.prepare"}, "quality_hold": {"readPermission": "quality.read", "writePermission": "quality.hold"}, "inventory_quality_hold": {"readPermission": "quality.read", "writePermission": "quality.hold"}, "commerce_order": {"readPermission": "commerce.read", "writePermission": "commerce.sales"}, "commerce_order_delivery": {"readPermission": "commerce.read", "writePermission": "inventory.prepare"}, "commerce_return": {"readPermission": "commerce.read", "writePermission": "commerce.sales"}, "asset": {"readPermission": "records.read", "writePermission": "records.write"}, "maintenance_work": {"readPermission": "records.read", "writePermission": "records.write"}, "field_service_visit": {"readPermission": "records.read", "writePermission": "records.write"}, "production_run": {"readPermission": "records.read", "writePermission": "records.write"}, "planning_scenario": {"readPermission": "records.read", "writePermission": "records.write"}, "project": {"readPermission": "records.read", "writePermission": "records.write"}};

function permissionFor(operation: AttachmentOperation, descriptor: Descriptor) {
  return operation === "read" ? descriptor.readPermission : descriptor.writePermission;
}

async function descriptorScope(db: AttachmentPolicyUnit, actor: WorkContext, targetType: string, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
  const descriptor = descriptors[targetType];
  if (!descriptor) throw ports.error(422, "ATTACHMENT_TARGET_UNSUPPORTED", "This record does not have a live attachment adapter.");
  if (!hasPermission(actor, permissionFor(operation, descriptor))) throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Your current assignment cannot access files for this record.");
  const row = await db.readDescriptor({targetId:targetId,companyId:actor.companyId,targetType:targetType});
  if (!row) throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "The file owner was not found in this Company.");
  if (actor.branchId && row.branch_id && actor.branchId !== row.branch_id) throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Branch scope.");
  if (actor.departmentId && row.department_id && actor.departmentId !== row.department_id) throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The file owner is outside your Department scope.");
  return { companyId: row.company_id, branchId: row.branch_id, departmentId: row.department_id, version: Number(row.version), editable: operation !== "read" };
}

async function crmAccountAttachmentScope(
  db: AttachmentPolicyUnit,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
    throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Your current Sales assignment cannot access files for this CRM account.");
  const row = await db.readCrmAccount({targetId:targetId,companyId:actor.companyId,branchId:actor.branchId,userId:actor.userId,departmentId:actor.departmentId});
  if(!row)throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","CRM account not found in this Sales scope.");
  return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
}

async function crmOpportunityAttachmentScope(
  db: AttachmentPolicyUnit,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  if (!hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.sales"))
    throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Your current Sales assignment cannot access files for this opportunity.");
  const row=await db.readCrmOpportunity({targetId:targetId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId});
  if(!row)throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Opportunity not found in this Sales scope.");
  if(operation!=="read"&&(row.owner_id!==actor.userId||!Number(row.owner_eligible)))
    throw ports.error(403,"ATTACHMENT_CRM_OWNER_REQUIRED","Only the current eligible salesperson can manage opportunity files.");
  return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
}

async function marketingCampaignAttachmentScope(
  db: AttachmentPolicyUnit,
  actor: WorkContext,
  targetId: string,
  operation: AttachmentOperation,
): Promise<AttachmentTargetScope> {
  const permission=operation==="read"?"records.read":"records.write";
  if(!hasPermission(actor,permission)||!hasModuleAccess(actor.modules,"marketing"))
    throw ports.error(403,"ATTACHMENT_FORBIDDEN","Your current Marketing assignment cannot access files for this campaign.");
  const row=await db.readMarketingCampaign({targetId:targetId,companyId:actor.companyId});
  if(!row)throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Marketing campaign not found in this Company.");
  return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
}

async function authorizePosProductImageTarget(db: AttachmentPolicyUnit, actor: WorkContext, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
  const permission = operation === "read" ? "pos.read" : "pos.configure";
  const row = await db.readPosProduct({targetId:targetId,companyId:actor.companyId,departmentId:actor.departmentId,assignmentId:actor.assignmentId,userId:actor.userId,role:actor.role,now:ports.clock.now(),permission:permission,branchId:actor.branchId});
  if (!row) throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Product image is unavailable in this POS scope.");
  return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
}

/** A cached WorkContext never keeps access after its selected grant changes. */
async function currentAttachmentActor(db:AttachmentPolicyUnit,actor:WorkContext,policy:AttachmentPolicy,targetId:string):Promise<WorkContext>{
  // This synthetic actor has no staff assignment. Its exact owner adapter below
  // rechecks the current customer identity, binding, scope and expiry instead.
  if(actor.role==='external_customer'&&policy.targetType==='customer_access_binding'&&actor.assignmentId===`customer-portal:${targetId}`)return actor;
  const now=ports.clock.now(),row=await db.readCurrentAssignment({assignmentId:actor.assignmentId,companyId:actor.companyId,userId:actor.userId,role:actor.role,now:now,branchId:actor.branchId,departmentId:actor.departmentId});
  if(!row)throw ports.error(404,'ATTACHMENT_TARGET_NOT_FOUND','The file owner is unavailable in your current work role.');
  const currentPermissions=new Set(JSON.parse(row.permissions) as string[]),permissions=permissionCatalog.filter(permission=>currentPermissions.has(permission.id)&&hasPermission(actor,permission.id)).map(permission=>permission.id),assigned=JSON.parse(row.modules_json) as string[],rolePermissions=JSON.parse(row.role_permissions) as string[],currentModules=actor.role.startsWith('custom_')?rolePermissions.filter(p=>p.startsWith('module.')).map(p=>p.split(".")[1]):modulesForRole(actor.role,assigned);
  // Intersect both sets: a freshly published broader role cannot expand the
  // request's original permission or module snapshot during an owner operation.
  return {...actor,...(actor.iam?{iam:{...actor.iam,permissions}}:{permissions}),modules:actor.modules.filter(module=>currentModules.includes(module))};
}
async function authorizeAttachmentTarget(db: AttachmentPolicyUnit, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation): Promise<AttachmentTargetScope> {
  actor=await currentAttachmentActor(db,actor,policy,targetId);
  const portalCollaboration=await db.authorizePortalCollaborationAttachmentTarget(actor,policy,targetId,operation);
  if(portalCollaboration)return portalCollaboration;
  const sensitive=await db.authorizeSensitiveAttachmentTarget(actor,policy,targetId,operation);
  if(sensitive)return sensitive;
  const financeOperations=await db.authorizeFinanceOperationsAttachmentTarget(actor,policy,targetId,operation);
  if(financeOperations)return financeOperations;
  const remainingOperational=await db.authorizeRemainingOperationalAttachmentTarget(actor,policy,targetId,operation);
  if(remainingOperational)return remainingOperational;
  if(policy.targetType==="messaging_room"){
    const row=await db.messagingRoom(actor,targetId) as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="legal_item"){
    const row=await db.legalAccess(actor,targetId,operation==="read"?"legal.read":"legal.edit") as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "leave_request") {
    if (db.leaveAccess) return db.leaveAccess(actor, { leaveRequestId: targetId, purposeKey: policy.purposeKey as "medical_certificate" | "leave_supporting_photo", operation: operation === "publish" ? "read" : operation === "withdraw" || operation === "revoke" || operation === "revoke-history" ? "write" : operation });
    throw ports.error(503, "LEAVE_ATTACHMENT_ADAPTER_UNAVAILABLE", "Leave evidence access is not ready.");
  }
  if (policy.targetType === "product" && policy.purposeKey === "product_image" &&
      !hasPermission(actor, operation === "read" ? "commerce.read" : "commerce.purchase") &&
      hasPermission(actor, operation === "read" ? "pos.read" : "pos.configure"))
    return authorizePosProductImageTarget(db,actor,targetId,operation);
  if (policy.targetType === "commerce_order_delivery") {
    const allowed = operation === "read"
      ? hasPermission(actor, "commerce.read")
      : hasPermission(actor, "inventory.prepare") && hasModuleAccess(actor.modules, "inventory");
    if (!allowed) throw ports.error(403, "ATTACHMENT_FORBIDDEN", "Use your current warehouse assignment to prepare delivery evidence.");
    const row = await db.readSalesDelivery({targetId:targetId,companyId:actor.companyId});
    if (!row) throw ports.error(404, "ATTACHMENT_TARGET_NOT_FOUND", "Sales delivery not found in this Company.");
    if (actor.branchId && actor.branchId !== row.branch_id) throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The delivery is outside your Branch scope.");
    if (actor.departmentId && actor.departmentId !== row.department_id) throw ports.error(403, "ATTACHMENT_SCOPE_FORBIDDEN", "The delivery is outside your Department scope.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "purchase_receipt") {
    const row = await db.readPurchaseReceipt({targetId:targetId,companyId:actor.companyId});
    if (!row || !hasPermission(actor, operation === "read" ? "commerce.read" : "inventory.prepare")&&!hasPermission(actor,"commerce.purchase")) throw ports.error(row ? 403 : 404,"ATTACHMENT_TARGET_NOT_FOUND","Purchase receipt not found or unavailable.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "commerce_fulfillment") {
    const row = await db.readFulfillment({targetId:targetId,companyId:actor.companyId});
    const permission=row?.kind==="purchase"?"commerce.purchase":"commerce.sales";
    if (!row || !hasPermission(actor, operation === "read" ? "commerce.read" : permission)) throw ports.error(row ? 403 : 404,"ATTACHMENT_TARGET_NOT_FOUND","Fulfillment not found or unavailable.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="purchase_order"&&["delivery_note","packing_list","damage_photo"].includes(policy.purposeKey)||policy.targetType==="commerce_order"&&policy.purposeKey==="purchase_receipt"){
    const row=await db.readPurchaseOrder({targetId:targetId,companyId:actor.companyId});
    const allowed=operation==="read"?hasPermission(actor,"commerce.read"):hasPermission(actor,"inventory.prepare")||hasPermission(actor,"commerce.purchase");
    if(!row||!allowed)throw ports.error(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Purchase order not found or unavailable.");
    if(actor.branchId&&actor.branchId!==row.branch_id)throw ports.error(403,"ATTACHMENT_SCOPE_FORBIDDEN","The purchase receipt is outside your Branch scope.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if(["quality_hold","inventory_quality_hold"].includes(policy.targetType)){
    const row=await db.readQualityHold({targetId:targetId,companyId:actor.companyId});
    const writePermission=["quality_inspection_evidence","inspection_evidence"].includes(policy.purposeKey)?"quality.inspect":["disposition_evidence"].includes(policy.purposeKey)?"quality.release":"quality.hold";
    if(!row||!hasPermission(actor,operation==="read"?"quality.read":writePermission))throw ports.error(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Quality hold not found or unavailable.");
    if(actor.branchId&&actor.branchId!==row.branch_id)throw ports.error(403,"ATTACHMENT_SCOPE_FORBIDDEN","The quality hold is outside your Branch scope.");
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="inventory_transit"){
    const row=await db.readTransit({targetId:targetId,companyId:actor.companyId});
    const branch=policy.purposeKey==="dispatch_evidence"?row?.source_branch_id:row?.target_branch_id;
    if(!row||!hasPermission(actor,operation==="read"?"inventory.read":"inventory.prepare"))throw ports.error(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Transit record not found or unavailable.");
    if(actor.branchId&&actor.branchId!==branch)throw ports.error(403,"ATTACHMENT_SCOPE_FORBIDDEN","Use the responsible warehouse Branch for this transit evidence.");
    return {companyId:row.company_id,branchId:branch??null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="product"){
    const row=await db.readProduct({targetId:targetId,companyId:actor.companyId});
    const allowed=operation==="read"
      ? hasPermission(actor,"commerce.read")||hasPermission(actor,"inventory.read")
      : hasPermission(actor,"commerce.purchase")||hasPermission(actor,"inventory.prepare");
    if(!row||!allowed)throw ports.error(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Product not found or unavailable.");
    return {companyId:row.company_id,branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="project"){
    const row=await db.readProject({userId:actor.userId,targetId:targetId,companyId:actor.companyId});
    const permission=operation==="read"?"records.read":"records.write";
    if(!row||!hasPermission(actor,permission))throw ports.error(row?403:404,"ATTACHMENT_TARGET_NOT_FOUND","Project not found or unavailable.");
    if(operation!=="read"&&!Number(row.is_owner))throw ports.error(403,"ATTACHMENT_PROJECT_OWNER_REQUIRED","Only the current project owner can manage project documents.");
    if(operation!=="read"&&row.lifecycle_status!=='active')throw ports.error(409,'PROJECT_STRUCTURE_FROZEN','Reopen the project before changing its documents.');
    return {companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,version:Number(row.version),editable:operation!=="read"};
  }
  if(policy.targetType==="crm_account")return crmAccountAttachmentScope(db,actor,targetId,operation);
  if(policy.targetType==="crm_opportunity")return crmOpportunityAttachmentScope(db,actor,targetId,operation);
  if(policy.targetType==="marketing_campaign")return marketingCampaignAttachmentScope(db,actor,targetId,operation);
  if (policy.targetType === "support_ticket") {
    const row = await db.supportTicket(actor,targetId) as Record<string,unknown>;
    const manager=hasPermission(actor,"support.manage"),assigned=hasPermission(actor,"support.work")&&row.owner_id===actor.userId&&Number(row.owner_eligible)>0;
    if(policy.purposeKey==="internal_evidence"&&!manager&&!assigned)throw ports.error(403,"ATTACHMENT_FORBIDDEN","Internal evidence is limited to the current assigned Support colleague and Support managers.");
    if(operation!=="read"){
      if(row.source_kind==="field_service")throw ports.error(409,"SOURCE_WORKSPACE_REQUIRED","Continue this case in Field Service.");
      if(policy.purposeKey==="customer_shareable"&&!manager)throw ports.error(403,"ATTACHMENT_CUSTOMER_SHARE_REVIEW","Only a Support manager may stage customer-shareable evidence.");
      if(!manager&&!assigned)throw ports.error(403,"ATTACHMENT_FORBIDDEN","Current Support responsibility is required.");
      await db.supportEvidenceGuard(actor,targetId,Number(row.version),`attachment-policy-${targetId}`);
    }
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "employee_request") {
    const row = await db.employeeRequest(actor,targetId) as Record<string,unknown>;
    if (operation !== "read") await db.employeeRequestUpload(actor,targetId,policy.purposeKey);
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "expense_claim") {
    const expense=await db.expenseOwner(actor,targetId),row=expense.row;
    if (operation !== "read" && !expense.canEdit) throw ports.error(403,"ATTACHMENT_FORBIDDEN","Only the claimant or preparer can change files on this claim.");
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "expense_payment") {
    const result = await db.expensePayment(actor,targetId,operation !== "read");
    const row = {...result.expense,...result.draft} as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(result.draft.version),editable:operation!=="read"};
  }
  if (policy.targetType === "invoice_payment") {
    const result = await db.invoicePaymentOwner(actor,targetId,operation !== "read");
    const row = result as unknown as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:null,departmentId:null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "staff_task") {
    const task=await db.taskOwner(actor,targetId),row=task.row;
    if (operation !== "read" && !task.canAttach) throw ports.error(403,"ATTACHMENT_FORBIDDEN","This task no longer accepts evidence from your assignment.");
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (policy.targetType === "social_draft") {
    const row = await db.socialUploadOwner(actor,targetId,operation !== "read") as Record<string,unknown>;
    return {companyId:String(row.company_id),branchId:row.branch_id?String(row.branch_id):null,departmentId:row.department_id?String(row.department_id):null,version:Number(row.version),editable:operation!=="read"};
  }
  if (["holding_brand","branch_brand","company_storefront_brand"].includes(policy.targetType)) {
    if(operation!=="read")await db.requireBrandManagement(actor,policy.targetType as "holding_brand"|"branch_brand"|"company_storefront_brand",targetId);
    else if(policy.targetType==="holding_brand"&&targetId!==actor.holdingId)throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Workspace brand not found.");
    else if(policy.targetType==="company_storefront_brand"&&targetId!==actor.companyId)throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Storefront brand not found.");
    else if(policy.targetType==="branch_brand"&&!await db.readBranchBrand({targetId:targetId,companyId:actor.companyId}))throw ports.error(404,"ATTACHMENT_TARGET_NOT_FOUND","Branch brand not found.");
    return {companyId:actor.companyId,branchId:policy.targetType==="branch_brand"?targetId:null,departmentId:null,version:1,editable:operation!=="read"};
  }
  return descriptorScope(db, actor, policy.targetType, targetId, operation);
}

function assertAttachmentMedia(policy: AttachmentPolicy, mediaType: string, bytes: number) {
  if (!policy.mediaTypes.includes(mediaType)) throw ports.error(422, "ATTACHMENT_MEDIA_TYPE", "This file type is not allowed for the selected purpose.");
  if (!Number.isSafeInteger(bytes) || bytes < 1 || bytes > policy.maxBytes) throw ports.error(422, "ATTACHMENT_SIZE", `Files for this purpose must be at most ${Math.floor(policy.maxBytes / 1024 / 1024)} MiB.`);
}

return {attachmentPolicyRegistry,attachmentRetentionSnapshot,attachmentPolicy,authorizePosProductImageTarget,authorizeAttachmentTarget,assertAttachmentMedia};}
