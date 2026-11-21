import {fieldOnlyPolicy} from '../domain/field-ownership';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
/** Routing fact only. The native reader must authorize the record and source. */
export async function nativeFieldException(actor:WorkContext,id:string):Promise<boolean>{
 const {write}=await holdingResources(actor.holdingId);
 const row=await write.prepare('SELECT source_kind FROM operations_exceptions WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{source_kind:string}>();
 return row?.source_kind==='field_service_order';
}
/** Routing metadata for the existing generic policy URL. Rust still authorizes
 * and validates the actual policy/source before any mutation. */
export async function nativeFieldPolicy(actor:WorkContext,id:string):Promise<boolean>{
 const {write}=await holdingResources(actor.holdingId);
 const row=await write.prepare('SELECT source_kinds_json FROM operations_queue_policies WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{source_kinds_json:string}>();
 if(!row)return false;
 const kinds:unknown=JSON.parse(row.source_kinds_json);
 return fieldOnlyPolicy(kinds);
}
