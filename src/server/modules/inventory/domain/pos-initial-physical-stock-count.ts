import type {InitialPhysicalStockCountView,InitialPhysicalStockCountRecord} from './initial-physical-stock-count';
export {initialPhysicalStockCountSchema as posInitialPhysicalStockCountSchema,initialPhysicalStockCountActionSchema as posInitialPhysicalStockCountActionSchema,initialPhysicalStockCountSelectionSchema as posInitialPhysicalStockCountSelectionSchema,initialPhysicalStockCountFlowSchema as posInitialPhysicalStockCountFlowSchema} from './initial-physical-stock-count';
export type {InitialPhysicalStockCountAction as PosInitialPhysicalStockCountAction,InitialPhysicalStockCountInput as PosInitialPhysicalStockCountInput,InitialPhysicalStockCountResult as PosInitialPhysicalStockCountResult} from './initial-physical-stock-count';
export type PosInitialPhysicalStockCountRecord=Omit<InitialPhysicalStockCountRecord,'sourceKind'>&{sourceKind:'pos-initial-physical-stock-count'};
export type PosInitialPhysicalStockCountView=Omit<InitialPhysicalStockCountView,'record'|'unavailableReason'|'readiness'>&{
 record:PosInitialPhysicalStockCountRecord|null;
 unavailableReason:Exclude<InitialPhysicalStockCountView['unavailableReason'],'goods-posting-required'>|'pos-posting-required'|'pos-accounting-required'|'schema-required';
 readiness:{schemaAvailable:boolean;postingConfigured:boolean;accountingApproved:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
};
