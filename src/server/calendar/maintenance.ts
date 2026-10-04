import 'server-only';
import type {SqlDatabase} from '../runtime/types';
import type {CalendarAccess} from './access';
import {rows,idList,inList} from './access';
import type {CalendarItem,CalendarQuery} from '@/lib/calendar';
import {hasPermission} from '@/lib/permissions';
import {hasModuleAccess} from '@/data/module-access';

export async function maintenanceCalendarItems(db:SqlDatabase,a:CalendarAccess,q:CalendarQuery,from:number,to:number,limit:number):Promise<CalendarItem[]>{
 if(!hasPermission(a.actor,'records.read')||!hasModuleAccess(a.actor.modules,'maintenance'))return [];
 const records=await rows<{id:string;source_kind:string;title:string;branch_id:string;department_id:string|null;status:string;version:number;due_at:number;owner_name:string|null}>(db,`WITH scheduled AS (
  SELECT p.id,'maintenance_plan' source_kind,p.company_id,p.branch_id,p.department_id,p.title,p.status,p.version,p.next_due_at due_at,p.owner_id,p.created_by
  FROM maintenance_preventive_plans p WHERE p.status='active'
  UNION ALL SELECT w.id,'maintenance_work',w.company_id,w.branch_id,w.department_id,w.title,w.status,w.version,w.scheduled_at,w.owner_id,w.created_by
  FROM maintenance_work_orders w WHERE w.status NOT IN ('completed','cancelled') AND w.scheduled_at IS NOT NULL)
  SELECT s.*,(SELECT legal_name FROM hr_people WHERE company_id=s.company_id AND user_id=s.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name
  FROM scheduled s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR s.department_id=?) AND ${inList('coalesce(s.department_id,s.branch_id)')}
  AND s.due_at>=? AND s.due_at<? AND (?='team' OR s.owner_id=? OR s.created_by=?)
  AND (?::text IS NULL OR s.owner_id IN (SELECT user_id FROM hr_people WHERE id=? AND company_id=s.company_id))
  AND (?='' OR s.title ILIKE '%'||?||'%') ORDER BY s.due_at,s.id LIMIT ?`,a.actor.companyId,a.actor.branchId,a.actor.branchId,a.actor.departmentId,a.actor.departmentId,idList(a.scopeIds),from,to,q.mode??'mine',a.actor.userId,a.actor.userId,q.person??null,q.person??null,q.search,q.search,limit);
 return records.map(r=>({id:`${r.source_kind}:${r.id}`,source:'maintenance',sourceId:r.id,kind:'task',title:`${r.source_kind==='maintenance_plan'?'Preventive due':'Maintenance'} · ${r.title}`,
  description:r.source_kind==='maintenance_plan'?'Generate the approved preventive work occurrence.':'Review the assigned maintenance work, spare costs and completion evidence.',allDay:false,startDate:null,endDate:null,
  startsAt:r.due_at,endsAt:r.due_at+1,timezone:q.timezone,branchId:r.branch_id,departmentId:r.department_id,participants:[],owner:r.owner_name??'Assigned maintenance colleague',status:r.status,version:r.version,
  permittedActions:[],href:r.source_kind==='maintenance_plan'?`/maintenance/plans/${r.id}`:`/maintenance/${r.id}`,warnings:[],history:[]}));
}
