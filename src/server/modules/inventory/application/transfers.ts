import type {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import type {ListPage} from '../../work_projection/public/list-page';import type {prepareAttachmentBinding} from '../../files/public/attachments/service';import {decimalToMinor} from "@/lib/finance/finance-money";import {scaledAmount} from "@/lib/commerce/commerce-math";import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';import {canReadCounts} from '../domain/counts';import {transferSchema,transferActionSchema,canPrepareTransfers,canEditTransfer,canReviewTransfer,type TransferRow} from '../domain/transfers';
export interface InventoryTransferRepository<Effect>{transfers(actor:WorkContext):Promise<{results:TransferRow[]}>;
readPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}):Promise<ListPage<TransferRow&{id:string;page_key:string}>>;
scopedTransfer(actor:WorkContext,id:string):Promise<TransferRow|null>;
activeWarehouses(companyId:string):Promise<{results:TransferRow[]}>;
availableStock(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:TransferRow[]}>;
availableLots(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:TransferRow[]}>;
transferLines(source_warehouse_id:string | number | null,id:string):Promise<{results:TransferRow[]}>;
transferLots(source_warehouse_id:string | number | null,id:string):Promise<{results:TransferRow[]}>;
transferDecisions(companyId:string,id:string):Promise<{results:TransferRow[]}>;
transferHistory(companyId:string,id:string):Promise<{results:TransferRow[]}>;
guardTransferVersion(key:string,value:"transfer-version",id:string | number | null,company_id:string | number | null,version:number,status:string | number | null):Effect;
guardSourceStock(key:string,value:string,warehouse:string | number | null,product_id:string | number | null,source_quantity_milli:string | number | null,source_value_minor:string | number | null,source_version:string | number | null):Effect;
guardMovementDate(key:string,value:string,from:string | number | null,to:string | number | null,product:string | number | null,date:string | number | null):Effect;
guardWarehouseVersion(key:string,value:string,value2:string | number | null,company_id:string | number | null,value3:string | number | null,value4:string | number | null):Effect;
routeWarehouses(sourceWarehouseId:string,targetWarehouseId:string,companyId:string):Promise<{results:TransferRow[]}>;
sourceStock(id:string | number | null,productId:string,companyId:string):Promise<TransferRow|null>;
sourceLot(lotId:string,companyId:string,productId:string,id:string | number | null):Promise<TransferRow|null>;
guardProductVersion(key:string,value:string,productId:string,product_version:string | number | null):Effect;
prepareTransferRevision(branch_id:string | number | null,branch_id2:string | number | null,version:string | number | null,version2:string | number | null,transferDate:string,reason:string,total:number,userId:string,now:number,record:string):Effect;
clearTransferLots(record:string):Effect;
clearTransferLines(record:string):Effect;
prepareTransfer(record:string,companyId:string,id:string | number | null,id2:string | number | null,branch_id:string | number | null,branch_id2:string | number | null,version:string | number | null,version2:string | number | null,value:string,transferDate:string,reason:string,total:number,userId:string,userId2:string,now:number,now2:number):Effect;
prepareDeliveryMode(mode:string,due:string | number | null,record:string):Effect;
prepareTransferLine(id:string | number | null,record:string,product_id:string | number | null,product_version:string | number | null,description:string | number | null,unit:string | number | null,quantity_milli:string | number | null,value_minor:string | number | null,source_quantity_milli:string | number | null,source_value_minor:string | number | null,source_version:string | number | null,ordinal:string | number | null):Effect;
prepareTransferLot(id:string | number | null,id2:string | number | null,lot_id:string | number | null,quantity_milli:string | number | null,value_minor:string | number | null,source_balance_version:string | number | null):Effect;
attachmentPurposeFacts(companyId:string,attachmentSetIds:string[]):Promise<{results:{id:string;purpose_key:string;component_kind:string;component_ref:string|null}[]}>;
guardReviewProductVersion(key:string,value:string,product_id:string | number | null,product_version:string | number | null):Effect;
guardSourceLot(key:string,value:string,lot_id:string | number | null,source_warehouse_id:string | number | null,source_balance_version:string | number | null,quantity_milli:string | number | null):Effect;
postingPeriods(companyId:string,transfer_date:string | number | null,transfer_date2:string | number | null):Promise<{results:TransferRow[]}>;
guardOpenPeriod(key:string,value:"period",id:string | number | null):Effect;
targetStock(target_warehouse_id:string | number | null,product_id:string | number | null):Promise<TransferRow|null>;
guardTargetStockVersion(key:string,value:string,target_warehouse_id:string | number | null,product_id:string | number | null,version:string | number | null):Effect;
guardTargetAbsent(key:string,value:string,target_warehouse_id:string | number | null,product_id:string | number | null):Effect;
prepareTargetStock(target_warehouse_id:string | number | null,product_id:string | number | null):Effect;
prepareSourceStockTransfer(quantity_milli:string | number | null,value_minor:string | number | null,source_warehouse_id:string | number | null,product_id:string | number | null):Effect;
prepareTargetStockTransfer(targetQty:number,targetValue:number,target_warehouse_id:string | number | null,product_id:string | number | null):Effect;
targetLotBalance(lot_id:string | number | null,target_warehouse_id:string | number | null):Promise<TransferRow|null>;
guardTargetLotVersion(key:string,value:string,lot_id:string | number | null,target_warehouse_id:string | number | null,version:string | number | null):Effect;
guardTargetLotAbsent(key:string,value:string,lot_id:string | number | null,target_warehouse_id:string | number | null):Effect;
prepareSourceLotTransfer(quantity_milli:string | number | null,value_minor:string | number | null,lot_id:string | number | null,source_warehouse_id:string | number | null):Effect;
prepareTargetLotTransfer(lot_id:string | number | null,target_warehouse_id:string | number | null,quantity_milli:string | number | null,value_minor:string | number | null):Effect;
prepareSourceLotMove(value:string,lot_id:string | number | null,source_warehouse_id:string | number | null,value2:"transfer",value3:string,value4:number,value5:number,transfer_date:string | number | null,userId:string,now:number):Effect;
prepareTargetLotMove(value:string,lot_id:string | number | null,target_warehouse_id:string | number | null,value2:"transfer",value3:string,quantity_milli:string | number | null,value_minor:string | number | null,transfer_date:string | number | null,userId:string,now:number):Effect;
prepareTransferMove(value:string,id:string,warehouse:string | number | null,product_id:string | number | null,value2:number,value3:number,transfer_date:string | number | null,now:number):Effect;
prepareTransferDecision(status:"submitted" | "posted" | "rejected" | "cancelled",action:"submit" | "post" | "reject" | "cancel",userId:string,action2:"submit" | "post" | "reject" | "cancel",now:number,action3:"submit" | "post" | "reject" | "cancel",now2:number,now3:number,id:string):Effect;
prepareDecisionHistory(value:string,id:string,key:string,action:"post" | "reject" | "cancel",userId:string,expectedVersion:number,reason:string | undefined,now:number):Effect;
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number):Effect;}
export interface InventoryTransferCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;}
export interface InventoryTransferPorts<Effect>{repository:InventoryTransferRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:string):Promise<string>;commands:InventoryTransferCommands<Effect>;requirePermission(actor:WorkContext,permission:string):WorkContext;requireBusinessRole(actor:WorkContext,roles:string[]):WorkContext;prepareAttachment(actor:WorkContext,input:Parameters<typeof prepareAttachmentBinding>[2]):Promise<{guards:Effect[];steps:Effect[]}>;prepareTransit(actor:WorkContext,transfer:TransferRow,lines:TransferRow[]):Promise<Effect[]>;}
export function createInventoryTransferApplication<Effect>(ports:InventoryTransferPorts<Effect>){const r=ports.repository;function readable(actor:WorkContext){if(!canReadCounts(actor))throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","An inventory or finance assignment is required.");}
async function listTransfers(actor:WorkContext){readable(actor);return (await r.transfers(actor)).results;}
async function transfersPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}) {
  readable(actor);
  return r.readPage(actor,query);
}
async function getTransfer(actor:WorkContext,id:string){readable(actor);const row=await r.scopedTransfer(actor,id);if(!row)throw new BusinessRuleError(404,"TRANSFER_NOT_FOUND","Transfer not found in this workspace.");return row;}
async function transferOptions(actor:WorkContext){
  actor = ports.requirePermission(actor,"inventory.prepare");
readable(actor);const [warehouses,stock,lots]=await Promise.all([
 r.activeWarehouses(actor.companyId),
 r.availableStock(actor.companyId,actor.branchId,actor.branchId),
 r.availableLots(actor.companyId,actor.branchId,actor.branchId),
 ]);return {warehouses:warehouses.results,sources:warehouses.results.filter(w=>!actor.branchId||actor.branchId===w.branch_id),stock:stock.results,lots:lots.results};}
