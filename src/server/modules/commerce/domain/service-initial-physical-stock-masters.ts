import {z} from 'zod';
import {productSchema,warehouseSchema} from './commerce';
/** Creation-only forms retain the original master validation and defaults. */
export const serviceInitialPhysicalStockProductSchema=productSchema.safeExtend({itemType:z.literal('stock').default('stock'),expectedVersion:z.never().optional()}).strict();
export const serviceInitialPhysicalStockWarehouseSchema=warehouseSchema.extend({expectedVersion:z.never().optional()}).strict();
export type ServiceInitialPhysicalStockProductInput=z.infer<typeof serviceInitialPhysicalStockProductSchema>;
export type ServiceInitialPhysicalStockWarehouseInput=z.infer<typeof serviceInitialPhysicalStockWarehouseSchema>;
export type ServiceInitialPhysicalStockMasterResult={id:string;version:1};
