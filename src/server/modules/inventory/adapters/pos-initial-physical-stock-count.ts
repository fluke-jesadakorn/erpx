import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import {minorToDecimal} from '@/lib/finance/finance-money';
import type {WorkContext} from '../../identity_access/public/authorization';
import {loadIamAuthority} from '../../identity_access/public/iam-store';
import {assignmentReadSql} from '../../identity_access/public/record-source-authority';
import {platformOwnerProjectionSources as identitySources} from '../../identity_access/public/platform-owner-projection-sources';
import {readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {posInitialPhysicalStockCommerceSource,initialPhysicalStockMasterChoices,initialPhysicalStockMasterSource} from '../../commerce/public/pos-initial-physical-stock-sources';
import {initialPhysicalStockCountFinanceSource} from '../../finance/public/initial-physical-stock-count-sources';
import {financeStarterOrganizationNodes} from '../../organization/public/finance-starter-sources';
import {inventoryQualitySnapshotSources} from '../../quality/public/inventory-quality-snapshot-sources';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {holdingResources} from '../../runtime/public/runtime';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {resolveFlow,assignFlowPrimary} from '../../workflow/public/flow-routing';
import {requireFlowAccess} from '../../workflow/public/flow-access';
import {replayCommand} from '../../workflow/public/commands';
import {inventoryCountApplication} from './inventory-counts';
import {posInitialPhysicalStockCapabilities,posInitialPhysicalStockCountSource,posInitialPhysicalStockCountImage,withPosInitialPhysicalStockCountUnit,type PosInitialPhysicalStockProof} from './pos-initial-physical-stock-count-source';
import {canPrepareCounts,canEditCount,canReviewCount} from '../domain/counts';
import {posInitialPhysicalStockCountSchema,posInitialPhysicalStockCountActionSchema,posInitialPhysicalStockCountSelectionSchema,posInitialPhysicalStockCountFlowSchema,type PosInitialPhysicalStockCountView,type PosInitialPhysicalStockCountRecord} from '../domain/pos-initial-physical-stock-count';
const quality=inventoryQualitySnapshotSources();
const hash=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const authorityImage=(actor:WorkContext)=>[actor.holdingId,actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam?.queuedAssignmentVersion,actor.iam?.identityVersion,actor.iam?.roleRevision,actor.iam?.hrAppointmentVersion];
function empty(actor:WorkContext,reason:PosInitialPhysicalStockCountView['unavailableReason']):PosInitialPhysicalStockCountView{return {companyId:actor.companyId,available:false,currency:null,documentCurrency:null,contextKey:null,unavailableReason:reason,canPrepare:false,canCreateWarehouse:false,canCreateProduct:false,selection:{warehouseId:null,productId:null,countDate:null},readiness:{schemaAvailable:false,postingConfigured:false,accountingApproved:false,varianceAccountAvailable:false,postingPeriodAvailable:false},warehouses:[],branches:[],products:[],record:null};}
async function owner(actor:WorkContext){return (await holdingResources(actor.holdingId)).write;}
async function schema(database:SqlDatabase){if(!await database.prepare("SELECT to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL ready").first<boolean>('ready'))throw new ApiError(503,'POS_INITIAL_PHYSICAL_STOCK_SCHEMA_REQUIRED','Install the reviewed Initial Physical Stock Count schema before using this command.');}
async function history(database:SqlDatabase,companyId:string,warehouseId:string,productId:string,except:string|null=null){
 return !!await database.prepare(`SELECT EXISTS(SELECT 1 FROM commerce_stock s WHERE s.warehouse_id=? AND s.product_id=?) OR EXISTS(SELECT 1 FROM inventory_ledger m WHERE m.warehouse_id=? AND m.product_id=?) OR EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE b.warehouse_id=? AND l.product_id=?) OR EXISTS(SELECT 1 FROM inventory_order_reservations r WHERE r.warehouse_id=? AND r.product_id=?) OR EXISTS(SELECT 1 ${quality.inventory_counts_inventory_quality_holds_from} WHERE warehouse_id=? AND product_id=?) OR EXISTS(SELECT 1 FROM inventory_count_lines l JOIN inventory_counts c ON c.id=l.count_id WHERE c.warehouse_id=? AND l.product_id=? AND (?::text IS NULL OR c.id<>?) AND (c.status<>'cancelled' OR c.posted_at IS NOT NULL OR c.journal_id IS NOT NULL)) history`).bind(warehouseId,productId,warehouseId,productId,warehouseId,productId,warehouseId,productId,warehouseId,productId,warehouseId,productId,except,except).first<boolean>('history');
}
async function selectionSource(database:SqlDatabase,actor:WorkContext,warehouseId:string,productId:string,date:string){
 const goods=await posInitialPhysicalStockCommerceSource(database,actor.companyId,warehouseId),masters=await initialPhysicalStockMasterSource(database,actor,warehouseId,productId),finance=await initialPhysicalStockCountFinanceSource(database,actor.companyId,date);
 return {goods,masters,finance,prior:await history(database,actor.companyId,warehouseId,productId),image:JSON.stringify([goods,masters,finance.image])};
}
function context(actor:WorkContext,warehouseId:string,productId:string,date:string,image:string){return hash([authorityImage(actor),warehouseId,productId,date,image]);}
function assertSelection(source:Awaited<ReturnType<typeof selectionSource>>){
 if(!source.goods)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SETUP_REQUIRED','Configure the real POS posting mapping first.');
 if(!source.goods.accounting_approved)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_ACCOUNTING_REQUIRED','Complete independent POS accounting review and choose its active register warehouse.');
 if(!source.masters)throw new ApiError(404,'POS_INITIAL_PHYSICAL_STOCK_SELECTION_UNAVAILABLE','Choose an active stock item and warehouse in the selected Stock scope.');
 if(source.prior)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_HISTORY','Existing stock or prior stock history cannot be initialized again.');
 if(!source.finance.config)throw new ApiError(409,'VARIANCE_ACCOUNT_REQUIRED','The original Count requires its real active functional-currency expense account 5400.');
 if(!source.finance.period)throw new ApiError(409,'PERIOD_UNAVAILABLE','The original Count requires exactly one open period covering its count date.');
}
async function record(database:SqlDatabase,actor:WorkContext,id:string):Promise<PosInitialPhysicalStockCountRecord>{
 const detail=await inventoryCountApplication(database).countDetail(actor,id),{count,lines,lotLines,decisions}=detail,line=lines[0],flow=await resolveFlow(database,actor,'stock_count',id);
 if(!line||lines.length!==1)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count source is unavailable.');
 const access=await readPurchasedAccesses(database,'enforced',actor.companyId,posInitialPhysicalStockCapabilities),newWork=posInitialPhysicalStockCapabilities.every(cap=>access[cap].newWork),settle=posInitialPhysicalStockCapabilities.every(cap=>access[cap].settleExisting),edit=canEditCount(actor,count),review=canReviewCount(actor,count)&&flow.canAct;
 return {sourceKind:'pos-initial-physical-stock-count',id,number:String(count.number),status:String(count.status),version:Number(count.version),countDate:String(count.count_date),reason:String(count.reason),warehouseId:String(count.warehouse_id),warehouseName:String(count.warehouse_name),productId:String(line.product_id),sku:String(line.sku),productName:String(line.description),unit:String(line.unit),trackingMode:String(line.tracking_mode),countedQuantity:minorToDecimal(Number(line.counted_quantity_milli),3),countedValue:minorToDecimal(Number(line.counted_value_minor)),unitCost:line.unit_cost_minor===null?null:minorToDecimal(Number(line.unit_cost_minor)),lots:lotLines.map(lot=>({lotCode:lot.lot_code===null?null:String(lot.lot_code),serialNumber:lot.serial_number===null?null:String(lot.serial_number),manufacturedDate:lot.manufactured_date===null?null:String(lot.manufactured_date),expiryDate:lot.expiry_date===null?null:String(lot.expiry_date),countedQuantity:minorToDecimal(Number(lot.counted_quantity_milli),3),unitCost:lot.unit_cost_minor===null?null:minorToDecimal(Number(lot.unit_cost_minor))})),journalId:count.journal_id===null?null:String(count.journal_id),canSubmit:edit&&newWork&&flow.canAct,canCancel:edit&&settle,canApprove:count.status==='submitted'&&review&&settle,canPost:count.status==='approved'&&review&&settle,canReject:['submitted','approved'].includes(String(count.status))&&review&&settle,decisions:decisions.map(d=>({action:String(d.action),reason:d.reason===null?null:String(d.reason),actorName:String(d.actor_name),createdAt:Number(d.created_at)}))};
}
export async function posInitialPhysicalStockCountWorkspace(actor:WorkContext,raw:unknown={}):Promise<PosInitialPhysicalStockCountView>{
 const selection=posInitialPhysicalStockCountSelectionSchema.parse(raw);
 if(actor.departmentId||!hasPermission(actor,'inventory.read'))return empty(actor,'stock-authority-required');
 return withPosInitialPhysicalStockCountUnit(await owner(actor),actor,{key:`pos-initial-stock-read:${randomUUID()}`,operation:'inventory.count.read',id:selection.countId,purpose:'history',readOnly:true},async(database,_proof,_replay,current)=>{
  const base=empty(actor,null);base.available=true;base.readiness.schemaAvailable=!!await database.prepare("SELECT to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL ready").first<boolean>('ready');
  if(selection.countId){base.record=await record(database,current,selection.countId);base.currency=(JSON.parse(_proof!) as PosInitialPhysicalStockProof).currency;return base;}
  const access=await readPurchasedAccesses(database,'enforced',actor.companyId,posInitialPhysicalStockCapabilities),newWork=posInitialPhysicalStockCapabilities.every(cap=>access[cap].newWork),prepare=canPrepareCounts(current)&&newWork;
  const goods=await posInitialPhysicalStockCommerceSource(database,actor.companyId,selection.warehouseId);base.documentCurrency=goods?String(goods.currency):null;base.currency=goods?String((await initialPhysicalStockCountFinanceSource(database,actor.companyId,selection.countDate??new Date().toISOString().slice(0,10))).config?.currency??'')||null:null;
  base.readiness.postingConfigured=!!goods;base.readiness.accountingApproved=!!goods?.accounting_approved;
  if(prepare){const [choices,nodes]=await Promise.all([initialPhysicalStockMasterChoices(database,current),financeStarterOrganizationNodes(database,actor.companyId)]);base.warehouses=choices.warehouses.map(w=>({id:String(w.id),name:String(w.name),branchId:String(w.branch_id)}));base.products=choices.products.map(p=>({id:String(p.id),sku:String(p.sku),name:String(p.name),unit:String(p.unit),trackingMode:String(p.tracking_mode)}));base.branches=nodes.filter(n=>n.kind==='branch'&&(!actor.branchId||n.id===actor.branchId)).map(n=>({id:n.id,name:n.display_name}));}
  base.selection={warehouseId:selection.warehouseId??null,productId:selection.productId??null,countDate:selection.countDate??null};
  if(!goods){base.unavailableReason='pos-posting-required';return base;}
  if(!goods.accounting_approved){base.unavailableReason='pos-accounting-required';return base;}
  if(!base.readiness.schemaAvailable){base.unavailableReason='schema-required';return base;}
  if(!selection.warehouseId||!selection.productId||!selection.countDate){base.unavailableReason='selection-required';return base;}
  const source=await selectionSource(database,current,selection.warehouseId,selection.productId,selection.countDate);base.currency=source.finance.config?String(source.finance.config.currency):base.currency;base.readiness.varianceAccountAvailable=!!source.finance.config;base.readiness.postingPeriodAvailable=!!source.finance.period;
  base.unavailableReason=!source.masters?'selection-unavailable':source.prior?'prior-stock-history':!source.finance.config?'variance-account-required':!source.finance.period?'period-required':null;
  base.canPrepare=prepare&&base.unavailableReason===null;
  if(base.canPrepare)base.contextKey=context(current,selection.warehouseId,selection.productId,selection.countDate,source.image);
  return base;
 });
}
export async function preparePosInitialPhysicalStockCount(actor:WorkContext,raw:unknown,key:string){
 const input=posInitialPhysicalStockCountSchema.parse(raw),id=`count-${key}`,productId=input.input.lines[0].productId;
 return withPosInitialPhysicalStockCountUnit(await owner(actor),actor,{key,operation:'inventory.count.create',purpose:'new-work',permission:'inventory.prepare'},async(database,_source,replay,current,finalGuard)=>{
  await schema(database);
  if(replay){await posInitialPhysicalStockCountSource(database,actor,id).then(source=>{if(!source)throw new ApiError(404,'COUNT_NOT_FOUND','The original Initial Physical Stock Count is unavailable.');});const result=await replayCommand<Record<string,unknown>>(database,actor,key,'inventory.count.create',id,input);if(!result)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED','The original create receipt changed.');const {_posInitialPhysicalStockCount:proof,...publicResult}=result;void proof;return publicResult as {id:string;status:string;version:number};}
  const source=await selectionSource(database,current,input.input.warehouseId,productId,input.input.countDate);assertSelection(source);
  if(input.contextKey!==context(current,input.input.warehouseId,productId,input.input.countDate,source.image))throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED','The original selected item, warehouse, posting references or Stock assignment changed.');
  const config=source.finance.config!,proof:PosInitialPhysicalStockProof={kind:'pos-initial-physical-stock-count',countId:id,companyId:actor.companyId,warehouseId:input.input.warehouseId,branchId:String(source.masters!.warehouse.branch_id),productId,countCommandId:key,posSetupCommandId:String(source.goods!.pos_setup_command_id),posSetupRequestHash:String(source.goods!.pos_setup_request_hash),posBootstrapReceiptId:String(source.goods!.bootstrap_receipt_id),posBootstrapRequestHash:String(source.goods!.bootstrap_request_hash),posBootstrapAuditId:String(source.goods!.bootstrap_audit_id),posApprovalReceiptId:String(source.goods!.approval_receipt_id),posApprovalRequestHash:String(source.goods!.approval_request_hash),posApprovalAuditId:String(source.goods!.approval_audit_id),posAccountingImageHash:String(source.goods!.accounting_image_hash),chartId:String(config.chart_id),inventoryAccountId:String(config.inventory_account_id),varianceAccountId:String(config.variance_account_id),currency:String(config.currency)};
  const result=await inventoryCountApplication(database,undefined,{input,proof:{...proof},marker:'_posInitialPhysicalStockCount'}).saveCount(actor,null,input.input,key);
  await database.prepare('INSERT INTO inventory_pos_initial_physical_sources(count_id,company_id,branch_id,warehouse_id,product_id,count_command_id,pos_setup_command_id,pos_bootstrap_receipt_id,pos_approval_receipt_id,chart_id,inventory_account_id,variance_account_id,currency,proof_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?::jsonb,?,?)').bind(id,actor.companyId,proof.branchId,proof.warehouseId,proof.productId,key,proof.posSetupCommandId,proof.posBootstrapReceiptId,proof.posApprovalReceiptId,proof.chartId,proof.inventoryAccountId,proof.varianceAccountId,proof.currency,JSON.stringify(proof),actor.userId,Date.now()).run();
  const createdImage=await posInitialPhysicalStockCountImage(database,actor,id);
  finalGuard(async()=>{const final=await selectionSource(database,current,input.input.warehouseId,productId,input.input.countDate);
   if(!createdImage||createdImage!==await posInitialPhysicalStockCountImage(database,actor,id)||final.image!==source.image||await history(database,actor.companyId,proof.warehouseId,productId,id))throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original first-count source changed before completion.');});
  return result;
 });
}
export async function transitionPosInitialPhysicalStockCount(actor:WorkContext,id:string,action:import('../domain/pos-initial-physical-stock-count').PosInitialPhysicalStockCountAction,raw:unknown,key:string){
 const input=posInitialPhysicalStockCountActionSchema.parse(raw);if(!['submit','approve','post','reject','cancel'].includes(action))throw new ApiError(404,'COUNT_ACTION_NOT_FOUND','Unknown Count action.');
 return withPosInitialPhysicalStockCountUnit(await owner(actor),actor,{key,operation:`inventory.count.${action}`,id,purpose:action==='submit'?'new-work':'settle-existing',permission:['approve','post','reject'].includes(action)?'inventory.approve':'inventory.prepare'},async(database,source,replay,_current,finalGuard)=>{
  const before=await inventoryCountApplication(database).getCount(actor,id),finance=!replay&&action==='post'?await initialPhysicalStockCountFinanceSource(database,actor.companyId,String(before.count_date)):null;
  if(finance){const proof=JSON.parse(source!) as PosInitialPhysicalStockProof,config=finance.config;if(!config||String(config.chart_id)!==proof.chartId||String(config.inventory_account_id)!==proof.inventoryAccountId||String(config.variance_account_id)!==proof.varianceAccountId||String(config.currency)!==proof.currency)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','Posting must retain the original captured Count chart, inventory, variance and currency.');}
  const result=await inventoryCountApplication(database).transitionCount(actor,id,action,input,key);
  if(finance)finalGuard(async()=>{if(finance.image!==(await initialPhysicalStockCountFinanceSource(database,actor.companyId,String(before.count_date))).image)throw new ApiError(409,'POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count posting references changed.');});return result;
 });
}
export async function posInitialPhysicalStockCountFlow(actor:WorkContext,id:string){return withPosInitialPhysicalStockCountUnit(await owner(actor),actor,{key:`pos-initial-stock-flow:${randomUUID()}`,operation:'flow.read',id,purpose:'history',readOnly:true},async database=>{await requireFlowAccess(actor,'stock_count',id,database);return resolveFlow(database,actor,'stock_count',id);});}
export async function assignPosInitialPhysicalStockCountFlow(actor:WorkContext,id:string,raw:unknown,key:string){const input=posInitialPhysicalStockCountFlowSchema.parse(raw);return withPosInitialPhysicalStockCountUnit(await owner(actor),actor,{key,operation:'flow.assign',id,purpose:'settle-existing',flowStep:input.stepKey,saveDefault:input.saveDefault},async database=>{await requireFlowAccess(actor,'stock_count',id,database);return assignFlowPrimary(database,actor,'stock_count',id,input,key);});}
export type PosInitialPhysicalStockCountProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string};
export async function posInitialPhysicalStockCountProductEntry(database:SqlDatabase,scope:PosInitialPhysicalStockCountProductScope,target:{countId?:string}){
 if(scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 try{await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))return null;throw error;}
 const count=target.countId?await posInitialPhysicalStockCountSource(database,scope,target.countId):null;if(target.countId&&!count)return null;
 const result=await database.prepare(`WITH initial_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN (SELECT ?::text company_id,?::text branch_id,NULL::text department_id) initial_entry_scope CROSS JOIN initial_entry_clock
 WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=initial_entry_scope.company_id AND a.version=? AND ${assignmentReadSql('initial_entry_scope','inventory.read')}
 ${target.countId?'':`AND ${assignmentReadSql('initial_entry_scope','inventory.prepare')}`}
 AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('initial_entry_scope.company_id',module)).join(' AND ')}
 AND ${posInitialPhysicalStockCapabilities.map(cap=>purchasedCapabilityPredicate('initial_entry_scope.company_id',cap,target.countId?'history':'new-work','initial_entry_clock.now_ms')).join(' AND ')}`).bind(scope.companyId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,scope.assignmentId,scope.userId,scope.role,...(target.countId?[]:[scope.assignmentId,scope.userId,scope.role])).first();
 return result?{capability:'inventory.stock-core' as const,purchasePurpose:'history' as const}:null;
}
