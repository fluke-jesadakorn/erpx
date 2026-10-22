import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function workReportRecordsFacts(db:SqlDatabase){return {events:(companyId:string,ids:string)=>db.prepare(`SELECT request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=? AND request_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at,id`).bind(companyId,ids).all<{record_id:string;action:string;actor:string;at:number;reason:string}>()};}
