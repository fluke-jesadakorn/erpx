import {z} from 'zod';import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";import {samePerson} from "@erpx/feature-identity_access/lib/iam";import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
export type CountRow=Record<string,string|number|null>;
export const countSchema=z.object({
  warehouseId:z.string().min(1),countDate:z.iso.date(),reason:z.string().trim().min(5).max(2000),
  expectedVersion:z.number().int().positive().optional(),
  lines:z.array(z.object({productId:z.string().min(1),countedQuantity:z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),unitCost:z.string().regex(/^\d{1,12}(\.\d{1,2})?$/).optional(),allocations:z.array(z.object({lotId:z.string().min(1).optional(),lotCode:z.string().trim().min(1).max(100).optional(),serialNumber:z.string().trim().min(1).max(160).optional(),manufacturedDate:z.union([z.iso.date(),z.literal('')]).default(''),expiryDate:z.union([z.iso.date(),z.literal('')]).default(''),countedQuantity:z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),unitCost:z.string().regex(/^\d{1,12}(\.\d{1,2})?$/).optional()})).max(500).default([])})).min(1).max(50),
});
export const countActionSchema=z.object({expectedVersion:z.number().int().positive(),attachmentSetIds:z.array(z.string().min(1)).max(10).default([]),reason:z.string().trim().min(3).max(2000).optional()});
export function canReadCounts(actor:WorkContext){return !actor.departmentId&&hasPermission(actor,"inventory.read");}
export function canPrepareCounts(actor:WorkContext){return !actor.departmentId&&hasPermission(actor,"inventory.prepare");}
export function canApproveCounts(actor:WorkContext){return !actor.departmentId&&!actor.branchId&&hasPermission(actor,"inventory.approve");}
export function canEditCount(actor:WorkContext,row:CountRow){return canPrepareCounts(actor)&&["draft","rejected"].includes(String(row.status))&&[row.created_by,row.prepared_by].some(id=>samePerson(actor,id));}
export function canReviewCount(actor:WorkContext,row:CountRow){return canApproveCounts(actor)&&["submitted","approved"].includes(String(row.status))&&![row.created_by,row.prepared_by].some(id=>samePerson(actor,id));}
export type PreparedCountLot={id:string;countLineId:string;sourceLotId:string|null;resultLotId:string;trackingMode:"lot"|"serial";lotCode:string|null;serialNumber:string|null;manufacturedDate:string|null;expiryDate:string|null;snapshotQuantity:number;snapshotValue:number;snapshotVersion:number;countedQuantity:number;countedValue:number;unitCost:number|null};
