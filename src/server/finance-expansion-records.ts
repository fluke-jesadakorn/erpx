import 'server-only';
import type {WorkContext} from './authorization';
import {requirePermission} from './iam-permissions';
import {holdingResources} from './runtime';
import {ApiError} from './http';
import {financeCompanyTools} from './finance-company-tools';
import {instrumentWorkspace} from './finance-instruments';
import {assetPolicyWorkspace} from './finance-asset-policies';
const tables={finance_instrument:'finance_instrument_editions',finance_instrument_run:'finance_instrument_runs',finance_asset_policy:'finance_asset_policy_editions',finance_asset_policy_run:'finance_asset_policy_runs'} as const;
export type FinanceExpansionKind=keyof typeof tables;
export function isFinanceExpansionKind(kind:string):kind is FinanceExpansionKind{return Object.hasOwn(tables,kind);}
export async function financeExpansionRecord(actor:WorkContext,kind:FinanceExpansionKind,id:string){
 actor=requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 const row=await write.prepare(`SELECT r.*,COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=r.prepared_by AND p.company_id=r.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked Finance account') prepared_by_name,COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=r.reviewed_by AND p.company_id=r.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked Finance account') reviewed_by_name FROM ${tables[kind]} r WHERE r.company_id=? AND r.id=?`).bind(actor.companyId,id).first<Record<string,string|number|null>>();
 if(!row)throw new ApiError(404,'FINANCE_RECORD_NOT_FOUND','Finance record not found.');
 const tools=await financeCompanyTools(actor);
 if(kind==='finance_instrument'||kind==='finance_instrument_run'){
  const data=await instrumentWorkspace(actor),editionId=kind==='finance_instrument'?id:String(row.instrument_id);
  const edition=kind==='finance_instrument'?row:await write.prepare('SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?').bind(actor.companyId,editionId).first<Record<string,string|number|null>>();
  if(!edition)throw new ApiError(404,'FINANCE_RECORD_NOT_FOUND','Finance edition not found.');
  const runs=kind==='finance_instrument_run'?[row]:(await write.prepare('SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id').bind(actor.companyId,id).all<Record<string,string|number|null>>()).results;
  return {kind,row:kind==='finance_instrument_run'?{...row,name:`${edition.number} · ${row.kind==='payoff'?'Early payoff':`Instalment ${row.ordinal}`}`} : row,enabled:tools.some(tool=>tool.tool==='instruments'&&tool.enabled===1),instrumentData:{...data,editions:[edition],runs}};
 }
 const data=await assetPolicyWorkspace(actor),policyId=kind==='finance_asset_policy'?id:String(row.policy_id);
 const policy=kind==='finance_asset_policy'?row:await write.prepare('SELECT * FROM finance_asset_policy_editions WHERE company_id=? AND id=?').bind(actor.companyId,policyId).first<Record<string,string|number|null>>();
 if(!policy)throw new ApiError(404,'FINANCE_RECORD_NOT_FOUND','Depreciation policy not found.');
 const runs=kind==='finance_asset_policy_run'?[row]:(await write.prepare('SELECT * FROM finance_asset_policy_runs WHERE company_id=? AND policy_id=? ORDER BY ordinal,id').bind(actor.companyId,id).all<Record<string,string|number|null>>()).results;
 const asset=data.assets.find(a=>a.id===policy.asset_id),reference=String(asset?.asset_number??'Fixed asset');
 const serviceMonth=kind==='finance_asset_policy_run'?String((JSON.parse(String(row.input_json)) as {depreciationMonth?:string}).depreciationMonth??String(row.entry_date).slice(0,7)):null;
 return {kind,row:{...row,name:serviceMonth?`${reference} · Depreciation ${serviceMonth}`:`${reference} · Depreciation policy`},enabled:tools.some(tool=>tool.tool==='asset_policies'&&tool.enabled===1),assetPolicyData:{...data,policies:[policy],runs}};
}
