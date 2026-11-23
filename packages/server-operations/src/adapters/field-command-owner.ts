import {fieldOnlyPolicy} from '../domain/field-ownership';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {holdingResources} from '@erpx/server-runtime/public/runtime';
import {nativeOperationsReadFirst,OPERATIONS_RD_FIELD_EXCEPTION,OPERATIONS_RD_FIELD_POLICY} from './native-operations-read';
/** Routing fact only. The native reader must authorize the record and source.
 * The statement is planned by the Operations native read port and executed on
 * the caller's open unit; this module carries no SQL. */
export async function nativeFieldException(actor:WorkContext,id:string):Promise<boolean>{
 const {write}=await holdingResources(actor.holdingId);
 const row=await nativeOperationsReadFirst<{source_kind:string}>(OPERATIONS_RD_FIELD_EXCEPTION,[id,actor.companyId],write);
 return row?.source_kind==='field_service_order';
}
/** Routing metadata for the existing generic policy URL. Rust still authorizes
 * and validates the actual policy/source before any mutation. The statement is
 * planned by the Operations native read port; this module carries no SQL. */
export async function nativeFieldPolicy(actor:WorkContext,id:string):Promise<boolean>{
 const {write}=await holdingResources(actor.holdingId);
 const row=await nativeOperationsReadFirst<{source_kinds_json:string}>(OPERATIONS_RD_FIELD_POLICY,[id,actor.companyId],write);
 if(!row)return false;
 const kinds:unknown=JSON.parse(row.source_kinds_json);
 return fieldOnlyPolicy(kinds);
}
