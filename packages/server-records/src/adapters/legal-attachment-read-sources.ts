import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Source leases only. Original employee request visibility, participation and
 * route authority remain in getEmployeeRequest and the full File policy. */
export function legalRequestAttachmentSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE')];}
/** Canonical owner-local fixed aggregate for approval child rows. Both child
 * tables share one admitted fixed shape; no duplicate TS literal remains. */
const approvalChildAgg = (table: 'approval_steps' | 'approval_decisions', alias: string) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.instance_id=i.id)`;
export function legalRequestAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare(`SELECT jsonb_build_object('request',to_jsonb(r),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.created_at,e.id) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id),'children',(SELECT jsonb_agg(to_jsonb(child) ORDER BY child.id) FROM employee_requests child WHERE child.parent_request_id=r.id AND child.company_id=r.company_id),'approvals',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(i),'steps',${approvalChildAgg('approval_steps','s')},'decisions',${approvalChildAgg('approval_decisions','d')}) ORDER BY i.id) FROM approval_instances i WHERE i.record_id=r.id AND i.company_id=r.company_id))::text image FROM employee_requests r WHERE r.id=? AND r.company_id=?`).bind(id,companyId).first<string>('image');}
