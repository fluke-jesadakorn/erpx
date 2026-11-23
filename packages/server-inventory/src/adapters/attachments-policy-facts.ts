import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {
  nativeInventoryReadFirst,
  INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
  INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LOT,
} from './native-inventory-read';
/**
 * Owner-native descriptor reads. The retired adapter interpolated a per-target
 * descriptor (table/branch/version columns, NULL department, empty extra); all
 * descriptors in use resolve to the five fixed owner plans below, so this
 * module emits no SQL. An unknown target type still fails closed with null.
 */
const descriptorKinds:Record<string,string>={
  landed_cost: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
  stock_count: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
  inventory_stock_count: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
  stock_transfer: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
  inventory_transfer: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSFER,
  inventory_transit: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
  inventory_lot: INVENTORY_RD_ATTACHMENT_DESCRIPTOR_LOT,
};
export function policyInventoryFacts(db:SqlDatabase){return {
readDescriptor:(facts:{targetId:string;companyId:string;targetType:string;})=>{
  const kind=descriptorKinds[facts.targetType];
  if(!kind)return Promise.resolve(null);
  return nativeInventoryReadFirst<{ company_id: string; branch_id: string | null; department_id: string | null; version: number; }>(kind,[facts.targetId,facts.companyId],db);
},
readTransit:(facts:{targetId:string;companyId:string;})=>nativeInventoryReadFirst<{ company_id: string; source_branch_id: string; target_branch_id: string; version: number; }>(INVENTORY_RD_TRANSIT_ATTACHMENT_DESCRIPTOR,[facts.targetId,facts.companyId],db)};}
