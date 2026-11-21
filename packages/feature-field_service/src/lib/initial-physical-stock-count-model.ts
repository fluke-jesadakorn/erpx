/** Service Count interaction shapes. These describe the retained UI exchange;
 * authority, validation and stock-count policy remain with their owners. */
export type ServiceInitialPhysicalCountAction='submit'|'approve'|'post'|'reject'|'cancel';
export type ServiceInitialPhysicalCountDecision={expectedVersion:number;attachmentSetIds?:string[];reason?:string};
export type ServiceInitialPhysicalCountSelection={countId?:string;warehouseId?:string;productId?:string;countDate?:string};
export type ServiceInitialPhysicalCountInput={
 expectedVersion:0;contextKey:string;
 input:{warehouseId:string;countDate:string;reason:string;lines:{productId:string;countedQuantity:string;unitCost?:string;allocations?:{lotId?:string;lotCode?:string;serialNumber?:string;manufacturedDate?:string;expiryDate?:string;countedQuantity:string;unitCost?:string}[]}[]};
};
export type ServiceInitialPhysicalCountProductInput={itemType?:'stock';sku:string;name:string;unit?:string;trackingMode?:'none'|'lot'|'serial';price:string;taxBps?:number;status?:'active'|'archived';expectedVersion?:never};
export type ServiceInitialPhysicalCountWarehouseInput={code:string;name:string;branchId:string;status?:'active'|'archived';expectedVersion?:never};
export type ServiceInitialPhysicalCountRecord={
 sourceKind:'service-initial-physical-stock-count';id:string;number:string;status:string;version:number;countDate:string;reason:string;
 warehouseId:string;warehouseName:string;productId:string;sku:string;productName:string;unit:string;trackingMode:string;countedQuantity:string;countedValue:string;unitCost:string|null;
 lots:{lotCode:string|null;serialNumber:string|null;manufacturedDate:string|null;expiryDate:string|null;countedQuantity:string;unitCost:string|null}[];
 journalId:string|null;canSubmit:boolean;canApprove:boolean;canPost:boolean;canReject:boolean;canCancel:boolean;
 decisions:{action:string;reason:string|null;actorName:string;createdAt:number}[];
};
export type ServiceInitialPhysicalCountView={
 companyId:string;available:boolean;currency:string|null;documentCurrency:string|null;contextKey:string|null;
 unavailableReason:'purchase-required'|'stock-authority-required'|'service-posting-required'|'selection-required'|'selection-unavailable'|'variance-account-required'|'period-required'|'prior-stock-history'|'schema-required'|null;
 canPrepare:boolean;canCreateWarehouse:boolean;canCreateProduct:boolean;selection:{warehouseId:string|null;productId:string|null;countDate:string|null};
 readiness:{schemaAvailable:boolean;postingConfigured:boolean;varianceAccountAvailable:boolean;postingPeriodAvailable:boolean};
 branches:{id:string;name:string}[];warehouses:{id:string;name:string;branchId:string}[];products:{id:string;sku:string;name:string;unit:string;trackingMode:string}[];
 record:ServiceInitialPhysicalCountRecord|null;
};
