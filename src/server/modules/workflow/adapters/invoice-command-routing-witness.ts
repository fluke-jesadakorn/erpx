import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {expenseAuthoritySnapshot} from '../../people_payroll/public/expense-routing';
import {invoiceRoutingOrganizationSourceReadLocks} from '../../organization/public/invoice-routing-source-locks';
import {flowRecord,prepareFlowCommand} from './flow-routing';
import {scopeFor} from '../domain/flow-routing';

type NativeRoute={company_id:string;kind:string;record_id:string;version:number;defaults_revision:number;steps_json:string;created_at:number};
/** Route creation is a real owner write. SRE avoids a SHARE→INSERT upgrade;
 * all locks are query-free and follow the normal receipt-before-route order. */
export function invoiceCommandRoutingSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE flow_routes IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE flow_defaults IN SHARE MODE'),...invoiceRoutingOrganizationSourceReadLocks(database)];}
/** A private captured source, never a replacement Flow authority. The original
 * owner resolves and authorizes the current reviewer before any decision. */
export async function captureInvoiceCommandRoutingWitness(database:SqlDatabase,actor:WorkContext,id:string,key:string,input:unknown,version:number){
 const startedAt=Date.now(),record=await flowRecord(database,actor.companyId,'invoice',id),scope=scopeFor(record),prepared=await prepareFlowCommand(database,actor,key,'invoice',id,'post',input,version),view=prepared.view;
 const route=()=>database.prepare("SELECT * FROM flow_routes WHERE company_id=? AND kind='invoice' AND record_id=?").bind(actor.companyId,id).first<NativeRoute>();
 const defaults=()=>database.prepare("SELECT to_jsonb(d)::text image FROM flow_defaults d WHERE company_id=? AND kind='invoice' AND (branch_id IS NULL OR branch_id=?) AND (department_id IS NULL OR department_id=?) AND revision<=? ORDER BY revision").bind(actor.companyId,scope.branch_id,scope.department_id,view.defaultsRevision).all<{image:string}>();
 const originalRoute=await route(),originalDefaults=(await defaults()).results,authority=await expenseAuthoritySnapshot(database,actor.companyId);
 // This shape is based only on the actual original owner's returned FlowView,
 // using its normal automatic route snapshot contract. It is not a new route.
 const automaticSteps=Object.fromEntries(view.steps.map(step=>[step.key,{primaryUserId:step.primary?.userId??null,teamName:step.teamName,defaultRevision:view.defaultsRevision}]));
 const equal=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b),changed=()=>new ApiError(409,'FLOW_ROUTING_CHANGED','The original invoice responsibility or reviewer eligibility changed before confirmation committed.');
 return {async assertCurrent(){
  const current=await route();
  if(originalRoute){if(!equal(current,originalRoute))throw changed();}
  else{
   if(!current||current.company_id!==actor.companyId||current.kind!=='invoice'||current.record_id!==id||current.version!==1||current.defaults_revision!==view.defaultsRevision||!equal(JSON.parse(current.steps_json),automaticSteps)||current.created_at<startedAt||current.created_at>Date.now())throw changed();
   const ownerEvent=await database.prepare("SELECT 1 FROM flow_events WHERE command_id=? AND company_id=? AND kind='invoice' AND record_id=? AND step_key=? AND action='post' AND actor_user_id=? AND record_version=?").bind(key,actor.companyId,id,view.current?.key??view.state,actor.userId,version).first();if(!ownerEvent)throw changed();
  }
  if(!equal((await defaults()).results,originalDefaults))throw changed();
  if(!await database.prepare(`SELECT (${authority.query}) same`).bind(...authority.args).first<boolean>('same'))throw changed();
 }};
}
