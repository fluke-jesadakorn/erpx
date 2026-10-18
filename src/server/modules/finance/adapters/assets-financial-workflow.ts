import 'server-only';
import {createHash} from 'node:crypto';
import {hasPermission} from '@/lib/identity_access/permissions';
import {samePerson} from '@/lib/identity_access/iam';
import {hasModuleAccess} from '@/data/module-access';
import type {WorkContext} from '../../identity_access/public/authorization';
import {checkPurchasedCapability,readPurchasedAccesses} from '../../commercial/public/purchase-command-boundary';
import {ApiError} from '../../runtime/public/http';
import {assetsFinancialPhysicalLinks} from '../../assets/public/financial-workflow-sources';
import {readAssetsFinancialInTransaction} from './finance-assets';
import {assetsFinancialOwner,assetsFinancialLeases,requireAssetsFinancialAuthority} from './assets-financial-unit';
import {assetsFinancialReferences,assetsFinancialOriginalSource} from './assets-financial-sources';
import type {Asset} from '../domain/assets';
import type {AssetsFinancialWorkspace} from '../domain/assets-financial-workflow';
const capabilities=['assets.register','finance.asset-recognition','finance.asset-disposal'] as const;
const empty=(companyId:string,unavailableReason:AssetsFinancialWorkspace['unavailableReason']):AssetsFinancialWorkspace=>({companyId,available:false,contextKey:null,canCreate:false,canReadPhysicalRegister:false,canRegisterPhysical:false,assets:[],detail:null,accounts:[],periods:[],dimensions:[],dimensionValues:[],unavailableReason});
const summary=(row:Record<string,unknown>)=>({id:String(row.id),assetNumber:String(row.asset_number),name:String(row.name),status:String(row.status),version:Number(row.version),currency:String(row.currency),costMinor:Number(row.cost_minor),bookValueMinor:Number(row.cost_minor)-Number(row.accumulated_depreciation_minor)});
const stamp=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.authority??null,actor.iam]);
export async function assetsFinancialWorkspace(actor:WorkContext,assetId?:string):Promise<AssetsFinancialWorkspace>{
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read'))return empty(actor.companyId,'authority-required');
 return (await assetsFinancialOwner(actor)).transaction(async database=>{
  await assetsFinancialLeases(database,actor.companyId);const current=await requireAssetsFinancialAuthority(database,actor),access=await readPurchasedAccesses(database,'enforced',actor.companyId,capabilities);
  if(!access['assets.register'].history||!access['finance.asset-recognition'].history&&!access['finance.asset-disposal'].history)return empty(actor.companyId,'purchase-required');
  const data=await readAssetsFinancialInTransaction(database,actor,assetId),references=await assetsFinancialReferences(database,actor),recognition=access['assets.register'].settleExisting&&access['finance.asset-recognition'].settleExisting,disposal=access['assets.register'].settleExisting&&access['finance.asset-disposal'].settleExisting,canWrite=hasPermission(current,'finance.write'),canApprove=hasPermission(current,'finance.approve');
  let detail:AssetsFinancialWorkspace['detail']=null;
  if(data.detail){
   const a:Asset=data.detail.asset,source=await assetsFinancialOriginalSource(database,actor,a.id);
   const chartId=await database.prepare('SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?').bind(a.account_id,actor.companyId).first<string>('chart_id');if(!chartId)throw new ApiError(409,'ASSETS_FINANCIAL_SOURCE_CHANGED','The original Asset chart identity is unavailable.');
   detail={asset:{...summary({...a}),chartId,accountId:a.account_id,depreciationAccountId:a.depreciation_account_id,depreciationExpenseAccountId:a.depreciation_expense_account_id,residualMinor:Number(a.residual_minor),usefulLifeMonths:Number(a.useful_life_months),inServiceDate:a.in_service_date,accumulatedDepreciationMinor:Number(a.accumulated_depreciation_minor),activationJournalId:a.activation_journal_id,
    canEdit:a.status==='draft'&&canWrite&&access['assets.register'].newWork&&access['finance.asset-recognition'].newWork,canCancel:a.status==='draft'&&canWrite&&recognition,canRecognize:a.status==='draft'&&!a.activation_journal_id&&!!a.depreciation_account_id&&!!a.depreciation_expense_account_id&&canApprove&&recognition&&!samePerson(current,a.created_by),canPrepareDisposal:a.status==='in_service'&&!!a.activation_journal_id&&canWrite&&disposal&&!data.detail.disposals.some(row=>row.status==='draft')},
    disposals:data.detail.disposals.map(row=>({id:String(row.id),assetId:a.id,status:String(row.status),version:Number(row.version),assetVersion:Number(row.asset_version),entryDate:String(row.entry_date).slice(0,10),proceedsMinor:Number(row.proceeds_minor),journalId:typeof row.journal_id==='string'?row.journal_id:null,canPost:row.status==='draft'&&Number(row.asset_version)===a.version&&canApprove&&disposal&&!samePerson(current,row.created_by),canCancel:row.status==='draft'&&Number(row.asset_version)===a.version&&canWrite&&disposal})),
    events:data.detail.events.map(row=>({id:String(row.id),action:String(row.event_type),assetVersion:Number(row.asset_version),createdAt:Number(row.created_at)})),depreciations:data.detail.depreciations.map(row=>({id:String(row.id),month:String(row.depreciation_month),entryDate:String(row.entry_date).slice(0,10),amountMinor:Number(row.amount_minor),journalId:String(row.journal_id)})),physicalLinks:(await assetsFinancialPhysicalLinks(database,current,a.id)).map(row=>({...row,version:Number(row.version)}))};
   if(source.physical.invalid)throw new ApiError(409,'ASSETS_FINANCIAL_SOURCE_CHANGED','The original physical Asset relationship is unavailable.');
  }
  const final=await requireAssetsFinancialAuthority(database,actor);if(stamp(current)!==stamp(final))throw new ApiError(403,'QUEUED_AUTHORITY_CHANGED','The selected Company Finance role changed.');
  await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:'assets.register',purpose:'history'});const selected=access['finance.asset-recognition'].history?'finance.asset-recognition':'finance.asset-disposal';await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:selected,purpose:'history'});
  return {companyId:actor.companyId,available:true,contextKey:createHash('sha256').update(JSON.stringify([stamp(current),data,references,...capabilities.map(capability=>access[capability].revision)])).digest('hex'),canCreate:canWrite&&access['assets.register'].newWork&&access['finance.asset-recognition'].newWork,canReadPhysicalRegister:hasPermission(current,'records.read')&&hasModuleAccess(current.modules,'assets')&&access['assets.register'].history,canRegisterPhysical:hasPermission(current,'records.write')&&hasModuleAccess(current.modules,'assets')&&access['assets.register'].newWork,assets:data.assets.map(row=>summary(row)),detail,
   accounts:data.options.accounts.filter(row=>row.status==='active').map(row=>({id:String(row.id),chartId:String(row.chart_id),code:String(row.code),name:String(row.name),category:String(row.category),currency:typeof row.currency==='string'?row.currency:null})),periods:data.options.periods.filter(row=>row.state==='open').map(row=>({id:String(row.id),name:String(row.name),startDate:String(row.start_date).slice(0,10),endDate:String(row.end_date).slice(0,10),state:String(row.state)})),dimensions:data.options.dimensions.map(row=>({id:String(row.id),code:String(row.code),name:String(row.name),required:!!Number(row.required)})),dimensionValues:data.options.dimensionValues.map(row=>({dimensionId:String(row.dimension_id),code:String(row.code),name:String(row.name)})),unavailableReason:null};
 });
}
