/** Frontend models are generated from Rust; Rust owns validation and writes. */
export type {ServiceCustomerParty,ServiceCustomerInput,ServiceCustomerResult,ServiceCustomerPreparation,ServiceCustomerConfirmation} from '@erpx/contracts/field-service-reserve';
import type {ServiceCustomerPreparation} from '@erpx/contracts/field-service-reserve';
const row=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
export function validServiceCustomerPreparation(v:unknown,company:string):v is ServiceCustomerPreparation{
 if(!row(v)||v.companyId!==company||typeof v.available!=='boolean'||typeof v.canCreate!=='boolean'||!Array.isArray(v.currencies)||!Array.isArray(v.customers)||!v.currencies.every(c=>row(c)&&typeof c.code==='string'&&/^[A-Z]{3}$/.test(c.code)&&text(c.name))||!v.customers.every(c=>row(c)&&['id','code','name','currency'].every(k=>text(c[k]))))return false;
 return v.available?v.unavailableReason===null&&typeof v.contextKey==='string'&&/^[a-f0-9]{64}$/.test(v.contextKey):v.contextKey===null&&v.canCreate===false&&v.currencies.length===0&&v.customers.length===0&&['authority-required','purchase-required'].includes(String(v.unavailableReason));
}
