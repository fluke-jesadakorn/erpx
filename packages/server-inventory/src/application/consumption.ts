import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {prepareJournal} from '@erpx/server-finance/public/journal-planning';import type {snapshotMoney} from '@erpx/server-finance/public/company-money';import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";import {scaledAmount} from "@erpx/feature-commerce/lib/commerce-math";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import type {ConsumptionInput,LotInput} from '@erpx/server-inventory/domain/consumption-input';
export interface InventoryConsumptionRepository<Effect>{availableStock(companyId:string,branchId:string):Promise<{warehouse_id:string;warehouse_name:string;product_id:string;sku:string;product_name:string;unit:string;tracking_mode:string;available_milli:number}[]>;
availableLots(companyId:string,branchId:string):Promise<{lot_id:string;warehouse_id:string;product_id:string;lot_code:string|null;serial_number:string|null;quantity_milli:number;version:number;expiry_date:string|null}[]>;
serviceStock(companyId:string,warehouseId:string,productId:string,branchId:string):Promise<{quantity_milli:number;value_minor:number;version:number;tracking_mode:string;branch_id:string}|null>;
servicePostingConfig(companyId:string):Promise<{chart_id:string;currency:string;inventory_account_id:string;cogs_account_id:string;base_currency:string}|null>;
postingPeriods(companyId:string,entryDate:string,entryDate2:string):Promise<{id:string}[]>;
lockStock(warehouseId:string,productId:string):Effect;
guardStockVersion(key:string,value:"part-stock-version",warehouseId:string,productId:string,version:number):Effect;
guardAvailableStock(key:string,value:"part-source-availability",warehouseId:string,productId:string,value2:number,quantityMilli:number):Effect;
guardMovementDate(key:string,value:"part-date",warehouseId:string,productId:string,entryDate:string):Effect;
guardFinanceMapping(key:string,value:"part-finance-mapping",companyId:string,inventory_account_id:string,cogs_account_id:string):Effect;
serviceLot(lotId:string,warehouseId:string,productId:string,companyId:string):Promise<{quantity_milli:number;value_minor:number;version:number;tracking_mode:string;expiry_date:string|null;status:string}|null>;
lockLot(lotId:string,warehouseId:string):Effect;
guardLotAvailability(key:string,value:string,lotId:string,warehouseId:string,balanceVersion:number,quantityMilli:number,entryDate:string):Effect;
prepareLotConsumption(quantityMilli:number,cost:number,lotId:string,warehouseId:string):Effect;
prepareLotDepletion(lotId:string,lotId2:string):Effect;
prepareLotEvidence(value:string,id:string,lotId:string,quantityMilli:number,cost:number,balanceVersion:number):Effect;
prepareLotMove(value:string,lotId:string,warehouseId:string,sourceKind:"maintenance" | "field_service",id:string,value2:number,value3:number,entryDate:string,userId:string,now:number):Effect;
prepareConsumption(id:string,companyId:string,branchId:string,departmentId:string | null,sourceKind:"maintenance" | "field_service",sourceId:string,sourceVersion:number,warehouseId:string,productId:string,quantityMilli:number,value:number,currency:string,entryDate:string,id2:string,reason:string,userId:string,now:number):Effect;
prepareStockConsumption(quantityMilli:number,value:number,warehouseId:string,productId:string):Effect;}
export interface InventoryConsumptionPorts<Effect>{repository:InventoryConsumptionRepository<Effect>;clock:Clock;prepareJournal(actor:WorkContext,input:Parameters<typeof prepareJournal>[2],key:string,status:Parameters<typeof prepareJournal>[4]):Promise<{result:{id:string};steps:Effect[]}>;snapshotMoney(actor:WorkContext,input:Parameters<typeof snapshotMoney>[2]):Promise<{statement:Effect}>;}
export function createInventoryConsumptionApplication<Effect>(ports:InventoryConsumptionPorts<Effect>){const r=ports.repository;async function availableServiceSpares(a:WorkContext,branchId:string){
 const stock=(await r.availableStock(a.companyId,branchId));
 const lots=(await r.availableLots(a.companyId,branchId));
 return {stock,lots};
}
async function prepareInventoryConsumption(a:WorkContext,input:ConsumptionInput<Effect>){
 const stock=await r.serviceStock(a.companyId,input.warehouseId,input.productId,input.branchId);
 if(!stock||stock.quantity_milli<input.quantityMilli)throw new BusinessRuleError(409,'PART_STOCK_UNAVAILABLE','Choose available spare stock from the service branch warehouse.');
 const config=await r.servicePostingConfig(a.companyId);
 if(!config||config.currency!==config.base_currency)throw new BusinessRuleError(422,'PART_FUNCTIONAL_CURRENCY_REQUIRED','Review the functional-currency inventory and service expense adapter before consuming parts.');
 const periods=(await r.postingPeriods(a.companyId,input.entryDate,input.entryDate));
 if(periods.length!==1)throw new BusinessRuleError(409,'PART_POSTING_PERIOD_REQUIRED','Choose a date in one open Finance period.');
 const id=`operational-part-${input.key}`,now=ports.clock.now(),steps:Effect[]=[
  r.lockStock(input.warehouseId,input.productId),
  r.guardStockVersion(input.key,'part-stock-version',input.warehouseId,input.productId,stock.version),
  r.guardAvailableStock(input.key,'part-source-availability',input.warehouseId,input.productId,input.ownReservationMilli??0,input.quantityMilli),
  r.guardMovementDate(input.key,'part-date',input.warehouseId,input.productId,input.entryDate),
  r.guardFinanceMapping(input.key,'part-finance-mapping',a.companyId,config.inventory_account_id,config.cogs_account_id),
 ];
 let value=0;const lots:Array<LotInput&{value:number}>=[];
 if(stock.tracking_mode==='none'){
  if(input.allocations.length)throw new BusinessRuleError(422,'PART_LOT_NOT_ALLOWED','This spare does not use lot or serial tracking.');
  value=input.quantityMilli===stock.quantity_milli?stock.value_minor:scaledAmount(stock.value_minor,input.quantityMilli,stock.quantity_milli);
 }else{
  if(!input.allocations.length||new Set(input.allocations.map(l=>l.lotId)).size!==input.allocations.length||input.allocations.reduce((n,l)=>n+l.quantityMilli,0)!==input.quantityMilli)
   throw new BusinessRuleError(422,'PART_LOT_ALLOCATION_REQUIRED','Select the full spare quantity from distinct available lots or serials.');
  for(const [index,allocation] of input.allocations.entries()){
   const lot=await r.serviceLot(allocation.lotId,input.warehouseId,input.productId,a.companyId);
   if(!lot||lot.status!=='active'||lot.version!==allocation.balanceVersion||lot.quantity_milli<allocation.quantityMilli||lot.expiry_date&&lot.expiry_date<input.entryDate||lot.tracking_mode==='serial'&&allocation.quantityMilli!==1000)
    throw new BusinessRuleError(409,'PART_LOT_CHANGED','Reload an available unexpired spare lot or whole serial.');
   const cost=allocation.quantityMilli===lot.quantity_milli?lot.value_minor:scaledAmount(lot.value_minor,allocation.quantityMilli,lot.quantity_milli);value+=cost;lots.push({...allocation,value:cost});
   steps.push(r.lockLot(allocation.lotId,input.warehouseId),
    r.guardLotAvailability(input.key,`part-lot-${index}`,allocation.lotId,input.warehouseId,allocation.balanceVersion,allocation.quantityMilli,input.entryDate),
    r.prepareLotConsumption(allocation.quantityMilli,cost,allocation.lotId,input.warehouseId),
    r.prepareLotDepletion(allocation.lotId,allocation.lotId),
    r.prepareLotEvidence(`${id}-lot-${index}`,id,allocation.lotId,allocation.quantityMilli,cost,allocation.balanceVersion),
    r.prepareLotMove(`${id}-lot-move-${index}`,allocation.lotId,input.warehouseId,input.sourceKind,id,-allocation.quantityMilli,-cost,input.entryDate,a.userId,now));
  }
 }
 if(value<=0)throw new BusinessRuleError(422,'PART_VALUATION_REQUIRED','Reconcile positive spare valuation before posting its service cost.');
 const journal=await ports.prepareJournal(a,{chartId:config.chart_id,periodId:periods[0].id,entryDate:input.entryDate,currency:config.currency,
  memo:`${input.sourceKind} spare usage ${input.sourceId} · ${input.reason}`,sourceType:`${input.sourceKind}_parts`,sourceId:id,isOpening:false,
  lines:[{accountId:config.cogs_account_id,debit:minorToDecimal(value),credit:'0'},{accountId:config.inventory_account_id,debit:'0',credit:minorToDecimal(value)}]},`${input.key}:parts`,'draft');
 const money=await ports.snapshotMoney(a,{sourceType:'operational_inventory_consumption',sourceId:id,amountKind:'stock_cost',transactionDate:input.entryDate,currency:config.currency,currencyScale:2,amountUnits:String(value)});
 // Insert source before its lot evidence to retain immediate FK checks; all writes
 // and the deferred lot-control totals remain in the owning command transaction.
 const beforeLots=steps.splice(0,5);
 return {id,journalId:journal.result.id,currency:config.currency,valueMinor:value,lots,
  steps:[...beforeLots,...(input.beforeStock??[]),...journal.steps,
   r.prepareConsumption(id,a.companyId,input.branchId,input.departmentId,input.sourceKind,input.sourceId,input.sourceVersion,input.warehouseId,input.productId,input.quantityMilli,value,config.currency,input.entryDate,journal.result.id,input.reason,a.userId,now),
   ...steps,money.statement,
   r.prepareStockConsumption(input.quantityMilli,value,input.warehouseId,input.productId)]};
}
return {availableServiceSpares,prepareInventoryConsumption};}
