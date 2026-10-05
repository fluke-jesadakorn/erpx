import 'server-only';
import {z} from 'zod';
import {authorizedBooking} from '../pms-booking';
import {ApiError} from '../http';
import type {TransactionalSqlDatabase} from '../runtime/types';
import type {Entry,Folio,Guest} from '@/lib/pms-model';

/** Project only the accepted document snapshot; never expose GL, staff or identity files. */
export async function guestBookingDocument(db:TransactionalSqlDatabase,token:string,raw:unknown,now=Date.now()){
 const {id:documentId}=z.object({id:z.string().min(1).max(128)}).strict().parse(raw);
 return db.transaction(async tx=>{
  const {s,id}=await authorizedBooking(tx,token,now);
  const row=await tx.prepare(`SELECT d.kind,d.snapshot_json,d.created_at FROM pms_documents d JOIN pms_records f ON f.id=d.folio_id AND f.property_id=d.property_id AND f.kind='folio'
    WHERE d.id=? AND d.property_id=? AND f.data_json::jsonb->>'reservationId'=? AND d.kind IN ('invoice','receipt','refund','credit_note')`).bind(documentId,s.property.id,id).first<{kind:string;snapshot_json:string;created_at:number}>();
  if(!row)throw new ApiError(404,'PMS_DOCUMENT_NOT_FOUND','Document not found for this booking');
  const snapshot=JSON.parse(row.snapshot_json) as {property:{name:string;currency:string};guest:Guest;folio:Folio;entries:Array<{data:Entry}>;amount:number;businessDate:string};
  return {number:`PMS-${documentId.slice(-12)}`,kind:row.kind,issuedAt:Number(row.created_at),date:snapshot.businessDate,hotel:snapshot.property.name,currency:snapshot.property.currency,guest:snapshot.guest.name,folio:snapshot.folio.name,amount:snapshot.amount,dueAtIssue:snapshot.folio.due,entries:snapshot.entries.map(({data:e})=>({date:e.date,description:e.description,net:e.net,service:e.service,tax:e.tax,total:e.total}))};
 });
}
