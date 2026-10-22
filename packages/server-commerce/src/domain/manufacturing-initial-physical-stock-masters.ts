import {z} from 'zod';
import {productSchema,warehouseSchema} from '@erpx/server-commerce/domain/commerce';
/** Creation-only forms retain the original master validation and defaults. */
export const manufacturingInitialPhysicalStockProductSchema=productSchema.safeExtend({itemType:z.literal('stock').default('stock'),expectedVersion:z.never().optional()}).strict();
export const manufacturingInitialPhysicalStockWarehouseSchema=warehouseSchema.extend({expectedVersion:z.never().optional()}).strict();
export type ManufacturingInitialPhysicalStockProductInput=z.infer<typeof manufacturingInitialPhysicalStockProductSchema>;
export type ManufacturingInitialPhysicalStockWarehouseInput=z.infer<typeof manufacturingInitialPhysicalStockWarehouseSchema>;
export type ManufacturingInitialPhysicalStockMasterResult={id:string;version:1};
