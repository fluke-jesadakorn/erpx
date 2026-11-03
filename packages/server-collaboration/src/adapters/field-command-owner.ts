import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
/** Immutable owner routing facts only; the chosen owner still authorizes the
 * complete command/read. No source body, grant, receipt or write is returned. */
export async function nativeFieldCollaborationCommand(actor:WorkContext,input:unknown):Promise<boolean>{
 if(!input||typeof input!=='object'||Array.isArray(input))return false;
 const v=input as Record<string,unknown>;
 if(typeof v.id!=='string')return v.entity==='relation'?v.sourceType==='field_service_order'||v.targetType==='field_service_order':v.sourceType==='field_service_order';
 const {write}=await holdingResources(actor.holdingId);
 if(v.entity==='relation'){
  const row=await write.prepare('SELECT source_type,target_type FROM collaboration_record_relations WHERE id=? AND company_id=?').bind(v.id,actor.companyId).first<{source_type:string;target_type:string}>();
  return row?.source_type==='field_service_order'||row?.target_type==='field_service_order';
 }
 const row=await write.prepare('SELECT source_type FROM collaboration_action_cards WHERE id=? AND company_id=?').bind(v.id,actor.companyId).first<{source_type:string}>();
 return row?.source_type==='field_service_order';
}
