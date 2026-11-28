import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
/** Source leases only. Original employee request visibility, participation and
 * route authority remain in getEmployeeRequest and the full File policy. */
export function legalRequestAttachmentSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE')];}
/** Canonical owner-local generator for request child-row aggregates. The
 * approval step/decision aggregates, the employee-request children aggregate
 * and the request-events aggregate share one admitted fixed shape; each call
 * renders byte-identical prior SQL. The optional order frame admits the events
 * aggregate's created_at,id ordering without a second query literal. */
export const childRowAgg = (table: string, alias: string, fk: string, parent: string, company?: string, order = `${alias}.id`) =>
  `(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${table} ${alias} WHERE ${alias}.${fk}=${parent}.id${company ? ` AND ${alias}.${company}=${parent}.${company}` : ''})`;
export function legalRequestAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare(`SELECT jsonb_build_object('request',to_jsonb(r),'events',${childRowAgg('employee_request_events','e','request_id','r','company_id','e.created_at,e.id')},'children',${childRowAgg('employee_requests','child','parent_request_id','r','company_id')},'approvals',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(i),'steps',${childRowAgg('approval_steps','s','instance_id','i')},'decisions',${childRowAgg('approval_decisions','d','instance_id','i')}) ORDER BY i.id) FROM approval_instances i WHERE i.record_id=r.id AND i.company_id=r.company_id))::text image FROM employee_requests r WHERE r.id=? AND r.company_id=?`).bind(id,companyId).first<string>('image');}
