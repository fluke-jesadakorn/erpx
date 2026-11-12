import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";
import {salesNoteCustomer} from "@erpx/server-finance/public/sales-customer-sources";
import 'server-only';
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {getOrder} from '@erpx/server-commerce/public/commerce-order-read';
import {type Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {ApiError} from "@erpx/server-runtime/public/http";

import {BusinessRuleError} from '@erpx/server-shared/index';
import {createCustomerNoteApplication,type CustomerNoteRepository} from '@erpx/server-sales/application/notes';
import type {ContactNote} from '@erpx/server-sales/domain/notes';
export type {CustomerNotesView} from '@erpx/server-sales/domain/notes';
export {customerNoteSchema,customerNoteChangeSchema} from '@erpx/server-sales/domain/notes';
/**
 * Retained customer note read projections. Create/correct/archive are
 * native-owned by the Customer 360 contact-note command; this module keeps only
 * the order/customer note and history reads used by presenters and Customer 360.
 */
const scope="o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)";
const args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];

function application(a:WorkContext){let resource:Promise<Awaited<ReturnType<typeof holdingResources>>['write']>|null=null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createCustomerNoteApplication({canSell:hasPermission(a,'commerce.sales'),canManage:hasPermission(a,'commerce.approve'),branchId:a.branchId,departmentId:a.departmentId,authorize:permission=>{requirePermission(a,permission);},isSamePerson:id=>samePerson(a,id),clock:{now:()=>Date.now()},repository:async()=>repository(await database(),a)});}
function repository(write:Awaited<ReturnType<typeof holdingResources>>['write'],a:WorkContext):CustomerNoteRepository{return {
 order:async id=>(await getOrder(a,id)).order,
 customer:async(id,version)=>await salesNoteCustomer(write,a,id,version),
 async notes(kind,id){const orderId=id,customerId=id;return kind==='order'?(await write.prepare(`SELECT n.* FROM customer_contact_notes n JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY n.created_at DESC,n.id DESC`).bind(orderId,...args(a)).all<ContactNote>()).results:(await write.prepare(`SELECT n.* FROM customer_contact_notes n LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY n.created_at DESC,n.id DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<ContactNote>()).results;},
 async history(kind,id){const orderId=id,customerId=id;return kind==='order'?(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.order_id=? AND ${scope} ORDER BY h.created_at DESC,h.version DESC`).bind(orderId,...args(a)).all<Row>()).results:(await write.prepare(`SELECT h.* FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id LEFT JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=n.order_id AND o.company_id=n.company_id WHERE n.company_id=? AND n.customer_id=? AND (n.order_id IS NULL AND ?::text IS NULL AND ?::text IS NULL OR n.order_id IS NOT NULL AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)) ORDER BY h.created_at DESC,h.version DESC`).bind(a.companyId,customerId,a.branchId,a.departmentId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results;},
 };}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function customerNotesForOrder(a:WorkContext,id:string){return translated(()=>application(a).forOrder(id));}
export function customerNotesForCustomer(a:WorkContext,id:string){return translated(()=>application(a).forCustomer(id));}
