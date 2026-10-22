import {z} from 'zod';
import {productSchema,warehouseSchema} from '@erpx/server-commerce/domain/commerce';
/** Original stock/service master semantics, creation only; no updates or parties. */
export const posStartupProductSchema=productSchema.safeExtend({expectedVersion:z.never().optional()}).strict();
export const posStartupWarehouseSchema=warehouseSchema.extend({expectedVersion:z.never().optional()}).strict();
export type PosStartupProductInput=z.infer<typeof posStartupProductSchema>;
export type PosStartupWarehouseInput=z.infer<typeof posStartupWarehouseSchema>;
export type PosStartupMasterResult={id:string;version:1};
