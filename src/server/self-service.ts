import "server-only";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { activeStaff } from "./staff-policy";
import { leaveSummary } from "./leave-self-service";
import { overtimeSetup } from "./overtime";
import { workSnapshot } from "./work-items";
import { isMyAction } from "@/lib/work-item";
import { selfServiceActions } from "@/lib/self-service";
import { ApiError } from "./http";
export async function selfServiceSummary(actor:WorkContext, onWorkCount?: (count: number | null) => void) {
 const {write}=await holdingResources(actor.holdingId);
 const sources=await Promise.allSettled([
  (async()=>{const staff=await activeStaff(write,actor.companyId,actor.userId);return write.prepare(`SELECT s.id,s.name,s.local_date,s.start_time,s.end_time,s.starts_at,s.ends_at,w.timezone FROM people_roster_assignments a JOIN people_roster_shifts s ON s.id=a.shift_id JOIN people_roster_weeks w ON w.id=s.week_id WHERE w.company_id=? AND a.engagement_id=? AND w.state='published' AND a.state='assigned' AND s.state='active' AND s.ends_at>? ORDER BY s.starts_at LIMIT 1`).bind(actor.companyId,staff.engagement_id,Date.now()).first<{id:string;name:string;local_date:string;start_time:string;end_time:string;starts_at:number;ends_at:number;timezone:string}>();})(),
  leaveSummary(actor),overtimeSetup(write,actor),workSnapshot(actor),
 ]);
 const [shift,leave,overtime,work]=sources;
 onWorkCount?.(work.status === 'fulfilled' && !work.value.unavailableSources.length ? work.value.items.filter(isMyAction).length : null);
 const staffSetupRequired=[shift,overtime].some(s=>s.status==='rejected'&&s.reason instanceof ApiError&&s.reason.code==='STAFF_LINK_REQUIRED');
 const unavailable=sources.flatMap((s,i)=>s.status==='rejected'&&!(s.reason instanceof ApiError&&s.reason.code==='STAFF_LINK_REQUIRED')?[["Schedule","Leave","Overtime","Work"][i]]:[]);
 const setup=sources.flatMap(s=>s.status==='rejected'&&s.reason instanceof ApiError&&s.reason.code!=='STAFF_LINK_REQUIRED'?[s.reason.message]:[]);
 if(staffSetupRequired)setup.push('Connect this account to one active employee or contractor profile to unlock personal schedule, leave, and overtime.');
 if(leave.status==='fulfilled'&&leave.value.setupMessage&&!staffSetupRequired)setup.push(leave.value.setupMessage);
 if(overtime.status==='fulfilled')setup.push(...overtime.value.blockers);
 return {asOf:Date.now(),companyId:actor.companyId,actions:selfServiceActions,staffSetupRequired,setupBlockers:[...new Set(setup)],unavailableSources:[...new Set([...unavailable,...(work.status==='fulfilled'?work.value.unavailableSources:[])])],
  nextShift:shift.status==='fulfilled'?shift.value:null,
  attention:work.status==='fulfilled'?work.value.items.filter(isMyAction).slice(0,3):null,
  recentRequests:work.status==='fulfilled'?work.value.items.filter(i=>i.relationships?.includes('requested')&&i.queue!=='completed').sort((a,b)=>b.updatedAt-a.updatedAt).slice(0,3):null,
  leave:leave.status==='fulfilled'?{setupRequired:leave.value.setupRequired,policy:leave.value.policy}:null,
 };
}
export type SelfServiceSummary=Awaited<ReturnType<typeof selfServiceSummary>>;

/** Internal streaming metadata; the HTTP JSON response stays unchanged. */
export async function selfServiceSeed(actor: WorkContext): Promise<{ data: SelfServiceSummary; workCount: number | null }> {
 let workCount: number | null = null;
 const data = await selfServiceSummary(actor, count => { workCount = count; });
 return { data, workCount };
}
export type SelfServiceSeed = Awaited<ReturnType<typeof selfServiceSeed>>;
