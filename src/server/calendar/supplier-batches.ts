import 'server-only';
import type {CalendarItem,CalendarQuery} from '@/lib/calendar';
import {addDays} from '@/lib/people-roster';
import {hasPermission} from '@/lib/permissions';
import type {SqlDatabase} from '../runtime/types';
import {type CalendarAccess,rows} from './access';
export async function supplierBatchCalendarItems(db:SqlDatabase,a:CalendarAccess,q:CalendarQuery,from:string,to:string,limit:number):Promise<CalendarItem[]> {
 if(!hasPermission(a.actor,'finance.read')||a.actor.branchId||a.actor.departmentId||!a.scopeIds.includes(a.actor.companyId))return [];
 const records=await rows<{id:string;planned_date:string;status:string;version:number;unknown_count:number}>(db,`
 SELECT b.id,b.planned_date,b.status,b.version,(SELECT count(*) FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status='unknown') unknown_count
 FROM finance_supplier_batches b WHERE b.company_id=? AND b.planned_date>=? AND b.planned_date<? AND b.status<>'cancelled'
 AND (?='team' OR b.created_by=? OR b.approved_by=? OR b.exported_by=?)
 AND (?::text IS NULL OR b.created_by IN (SELECT user_id FROM hr_people WHERE id=? AND company_id=b.company_id))
 AND (?='' OR b.id ILIKE '%'||?||'%') ORDER BY b.id LIMIT ?`,a.actor.companyId,from,to,q.mode??'mine',a.actor.userId,a.actor.userId,a.actor.userId,q.person??null,q.person??null,q.search,q.search,limit);
 return records.map(row=>({id:`supplier_batch:${row.id}`,source:'supplier_batches',sourceId:row.id,kind:'task',title:'Supplier payment batch',description:'Review invoice sources, approval and individual bank results in Finance.',allDay:true,startDate:row.planned_date,endDate:addDays(row.planned_date,1),startsAt:null,endsAt:null,timezone:q.timezone,branchId:null,departmentId:null,participants:[],owner:'Company Finance / Treasury',status:row.status,version:row.version,permittedActions:[],href:`/finance/supplier-batches/${row.id}`,warnings:row.unknown_count?['Bank result uncertain: reservations remain in place.']:[],history:[]}));
}
