import {samePerson} from "@/lib/identity_access/iam";import {hasPermission} from "@/lib/identity_access/permissions";import type {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import {BusinessRuleError,type Clock} from '../../shared';import {milliQuantity as resolveReorderQuantity} from '../domain/reorder-quantity';import {inventoryReorderPolicySchema,inventoryReorderPolicyActionSchema,type InventoryReorderPolicyRow} from '../domain/reorder-policies';
export interface InventoryReorderRepository<Effect>{policies(companyId:string,branchId:string | null,branchId2:string | null):Promise<InventoryReorderPolicyRow[]>;
activeWarehouse(warehouseId:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<{ id: string; branch_id: string; version: number }|null>;
activeProduct(productId:string,companyId:string):Promise<{ id: string; version: number }|null>;
policy(id:string,companyId:string):Promise<Pick<InventoryReorderPolicyRow, "id" | "version" | "proposed_reorder_point_milli" | "current_reorder_point_milli" | "retired_at">|null>;
guardWarehouseVersion(key:string,value:"warehouse-version",id:string,companyId:string,version:number,branchId:string | null,branchId2:string | null):Effect;
guardProductVersion(key:string,value:"product-version",id:string,companyId:string,version:number):Effect;
guardEditablePolicy(key:string,value:"policy-version",id:string,companyId:string,version:number):Effect;
guardUniquePolicy(key:string,value:"policy-unique",id:string,id2:string,id3:string):Effect;
preparePolicyRevision(point:string,userId:string,now:number,id:string,companyId:string,version:number):Effect;
preparePolicyProposal(id:string,companyId:string,id2:string,id3:string,point:string,userId:string,now:number,now2:number):Effect;
prepareProposalEvent(value:string,id:string,version:number,value2:"revise" | "propose",userId:string,reason:string,snapshot:string,now:number):Effect;
scopedPolicy(id:string,companyId:string,companyId2:string,branchId:string | null,branchId2:string | null):Promise<InventoryReorderPolicyRow|null>;
preparePolicyApproval(userId:string,now:number,userId2:string,reason:string,now2:number,now3:number,id:string,companyId:string,expectedVersion:number):Effect;
preparePolicyRejection(userId:string,reason:string,now:number,now2:number,id:string,companyId:string,expectedVersion:number):Effect;
preparePolicyWithdrawal(now:number,id:string,companyId:string,expectedVersion:number):Effect;
preparePolicyRetirement(now:number,userId:string,reason:string,now2:number,now3:number,id:string,companyId:string,expectedVersion:number):Effect;
guardPolicyVersion(key:string,value:"policy-version",id:string,companyId:string,expectedVersion:number,proposed_reorder_point_milli:string | null):Effect;
prepareDecisionEvent(value:string,id:string,version:number,action:"approve" | "reject" | "withdraw" | "retire",userId:string,reason:string,snapshot:string,now:number):Effect;}
export interface InventoryReorderCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,'companyId'|'branchId'|'departmentId'>):Promise<Result>;}
export interface InventoryReorderPorts<Effect>{repository:InventoryReorderRepository<Effect>;commands:InventoryReorderCommands<Effect>;clock:Clock;hash(value:string):Promise<string>;}
export function createInventoryReorderApplication<Effect>(ports:InventoryReorderPorts<Effect>){const r=ports.repository;function requireCompanyInventoryScope(actor: WorkContext) {
  if (actor.departmentId) throw new BusinessRuleError(403, "INVENTORY_SCOPE_REQUIRED", "Use a Company or Branch inventory assignment.");
}
async function listInventoryReorderPolicies(actor: WorkContext) {
  if (!hasPermission(actor, "inventory.read")) throw new BusinessRuleError(403, "INVENTORY_FORBIDDEN", "The current role cannot read inventory reorder policies.");
  requireCompanyInventoryScope(actor);
  
  return (await r.policies(actor.companyId,actor.branchId,actor.branchId));
}
async function saveInventoryReorderPolicy(actor: WorkContext, input: z.infer<typeof inventoryReorderPolicySchema>, key: string) {
  if (!hasPermission(actor, "inventory.prepare")) throw new BusinessRuleError(403, "INVENTORY_PREPARE_FORBIDDEN", "The current role cannot propose inventory reorder points.");
  requireCompanyInventoryScope(actor);
  const point = resolveReorderQuantity(input.reorderPoint);
  const warehouse = await r.activeWarehouse(input.warehouseId,actor.companyId,actor.branchId,actor.branchId);
  if (!warehouse) throw new BusinessRuleError(404, "WAREHOUSE_NOT_FOUND", "The active warehouse is unavailable in the selected scope.");
  const product = await r.activeProduct(input.productId,actor.companyId);
  if (!product) throw new BusinessRuleError(404, "PRODUCT_NOT_FOUND", "The active product is unavailable in the selected Company.");
  const id = `reorder-${(await ports.hash(`${actor.companyId}:${warehouse.id}:${product.id}`)).slice(0, 32)}`;
  const prior = await ports.commands.replay( actor, key, "inventory.reorder.propose", id, input);
  if (prior) return prior;
  const current = await r.policy(id,actor.companyId);
  if (current?.proposed_reorder_point_milli !== null && current?.proposed_reorder_point_milli !== undefined)
    throw new BusinessRuleError(409, "REORDER_PROPOSAL_PENDING", "A reorder point for this product and warehouse is already awaiting review.");
  if (current && input.expectedVersion !== Number(current.version))
    throw new BusinessRuleError(409, "REORDER_POLICY_CHANGED", "The reorder policy changed. Refresh and review it before proposing another value.");
  if (!current && input.expectedVersion !== undefined)
    throw new BusinessRuleError(409, "REORDER_POLICY_CHANGED", "This reorder policy does not exist. Refresh the stock view.");
  const version = current ? Number(current.version) + 1 : 1, now = ports.clock.now();
  const snapshot = JSON.stringify({ warehouseId: warehouse.id, productId: product.id, proposedReorderPointMilli: point, currentReorderPointMilli: current?.current_reorder_point_milli ?? null, reason: input.reason });
  const steps: Effect[] = [
    r.guardWarehouseVersion(key,"warehouse-version",warehouse.id,actor.companyId,warehouse.version,actor.branchId,actor.branchId),
    r.guardProductVersion(key,"product-version",product.id,actor.companyId,product.version),
    current
      ? r.guardEditablePolicy(key,"policy-version",id,actor.companyId,current.version)
      : r.guardUniquePolicy(key,"policy-unique",id,warehouse.id,product.id),
    current
      ? r.preparePolicyRevision(point,actor.userId,now,id,actor.companyId,current.version)
      : r.preparePolicyProposal(id,actor.companyId,warehouse.id,product.id,point,actor.userId,now,now),
    r.prepareProposalEvent(`${id}:${version}`,id,version,current ? "revise" : "propose",actor.userId,input.reason,snapshot,now),
  ];
  return ports.commands.commit( actor, key, "inventory.reorder.propose", input, id, version, { id, status: "pending", version }, steps, { companyId: actor.companyId, branchId: warehouse.branch_id, departmentId: null });
}
async function transitionInventoryReorderPolicy(actor: WorkContext, id: string, input: z.infer<typeof inventoryReorderPolicyActionSchema>, key: string) {
  requireCompanyInventoryScope(actor);
  const operation = `inventory.reorder.${input.action}`;
  const prior = await ports.commands.replay( actor, key, operation, id, input);
  if (prior) return prior;
  const row = await r.scopedPolicy(id,actor.companyId,actor.companyId,actor.branchId,actor.branchId);
  if (!row) throw new BusinessRuleError(404, "REORDER_POLICY_NOT_FOUND", "The inventory reorder policy is unavailable in the selected scope.");
  if (Number(row.version) !== input.expectedVersion) throw new BusinessRuleError(409, "REORDER_POLICY_CHANGED", "The reorder policy changed. Refresh and review it before continuing.");
  const review = input.action === "approve" || input.action === "reject" || input.action === "retire";
  if (review && (!hasPermission(actor, "inventory.approve") || actor.branchId || actor.departmentId))
    throw new BusinessRuleError(403, "INVENTORY_APPROVAL_FORBIDDEN", "An independent Company inventory approver must review this policy.");
  if (input.action === "withdraw" && !hasPermission(actor, "inventory.prepare"))
    throw new BusinessRuleError(403, "INVENTORY_PREPARE_FORBIDDEN", "The current role cannot withdraw a reorder proposal.");
  if (input.action === "approve" || input.action === "reject") {
    if (!row.proposed_reorder_point_milli) throw new BusinessRuleError(409, "REORDER_PROPOSAL_MISSING", "There is no reorder proposal awaiting review.");
    if (samePerson(actor, row.requested_by)) throw new BusinessRuleError(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Company approver must review the proposed reorder point.");
  } else if (input.action === "withdraw") {
    if (!row.proposed_reorder_point_milli || !samePerson(actor, row.requested_by)) throw new BusinessRuleError(403, "REORDER_WITHDRAW_FORBIDDEN", "Only the person who proposed a pending reorder point can withdraw it.");
  } else {
    if (row.proposed_reorder_point_milli || !row.current_reorder_point_milli || row.retired_at) throw new BusinessRuleError(409, "REORDER_POLICY_NOT_ACTIVE", "Only an active approved reorder point without a pending proposal can be retired.");
    if (row.approved_by && samePerson(actor, row.approved_by)) throw new BusinessRuleError(403, "INDEPENDENT_APPROVER_REQUIRED", "A different Company approver must retire this reorder point.");
  }
  const now = ports.clock.now(), version = input.expectedVersion + 1;
  const update = input.action === "approve"
    ? r.preparePolicyApproval(actor.userId,now,actor.userId,input.reason,now,now,id,actor.companyId,input.expectedVersion)
    : input.action === "reject"
      ? r.preparePolicyRejection(actor.userId,input.reason,now,now,id,actor.companyId,input.expectedVersion)
      : input.action === "withdraw"
        ? r.preparePolicyWithdrawal(now,id,actor.companyId,input.expectedVersion)
        : r.preparePolicyRetirement(now,actor.userId,input.reason,now,now,id,actor.companyId,input.expectedVersion);
  const snapshot = JSON.stringify({ action: input.action, currentReorderPointMilli: row.current_reorder_point_milli, proposedReorderPointMilli: row.proposed_reorder_point_milli, reason: input.reason });
  const steps: Effect[] = [
    r.guardPolicyVersion(key,"policy-version",id,actor.companyId,input.expectedVersion,row.proposed_reorder_point_milli),
    update,
    r.prepareDecisionEvent(`${id}:${version}`,id,version,input.action,actor.userId,input.reason,snapshot,now),
  ];
  return ports.commands.commit( actor, key, operation, input, id, version, { id, status: input.action === "reject" ? (row.current_reorder_point_milli ? "approved" : "unconfigured") : input.action === "retire" ? "retired" : input.action === "withdraw" ? (row.current_reorder_point_milli ? "approved" : "unconfigured") : "approved", version }, steps, { companyId: actor.companyId, branchId: row.branch_id, departmentId: null });
}
return {listInventoryReorderPolicies,saveInventoryReorderPolicy,transitionInventoryReorderPolicy};}
