export type PosStartupWorkspace={
 companyId:string;available:boolean;contextKey:string|null;documentCurrency:string|null;
 canCreateWarehouse:boolean;canCreateProduct:boolean;canBootstrap:boolean;canConfigureCatalogue:boolean;canApproveAccounting:boolean;
 configured:boolean|null;settingsVersion:number|null;accountingPrepared:boolean;accountingApproved:boolean;
 bankAccounts:{id:string;code:string;name:string}[];
 branches:{id:string;name:string}[];
 warehouses:{id:string;branchId:string;code:string;name:string;status:string;version:number}[];
 products:{id:string;sku:string;name:string;itemType:string;trackingMode:string;unit:string;priceMinor:number;taxBps:number;status:string;version:number}[];
 registers:{id:string;branchId:string;warehouseId:string;name:string;mode:string;active:boolean;version:number}[];
 unavailableReason:'authority-required'|'purchase-required'|'posting-setup-required'|'unsupported-existing-mapping'|null;
};
