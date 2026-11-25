/**
 * Read-only inventory reorder-policy compatibility surface.
 *
 * Reorder propose/approve/reject/withdraw/retire now belong to the native
 * command mount (`packages/server-inventory/src/public/reorder_policies.rs` +
 * composition); every write entry point fails closed with
 * `NATIVE_INVENTORY_REORDER_REQUIRED`. The policy list read is owned by the
 * native lane 17 axum route: this adapter transports bounded identities only
 * and fails closed when the native owner is unavailable. No SQL remains here.
 */
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {
  readLane17ReplenishmentRows,
  LANE17_RD_REORDER_POLICIES,
} from './native-lane-17/client';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import "server-only";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from '@erpx/server-shared/index';
import {inventoryReorderPolicySchema,inventoryReorderPolicyActionSchema,type InventoryReorderPolicyRow} from '@erpx/server-inventory/domain/reorder-policies';
export {inventoryReorderPolicySchema,inventoryReorderPolicyActionSchema} from '@erpx/server-inventory/domain/reorder-policies';
export type {InventoryReorderPolicyRow} from '@erpx/server-inventory/domain/reorder-policies';
type ReorderCommandResult={id:string;status:string;version:number};
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
function requireCompanyInventoryScope(actor:WorkContext){if(actor.departmentId)throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","Use a Company or Branch inventory assignment.");}
export async function listInventoryReorderPolicies(actor:WorkContext){if(!hasPermission(actor,"inventory.read"))throw new BusinessRuleError(403,"INVENTORY_FORBIDDEN","The current role cannot read inventory reorder policies.");requireCompanyInventoryScope(actor);return exposed(async()=>readLane17ReplenishmentRows<InventoryReorderPolicyRow>(LANE17_RD_REORDER_POLICIES,[actor.companyId,actor.branchId,actor.branchId],actor));}
/** Writers are native-only. These retained names fail closed instead of returning to SQL effects. */
export async function saveInventoryReorderPolicy(..._input:unknown[]):Promise<ReorderCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_REORDER_REQUIRED','The native inventory reorder command is required.');}
export async function transitionInventoryReorderPolicy(..._input:unknown[]):Promise<ReorderCommandResult>{void _input;throw new ApiError(503,'NATIVE_INVENTORY_REORDER_REQUIRED','The native inventory reorder command is required.');}
/** Retained diagnostic shape for engineering fixtures; writes fail closed to native. */
export function inventoryReorderApplication(database:SqlDatabase){return {listInventoryReorderPolicies,saveInventoryReorderPolicy,transitionInventoryReorderPolicy,repository:database};}
