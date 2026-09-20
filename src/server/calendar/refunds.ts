import 'server-only';
import type {CalendarItem,CalendarQuery} from '@/lib/calendar';
import {addDays} from '@/lib/people-roster';
import {hasPermission} from '@/lib/permissions';
import type {SqlDatabase} from '../runtime/types';
import {type CalendarAccess,idList,inList,rows} from './access';
export async function refundCalendarItems(db:SqlDatabase,a:CalendarAccess,q:CalendarQuery,from:string,to:string,limit:number):Promise<CalendarItem[]> {
 if(!hasPermission(a.actor,'finance.read'))return [];
 const records=await rows<{id:string;obligation_id:string;planned_date:string;status:string;version:number;branch_id:string;department_id:string|null;prepared_by:string;owner_name:string;number:string}>(db,`
 SELECT i.id,i.obligation_id,i.planned_date,i.status,i.version,i.prepared_by,o.branch_id,o.department_id,r.number,coalesce((SELECT legal_name FROM hr_people WHERE user_id=i.prepared_by AND company_id=o.company_id AND merged_into_id IS NULL LIMIT 1),'Treasury') owner_name
 FROM finance_refund_instructions i JOIN finance_refund_obligations o ON o.id=i.obligation_id JOIN commerce_returns r ON r.id=o.return_id
 WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)
 AND ${inList('coalesce(o.department_id,o.branch_id,o.company_id)')}
 AND i.planned_date>=? AND i.planned_date<? AND i.status NOT IN ('cancelled','failed')
 AND (?='team' OR i.prepared_by=? OR o.created_by=? OR o.approved_by=?)
 AND (?::text IS NULL OR i.prepared_by IN (SELECT user_id FROM hr_people WHERE id=? AND company_id=o.company_id))
 AND (?='' OR r.number ILIKE '%'||?||'%') ORDER BY i.id LIMIT ?`,a.actor.companyId,a.actor.branchId,a.actor.branchId,a.actor.departmentId,a.actor.departmentId,idList(a.scopeIds),from,to,q.mode??'mine',a.actor.userId,a.actor.userId,a.actor.userId,q.person??null,q.person??null,q.search,q.search,limit);
 return records.map(row=>({id:`refund:${row.id}`,source:'refunds',sourceId:row.id,kind:'task',title:`Refund · ${row.number}`,description:'Review the instruction and bank evidence in Finance.',allDay:true,startDate:row.planned_date,endDate:addDays(row.planned_date,1),startsAt:null,endsAt:null,timezone:q.timezone,branchId:row.branch_id,departmentId:row.department_id,participants:[],owner:row.owner_name,status:row.status,version:row.version,permittedActions:[],href:`/finance/refunds/${row.obligation_id}`,warnings:row.status==='unknown'?['Bank result uncertain: keep the amount reserved until evidence is reviewed.']:[],history:[]}));
}
