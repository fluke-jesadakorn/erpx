import type {InitialPhysicalStockCountView,InitialPhysicalStockCountRecord} from './initial-physical-stock-count';
export {initialPhysicalStockCountSchema as serviceInitialPhysicalStockCountSchema,initialPhysicalStockCountActionSchema as serviceInitialPhysicalStockCountActionSchema,initialPhysicalStockCountSelectionSchema as serviceInitialPhysicalStockCountSelectionSchema,initialPhysicalStockCountFlowSchema as serviceInitialPhysicalStockCountFlowSchema} from './initial-physical-stock-count';
export type {InitialPhysicalStockCountAction as ServiceInitialPhysicalStockCountAction,InitialPhysicalStockCountInput as ServiceInitialPhysicalStockCountInput,InitialPhysicalStockCountResult as ServiceInitialPhysicalStockCountResult} from './initial-physical-stock-count';
export type ServiceInitialPhysicalStockCountRecord=Omit<InitialPhysicalStockCountRecord,'sourceKind'>&{sourceKind:'service-initial-physical-stock-count'};
export type ServiceInitialPhysicalStockCountView=Omit<InitialPhysicalStockCountView,'record'|'unavailableReason'|'readiness'>&{
 record:ServiceInitialPhysicalStockCountRecord|null;
 unavailableReason:Exclude<InitialPhysicalStockCountView['unavailableReason'],'goods-posting-required'>|'service-posting-required'|'schema-required';
 readiness:{schemaAvailable:boolean;postingConfigured:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
};
