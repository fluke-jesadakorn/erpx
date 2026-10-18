import 'server-only';
import {createHash} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import {samePerson} from '@/lib/identity_access/iam';
import type {WorkContext} from '../../identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {companyPurchaseReadLocks,readPurchasedAccesses,checkPurchasedCapability} from '../../commercial/public/purchase-command-boundary';
import {lockScheduledFeatureSources} from '../../pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import {financeStarterOrganizationNodes} from '../../organization/public/finance-starter-sources';
import {posStartupMasterChoiceLocks,posStartupMasterChoices} from '../../commerce/public/pos-startup-master-choices';
import {posStartupMasterFinanceSource,posStartupBootstrapFinanceReferences} from '../../finance/public/pos-startup-master-sources';
import {posStartupFinanceReadLocks} from '../../finance/public/pos-posting-setup';
import {holdingResources} from '../../runtime/public/runtime';
import {isTransactionalSqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {posStartupSourceReadLocks} from './startup-sources';
import type {PosStartupWorkspace} from '../domain/startup';
const capabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
const empty=(companyId:string,unavailableReason:PosStartupWorkspace['unavailableReason']):PosStartupWorkspace=>({companyId,available:false,contextKey:null,documentCurrency:null,canCreateWarehouse:false,canCreateProduct:false,canBootstrap:false,canConfigureCatalogue:false,canApproveAccounting:false,configured:null,settingsVersion:null,accountingPrepared:false,accountingApproved:false,bankAccounts:[],branches:[],warehouses:[],products:[],registers:[],unavailableReason});
const authorityImage=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.branchId,actor.departmentId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
function permission(actor:WorkContext){
 if(hasPermission(actor,'inventory.read')&&hasPermission(actor,'inventory.prepare'))return 'inventory.prepare';
 if(hasPermission(actor,'pos.read')&&hasPermission(actor,'pos.configure'))return 'pos.configure';
 if(hasPermission(actor,'finance.read')&&hasPermission(actor,'pos.accounting.approve'))return 'pos.accounting.approve';
 if(hasPermission(actor,'finance.read')&&hasPermission(actor,'pos.accounting.prepare'))return 'pos.accounting.prepare';
 return null;
}
export async function posStartupWorkspace(actor:WorkContext):Promise<PosStartupWorkspace>{
 const needed=permission(actor);if(actor.departmentId||!needed)return empty(actor.companyId,'authority-required');
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','POS startup discovery requires its original Holding transaction.');
 return owner.transaction(async database=>{
  // Page discovery never takes a Company advisory or writer lease after SHARE.
  await database.batch([companyPurchaseReadLocks(database,actor.companyId)[0],...posStartupMasterChoiceLocks(database),...posStartupSourceReadLocks(database),...posStartupFinanceReadLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);
  const current=await requireQueuedCommandAuthority(database,actor,needed);if(permission(current)!==needed)throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','Choose a current POS startup role.');
  for(const feature of ['pos','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
  const access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);if(!capabilities.every(capability=>access[capability].history))return empty(actor.companyId,'purchase-required');
  const [choices,nodes,settings,registers]=await Promise.all([
   posStartupMasterChoices(database,actor),financeStarterOrganizationNodes(database,actor.companyId),
   database.prepare('SELECT version,accounting_prepared_by,accounting_approved_by FROM pos_settings WHERE company_id=?').bind(actor.companyId).first<{version:number;accounting_prepared_by:string|null;accounting_approved_by:string|null}>(),
   database.prepare('SELECT id,branch_id,warehouse_id,name,mode,active,version FROM pos_registers WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id').bind(actor.companyId,actor.branchId,actor.branchId).all<{id:string;branch_id:string;warehouse_id:string;name:string;mode:string;active:boolean;version:number}>(),
  ]);
  const finance=choices.mapping?await posStartupMasterFinanceSource(database,actor.companyId,choices.mapping.chartId,choices.mapping.currency):null;
  const bootstrap=choices.mapping?await posStartupBootstrapFinanceReferences(database,actor.companyId,choices.mapping):null;
  const masterCompatible=!!choices.mapping&&finance?.effective?.chart_id===choices.mapping.chartId&&finance.effective.currency===choices.mapping.currency&&finance.effective.base_currency===choices.mapping.currency;
  const compatible=masterCompatible&&!!bootstrap?.ready;
  const active=capabilities.every(capability=>access[capability].newWork),stock=hasPermission(current,'inventory.read')&&hasPermission(current,'inventory.prepare'),pos=hasPermission(current,'pos.read')&&hasPermission(current,'pos.configure');
  const final=await requireQueuedCommandAuthority(database,actor,needed);if(authorityImage(current)!==authorityImage(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The selected POS startup assignment changed.');
  for(const feature of ['pos','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
  for(const capability of capabilities)await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability,purpose:'history'});
  return {companyId:actor.companyId,available:true,contextKey:createHash('sha256').update(JSON.stringify([authorityImage(current),choices,nodes,settings,registers.results,finance?.image,bootstrap?.image,...capabilities.map(capability=>access[capability].revision)])).digest('hex'),documentCurrency:choices.mapping?.currency??null,
   canCreateWarehouse:active&&stock&&masterCompatible,canCreateProduct:active&&stock&&masterCompatible,canBootstrap:active&&compatible&&choices.warehouses.some(row=>row.status==='active')&&!settings&&!actor.branchId&&pos&&hasPermission(current,'pos.accounting.prepare')&&hasPermission(current,'finance.write'),canConfigureCatalogue:active&&pos&&!!settings,
   canApproveAccounting:active&&!actor.branchId&&!!settings?.accounting_prepared_by&&!settings.accounting_approved_by&&!samePerson(current,settings.accounting_prepared_by)&&hasPermission(current,'pos.accounting.approve'),configured:!!settings,settingsVersion:settings?Number(settings.version):null,accountingPrepared:!!settings?.accounting_prepared_by,accountingApproved:!!settings?.accounting_approved_by,
   bankAccounts:!actor.branchId&&pos&&hasPermission(current,'pos.accounting.prepare')&&hasPermission(current,'finance.read')&&hasPermission(current,'finance.write')?bootstrap?.bankAccounts??[]:[],
   branches:stock?nodes.filter(row=>row.kind==='branch'&&(!actor.branchId||row.id===actor.branchId)).map(row=>({id:row.id,name:row.display_name})):[],
   warehouses:choices.warehouses.map(row=>({id:row.id,branchId:row.branch_id,code:row.code,name:row.name,status:row.status,version:Number(row.version)})),
   products:choices.products.map(row=>({id:row.id,sku:row.sku,name:row.name,itemType:row.item_type,trackingMode:row.tracking_mode,unit:row.unit,priceMinor:Number(row.price_minor),taxBps:Number(row.tax_bps),status:row.status,version:Number(row.version)})),
   registers:registers.results.map(row=>({id:row.id,branchId:row.branch_id,warehouseId:row.warehouse_id,name:row.name,mode:row.mode,active:row.active,version:Number(row.version)})),unavailableReason:!choices.mapping?'posting-setup-required':!compatible?'unsupported-existing-mapping':null};
 });
}