async function transferDetail(actor:WorkContext,id:string){const transfer=await getTransfer(actor,id);const [lines,allocations,decisions,history]=await Promise.all([
 r.transferLines(transfer.source_warehouse_id,id),
 r.transferLots(transfer.source_warehouse_id,id),
 r.transferDecisions(actor.companyId,id),
 r.transferHistory(actor.companyId,id),
 ]);return {transfer,lines:lines.results,allocations:allocations.results,decisions:decisions.results,history:history.results};}
function transferGuard(key:string,row:TransferRow,version:number){return r.guardTransferVersion(key,"transfer-version",row.id,row.company_id,version,row.status);}
function sourceGuard(key:string,index:number,warehouse:TransferRow[string],l:TransferRow){return r.guardSourceStock(key,`source-${index}`,warehouse,l.product_id,l.source_quantity_milli,l.source_value_minor,l.source_version);}
function dateGuard(key:string,index:number,from:TransferRow[string],to:TransferRow[string],product:TransferRow[string],date:TransferRow[string]){return r.guardMovementDate(key,`date-${index}`,from,to,product,date);}
function warehouseGuards(key:string,row:TransferRow){return ["source","target"].map(side=>r.guardWarehouseVersion(key,`${side}-warehouse`,row[`${side}_warehouse_id`],row.company_id,row[`${side}_branch_id`],row[`${side}_warehouse_version`]));}
async function saveTransfer(actor:WorkContext,id:string|null,input:z.infer<typeof transferSchema>,key:string){
 input=transferSchema.parse(input);
  actor = ports.requirePermission(actor,"inventory.prepare");

 if(!canPrepareTransfers(actor))throw new BusinessRuleError(403,"TRANSFER_PREPARER_REQUIRED","An inventory operator must prepare a warehouse transfer.");const record=id??`transfer-${key}`,operation=id?"inventory.transfer.update":"inventory.transfer.create",prior=await ports.commands.replay(actor,key,operation,record,input);if(prior)return prior;
 const current=id?await getTransfer(actor,id):null;const mode=input.deliveryMode??String(current?.delivery_mode??'direct');const due=input.arrivalDueDate??current?.arrival_due_date??null;if(mode==='transit'&&(!due||String(due)<input.transferDate))throw new BusinessRuleError(422,'ARRIVAL_DUE_REQUIRED','Choose an arrival date on or after the transfer date.');if(current&&current.delivery_mode!==mode)throw new BusinessRuleError(422,'DELIVERY_MODE_LOCKED','Create a new transfer to change its transport mode.');if(current&&!canEditTransfer(actor,current))throw new BusinessRuleError(403,"TRANSFER_LOCKED","Only the source preparer can edit an unsubmitted transfer.");
 if(input.sourceWarehouseId===input.targetWarehouseId)throw new BusinessRuleError(422,"SAME_WAREHOUSE","Choose different source and receiving warehouses.");
 if(current&&(current.source_warehouse_id!==input.sourceWarehouseId||current.target_warehouse_id!==input.targetWarehouseId))throw new BusinessRuleError(422,"WAREHOUSE_LOCKED","Create a new transfer to change its route.");
 if(new Set(input.lines.map(l=>l.productId)).size!==input.lines.length)throw new BusinessRuleError(422,"DUPLICATE_PRODUCT","Use one line per product.");
 const warehouses=await r.routeWarehouses(input.sourceWarehouseId,input.targetWarehouseId,actor.companyId);const source=warehouses.results.find(w=>w.id===input.sourceWarehouseId),target=warehouses.results.find(w=>w.id===input.targetWarehouseId);
 if(!source||!target||actor.branchId&&actor.branchId!==source.branch_id)throw new BusinessRuleError(422,"WAREHOUSE_INVALID","Choose an authorized source and a receiving warehouse in this company.");
 const row:TransferRow={company_id:actor.companyId,source_warehouse_id:source.id,target_warehouse_id:target.id,source_branch_id:source.branch_id,target_branch_id:target.branch_id,source_warehouse_version:source.version,target_warehouse_version:target.version};const steps=warehouseGuards(key,row),lines:TransferRow[]=[];let total=0;
 for(const [index,item]of input.lines.entries()){
  const stock=await r.sourceStock(source.id,item.productId,actor.companyId);
  const qty=decimalToMinor(item.quantity,3);if(!stock||qty<=0||qty>Number(stock.quantity_milli)-Math.max(Number(stock.reserved_milli),Number(stock.quarantined_milli)))throw new BusinessRuleError(422,"INSUFFICIENT_STOCK","Transfer quantity must be positive and within available source stock.");
  const tracking=String(stock.tracking_mode??'none'),prepared:TransferRow[]=[];let value=0;
  if(tracking==='none'){
   if(item.allocations.length)throw new BusinessRuleError(422,'LOT_ALLOCATION_UNEXPECTED','This product does not use lot or serial tracking.');
   value=scaledAmount(Number(stock.value_minor),qty,Number(stock.quantity_milli));
  }else{
   if(new Set(item.allocations.map(a=>a.lotId)).size!==item.allocations.length)throw new BusinessRuleError(422,'LOT_ALLOCATION_DUPLICATE','Use each lot or serial once per transfer line.');
   for(const allocation of item.allocations){const allocationQty=decimalToMinor(allocation.quantity,3),lot=await r.sourceLot(allocation.lotId,actor.companyId,item.productId,source.id);if(!lot||Number(lot.quality_held)||allocationQty>Number(lot.quantity_milli))throw new BusinessRuleError(409,'LOT_UNAVAILABLE','The selected lot or serial is unavailable, quarantined, or has insufficient quantity.');if(tracking==='serial'&&allocationQty!==1000)throw new BusinessRuleError(422,'SERIAL_QUANTITY_INVALID','Each serial allocation must be exactly one unit.');const allocationValue=allocationQty===Number(lot.quantity_milli)?Number(lot.value_minor):scaledAmount(Number(lot.value_minor),allocationQty,Number(lot.quantity_milli));value+=allocationValue;prepared.push({id:`${record}-${index}-${prepared.length}`,lot_id:lot.id,quantity_milli:allocationQty,value_minor:allocationValue,source_balance_version:lot.balance_version});}
   if(prepared.reduce((sum,a)=>sum+Number(a.quantity_milli),0)!==qty)throw new BusinessRuleError(422,'LOT_ALLOCATION_REQUIRED','Allocate the full transfer quantity to lots or serial numbers.');
  }
  if(value<=0)throw new BusinessRuleError(422,"VALUATION_REQUIRED","The transferred stock must have a positive recorded value.");total+=value;
  const l:TransferRow={id:`${record}-${index}`,product_id:item.productId,product_version:stock.product_version,description:stock.name,unit:stock.unit,tracking_mode:tracking,quantity_milli:qty,value_minor:value,source_quantity_milli:stock.quantity_milli,source_value_minor:stock.value_minor,source_version:stock.version,ordinal:index,allocations:JSON.stringify(prepared)};lines.push(l);steps.push(sourceGuard(key,index,source.id,l),dateGuard(key,index,source.id,target.id,item.productId,input.transferDate),r.guardProductVersion(key,`product-${index}`,item.productId,l.product_version));
 }
 if(!Number.isSafeInteger(total))throw new BusinessRuleError(422,"VALUE_OVERFLOW","Transfer value exceeds supported precision.");const now=ports.clock.now(),version=current?(input.expectedVersion??0)+1:1;
 if(current)steps.push(transferGuard(key,current,input.expectedVersion??0),r.prepareTransferRevision(source.branch_id,target.branch_id,source.version,target.version,input.transferDate,input.reason,total,actor.userId,now,record),r.clearTransferLots(record),r.clearTransferLines(record));
 else steps.push(r.prepareTransfer(record,actor.companyId,source.id,target.id,source.branch_id,target.branch_id,source.version,target.version,`TR-${(await ports.hash(record)).slice(0,24).toUpperCase()}`,input.transferDate,input.reason,total,actor.userId,actor.userId,now,now));
 steps.push(r.prepareDeliveryMode(mode,due,record));
 for(const l of lines){steps.push(r.prepareTransferLine(l.id,record,l.product_id,l.product_version,l.description,l.unit,l.quantity_milli,l.value_minor,l.source_quantity_milli,l.source_value_minor,l.source_version,l.ordinal));for(const allocation of JSON.parse(String(l.allocations)) as TransferRow[])steps.push(r.prepareTransferLot(allocation.id,l.id,allocation.lot_id,allocation.quantity_milli,allocation.value_minor,allocation.source_balance_version));}
 return ports.commands.commit(actor,key,operation,input,record,version,{id:record,status:"draft",version},steps);
}
async function transitionTransfer(actor:WorkContext,id:string,action:"submit"|"post"|"reject"|"cancel",input:z.infer<typeof transferActionSchema>,key:string){
 actor = ports.requireBusinessRole(actor, ["holding_owner","warehouse_manager","operations_manager", ...(action === "post" || action === "reject" ? ["finance_approver"] : [])]);
 input=transferActionSchema.parse(input);const operation=`inventory.transfer.${action}`,prior=await ports.commands.replay(actor,key,operation,id,input);if(prior)return prior;const {transfer,lines,allocations}=await transferDetail(actor,id),review=action==="post"||action==="reject";
 if(review?!canReviewTransfer(actor,transfer):!canEditTransfer(actor,transfer))throw new BusinessRuleError(403,"TRANSFER_ACTION_FORBIDDEN",review?"An independent receiving inventory manager or holding finance approver must review this transfer.":"Only the source preparer can submit or cancel an editable transfer.");
 if(action!=="submit"&&!input.reason)throw new BusinessRuleError(422,"REASON_REQUIRED","Record the receipt, correction or cancellation reason.");
 if(input.attachmentSetIds.length&&(action!=="post"||transfer.delivery_mode!=="direct"))throw new BusinessRuleError(422,"TRANSFER_ATTACHMENT_ACTION","Direct receipt evidence can be bound only while posting a direct warehouse transfer.");
 let attachmentBinding:null|{guards:Effect[];steps:Effect[]}=null;
 if(input.attachmentSetIds.length){const sets=(await r.attachmentPurposeFacts(actor.companyId,input.attachmentSetIds)).results;
  if(sets.length!==new Set(input.attachmentSetIds).size||sets.some(set=>set.purpose_key!=='direct_receipt_evidence'||set.component_kind!=='action'||set.component_ref!=='direct-receipt'))throw new BusinessRuleError(422,'TRANSFER_ATTACHMENT_PURPOSE','Choose only direct receipt evidence prepared for this transfer review.');
  attachmentBinding=await ports.prepareAttachment(actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'inventory_transfer',bindingType:'direct_transfer_receipt',contexts:[{purposeKey:'direct_receipt_evidence',componentKind:'action',componentRef:'direct-receipt'}]},targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${id}:${input.expectedVersion+1}`});
 }
 const status=action==="submit"?"submitted":action==="post"?"posted":action==="reject"?"rejected":"cancelled",now=ports.clock.now(),steps=[...(attachmentBinding?.guards??[]),transferGuard(key,transfer,input.expectedVersion)];
 if(action==="submit"||action==="post"){
  if(!lines.length)throw new BusinessRuleError(409,"EMPTY_TRANSFER","Add products to transfer.");steps.push(...warehouseGuards(key,transfer));
  for(const [index,l]of lines.entries()){const selected=allocations.filter(a=>a.transfer_line_id===l.id);if(l.tracking_mode!=='none'&&selected.reduce((sum,a)=>sum+Number(a.quantity_milli),0)!==Number(l.quantity_milli))throw new BusinessRuleError(409,'LOT_ALLOCATION_REQUIRED','Allocate the full transfer quantity to lots or serial numbers.');steps.push(sourceGuard(key,index,transfer.source_warehouse_id,l),dateGuard(key,index,transfer.source_warehouse_id,transfer.target_warehouse_id,l.product_id,transfer.transfer_date),r.guardReviewProductVersion(key,`product-${index}`,l.product_id,l.product_version));for(const [allocationIndex,a] of selected.entries())steps.push(r.guardSourceLot(key,`lot-${index}-${allocationIndex}`,a.lot_id,transfer.source_warehouse_id,a.source_balance_version,a.quantity_milli));}
 }
 if(action==="post"&&transfer.delivery_mode!=="transit"){
  const periods=await r.postingPeriods(actor.companyId,transfer.transfer_date,transfer.transfer_date);if(periods.results.length!==1)throw new BusinessRuleError(409,"PERIOD_UNAVAILABLE","Exactly one open period must cover the transfer date.");steps.push(r.guardOpenPeriod(key,"period",periods.results[0].id));
  for(const [index,l]of lines.entries()){
   const target=await r.targetStock(transfer.target_warehouse_id,l.product_id);const targetQty=Number(target?.quantity_milli??0)+Number(l.quantity_milli),targetValue=Number(target?.value_minor??0)+Number(l.value_minor);if(!Number.isSafeInteger(targetQty)||!Number.isSafeInteger(targetValue))throw new BusinessRuleError(422,"VALUE_OVERFLOW","Receiving stock exceeds supported precision.");
   steps.push(target?r.guardTargetStockVersion(key,`target-${index}`,transfer.target_warehouse_id,l.product_id,target.version):r.guardTargetAbsent(key,`target-${index}`,transfer.target_warehouse_id,l.product_id));
   if(!target)steps.push(r.prepareTargetStock(transfer.target_warehouse_id,l.product_id));
   steps.push(r.prepareSourceStockTransfer(l.quantity_milli,l.value_minor,transfer.source_warehouse_id,l.product_id),r.prepareTargetStockTransfer(targetQty,targetValue,transfer.target_warehouse_id,l.product_id));
   for(const [allocationIndex,a] of allocations.filter(a=>a.transfer_line_id===l.id).entries()){
    const targetLot=await r.targetLotBalance(a.lot_id,transfer.target_warehouse_id);
    if(targetLot)steps.push(r.guardTargetLotVersion(key,`target-lot-${index}-${allocationIndex}`,a.lot_id,transfer.target_warehouse_id,targetLot.version));else steps.push(r.guardTargetLotAbsent(key,`target-lot-${index}-${allocationIndex}`,a.lot_id,transfer.target_warehouse_id));
    steps.push(r.prepareSourceLotTransfer(a.quantity_milli,a.value_minor,a.lot_id,transfer.source_warehouse_id),r.prepareTargetLotTransfer(a.lot_id,transfer.target_warehouse_id,a.quantity_milli,a.value_minor),r.prepareSourceLotMove(`lot-move-${id}-${index}-${allocationIndex}-out`,a.lot_id,transfer.source_warehouse_id,'transfer',`${id}-${index}-out`,-Number(a.quantity_milli),-Number(a.value_minor),transfer.transfer_date,actor.userId,now),r.prepareTargetLotMove(`lot-move-${id}-${index}-${allocationIndex}-in`,a.lot_id,transfer.target_warehouse_id,'transfer',`${id}-${index}-in`,a.quantity_milli,a.value_minor,transfer.transfer_date,actor.userId,now));
   }
   for(const [side,warehouse,direction]of [["out",transfer.source_warehouse_id,-1],["in",transfer.target_warehouse_id,1]] as const)steps.push(r.prepareTransferMove(`${id}-${index}-${side}`,id,warehouse,l.product_id,direction*Number(l.quantity_milli),direction*Number(l.value_minor),transfer.transfer_date,now));
  }
 }
 if(action==='post'&&transfer.delivery_mode==='transit')steps.push(...await ports.prepareTransit(actor,transfer,lines));
 steps.push(r.prepareTransferDecision(status,action,actor.userId,action,now,action,now,now,id));
 if(action!=="submit")steps.push(r.prepareDecisionHistory(ports.identifiers.next(),id,key,action,actor.userId,input.expectedVersion,input.reason,now));
 if(review)steps.push(r.prepareDecisionEvidence(ports.identifiers.next(),key,id,operation,actor.userId,actor.assignmentId,input.expectedVersion,await ports.hash(JSON.stringify({reason:input.reason,lines})),now));
 if(attachmentBinding)steps.push(...attachmentBinding.steps);
 return ports.commands.commit(actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},steps);
}
return {listTransfers,transfersPage,getTransfer,transferOptions,transferDetail,saveTransfer,transitionTransfer};}
