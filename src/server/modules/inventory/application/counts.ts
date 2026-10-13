import type {z} from 'zod';import type {WorkContext} from '../../identity_access/public/authorization';import type {ListPage} from '../../work_projection/public/list-page';import type {prepareJournal} from '../../finance/public/finance';import type {prepareBusinessEvidence} from '../../records/public/business-evidence';import {decimalToMinor,minorToDecimal} from "@/lib/finance/finance-money";import {scaledAmount} from "@/lib/commerce/commerce-math";import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';import {countSchema,countActionSchema,canReadCounts,canPrepareCounts,canEditCount,canReviewCount,type CountRow,type PreparedCountLot} from '../domain/counts';
export interface InventoryCountRepository<Effect>{counts(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:CountRow[]}>;
readPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}):Promise<ListPage<CountRow&{id:string;page_key:string}>>;
scopedCount(id:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<CountRow|null>;
countLines(warehouse_id:string | number | null,id:string):Promise<{results:CountRow[]}>;
countLotLines(warehouse_id:string | number | null,id:string):Promise<{results:CountRow[]}>;
countHistory(companyId:string,id:string):Promise<{results:CountRow[]}>;
countDecisions(companyId:string,id:string):Promise<{results:CountRow[]}>;
activeWarehouses(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:CountRow[]}>;
activeProducts(companyId:string):Promise<{results:CountRow[]}>;
stockOptions(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:CountRow[]}>;
lotOptions(companyId:string,branchId:string | null,branchId2:string | null):Promise<{results:CountRow[]}>;
functionalCurrency(companyId:string):Promise<{currency:string}|null>;
guardCountVersion(key:string,value:"count-version",id:string | number | null,company_id:string | number | null,version:number,statuses:string[]):Effect;
guardAbsentStock(key:string,value:string,warehouse:string | number | null,product_id:string | number | null):Effect;
guardStockSnapshot(key:string,value:string,warehouse:string | number | null,product_id:string | number | null,snapshot_version:string | number | null,snapshot_quantity_milli:string | number | null,snapshot_value_minor:string | number | null):Effect;
activeWarehouse(warehouseId:string,companyId:string,branchId:string | null,branchId2:string | null):Promise<CountRow|null>;
activeProduct(productId:string,companyId:string):Promise<CountRow|null>;
currentStock(id:string | number | null,productId:string):Promise<CountRow|null>;
currentProductLots(companyId:string,productId:string,id:string | number | null):Promise<{results:CountRow[]}>;
guardProductVersion(key:string,value:string,productId:string,version:string | number | null):Effect;
guardMovementDate(key:string,value:string,id:string | number | null,productId:string,countDate:string):Effect;
guardWarehouseVersion(key:string,value:"warehouse",id:string | number | null,version:string | number | null):Effect;
prepareCountRevision(countDate:string,reason:string,value:number,userId:string,now:number,record:string):Effect;
clearCountLots(record:string):Effect;
clearCountLines(record:string):Effect;
prepareCount(record:string,companyId:string,branch_id:string | number | null,id:string | number | null,value:string,countDate:string,reason:string,value2:number,userId:string,userId2:string,now:number,now2:number):Effect;
prepareCountLine(id:string | number | null,record:string,product_id:string | number | null,product_version:string | number | null,description:string | number | null,unit:string | number | null,snapshot_quantity_milli:string | number | null,snapshot_value_minor:string | number | null,snapshot_version:string | number | null,counted_quantity_milli:string | number | null,counted_value_minor:string | number | null,unit_cost_minor:string | number | null,ordinal:string | number | null):Effect;
prepareCountLotLine(id:string,countLineId:string,sourceLotId:string | null,resultLotId:string,trackingMode:"lot" | "serial",lotCode:string | null,serialNumber:string | null,manufacturedDate:string | null,expiryDate:string | null,snapshotQuantity:number,snapshotValue:number,snapshotVersion:number,countedQuantity:number,countedValue:number,unitCost:number | null):Effect;
qualityHold(quality_hold_id:string | number,companyId:string):Promise<CountRow|null>;
guardWarehouseActive(key:string,value:"warehouse",warehouse_id:string | number | null,companyId:string):Effect;
guardReviewProductVersion(key:string,value:string,product_id:string | number | null,product_version:string | number | null):Effect;
guardCountLotSnapshot(key:string,value:string,source_lot_id:string | number,warehouse_id:string | number | null,snapshot_version:string | number | null,snapshot_quantity_milli:string | number | null,snapshot_value_minor:string | number | null):Effect;
guardNewLotIdentity(key:string,value:string,result_lot_id:string | number | null,companyId:string,product_id:string | number | null,lot_code:string | number | null,serial_number:string | number | null):Effect;
variancePostingConfig(companyId:string):Promise<CountRow|null>;
postingPeriods(companyId:string,count_date:string | number | null,count_date2:string | number | null):Promise<{results:CountRow[]}>;
guardQualityDisposition(key:string,value:"quality-disposition",id:string | number | null,companyId:string,id2:string,dispositionQuantity:number,heldQuantity:number):Effect;
guardRemainingHoldUnique(key:string,value:"quality-remaining-hold",remainingHoldId:string):Effect;
prepareRemainingHold(remainingHoldId:string,company_id:string | number | null,branch_id:string | number | null,warehouse_id:string | number | null,product_id:string | number | null,value:string,due_date:string | number | null,created_by:string | number | null,now:number,now2:number,lot_id:string | number | null,remainingQuantity:number,source_type:string | number | null,remainingSourceId:string | null,spec_id:string | number | null,spec_version:string | number | null,value2:"partial"):Effect;
prepareRemainingHoldNotice(value:string,company_id:string | number | null,remainingHoldId:string,now:number,company_id2:string | number | null,branch_id:string | number | null,now2:number):Effect;
prepareQualityDisposal(userId:string,now:number,now2:number,id:string | number | null):Effect;
guardPostingMovementDate(key:string,value:string,warehouse_id:string | number | null,product_id:string | number | null,count_date:string | number | null):Effect;
prepareStock(warehouse_id:string | number | null,product_id:string | number | null):Effect;
prepareCountedStock(counted_quantity_milli:string | number | null,counted_value_minor:string | number | null,warehouse_id:string | number | null,product_id:string | number | null):Effect;
prepareAdjustmentMove(value:string,id:string,warehouse_id:string | number | null,product_id:string | number | null,qty:number,value2:number,count_date:string | number | null,now:number):Effect;
prepareCountedLotBalance(counted_quantity_milli:string | number | null,counted_value_minor:string | number | null,source_lot_id:string | number,warehouse_id:string | number | null):Effect;
prepareCountedLotStatus(counted_quantity_milli:string | number | null,source_lot_id:string | number):Effect;
prepareFoundLot(result_lot_id:string | number | null,companyId:string,product_id:string | number | null,tracking_mode:string | number | null,lot_code:string | number | null,serial_number:string | number | null,manufactured_date:string | number | null,expiry_date:string | number | null,value:"active",userId:string,now:number):Effect;
prepareFoundLotBalance(result_lot_id:string | number | null,warehouse_id:string | number | null,counted_quantity_milli:string | number | null,counted_value_minor:string | number | null):Effect;
prepareAdjustmentLotMove(value:string,result_lot_id:string | number | null,warehouse_id:string | number | null,value2:"stock_count",value3:string,lotQty:number,lotValue:number,count_date:string | number | null,userId:string,now:number):Effect;
prepareScrappedLot(lot_id:string | number,lot_id2:string | number):Effect;
guardOpenPeriod(key:string,value:"open-period",id:string | number | null):Effect;
prepareCountDecision(status:"submitted" | "posted" | "approved" | "rejected" | "cancelled",value:string | null,action:"submit" | "reject" | "cancel" | "approve" | "post",userId:string,action2:"submit" | "reject" | "cancel" | "approve" | "post",now:number,action3:"submit" | "reject" | "cancel" | "approve" | "post",now2:number,journalId:string | null,now3:number,id:string):Effect;
prepareQualityRejection(now:number,id:string | number | null,id2:string):Effect;
prepareQualityRejectionEvent(value:string,id:string | number | null,id2:string,userId:string,version:string | number | null,reason:string | undefined,now:number):Effect;
prepareQualityPostEvent(value:string,id:string | number | null,id2:string,userId:string,version:string | number | null,reason:string | undefined,now:number):Effect;
prepareQualityRequesterNotice(value:string,companyId:string,disposition_requested_by:string | number | null,value2:"quality_hold",id:string | number | null,value3:number,value4:"Quality scrap posted with inventory and accounting evidence",now:number):Effect;
prepareDecisionHistory(value:string,id:string,key:string,action:"reject" | "cancel" | "approve" | "post",userId:string,expectedVersion:number,reason:string | undefined,now:number):Effect;
prepareDecisionEvidence(value:string,key:string,id:string,operation:string,userId:string,assignmentId:string,expectedVersion:number,value2:string,now:number):Effect;}
export interface InventoryCountCommands<Effect>{replay<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;commit<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[]):Promise<Result>;}
export interface InventoryCountPorts<Effect>{repository:InventoryCountRepository<Effect>;clock:Clock;identifiers:IdentifierSource;hash(value:string):Promise<string>;commands:InventoryCountCommands<Effect>;requirePermission(actor:WorkContext,permission:string):WorkContext;prepareJournal(actor:WorkContext,input:Parameters<typeof prepareJournal>[2],key:string,status:Parameters<typeof prepareJournal>[4]):Promise<{result:{id:string};steps:Effect[]}>;prepareEvidence(actor:WorkContext,input:Parameters<typeof prepareBusinessEvidence>[2]):Promise<{guards:Effect[];steps:Effect[]}|null>;}
export function createInventoryCountApplication<Effect>(ports:InventoryCountPorts<Effect>){const r=ports.repository;function readable(actor:WorkContext){if(!canReadCounts(actor))throw new BusinessRuleError(403,"INVENTORY_SCOPE_REQUIRED","Use a branch or holding inventory or finance assignment.");}
async function listCounts(actor:WorkContext){
  readable(actor);
  return (await r.counts(actor.companyId,actor.branchId,actor.branchId)).results;
}
async function countsPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}) {
  readable(actor);
  return r.readPage(actor,query);
}
async function getCount(actor:WorkContext,id:string){
  readable(actor);
  const row=await r.scopedCount(id,actor.companyId,actor.branchId,actor.branchId);
  if(!row)throw new BusinessRuleError(404,"COUNT_NOT_FOUND","Stock count not found in your workspace.");return row;
}
async function countDetail(actor:WorkContext,id:string){
  const count=await getCount(actor,id);
  const [lines,lotLines,history,decisions]=await Promise.all([
    r.countLines(count.warehouse_id,id),
    r.countLotLines(count.warehouse_id,id),
    r.countHistory(actor.companyId,id),
    r.countDecisions(actor.companyId,id),
  ]);return {count,lines:lines.results,lotLines:lotLines.results,history:history.results,decisions:decisions.results};
}
async function countOptions(actor:WorkContext){
  actor = ports.requirePermission(actor,"inventory.prepare");

  readable(actor);
  const [warehouses,products,stock,lots,settings]=await Promise.all([
    r.activeWarehouses(actor.companyId,actor.branchId,actor.branchId),
    r.activeProducts(actor.companyId),
    r.stockOptions(actor.companyId,actor.branchId,actor.branchId),
    r.lotOptions(actor.companyId,actor.branchId,actor.branchId),
    r.functionalCurrency(actor.companyId),
  ]);return {warehouses:warehouses.results,products:products.results,stock:stock.results,lots:lots.results,currency:settings?.currency??""};
}
function countGuard(key:string,row:CountRow,version:number,statuses:string[]){return r.guardCountVersion(key,"count-version",row.id,row.company_id,version,statuses);}
function stockGuard(key:string,index:number,warehouse:CountRow[string],line:CountRow){
  return Number(line.snapshot_version)===0?r.guardAbsentStock(key,`stock-${index}`,warehouse,line.product_id):r.guardStockSnapshot(key,`stock-${index}`,warehouse,line.product_id,line.snapshot_version,line.snapshot_quantity_milli,line.snapshot_value_minor);
}
async function saveCount(actor:WorkContext,id:string|null,input:z.infer<typeof countSchema>,key:string){
  actor = ports.requirePermission(actor,"inventory.prepare");
  input=countSchema.parse(input);

  if(!canPrepareCounts(actor))throw new BusinessRuleError(403,"COUNT_PREPARER_REQUIRED","An inventory operator must prepare the physical count.");
  const record=id??`count-${key}`,operation=id?"inventory.count.update":"inventory.count.create";
  const prior=await ports.commands.replay(actor,key,operation,record,input);if(prior)return prior;
  const current=id?await getCount(actor,id):null;if(current&&!canEditCount(actor,current))throw new BusinessRuleError(403,"COUNT_LOCKED","Only the preparer can edit a draft or returned count.");
  if(current&&current.warehouse_id!==input.warehouseId)throw new BusinessRuleError(422,"WAREHOUSE_LOCKED","Create a separate count for another warehouse.");
  if(new Set(input.lines.map(l=>l.productId)).size!==input.lines.length)throw new BusinessRuleError(422,"DUPLICATE_PRODUCT","Count each product once.");
  const warehouse=await r.activeWarehouse(input.warehouseId,actor.companyId,actor.branchId,actor.branchId);
  if(!warehouse)throw new BusinessRuleError(422,"WAREHOUSE_INVALID","Choose an active warehouse in your scope.");
  const lines:CountRow[]=[],lotsByLine=new Map<string,PreparedCountLot[]>(),steps:Effect[]=[];
  for(const [index,item] of input.lines.entries()){
    const product=await r.activeProduct(item.productId,actor.companyId);if(!product)throw new BusinessRuleError(422,"PRODUCT_INVALID","Choose an active stock item in this company.");
    const stock=await r.currentStock(warehouse.id,item.productId);
    const qty=decimalToMinor(item.countedQuantity,3),oldQty=Number(stock?.quantity_milli??0),oldValue=Number(stock?.value_minor??0),unitCost=item.unitCost?decimalToMinor(item.unitCost):null,tracking=String(product.tracking_mode??"none");
    const preparedLots:PreparedCountLot[]=[];
    let value:number;
    if(tracking==="none"){
      if(item.allocations.length)throw new BusinessRuleError(422,"LOT_ALLOCATION_NOT_ALLOWED","Lot allocations are only valid for tracked products.");
      if(oldQty===0&&qty>0&&(!unitCost||unitCost<=0))throw new BusinessRuleError(422,"VALUATION_REQUIRED","Enter a verified unit cost when counting stock without an existing valuation.");
      value=qty===0?0:oldQty>0?scaledAmount(oldValue,qty,oldQty):scaledAmount(unitCost!,qty,1000);
    }else{
      const current=(await r.currentProductLots(actor.companyId,item.productId,warehouse.id)).results;
      const currentById=new Map(current.map(l=>[String(l.id),l])),seen=new Set<string>(),identities=new Set<string>();
      for(const [lotIndex,allocation] of item.allocations.entries()){
        const counted=decimalToMinor(allocation.countedQuantity,3),source=allocation.lotId?currentById.get(allocation.lotId):undefined;
        if(allocation.lotId&&!source)throw new BusinessRuleError(422,"COUNT_LOT_INVALID","Choose a current lot or serial from this warehouse.");
        if(source&&!seen.add(String(source.id)))throw new BusinessRuleError(422,"DUPLICATE_COUNT_LOT","Count each lot or serial once.");
        const lotCode=tracking==="lot"?String(source?.lot_code??allocation.lotCode??"").trim():null,serial=tracking==="serial"?String(source?.serial_number??allocation.serialNumber??"").trim():null;
        if(!lotCode&&!serial)throw new BusinessRuleError(422,"TRACKING_IDENTITY_REQUIRED",tracking==="serial"?"Enter the serial number found during the count.":"Enter the lot code found during the count.");
        const identity=`${tracking}:${lotCode??serial}`.toLowerCase();if(identities.has(identity))throw new BusinessRuleError(422,"DUPLICATE_TRACKING_IDENTITY","Each lot code or serial number may appear once per product.");identities.add(identity);
        if(tracking==="serial"&&![0,1000].includes(counted))throw new BusinessRuleError(422,"SERIAL_QUANTITY_INVALID","Each serial number must be counted as zero or one unit.");
        const snapshotQty=Number(source?.quantity_milli??0),snapshotValue=Number(source?.value_minor??0),newUnitCost=allocation.unitCost?decimalToMinor(allocation.unitCost):null;
        if(!source&&counted===0)throw new BusinessRuleError(422,"FOUND_LOT_QUANTITY_REQUIRED","A newly found lot or serial must have a positive counted quantity.");
        if(!source&&(!newUnitCost||newUnitCost<=0))throw new BusinessRuleError(422,"VALUATION_REQUIRED","Enter a verified unit cost for a newly found lot or serial.");
        const countedValue=counted===0?0:source?scaledAmount(snapshotValue,counted,snapshotQty):scaledAmount(newUnitCost!,counted,1000);
        if(counted>0&&countedValue===0)throw new BusinessRuleError(422,"ZERO_VALUATION","A positive lot count requires a positive inventory valuation.");
        const resultLotId=source?String(source.id):`lot-${(await ports.hash(`${actor.companyId}:${item.productId}:${identity}`)).slice(0,32)}`;
        preparedLots.push({id:`${record}-${index}-lot-${lotIndex}`,countLineId:`${record}-${index}`,sourceLotId:source?String(source.id):null,resultLotId,trackingMode:tracking as "lot"|"serial",lotCode,serialNumber:serial,manufacturedDate:source?.manufactured_date?String(source.manufactured_date):allocation.manufacturedDate||null,expiryDate:source?.expiry_date?String(source.expiry_date):allocation.expiryDate||null,snapshotQuantity:snapshotQty,snapshotValue,snapshotVersion:Number(source?.balance_version??0),countedQuantity:counted,countedValue,unitCost:source?null:newUnitCost});
      }
      if(current.some(l=>!seen.has(String(l.id))))throw new BusinessRuleError(422,"COUNT_LOT_MISSING","Count every current lot or serial, including zero when it is missing physically.");
      if(preparedLots.reduce((sum,l)=>sum+l.countedQuantity,0)!==qty)throw new BusinessRuleError(422,"COUNT_LOT_TOTAL_MISMATCH","The lot or serial quantities must equal the product count.");
      value=preparedLots.reduce((sum,l)=>sum+l.countedValue,0);
    }
    if(qty>0&&value===0)throw new BusinessRuleError(422,"ZERO_VALUATION","A positive count requires a positive inventory valuation.");
    const line:CountRow={id:`${record}-${index}`,product_id:item.productId,product_version:product.version,description:product.name,unit:product.unit,tracking_mode:tracking,snapshot_quantity_milli:oldQty,snapshot_value_minor:oldValue,snapshot_version:stock?.version??0,counted_quantity_milli:qty,counted_value_minor:value,unit_cost_minor:tracking==="none"&&oldQty===0?unitCost:null,ordinal:index};lines.push(line);lotsByLine.set(String(line.id),preparedLots);
    steps.push(stockGuard(key,index,warehouse.id,line),r.guardProductVersion(key,`product-${index}`,item.productId,product.version),r.guardMovementDate(key,`date-${index}`,warehouse.id,item.productId,input.countDate));
  }
  const net=lines.reduce((s,l)=>s+BigInt(Number(l.counted_value_minor)-Number(l.snapshot_value_minor)),BigInt(0));if(net>BigInt(Number.MAX_SAFE_INTEGER)||net< -BigInt(Number.MAX_SAFE_INTEGER))throw new BusinessRuleError(422,"VALUE_OVERFLOW","Stock count value exceeds supported precision.");
  const now=ports.clock.now(),version=current?Number(input.expectedVersion??0)+1:1;
  steps.push(r.guardWarehouseVersion(key,"warehouse",warehouse.id,warehouse.version));
  if(current)steps.push(countGuard(key,current,input.expectedVersion??0,["draft","rejected"]),r.prepareCountRevision(input.countDate,input.reason,Number(net),actor.userId,now,record),r.clearCountLots(record),r.clearCountLines(record));
  else steps.push(r.prepareCount(record,actor.companyId,warehouse.branch_id,warehouse.id,`COUNT-${(await ports.hash(record)).slice(0,16).toUpperCase()}`,input.countDate,input.reason,Number(net),actor.userId,actor.userId,now,now));
  for(const l of lines){
    steps.push(r.prepareCountLine(l.id,record,l.product_id,l.product_version,l.description,l.unit,l.snapshot_quantity_milli,l.snapshot_value_minor,l.snapshot_version,l.counted_quantity_milli,l.counted_value_minor,l.unit_cost_minor,l.ordinal));
    for(const lot of lotsByLine.get(String(l.id))??[])steps.push(r.prepareCountLotLine(lot.id,lot.countLineId,lot.sourceLotId,lot.resultLotId,lot.trackingMode,lot.lotCode,lot.serialNumber,lot.manufacturedDate,lot.expiryDate,lot.snapshotQuantity,lot.snapshotValue,lot.snapshotVersion,lot.countedQuantity,lot.countedValue,lot.unitCost));
  }
  return ports.commands.commit(actor,key,operation,input,record,version,{id:record,status:"draft",version},steps);
}
async function transitionCount(actor:WorkContext,id:string,action:"submit"|"reject"|"cancel"|"approve"|"post",input:z.infer<typeof countActionSchema>,key:string){
  input=countActionSchema.parse(input);
  actor = ports.requirePermission(actor,action === "post" || action === "approve" || action === "reject" ? "inventory.approve" : "inventory.prepare");
  const operation=`inventory.count.${action}`,prior=await ports.commands.replay(actor,key,operation,id,input);if(prior)return prior;
  const {count,lines,lotLines}=await countDetail(actor,id),review=action==="post"||action==="approve"||action==="reject";
  const qualityHold=count.quality_hold_id?await r.qualityHold(count.quality_hold_id,actor.companyId):null;
  if(count.quality_hold_id&&!qualityHold)throw new BusinessRuleError(409,"QUALITY_HOLD_MISSING","The linked quality hold is unavailable.");
  if(review?!canReviewCount(actor,count):!canEditCount(actor,count))throw new BusinessRuleError(403,"COUNT_ACTION_FORBIDDEN",review?"An independent holding finance approver must review a submitted stock count.":"Only the preparer can submit or cancel an editable count.");
  if(action!=="submit"&&input.attachmentSetIds.length)throw new BusinessRuleError(422,"ATTACHMENT_ACTION_INVALID","Count sheets and photos are bound when the preparer submits the count.");
  if(action!=="submit"&&!input.reason)throw new BusinessRuleError(422,"REASON_REQUIRED","Record the reason for this decision.");
  const status=action==="submit"?"submitted":action==="post"?"posted":action==="approve"?"approved":action==="reject"?"rejected":"cancelled",now=ports.clock.now();
  const steps=[countGuard(key,count,input.expectedVersion,action==="post"?["approved"]:action==="approve"?["submitted"]:review?["submitted","approved"]:["draft","rejected"])];let journalId:string|null=null,remainingHoldSteps:Effect[]=[];
  const attachmentBinding=action==="submit"?await ports.prepareEvidence(actor,{attachmentSetIds:input.attachmentSetIds,targetTypes:["stock_count","inventory_stock_count"],bindingType:"stock_count_submission",contexts:["count_sheet","count_photo","count_evidence"].map(purposeKey=>({purposeKey,componentKind:"action" as const,componentRef:"submit"})),targetId:id,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`count-submit-${key}`}):null;
  if(attachmentBinding)steps.push(...attachmentBinding.guards);
  if(action==="submit"||action==="approve"||action==="post"){
    if(!lines.length)throw new BusinessRuleError(409,"EMPTY_COUNT","Enter the counted products first.");
    steps.push(r.guardWarehouseActive(key,"warehouse",count.warehouse_id,actor.companyId));
    for(const [index,line] of lines.entries()){
      steps.push(stockGuard(key,index,count.warehouse_id,line),r.guardReviewProductVersion(key,`product-${index}`,line.product_id,line.product_version));
      const tracked=String(line.tracking_mode)!=="none",selected=lotLines.filter(l=>l.count_line_id===line.id);
      if(tracked&&(!selected.length||selected.reduce((sum,l)=>sum+Number(l.counted_quantity_milli),0)!==Number(line.counted_quantity_milli)))throw new BusinessRuleError(409,"COUNT_LOT_TOTAL_MISMATCH","Saved lot or serial evidence no longer matches the product count.");
      if(!tracked&&selected.length)throw new BusinessRuleError(409,"COUNT_LOT_INVALID","An untracked product contains unexpected lot evidence.");
      for(const [lotIndex,lot] of selected.entries())steps.push(
        lot.source_lot_id
          ? r.guardCountLotSnapshot(key,`count-lot-${index}-${lotIndex}`,lot.source_lot_id,count.warehouse_id,lot.snapshot_version,lot.snapshot_quantity_milli,lot.snapshot_value_minor)
          : r.guardNewLotIdentity(key,`count-lot-${index}-${lotIndex}`,lot.result_lot_id,actor.companyId,line.product_id,lot.lot_code,lot.serial_number)
      );
    }
  }
  if(action==="post"){
    const config=await r.variancePostingConfig(actor.companyId);if(!config)throw new BusinessRuleError(409,"VARIANCE_MAPPING_REQUIRED","Configure the stock count variance account (5400) in the operations ledger.");
    const periods=await r.postingPeriods(actor.companyId,count.count_date,count.count_date);if(periods.results.length!==1)throw new BusinessRuleError(409,"PERIOD_UNAVAILABLE","Exactly one open period must cover the count date.");
    if(qualityHold){
      const heldQuantity=Number(qualityHold.held_quantity_milli??0),dispositionQuantity=Number(qualityHold.disposition_quantity_milli??0),remainingQuantity=heldQuantity-dispositionQuantity;
      if(!Number.isSafeInteger(dispositionQuantity)||dispositionQuantity<=0||dispositionQuantity>heldQuantity)throw new BusinessRuleError(409,"QUALITY_DISPOSITION_INVALID","The controlled disposition quantity is no longer valid. Refresh the quality hold.");
      steps.push(r.guardQualityDisposition(key,"quality-disposition",qualityHold.id,actor.companyId,id,dispositionQuantity,heldQuantity));
      if(remainingQuantity>0){
        const remainingHoldId=`${qualityHold.id}-remaining-${id}`,remainingSourceId=qualityHold.source_id?`${qualityHold.source_id}:remaining:${id}`:null;
        remainingHoldSteps=[
          r.guardRemainingHoldUnique(key,"quality-remaining-hold",remainingHoldId),
          r.prepareRemainingHold(remainingHoldId,qualityHold.company_id,qualityHold.branch_id,qualityHold.warehouse_id,qualityHold.product_id,`Remaining quantity after partial quality scrap: ${String(qualityHold.reason)}`,qualityHold.due_date,qualityHold.created_by,now,now,qualityHold.lot_id,remainingQuantity,qualityHold.source_type,remainingSourceId,qualityHold.spec_id,qualityHold.spec_version,"partial"),
          r.prepareRemainingHoldNotice(`${remainingHoldId}-notice`,qualityHold.company_id,remainingHoldId,now,qualityHold.company_id,qualityHold.branch_id,now),
        ];
      }
      steps.push(r.prepareQualityDisposal(actor.userId,now,now,qualityHold.id));
    }
    let gains=0,losses=0;
    for(const [index,line] of lines.entries()){
      const qty=Number(line.counted_quantity_milli)-Number(line.snapshot_quantity_milli),value=Number(line.counted_value_minor)-Number(line.snapshot_value_minor);if(value>0)gains+=value;else losses-=value;
      steps.push(r.guardPostingMovementDate(key,`date-${index}`,count.warehouse_id,line.product_id,count.count_date));
      if(Number(line.snapshot_version)===0)steps.push(r.prepareStock(count.warehouse_id,line.product_id));
      steps.push(r.prepareCountedStock(line.counted_quantity_milli,line.counted_value_minor,count.warehouse_id,line.product_id));
      steps.push(r.prepareAdjustmentMove(`${id}-${index}`,id,count.warehouse_id,line.product_id,qty,value,count.count_date,now));
      for(const [lotIndex,lot] of lotLines.filter(l=>l.count_line_id===line.id).entries()){
        if(lot.source_lot_id)steps.push(r.prepareCountedLotBalance(lot.counted_quantity_milli,lot.counted_value_minor,lot.source_lot_id,count.warehouse_id),r.prepareCountedLotStatus(lot.counted_quantity_milli,lot.source_lot_id));
        else steps.push(r.prepareFoundLot(lot.result_lot_id,actor.companyId,line.product_id,lot.tracking_mode,lot.lot_code,lot.serial_number,lot.manufactured_date,lot.expiry_date,"active",actor.userId,now),r.prepareFoundLotBalance(lot.result_lot_id,count.warehouse_id,lot.counted_quantity_milli,lot.counted_value_minor));
        const lotQty=Number(lot.counted_quantity_milli)-Number(lot.snapshot_quantity_milli),lotValue=Number(lot.counted_value_minor)-Number(lot.snapshot_value_minor);
        if(lotQty||lotValue)steps.push(r.prepareAdjustmentLotMove(`${id}-${index}-${lotIndex}`,lot.result_lot_id,count.warehouse_id,"stock_count",String(lot.id),lotQty,lotValue,count.count_date,actor.userId,now));
      }
    }
    if(qualityHold?.lot_id)steps.push(r.prepareScrappedLot(qualityHold.lot_id,qualityHold.lot_id));
    if(remainingHoldSteps.length)steps.push(...remainingHoldSteps);
    if(!Number.isSafeInteger(gains+losses))throw new BusinessRuleError(422,"VALUE_OVERFLOW","Stock count exceeds supported precision.");
    steps.push(r.guardOpenPeriod(key,"open-period",periods.results[0].id));
    if(gains||losses){const journal=await ports.prepareJournal(actor,{chartId:String(config.chart_id),periodId:String(periods.results[0].id),entryDate:String(count.count_date),currency:String(config.currency),memo:`${count.number} · ${input.reason}`,sourceType:"stock_count",sourceId:id,isOpening:false,lines:[...(gains?[{accountId:String(config.inventory_account_id),debit:minorToDecimal(gains),credit:"0"},{accountId:String(config.variance_account_id),debit:"0",credit:minorToDecimal(gains)}]:[]),...(losses?[{accountId:String(config.variance_account_id),debit:minorToDecimal(losses),credit:"0"},{accountId:String(config.inventory_account_id),debit:"0",credit:minorToDecimal(losses)}]:[])]},key,"posted");journalId=journal.result.id;steps.push(...journal.steps);}
  }
  steps.push(r.prepareCountDecision(status,input.reason??null,action,actor.userId,action,now,action,now,journalId,now,id));
  if(qualityHold&&action==='reject')steps.push(
    r.prepareQualityRejection(now,qualityHold.id,id),
    r.prepareQualityRejectionEvent(`quality-disposition-${key}`,qualityHold.id,id,actor.userId,qualityHold.version,input.reason,now)
  );
  if(qualityHold&&action==='post')steps.push(
    r.prepareQualityPostEvent(`quality-disposition-${key}`,qualityHold.id,id,actor.userId,qualityHold.version,input.reason,now),
    r.prepareQualityRequesterNotice(`${key}-quality-requester`,actor.companyId,qualityHold.disposition_requested_by,'quality_hold',qualityHold.id,Number(qualityHold.version)+1,'Quality scrap posted with inventory and accounting evidence',now)
  );
  if(action!=="submit")steps.push(r.prepareDecisionHistory(ports.identifiers.next(),id,key,action,actor.userId,input.expectedVersion,input.reason,now));
  if(review)steps.push(r.prepareDecisionEvidence(ports.identifiers.next(),key,id,operation,actor.userId,actor.assignmentId,input.expectedVersion,await ports.hash(JSON.stringify({reason:input.reason,lines,lotLines})),now));
  if(attachmentBinding)steps.push(...attachmentBinding.steps);
  return ports.commands.commit(actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1,journalId},steps);
}
return {listCounts,countsPage,getCount,countDetail,countOptions,saveCount,transitionCount};}
