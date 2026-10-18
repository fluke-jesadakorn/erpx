import {z} from 'zod';
import {countSchema,countActionSchema} from './counts';

/** Original physical-count variance only, never an opening-equity balance or
 * a Purchase acquisition. The first bounded source contains one stock item. */
const allocation=countSchema.shape.lines.element.shape.allocations.unwrap().element.strict();
const line=countSchema.shape.lines.element.extend({allocations:z.array(allocation).max(500).default([])}).strict();
export const initialPhysicalStockCountSchema=z.object({
 expectedVersion:z.literal(0),contextKey:z.string().regex(/^[a-f0-9]{64}$/),
 input:countSchema.omit({expectedVersion:true}).extend({lines:z.array(line).length(1)}).strict(),
}).strict();
export const initialPhysicalStockCountActionSchema=countActionSchema.strict();
export const initialPhysicalStockCountSelectionSchema=z.object({
 countId:z.string().min(1).optional(),warehouseId:z.string().min(1).optional(),
 productId:z.string().min(1).optional(),countDate:z.iso.date().optional(),
}).strict();
export type InitialPhysicalStockCountInput=z.input<typeof initialPhysicalStockCountSchema>;
export type InitialPhysicalStockCountAction='submit'|'approve'|'post'|'reject'|'cancel';
export type InitialPhysicalStockCountActionInput=z.input<typeof initialPhysicalStockCountActionSchema>;
export type InitialPhysicalStockCountSelection=z.input<typeof initialPhysicalStockCountSelectionSchema>;
export type InitialPhysicalStockCountResult={id:string;status:string;version:number;journalId?:string|null};
export type InitialPhysicalStockCountRecord={
 sourceKind:'initial-physical-stock-count';
 id:string;number:string;status:string;version:number;countDate:string;reason:string;
 warehouseId:string;warehouseName:string;productId:string;sku:string;productName:string;
 unit:string;trackingMode:string;countedQuantity:string;countedValue:string;unitCost:string|null;
 lots:{lotCode:string|null;serialNumber:string|null;manufacturedDate:string|null;expiryDate:string|null;countedQuantity:string;unitCost:string|null}[];
 journalId:string|null;canSubmit:boolean;canApprove:boolean;canPost:boolean;canReject:boolean;canCancel:boolean;
 decisions:{action:string;reason:string|null;actorName:string;createdAt:number}[];
};
export type InitialPhysicalStockCountView={
 companyId:string;available:boolean;currency:string|null;contextKey:string|null;
 unavailableReason:'purchase-required'|'stock-authority-required'|'goods-posting-required'|'selection-required'|'selection-unavailable'|'variance-account-required'|'period-required'|'prior-stock-history'|null;
 canPrepare:boolean;canCreateWarehouse:boolean;canCreateProduct:boolean;documentCurrency:string|null;selection:{warehouseId:string|null;productId:string|null;countDate:string|null};
 readiness:{goodsConfigured:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
 warehouses:{id:string;name:string;branchId:string}[];
 branches:{id:string;name:string}[];
 products:{id:string;sku:string;name:string;unit:string;trackingMode:string}[];
 record:InitialPhysicalStockCountRecord|null;
};

export const initialPhysicalStockCountFlowSchema=z.object({stepKey:z.string().min(1).max(100),userId:z.string().min(1).max(180),expectedVersion:z.number().int().min(0),recordVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000),saveDefault:z.literal(false).default(false)}).strict();
export type InitialPhysicalStockCountFlowInput=z.input<typeof initialPhysicalStockCountFlowSchema>;
