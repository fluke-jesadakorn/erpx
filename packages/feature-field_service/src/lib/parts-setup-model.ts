/** Presentation-owned Service parts mapping shape; no owner authority is implied. */
export type ServicePartsMapping = {
 chartId:string;currency:string;receivableAccountId:string;payableAccountId:string;revenueAccountId:string;
 inventoryAccountId:string;clearingAccountId:string;cogsAccountId:string;inputTaxAccountId:string;outputTaxAccountId:string;cashAccountId:string;
};
export type ServicePartsSalesDefaultIdentity={id:string;version:number};
export type ServicePartsPostingSetupInput=ServicePartsMapping&{expectedVersion:0;contextKey:string;reason:string;expectedSalesDefaults?:ServicePartsSalesDefaultIdentity|null};
export type ServicePartsExistingPostingSetupInput={expectedVersion:0;contextKey:string;reason:string};
export type ServicePartsPostingSetupView={
 companyId:string;available:boolean;schemaAvailable:boolean;configured:boolean|null;sourceReady:boolean;version:0|1|null;
 canConfigure:boolean;canConfirmExisting:boolean;contextKey:string|null;configuration:ServicePartsMapping|null;
 charts:{id:string;name:string;baseCurrency:string}[];
 accounts:{id:string;chartId:string;code:string;name:string;category:string;currency:string;controlKind:string}[];
 expectedSalesDefaults?:ServicePartsSalesDefaultIdentity|null;
 approvedSalesDefaults?:(ServicePartsSalesDefaultIdentity&{chartId:string;currency:string;receivableAccountId:string;revenueAccountId:string;taxAccountId:string|null})|null;
 unavailableReason:'authority-required'|'purchase-required'|'schema-required'|'history-requires-review'|'invalid-existing-mapping'|'sales-defaults-required'|null;
};

export const servicePartsPostingAccountFields=['receivableAccountId','payableAccountId','revenueAccountId','inventoryAccountId','clearingAccountId','cogsAccountId','inputTaxAccountId','outputTaxAccountId','cashAccountId'] as const;
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const context=(value:unknown)=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
export function validServicePartsPostingSetup(value:unknown,companyId:string):value is ServicePartsPostingSetupView{
 if(!row(value)||value.companyId!==companyId||!['available','schemaAvailable','sourceReady','canConfigure','canConfirmExisting'].every(key=>typeof value[key]==='boolean')||!Array.isArray(value.charts)||!Array.isArray(value.accounts))return false;
 if(!value.charts.every(item=>row(item)&&['id','name','baseCurrency'].every(field=>text(item[field])))||!value.accounts.every(item=>row(item)&&['id','chartId','code','name','category','currency','controlKind'].every(field=>text(item[field]))))return false;
 if(![null,'authority-required','purchase-required','schema-required','history-requires-review','invalid-existing-mapping','sales-defaults-required'].includes(value.unavailableReason as string|null))return false;
 const expected=value.expectedSalesDefaults,approved=value.approvedSalesDefaults;
 const legacy=expected===undefined&&approved===undefined,empty=expected===null&&approved===null;
 if(!legacy&&!empty&&(!row(expected)||!text(expected.id)||!Number.isSafeInteger(expected.version)||Number(expected.version)<1||!row(approved)||!['id','chartId','currency','receivableAccountId','revenueAccountId'].every(field=>text(approved[field]))||!Number.isSafeInteger(approved.version)||Number(approved.version)<1||!(approved.taxAccountId===null||text(approved.taxAccountId))||approved.id!==expected.id||approved.version!==expected.version))return false;
 if(!value.available&&!legacy&&!empty)return false;
 if(!value.available)return value.configured===null&&value.version===null&&value.contextKey===null&&value.configuration===null&&!value.canConfigure&&!value.canConfirmExisting&&!value.sourceReady&&value.charts.length===0&&value.accounts.length===0;
 if(!(value.contextKey===null||context(value.contextKey)))return false;
 return (value.configured===false&&value.version===0&&value.configuration===null||value.configured===true&&(value.version===0||value.version===1)&&row(value.configuration)&&['chartId','currency',...servicePartsPostingAccountFields].every(field=>text((value.configuration as Record<string,unknown>)[field])))&&(!value.sourceReady||value.schemaAvailable===true&&value.configured===true&&value.version===1)&&(!value.canConfigure||value.schemaAvailable===true&&value.configured===false&&context(value.contextKey))&&(!value.canConfirmExisting||value.schemaAvailable===true&&value.configured===true&&!value.sourceReady&&context(value.contextKey));
}
