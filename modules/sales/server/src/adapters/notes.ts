import 'server-only';
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {salesNoteCustomer} from "@erpx/finance-server/public/sales-customer-sources";
import {getOrder} from '@erpx/commerce-server/public/commerce-order-read';
import {type Row} from '@erpx/commerce-server/public/commerce';
import {holdingResources} from "../../../../../platform/runtime/server/src/public/runtime";
import {requirePermission} from "@erpx/identity-access-server/public/iam-permissions";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {samePerson} from "@erpx/identity-access-ui/lib/iam";
import {ApiError} from "@erpx/platform-runtime-server/public/http";

import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {createCustomerNoteApplication,type CustomerNoteRepository} from '@erpx/sales-server/application/notes';
import {type ContactNote} from '@erpx/sales-server/domain/notes';
import {readCustomerNotes} from './native-lane-30/client';
export type {CustomerNotesView} from '@erpx/sales-server/domain/notes';
export {customerNoteSchema,customerNoteChangeSchema} from '@erpx/sales-server/domain/notes';
/**
 * Customer note read projections. Create/correct/archive are native-owned by
 * the Customer 360 contact-note command. The note and immutable-history reads
 * were migrated to the native Sales axum owner (lane 30); this module now only
 * transports the bounded `{notes,history}` result through the strict client.
 * There is no TypeScript SQL fallback: an unavailable native owner fails closed.
 */
function application(a:WorkContext){let resource:Promise<Awaited<ReturnType<typeof holdingResources>>['write']>|null=null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createCustomerNoteApplication({canSell:hasPermission(a,'commerce.sales'),canManage:hasPermission(a,'commerce.approve'),branchId:a.branchId,departmentId:a.departmentId,authorize:permission=>{requirePermission(a,permission);},isSamePerson:id=>samePerson(a,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),a)});}
function repository(write:Awaited<ReturnType<typeof holdingResources>>['write'],a:WorkContext):CustomerNoteRepository{return {
  order:async id=>(await getOrder(a,id)).order,
  customer:async(id,version)=>(await salesNoteCustomer(write,a,id,version))??null,
  async notes(kind,id){const result=await readCustomerNotes(a,kind,id);return result.notes as unknown as ContactNote[];},
  async history(kind,id){const result=await readCustomerNotes(a,kind,id);return result.history as Row[];},
  };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerNotesForOrder(a:WorkContext,id:string){return translated(()=>application(a).forOrder(id));}
export function customerNotesForCustomer(a:WorkContext,id:string){return translated(()=>application(a).forCustomer(id));}
