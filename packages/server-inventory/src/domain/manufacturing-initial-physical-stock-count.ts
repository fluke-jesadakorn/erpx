import type {InitialPhysicalStockCountView,InitialPhysicalStockCountRecord} from '@erpx/server-inventory/domain/initial-physical-stock-count';
export {initialPhysicalStockCountSchema as manufacturingInitialPhysicalStockCountSchema,initialPhysicalStockCountActionSchema as manufacturingInitialPhysicalStockCountActionSchema,initialPhysicalStockCountSelectionSchema as manufacturingInitialPhysicalStockCountSelectionSchema,initialPhysicalStockCountFlowSchema as manufacturingInitialPhysicalStockCountFlowSchema} from '@erpx/server-inventory/domain/initial-physical-stock-count';
export type {InitialPhysicalStockCountAction as ManufacturingInitialPhysicalStockCountAction,InitialPhysicalStockCountInput as ManufacturingInitialPhysicalStockCountInput,InitialPhysicalStockCountResult as ManufacturingInitialPhysicalStockCountResult} from '@erpx/server-inventory/domain/initial-physical-stock-count';
export type ManufacturingInitialPhysicalStockCountRecord=Omit<InitialPhysicalStockCountRecord,'sourceKind'>&{sourceKind:'manufacturing-initial-physical-stock-count'};
export type ManufacturingInitialPhysicalStockCountView=Omit<InitialPhysicalStockCountView,'record'|'unavailableReason'|'readiness'>&{
 record:ManufacturingInitialPhysicalStockCountRecord|null;
 unavailableReason:Exclude<InitialPhysicalStockCountView['unavailableReason'],'goods-posting-required'>|'manufacturing-posting-required'|'schema-required';
 readiness:{schemaAvailable:boolean;postingConfigured:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
};
