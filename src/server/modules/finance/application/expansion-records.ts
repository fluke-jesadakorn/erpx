import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import type {FinanceExpansionKind,FinanceRecordRow as Row} from '../domain/record-kinds';
export interface FinanceExpansionRecordPorts<Instrument extends object,Policies extends {assets:Row[]}>{
 error:FinanceError;permission(actor:WorkContext,permission:'finance.read'):WorkContext;ready(actor:WorkContext):Promise<void>;
 row(actor:WorkContext,kind:FinanceExpansionKind,id:string):Promise<Row|null>;tools(actor:WorkContext):Promise<{tool:string;enabled:number}[]>;
 instrumentWorkspace(actor:WorkContext):Promise<Instrument>;assetPolicyWorkspace(actor:WorkContext):Promise<Policies>;
 instrumentEdition(actor:WorkContext,id:string):Promise<Row|null>;instrumentRuns(actor:WorkContext,id:string):Promise<Row[]>;
 assetPolicy(actor:WorkContext,id:string):Promise<Row|null>;assetPolicyRuns(actor:WorkContext,id:string):Promise<Row[]>;
}
export function createFinanceExpansionRecords<Instrument extends object,Policies extends {assets:Row[]}>(ports:FinanceExpansionRecordPorts<Instrument,Policies>){
 async function financeExpansionRecord(actor:WorkContext,kind:FinanceExpansionKind,id:string){
  actor=ports.permission(actor,'finance.read');await ports.ready(actor);
  const row=await ports.row(actor,kind,id);if(!row)throw ports.error(404,'FINANCE_RECORD_NOT_FOUND','Finance record not found.');
  const tools=await ports.tools(actor);
  if(kind==='finance_instrument'||kind==='finance_instrument_run'){
   const data=await ports.instrumentWorkspace(actor),editionId=kind==='finance_instrument'?id:String(row.instrument_id);
   const edition=kind==='finance_instrument'?row:await ports.instrumentEdition(actor,editionId);
   if(!edition)throw ports.error(404,'FINANCE_RECORD_NOT_FOUND','Finance edition not found.');
   const runs=kind==='finance_instrument_run'?[row]:await ports.instrumentRuns(actor,id);
   return {kind,row:kind==='finance_instrument_run'?{...row,name:`${edition.number} · ${row.kind==='payoff'?'Early payoff':`Instalment ${row.ordinal}`}`} : row,enabled:tools.some(tool=>tool.tool==='instruments'&&tool.enabled===1),instrumentData:{...data,editions:[edition],runs}};
  }
  const data=await ports.assetPolicyWorkspace(actor),policyId=kind==='finance_asset_policy'?id:String(row.policy_id);
  const policy=kind==='finance_asset_policy'?row:await ports.assetPolicy(actor,policyId);
  if(!policy)throw ports.error(404,'FINANCE_RECORD_NOT_FOUND','Depreciation policy not found.');
  const runs=kind==='finance_asset_policy_run'?[row]:await ports.assetPolicyRuns(actor,id);
  const asset=data.assets.find(a=>a.id===policy.asset_id),reference=String(asset?.asset_number??'Fixed asset');
  const serviceMonth=kind==='finance_asset_policy_run'?String((JSON.parse(String(row.input_json)) as {depreciationMonth?:string}).depreciationMonth??String(row.entry_date).slice(0,7)):null;
  return {kind,row:{...row,name:serviceMonth?`${reference} · Depreciation ${serviceMonth}`:`${reference} · Depreciation policy`},enabled:tools.some(tool=>tool.tool==='asset_policies'&&tool.enabled===1),assetPolicyData:{...data,policies:[policy],runs}};
 }
 return {financeExpansionRecord};
}
