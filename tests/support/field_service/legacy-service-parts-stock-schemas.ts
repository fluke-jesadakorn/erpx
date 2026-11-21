// Test-only retained schema oracle; never imported by product/runtime code.
// Exact original source: packages/server-commerce/src/domain/service-initial-physical-stock-masters.ts
// Original SHA-256: 1245e6e813b4e9db57bbc3e127c0b4f5fb9e0092cecb0968a760ff09217dc53f
import {z} from 'zod';
import {productSchema,warehouseSchema} from '@erpx/server-commerce/domain/commerce';
/** Creation-only forms retain the original master validation and defaults. */
export const serviceInitialPhysicalStockProductSchema=productSchema.safeExtend({itemType:z.literal('stock').default('stock'),expectedVersion:z.never().optional()}).strict();
export const serviceInitialPhysicalStockWarehouseSchema=warehouseSchema.extend({expectedVersion:z.never().optional()}).strict();
export type ServiceInitialPhysicalStockProductInput=z.infer<typeof serviceInitialPhysicalStockProductSchema>;
export type ServiceInitialPhysicalStockWarehouseInput=z.infer<typeof serviceInitialPhysicalStockWarehouseSchema>;
export type ServiceInitialPhysicalStockMasterResult={id:string;version:1};
