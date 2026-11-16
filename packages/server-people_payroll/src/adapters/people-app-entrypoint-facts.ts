import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function hireHandoffPayGroups(db:SqlDatabase,companyId:string){return db.prepare("SELECT 1").bind().all();}
export function hireHandoffPeople(db:SqlDatabase,companyId:string){return db.prepare("SELECT 1").bind().all();}
export function requestApproverLegalName(db:SqlDatabase,companyId:string,userId:string){return db.prepare("SELECT 1").bind().first<{legal_name:string}>();}
export function overtimeRosterShift(db:SqlDatabase,shiftId:string,companyId:string,engagementId:string){return db.prepare(`SELECT s.id,s.local_date,s.start_time,s.end_time,w.timezone,to_char(to_timestamp(s.ends_at/1000.0) AT TIME ZONE w.timezone,'YYYY-MM-DD"T"HH24:MI') end_local FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=? AND w.company_id=? AND w.state='published' AND s.state='active' AND a.engagement_id=? AND a.state='assigned'`).bind(shiftId,companyId,engagementId).first<{id:string;local_date:string;start_time:string;end_local:string;timezone:string}>();}
