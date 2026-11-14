/**
 * Read-only inventory reorder-policy compatibility surface.
 *
 * Reorder propose/approve/reject/withdraw/retire now belong to the native
 * command mount (`packages/server-inventory/src/public/reorder_policies.rs` +
 * composition); every write entry point fails closed with
 * `NATIVE_INVENTORY_REORDER_REQUIRED`. This module keeps the current policy
 * read used by stock, commerce, records and work projections.
 */
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {inventoryReadProjection as inventorycommerceProjection} from '@erpx/server-commerce/public/inventory-read-projections';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import "server-only";
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {inventoryReorderPolicySchema,inventoryReorderPolicyActionSchema,type InventoryReorderPolicyRow} from '@erpx/server-inventory/domain/reorder-policies';
export {inventoryReorderPolicySchema,inventoryReorderPolicyActionSchema} from '@erpx/server-inventory/domain/reorder-policies';
export type {InventoryReorderPolicyRow} from '@erpx/server-inventory/domain/reorder-policies';
type ReorderCommandResult={id:string;status:string;version:number};
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function requireCompanyInventoryScope(actor:WorkContext){if(actor.departmentId)throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","Use a Company or Branch inventory assignment.");}
export async function listInventoryReorderPolicies(actor:WorkContext){if(!hasPermission(actor,"inventory.read"))throw new BusinessRuleError(403,"INVENTORY_FORBIDDEN","The current role cannot read inventory reorder policies.");requireCompanyInventoryScope(actor);const {write}=await holdingResources(actor.holdingId);return exposed(async()=>{return (await write.prepare(`SELECT r.id,r.company_id,r.warehouse_id,r.product_id,r.current_reorder_point_milli::text current_reorder_point_milli,
    r.proposed_reorder_point_milli::text proposed_reorder_point_milli,r.requested_by,r.approved_by,r.approved_at,r.retired_at,
    r.last_decision,r.last_reviewed_by,r.last_review_reason,r.last_reviewed_at,r.version,r.created_at,r.updated_at,
    w.name warehouse_name,w.branch_id,p.sku,p.name product_name,p.unit
    FROM inventory_reorder_policies r
    ${inventorycommerceProjection.policyWarehouseScoped}
    ${inventorycommerceProjection.policyProduct}
    WHERE r.company_id=? AND (?::text IS NULL OR w.branch_id=?)
    ORDER BY w.name,p.sku,r.id LIMIT 500`).bind(actor.companyId,actor.branchId,actor.branchId).all<InventoryReorderPolicyRow>()).results;});}
/** Writers are native-only. These retained names fail closed instead of returning to SQL effects. */
export async function saveInventoryReorderPolicy(..._input:unknown[]):Promise<ReorderCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_REORDER_REQUIRED','The native inventory reorder command is required.');}
export async function transitionInventoryReorderPolicy(..._input:unknown[]):Promise<ReorderCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_REORDER_REQUIRED','The native inventory reorder command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryReorderApplication(database:SqlDatabase){return {listInventoryReorderPolicies,saveInventoryReorderPolicy,transitionInventoryReorderPolicy,repository:database};}
