import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {InitialPhysicalStockCountAction,InitialPhysicalStockCountRecord,InitialPhysicalStockCountView} from '@erpx/server-inventory/domain/initial-physical-stock-count';
import type {FlowView} from '@erpx/feature-shared/lib/flow';

type HistoricalRecord=Omit<InitialPhysicalStockCountRecord,'sourceKind'>&{sourceKind:'service-initial-physical-stock-count'};
type HistoricalView=Omit<InitialPhysicalStockCountView,'record'|'unavailableReason'|'readiness'>&{
 record:HistoricalRecord|null;
 unavailableReason:Exclude<InitialPhysicalStockCountView['unavailableReason'],'goods-posting-required'>|'service-posting-required'|'schema-required';
 readiness:{schemaAvailable:boolean;postingConfigured:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
};
type ProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string};
type HistoricalCountOracle={
 serviceInitialPhysicalStockCountWorkspace(actor:WorkContext,raw?:unknown):Promise<HistoricalView>;
 prepareServiceInitialPhysicalStockCount(actor:WorkContext,raw:unknown,key:string):Promise<{id:string;status:string;version:number}>;
 transitionServiceInitialPhysicalStockCount(actor:WorkContext,id:string,action:InitialPhysicalStockCountAction,raw:unknown,key:string):Promise<Record<string,unknown>>;
 serviceInitialPhysicalStockCountFlow(actor:WorkContext,id:string):Promise<FlowView>;
 assignServiceInitialPhysicalStockCountFlow(actor:WorkContext,id:string,raw:unknown,key:string):Promise<Record<string,unknown>>;
 serviceInitialPhysicalStockCountProductEntry(db:SqlDatabase,scope:ProductScope,target:{countId?:string}):Promise<{capability:'service.parts-assets';purchasePurpose:'history'}|null>;
};
/** Explicit historical interoperability reference, never a production backend. */
export async function historicalServiceCount():Promise<HistoricalCountOracle>{
 // @ts-expect-error This virtual module exists only in the integration-test plugin.
 return import('virtual:service-count-historical') as Promise<HistoricalCountOracle>;
}
